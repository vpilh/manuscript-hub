// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod keychain;
mod mail;
mod notify;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mh_connectors::pasted::{parse_pasted, PASTED_ACCOUNT};
use mh_connectors::{mock::MockProvider, MailProvider};
use mh_core::{Provider, TransientMessage};
use mh_parser::{extract, CandidateRules, PARSER_VERSION};
use mh_store::{
    EventEdits, ManualTarget, ManualUpdate, ManuscriptSummary, QueueItem, RecordOutcome,
    ReviewDecision, SenderRule, SenderRuleKind, Store, StoreError, TimelineEntry,
};
use serde::Serialize;
use tauri::{Manager, State, WindowEvent};

/// Synthetic mailbox compiled into the binary, so a fresh build runs without
/// any mailbox credentials (FR22).
const DEMO_MAILBOX: &[&str] = &[
    include_str!("../../fixtures/synthetic/jss-revision-requested.eml"),
    include_str!("../../fixtures/synthetic/jss-submitted.eml"),
    include_str!("../../fixtures/synthetic/zss-accepted-de.eml"),
    include_str!("../../fixtures/synthetic/multiple-manuscripts.eml"),
    include_str!("../../fixtures/synthetic/reply-quoting-old-decision.eml"),
    include_str!("../../fixtures/synthetic/publisher-newsletter.eml"),
    include_str!("../../fixtures/synthetic/em-submission-confirmation.eml"),
    include_str!("../../fixtures/synthetic/em-pdf-approval.eml"),
    include_str!("../../fixtures/synthetic/em-revision-received.eml"),
    include_str!("../../fixtures/synthetic/scholarone-minor-revision.eml"),
    include_str!("../../fixtures/synthetic/tracker-under-review.eml"),
    include_str!("../../fixtures/synthetic/annals-accepted.eml"),
    include_str!("../../fixtures/synthetic/redaktion-abgelehnt-de.eml"),
    include_str!("../../fixtures/synthetic/shop-order.eml"),
    include_str!("../../fixtures/synthetic/ticket-review-request.eml"),
];
const DEMO_ACCOUNT: &str = "Demo mailbox";
const DATABASE_FILE: &str = "manuscript-hub.db";

/// The "still running in the tray" notice is shown once per run.
static CLOSE_HINT_SHOWN: AtomicBool = AtomicBool::new(false);

/// The open database, or why it could not be opened (shown in the UI).
struct AppState(Result<Arc<Mutex<Store>>, String>);

impl AppState {
    fn with_store<T>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, mh_store::StoreError>,
    ) -> Result<T, String> {
        let store = self.0.as_ref().map_err(Clone::clone)?;
        let mut store = store
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;
        f(&mut store).map_err(|e| e.to_string())
    }

    /// A handle that outlives the command, for work on a blocking thread.
    fn handle(&self) -> Result<Arc<Mutex<Store>>, String> {
        self.0.clone()
    }
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    parser_version: &'static str,
    storage_error: Option<String>,
}

#[derive(Clone, Default, Serialize)]
struct ScanSummary {
    messages: usize,
    proposed: usize,
    already_seen: usize,
    not_journal_mail: usize,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        parser_version: PARSER_VERSION,
        storage_error: state.0.as_ref().err().cloned(),
    }
}

/// Parses the bundled synthetic mailbox into the review queue. No network access.
#[tauri::command]
fn scan_demo_mailbox(state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let mut provider = MockProvider::from_eml(DEMO_ACCOUNT, DEMO_MAILBOX);
    state.with_store(|store| {
        let rules = candidate_rules(store)?;
        let account = store.ensure_account(Provider::Mock, DEMO_ACCOUNT)?;
        let mut summary = ScanSummary::default();
        let mut cursor = None;
        loop {
            let batch = provider
                .fetch_changes(cursor.as_ref())
                .map_err(|e| StoreError::Format(e.to_string()))?;
            record_batch(store, account, &batch.messages, &rules, &mut summary)?;
            if !batch.more {
                return Ok(summary);
            }
            cursor = Some(batch.next_cursor);
        }
    })
}

