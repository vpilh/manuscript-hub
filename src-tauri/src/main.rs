// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mh_connectors::{mock::MockProvider, MailProvider};
use mh_core::ProposedEvent;
use mh_parser::{extract, CandidateRules, PARSER_VERSION};
use serde::Serialize;

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

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    parser_version: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        parser_version: PARSER_VERSION,
    }
}

/// Parses the bundled synthetic mailbox. Makes no network requests.
#[tauri::command]
fn scan_demo_mailbox() -> Result<Vec<ProposedEvent>, String> {
    let mut provider = MockProvider::from_eml("Demo mailbox", DEMO_MAILBOX);
    let rules = CandidateRules::default();
    let mut proposals = Vec::new();
    let mut cursor = None;
    loop {
        let batch = provider
            .fetch_changes(cursor.as_ref())
            .map_err(|e| e.to_string())?;
        proposals.extend(batch.messages.iter().filter_map(|m| extract(m, &rules)));
        if !batch.more {
            return Ok(proposals);
        }
        cursor = Some(batch.next_cursor);
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info, scan_demo_mailbox])
        .run(tauri::generate_context!())
        .expect("failed to start Manuscript Hub");
}
