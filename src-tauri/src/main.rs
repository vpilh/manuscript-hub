// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod keychain;

use std::sync::Mutex;

use mh_connectors::{mock::MockProvider, MailProvider};
use mh_core::Provider;
use mh_parser::{extract, CandidateRules, PARSER_VERSION};
use mh_store::{ManuscriptSummary, QueueItem, RecordOutcome, ReviewDecision, Store, TimelineEntry};
use serde::Serialize;
use tauri::{Manager, State};

/// Synthetic mailbox compiled into the binary, so a fresh build runs without
/// any mailbox credentials (FR22).
const DEMO_MAILBOX: &[&str] = &[
    include_str!("../../fixtures/synthetic/jss-revision-requested.eml"),
    include_str!("../../fixtures/synthetic/jss-submitted.eml"),
    include_str!("../../fixtures/synthetic/zss-accepted-de.eml"),
    include_str!("../../fixtures/synthetic/multiple-manuscripts.eml"),
    include_str!("../../fixtures/synthetic/reply-quoting-old-decision.eml"),
    include_str!("../../fixtures/synthetic/publisher-newsletter.eml"),
];
const DEMO_ACCOUNT: &str = "Demo mailbox";
const DATABASE_FILE: &str = "manuscript-hub.db";

/// The open database, or why it could not be opened (shown in the UI).
struct AppState(Result<Mutex<Store>, String>);

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
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    parser_version: &'static str,
    storage_error: Option<String>,
}

#[derive(Serialize)]
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
    let rules = CandidateRules::default();
    state.with_store(|store| {
        let account = store.ensure_account(Provider::Mock, DEMO_ACCOUNT)?;
        let mut summary = ScanSummary {
            messages: 0,
            proposed: 0,
            already_seen: 0,
            not_journal_mail: 0,
        };
        let mut cursor = None;
        loop {
            let batch = provider
                .fetch_changes(cursor.as_ref())
                .map_err(|e| mh_store::StoreError::Format(e.to_string()))?;
            for message in &batch.messages {
                summary.messages += 1;
                match extract(message, &rules) {
                    None => summary.not_journal_mail += 1,
                    Some(proposal) => match store.record_proposal(account, &proposal)? {
                        RecordOutcome::Proposed { .. } => summary.proposed += 1,
                        RecordOutcome::Duplicate => summary.already_seen += 1,
                    },
                }
            }
            if !batch.more {
                return Ok(summary);
            }
            cursor = Some(batch.next_cursor);
        }
    })
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
        .setup(|app| {
            let store = open_store(app).map(Mutex::new);
            app.manage(AppState(store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            scan_demo_mailbox,
            review_queue,
            review_event,
            manuscripts,
            timeline
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Manuscript Hub");
}