#[tauri::command]
fn add_manual_update(
    state: State<'_, AppState>,
    target: ManualTarget,
    update: ManualUpdate,
) -> Result<i64, String> {
    state.with_store(|store| store.add_manual_update(target, update))
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum PasteOutcome {
    /// Added to the review queue.
    Proposed,
    /// The same message was pasted or synced before.
    Duplicate,
    /// No manuscript ID or editorial status in the text.
    NothingFound,
}

/// Larger pastes are almost certainly not a single letter.
const MAX_PASTE_BYTES: usize = 1024 * 1024;

/// Reads a pasted message into the review queue. The text is parsed in
/// memory and dropped; only the proposal is stored.
#[tauri::command]
fn import_pasted_mail(state: State<'_, AppState>, text: String) -> Result<PasteOutcome, String> {
    if text.trim().is_empty() {
        return Err("Paste the text of a message first.".into());
    }
    if text.len() > MAX_PASTE_BYTES {
        return Err("That's more than one message. Paste a single letter.".into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let message = parse_pasted(&text, now);
    drop(text);
    state.with_store(|store| {
        let rules = CandidateRules {
            assume_journal_mail: true,
            ..candidate_rules(store)?
        };
        let Some(proposal) = extract(&message, &rules) else {
            return Ok(PasteOutcome::NothingFound);
        };
        let account = store.ensure_account(Provider::Mock, PASTED_ACCOUNT)?;
        Ok(match store.record_proposal(account, &proposal)? {
            RecordOutcome::Proposed { .. } => PasteOutcome::Proposed,
            RecordOutcome::Duplicate => PasteOutcome::Duplicate,
        })
    })
}

/// The built-in candidate rules plus the user's sender rules.
fn candidate_rules(store: &Store) -> Result<CandidateRules, StoreError> {
    let mut rules = CandidateRules::default();
    for rule in store.sender_rules()? {
        match rule.kind {
            SenderRuleKind::Journal => rules.trusted_domains.push(rule.domain),
            SenderRuleKind::Ignore => rules.blocked_domains.push(rule.domain),
        }
    }
    Ok(rules)
}

#[tauri::command]
fn sender_rules(state: State<'_, AppState>) -> Result<Vec<SenderRule>, String> {
    state.with_store(|store| store.sender_rules())
}

/// Returns how many waiting proposals an "ignore" rule set aside.
#[tauri::command]
fn set_sender_rule(
    state: State<'_, AppState>,
    domain: String,
    kind: SenderRuleKind,
) -> Result<usize, String> {
    state.with_store(|store| store.set_sender_rule(&domain, kind))
}

#[tauri::command]
fn remove_sender_rule(state: State<'_, AppState>, domain: String) -> Result<(), String> {
    state.with_store(|store| store.remove_sender_rule(&domain))
}

/// Parses messages into the review queue; bodies are dropped with `messages`.
fn record_batch(
    store: &mut Store,
    account: i64,
    messages: &[TransientMessage],
    rules: &CandidateRules,
    summary: &mut ScanSummary,
) -> Result<(), StoreError> {
    for message in messages {
        summary.messages += 1;
        match extract(message, rules) {
            None => summary.not_journal_mail += 1,
            Some(proposal) => match store.record_proposal(account, &proposal)? {
                RecordOutcome::Proposed { .. } => summary.proposed += 1,
                RecordOutcome::Duplicate => summary.already_seen += 1,
            },
        }
    }
    Ok(())
}

#[tauri::command]
fn review_queue(state: State<'_, AppState>) -> Result<Vec<QueueItem>, String> {
    state.with_store(|store| store.review_queue())
}

#[tauri::command]
fn review_event(
    state: State<'_, AppState>,
    event_id: i64,
    decision: ReviewDecision,
) -> Result<(), String> {
    state.with_store(|store| store.review(event_id, decision))
}

#[tauri::command]
fn manuscripts(state: State<'_, AppState>) -> Result<Vec<ManuscriptSummary>, String> {
    state.with_store(|store| store.manuscripts())
}

#[tauri::command]
fn timeline(state: State<'_, AppState>, manuscript_id: i64) -> Result<Vec<TimelineEntry>, String> {
    state.with_store(|store| store.timeline(manuscript_id))
}

#[tauri::command]
fn amend_event(state: State<'_, AppState>, event_id: i64, edits: EventEdits) -> Result<(), String> {
    state.with_store(|store| store.amend_event(event_id, edits))
}

#[tauri::command]
fn remove_event(state: State<'_, AppState>, event_id: i64) -> Result<(), String> {
    state.with_store(|store| store.remove_event(event_id))
}

#[tauri::command]
fn merge_manuscripts(state: State<'_, AppState>, from_id: i64, into_id: i64) -> Result<(), String> {
    state.with_store(|store| store.merge_manuscripts(from_id, into_id))
}

fn open_store(app: &tauri::App) -> Result<Store, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No application data directory: {e}"))?;
    let path = dir.join(DATABASE_FILE);
    let key = keychain::database_key(path.exists())?;
    Store::open(path, key).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let store = open_store(app).map(|store| Arc::new(Mutex::new(store)));
            app.manage(AppState(store));
            app.manage(mail::Checks::default());
            mail::start_background_checks(app.handle().clone());
            // Without a tray (some Linux desktops), closing the window quits.
            if let Err(e) = tray::create(app.handle()) {
                eprintln!("tray icon unavailable: {e}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // With a tray icon, closing the window keeps checks running.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.try_state::<tray::PauseItem>().is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                    if !CLOSE_HINT_SHOWN.swap(true, Ordering::Relaxed) {
                        notify::still_running(app);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            scan_demo_mailbox,
            review_queue,
            review_event,
            manuscripts,
            timeline,
            merge_manuscripts,
            mail::mail_accounts,
            mail::connect_mail_account,
            mail::disconnect_mail_account,
            mail::sync_mail_account,
            mail::set_check_interval,
            mail::checks_paused,
            mail::set_checks_paused,
            amend_event,
            remove_event,
            sender_rules,
            add_manual_update,
            import_pasted_mail,
            set_sender_rule,
            remove_sender_rule
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Manuscript Hub");
}
