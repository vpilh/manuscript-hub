//! Mailbox accounts over IMAP (WP 2.2): connect, check for new mail, disconnect.
//!
//! Passwords go from the setup form straight into the OS keychain and are read
//! back only to log in. Network work runs on a blocking thread so the window
//! stays responsive.

use std::sync::{Arc, Mutex, MutexGuard};

use mh_connectors::imap::{check_account, Folder, ImapProvider, ImapSettings};
use mh_connectors::{MailProvider, ProviderError, SyncCursor};
use mh_core::Provider;
use mh_parser::CandidateRules;
use mh_store::{AccountSummary, Store};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{keychain, record_batch, AppState, ScanSummary};

#[derive(Serialize)]
pub struct MailAccount {
    id: i64,
    provider: Provider,
    alias: String,
    settings: ImapSettings,
    last_sync_at: Option<i64>,
    last_error: Option<String>,
}

/// The setup form. Deliberately not `Debug`, so the password can't be logged.
#[derive(Deserialize)]
pub struct NewMailAccount {
    provider: Provider,
    alias: String,
    settings: ImapSettings,
    password: String,
}

fn to_view(account: AccountSummary) -> Option<MailAccount> {
    if !matches!(account.provider, Provider::Gmail | Provider::GroupwiseImap) {
        return None;
    }
    let settings = serde_json::from_str(&account.settings_json).ok()?;
    Some(MailAccount {
        id: account.id,
        provider: account.provider,
        alias: account.alias,
        settings,
        last_sync_at: account.last_sync_at,
        last_error: account.last_error,
    })
}

fn lock(store: &Mutex<Store>) -> Result<MutexGuard<'_, Store>, String> {
    store
        .lock()
        .map_err(|_| "database lock poisoned".to_string())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn mail_accounts(state: State<'_, AppState>) -> Result<Vec<MailAccount>, String> {
    state.with_store(|store| Ok(store.accounts()?.into_iter().filter_map(to_view).collect()))
}

/// Logs in once to check the settings, picks the folders to watch, then saves
/// the account and its password.
#[tauri::command]
pub async fn connect_mail_account(
    state: State<'_, AppState>,
    account: NewMailAccount,
) -> Result<MailAccount, String> {
    let store = state.handle()?;
    let NewMailAccount {
        provider,
        alias,
        mut settings,
        password,
    } = account;
    if !matches!(provider, Provider::Gmail | Provider::GroupwiseImap) {
        return Err("Only Gmail and IMAP mailboxes can be connected here.".into());
    }
    let alias = alias.trim().to_string();
    settings.host = settings.host.trim().to_string();
    settings.username = settings.username.trim().to_string();
    if alias.is_empty() || settings.host.is_empty() || settings.username.is_empty() {
        return Err("Enter a name, a server and a user name.".into());
    }

    blocking(move || {
        let folders = check_account(&settings, &password).map_err(|e| e.to_string())?;
        settings.folders = choose_folders(&settings.folders, &folders)?;
        let settings_json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
        let id = lock(&store)?
            .add_account(provider, &alias, &settings_json)
            .map_err(|e| e.to_string())?;
        if let Err(e) = keychain::save_mail_password(id, &password) {
            // Without a stored password the account can't sync; don't keep it.
            let _ = lock(&store)?.remove_account(id);
            return Err(e);
        }
        let account = lock(&store)?.account(id).map_err(|e| e.to_string())?;
        to_view(account).ok_or_else(|| "The account could not be read back.".to_string())
    })
    .await
}

/// Requested folders must exist; with none requested, Gmail's "All Mail"
/// (found by its `\All` attribute, whatever the interface language) is used,
/// otherwise the inbox.
fn choose_folders(requested: &[String], available: &[Folder]) -> Result<Vec<String>, String> {
    if requested.is_empty() {
        let all_mail = available.iter().find(|f| f.all_mail && f.selectable);
        return Ok(vec![
            all_mail.map_or("INBOX".to_string(), |f| f.name.clone())
        ]);
    }
    for name in requested {
        if !available.iter().any(|f| f.selectable && f.name == *name) {
            return Err(format!(
                "The folder \"{name}\" doesn't exist on this server."
            ));
        }
    }
    Ok(requested.to_vec())
}

