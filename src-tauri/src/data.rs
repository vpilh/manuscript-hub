//! The user's data (WP 1.8, FR16): CSV and JSON export, password-protected
//! backups, import and restore, and deleting everything on this computer.
//!
//! File dialogs block, so every command runs on a blocking thread. Paths
//! come only from the dialogs the user answered; nothing is written
//! anywhere else.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use mh_core::CalendarDate;
use mh_store::{open_backup, seal_backup, Export, ImportSummary, Store};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{keychain, AppState};

const BACKUP_EXTENSION: &str = "mhbackup";

fn today() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    CalendarDate::from_unix_seconds(now).to_string()
}

fn ask_save_path(app: &AppHandle, name: &str, kind: &str, extension: &str) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_file_name(format!("{name}-{}.{extension}", today()))
        .add_filter(kind, &[extension])
        .blocking_save_file()
        .and_then(|path| path.into_path().ok())
}

fn ask_open_path(app: &AppHandle, kind: &str, extension: &str) -> Option<PathBuf> {
    app.dialog()
        .file()
        .add_filter(kind, &[extension])
        .blocking_pick_file()
        .and_then(|path| path.into_path().ok())
}

fn with_store<T>(
    store: &Mutex<Store>,
    f: impl FnOnce(&mut Store) -> Result<T, mh_store::StoreError>,
) -> Result<T, String> {
    let mut store = store
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;
    f(&mut store).map_err(|e| e.to_string())
}

fn write_file(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("The file couldn't be saved: {e}"))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Where a file went; `None` when the user closed the dialog.
#[derive(Serialize)]
pub struct Saved {
    path: Option<String>,
}

fn saved(path: Option<PathBuf>) -> Saved {
    Saved {
        path: path.map(|p| p.display().to_string()),
    }
}

fn handle(state: &State<'_, AppState>) -> Result<Arc<Mutex<Store>>, String> {
    state.handle()
}

#[tauri::command]
pub async fn export_csv(app: AppHandle, state: State<'_, AppState>) -> Result<Saved, String> {
    let store = handle(&state)?;
    blocking(move || {
        let csv = with_store(&store, |s| s.export_csv())?;
        let Some(path) = ask_save_path(&app, "manuscripts", "Spreadsheet (CSV)", "csv") else {
            return Ok(saved(None));
        };
        // A byte-order mark makes Excel read the UTF-8 umlauts correctly.
        let mut bytes = "\u{FEFF}".as_bytes().to_vec();
        bytes.extend_from_slice(csv.as_bytes());
        write_file(&path, &bytes)?;
        Ok(saved(Some(path)))
    })
    .await
}

#[tauri::command]
pub async fn export_json(app: AppHandle, state: State<'_, AppState>) -> Result<Saved, String> {
    let store = handle(&state)?;
    blocking(move || {
        let export = with_store(&store, |s| s.export())?;
        let json = serde_json::to_vec_pretty(&export).map_err(|e| e.to_string())?;
        let Some(path) = ask_save_path(&app, "manuscript-hub-export", "Export (JSON)", "json")
        else {
            return Ok(saved(None));
        };
        write_file(&path, &json)?;
        Ok(saved(Some(path)))
    })
    .await
}

#[tauri::command]
pub async fn create_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
) -> Result<Saved, String> {
    let store = handle(&state)?;
    blocking(move || {
        let export = with_store(&store, |s| s.export())?;
        // Seal before asking for a path, so a weak password fails right away.
        let file = seal_backup(&export, &password).map_err(|e| e.to_string())?;
        drop(password);
        let Some(path) = ask_save_path(
            &app,
            "manuscript-hub-backup",
            "Manuscript Hub backup",
            BACKUP_EXTENSION,
        ) else {
            return Ok(saved(None));
        };
        write_file(&path, &file)?;
        Ok(saved(Some(path)))
    })
    .await
}

/// `None` when the user closed the dialog.
#[derive(Serialize)]
pub struct Imported {
    summary: Option<ImportSummary>,
}

#[tauri::command]
pub async fn import_json(app: AppHandle, state: State<'_, AppState>) -> Result<Imported, String> {
    let store = handle(&state)?;
    blocking(move || {
        let Some(path) = ask_open_path(&app, "Export (JSON)", "json") else {
            return Ok(Imported { summary: None });
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("The file couldn't be read: {e}"))?;
        let export: Export = serde_json::from_slice(&bytes)
            .map_err(|_| "This file isn't a Manuscript Hub export.".to_string())?;
        let summary = with_store(&store, |s| s.import(&export))?;
        Ok(Imported {
            summary: Some(summary),
        })
    })
    .await
}

#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
) -> Result<Imported, String> {
    let store = handle(&state)?;
    blocking(move || {
        let Some(path) = ask_open_path(&app, "Manuscript Hub backup", BACKUP_EXTENSION) else {
            return Ok(Imported { summary: None });
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("The file couldn't be read: {e}"))?;
        let export = open_backup(&bytes, &password).map_err(|e| e.to_string())?;
        drop(password);
        let summary = with_store(&store, |s| s.import(&export))?;
        Ok(Imported {
            summary: Some(summary),
        })
    })
    .await
}

/// Removes every manuscript, update, account and rule, and the mailbox
/// passwords in the keychain. The database key stays, so the app keeps
/// working with an empty database.
#[tauri::command]
pub fn delete_all_data(state: State<'_, AppState>) -> Result<(), String> {
    state.with_store(|store| {
        for account in store.accounts()? {
            // Accounts without a password (the demo mailbox) are fine.
            if let Err(e) = keychain::delete_mail_password(account.id) {
                return Err(mh_store::StoreError::Format(e));
            }
        }
        store.wipe()
    })
}