#[tauri::command]
pub fn disconnect_mail_account(state: State<'_, AppState>, account_id: i64) -> Result<(), String> {
    // The password goes first: an account row without one is harmless and can
    // be removed again, a stray password is not.
    keychain::delete_mail_password(account_id)?;
    state.with_store(|store| store.remove_account(account_id))
}

#[tauri::command]
pub async fn sync_mail_account(
    state: State<'_, AppState>,
    account_id: i64,
) -> Result<ScanSummary, String> {
    let store = state.handle()?;
    blocking(move || sync(&store, account_id)).await
}

fn sync(store: &Arc<Mutex<Store>>, account_id: i64) -> Result<ScanSummary, String> {
    let account = lock(store)?
        .account(account_id)
        .map_err(|e| e.to_string())?;
    let account = to_view(account).ok_or("This account can't be checked for mail.")?;
    let result = keychain::mail_password(account_id)
        .and_then(|password| sync_folders(store, &account, &password));
    lock(store)?
        .record_sync(account_id, result.as_ref().err().map(String::as_str))
        .map_err(|e| e.to_string())?;
    result
}

fn sync_folders(
    store: &Mutex<Store>,
    account: &MailAccount,
    password: &str,
) -> Result<ScanSummary, String> {
    // Gmail's All Mail includes what the user sent; their own messages
    // about a manuscript aren't news from the journal.
    let username = account.settings.username.to_lowercase();
    let rules = CandidateRules {
        own_addresses: username
            .contains('@')
            .then_some(username)
            .into_iter()
            .collect(),
        ..CandidateRules::default()
    };
    let mut summary = ScanSummary::default();
    for folder in &account.settings.folders {
        let mut provider = ImapProvider::new(
            account.provider,
            &account.alias,
            account.settings.clone(),
            password,
            folder,
        );
        let saved = lock(store)?
            .sync_cursor(account.id, folder)
            .map_err(|e| e.to_string())?;
        let mut cursor: Option<SyncCursor> =
            saved.and_then(|json| serde_json::from_str(&json).ok());
        let mut rescanned = false;
        loop {
            let batch = match provider.fetch_changes(cursor.as_ref()) {
                Ok(batch) => batch,
                // The server renumbered the folder: scan the backfill window
                // again. Already-recorded messages are skipped as duplicates.
                Err(ProviderError::CursorExpired) if !rescanned => {
                    rescanned = true;
                    cursor = None;
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            let cursor_json =
                serde_json::to_string(&batch.next_cursor).map_err(|e| e.to_string())?;
            let mut store = lock(store)?;
            record_batch(
                &mut store,
                account.id,
                &batch.messages,
                &rules,
                &mut summary,
            )
            .map_err(|e| e.to_string())?;
            store
                .save_sync_cursor(account.id, folder, &cursor_json)
                .map_err(|e| e.to_string())?;
            drop(store);
            if !batch.more {
                break;
            }
            cursor = Some(batch.next_cursor);
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, all_mail: bool) -> Folder {
        Folder {
            name: name.into(),
            all_mail,
            selectable: true,
        }
    }

    #[test]
    fn prefers_all_mail_then_inbox() {
        let gmail = [
            folder("INBOX", false),
            folder("[Gmail]/Alle Nachrichten", true),
        ];
        assert_eq!(
            choose_folders(&[], &gmail).unwrap(),
            ["[Gmail]/Alle Nachrichten"]
        );
        assert_eq!(
            choose_folders(&[], &[folder("INBOX", false)]).unwrap(),
            ["INBOX"]
        );
    }

    #[test]
    fn rejects_unknown_folders() {
        let available = [folder("INBOX", false)];
        assert!(choose_folders(&["Archive".into()], &available).is_err());
        assert_eq!(
            choose_folders(&["INBOX".into()], &available).unwrap(),
            ["INBOX"]
        );
    }
}
