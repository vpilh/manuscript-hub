//! Export and re-import (FR16): a CSV overview for spreadsheets and a
//! complete JSON export that round-trips through [`Store::import`].
//!
//! Exports hold what the user confirmed: manuscripts, their confirmed or
//! corrected updates with evidence phrases and message references, and
//! sender rules. Proposals still waiting for review, mail content and
//! credentials are never exported.

use mh_core::{CalendarDate, EditorialStatus, Provider, ReviewState};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::review::create_manuscript;
use crate::{store::now, SenderRule, Store, StoreError};

pub const EXPORT_FORMAT: &str = "manuscript-hub-export";
pub const EXPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub version: u32,
    pub exported_at: i64,
    pub manuscripts: Vec<ExportedManuscript>,
    pub sender_rules: Vec<SenderRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedManuscript {
    pub journal: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub updates: Vec<ExportedUpdate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportedUpdate {
    /// Unix seconds.
    pub occurred_at: i64,
    /// `confirmed` or `edited`.
    pub review_state: ReviewState,
    pub status: Option<EditorialStatus>,
    pub deadline: Option<CalendarDate>,
    pub requested_action: Option<String>,
    pub original_phrase: Option<String>,
    pub source: Option<ExportedSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportedSource {
    pub provider: Provider,
    pub account_alias: String,
    pub folder: String,
    pub message_id: String,
    pub received_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportSummary {
    pub manuscripts_added: usize,
    pub updates_added: usize,
    /// Updates already present, e.g. when the same file is imported twice.
    pub updates_skipped: usize,
}

impl Store {
    pub fn export(&self) -> Result<Export, StoreError> {
        let mut manuscripts = Vec::new();
        for summary in self.manuscripts()? {
            let mut updates = Vec::new();
            for entry in self.timeline(summary.id)? {
                if !matches!(
                    entry.review_state,
                    ReviewState::Confirmed | ReviewState::Edited
                ) {
                    continue;
                }
                updates.push(ExportedUpdate {
                    occurred_at: entry.occurred_at,
                    review_state: entry.review_state,
                    status: entry.status,
                    deadline: entry.deadline,
                    requested_action: entry.requested_action,
                    original_phrase: entry.original_phrase,
                    source: entry.source.map(|s| ExportedSource {
                        provider: s.provider,
                        account_alias: s.account_alias,
                        folder: s.folder,
                        message_id: s.message_id,
                        received_at: s.received_at,
                    }),
                });
            }
            // Oldest first reads naturally in a file.
            updates.reverse();
            manuscripts.push(ExportedManuscript {
                journal: summary.journal,
                external_id: summary.external_id,
                title: summary.title,
                updates,
            });
        }
        Ok(Export {
            format: EXPORT_FORMAT.to_string(),
            version: EXPORT_VERSION,
            exported_at: now(),
            manuscripts,
            sender_rules: self.sender_rules()?,
        })
    }

    /// One row per manuscript with its current state, for spreadsheets.
    pub fn export_csv(&self) -> Result<String, StoreError> {
        let mut csv = String::from(
            "Journal,Manuscript ID,Title,Status,Status since,Deadline,To do,Updates waiting for review\r\n",
        );
        for m in self.manuscripts()? {
            let since = m
                .status_since
                .map(|s| CalendarDate::from_unix_seconds(s).to_string());
            let row = [
                Some(m.journal),
                m.external_id,
                m.title,
                m.status.map(|s| s.as_str().to_string()),
                since,
                m.deadline.map(|d| d.to_string()),
                m.requested_action,
                Some(m.pending_reviews.to_string()),
            ];
            let cells: Vec<String> = row
                .into_iter()
                .map(|cell| csv_cell(&cell.unwrap_or_default()))
                .collect();
            csv.push_str(&cells.join(","));
            csv.push_str("\r\n");
        }
        Ok(csv)
    }

    /// Adds what an export holds. Manuscripts are matched by journal and ID
    /// (or journal and title when there is no ID);
    /// updates already present are skipped, so importing twice changes
    /// nothing. Message references are restored for accounts that exist
    /// here, so a later mailbox check doesn't propose those messages again.
    pub fn import(&mut self, export: &Export) -> Result<ImportSummary, StoreError> {
        if export.format != EXPORT_FORMAT {
            return Err(StoreError::Format(
                "This file isn't a Manuscript Hub export.".into(),
            ));
        }
        if export.version > EXPORT_VERSION {
            return Err(StoreError::Format(
                "This export was made by a newer version of Manuscript Hub. Update the app first."
                    .into(),
            ));
        }
        for rule in &export.sender_rules {
            self.set_sender_rule(&rule.domain, rule.kind)?;
        }
        self.write(|tx| {
            let mut summary = ImportSummary::default();
            for manuscript in &export.manuscripts {
                let existing: Option<i64> = match &manuscript.external_id {
                    Some(id) => tx
                        .query_row(
                            "SELECT id FROM manuscripts WHERE journal = ?1 AND external_id = ?2",
                            params![manuscript.journal, id],
                            |r| r.get(0),
                        )
                        .optional()?,
                    // Without an ID, the same journal and title identify it.
                    None => match &manuscript.title {
                        Some(title) => tx
                            .query_row(
                                "SELECT id FROM manuscripts
                                 WHERE journal = ?1 AND external_id IS NULL AND title = ?2",
                                params![manuscript.journal, title],
                                |r| r.get(0),
                            )
                            .optional()?,
                        None => None,
                    },
                };
                let manuscript_id = match existing {
                    Some(id) => id,
                    None => {
                        summary.manuscripts_added += 1;
                        create_manuscript(
                            tx,
                            Some(&manuscript.journal),
                            manuscript.external_id.as_deref(),
                            manuscript.title.as_deref(),
                        )?
                    }
                };
                for update in &manuscript.updates {
                    let status = update.status.map(EditorialStatus::as_str);
                    let deadline = update.deadline.map(|d| d.to_string());
                    let present: bool = tx.query_row(
                        "SELECT EXISTS (SELECT 1 FROM events
                         WHERE manuscript_id = ?1 AND occurred_at = ?2
                           AND review_state IN ('confirmed', 'edited')
                           AND status IS ?3 AND deadline IS ?4)",
                        params![manuscript_id, update.occurred_at, status, deadline],
                        |r| r.get(0),
                    )?;
                    if present {
                        summary.updates_skipped += 1;
                        continue;
                    }
                    let source_id = match &update.source {
                        Some(source) => restore_source(tx, source)?,
                        None => None,
                    };
                    let state = if update.review_state == ReviewState::Edited {
                        "edited"
                    } else {
                        "confirmed"
                    };
                    tx.execute(
                        "INSERT INTO events (source_message_id, manuscript_id, match_kind,
                             journal, external_id, title, status, original_phrase, deadline,
                             requested_action, extraction_json, review_state, occurred_at,
                             reviewed_at)
                         VALUES (?1, ?2, 'manual', ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                             '{\"imported\":true}', ?10, ?11, ?12)",
                        params![
                            source_id,
                            manuscript_id,
                            manuscript.journal,
                            manuscript.external_id,
                            manuscript.title,
                            status,
                            update.original_phrase,
                            deadline,
                            update.requested_action,
                            state,
                            update.occurred_at,
                            now(),
                        ],
                    )?;
                    summary.updates_added += 1;
                }
            }
            Ok(summary)
        })
    }

    /// Removes every manuscript, update, account, rule and reminder. The
    /// database key stays, so the empty database remains usable.
    pub fn wipe(&mut self) -> Result<(), StoreError> {
        self.write(|tx| {
            tx.execute_batch(
                "DELETE FROM reminders_sent;
                 DELETE FROM events;
                 DELETE FROM source_messages;
                 DELETE FROM sync_cursors;
                 DELETE FROM manuscripts;
                 DELETE FROM accounts;
                 DELETE FROM sender_rules;",
            )?;
            Ok(())
        })
    }
}

/// Links an imported update to its message when that account exists here.
fn restore_source(
    tx: &rusqlite::Transaction<'_>,
    source: &ExportedSource,
) -> Result<Option<i64>, StoreError> {
    let account: Option<i64> = tx
        .query_row(
            "SELECT id FROM accounts WHERE provider = ?1 AND alias = ?2",
            params![source.provider.as_str(), source.account_alias],
            |r| r.get(0),
        )
        .optional()?;
    let Some(account) = account else {
        return Ok(None);
    };
    tx.execute(
        "INSERT OR IGNORE INTO source_messages
             (account_id, message_id, folder, received_at, parser_version)
         VALUES (?1, ?2, ?3, ?4, 'import')",
        params![
            account,
            source.message_id,
            source.folder,
            source.received_at
        ],
    )?;
    Ok(Some(tx.query_row(
        "SELECT id FROM source_messages WHERE account_id = ?1 AND message_id = ?2",
        params![account, source.message_id],
        |r| r.get(0),
    )?))
}

/// Quotes a CSV cell when needed, and defuses spreadsheet formulas: a cell
/// starting with `=`, `+`, `-` or `@` came from mail and must stay text.
fn csv_cell(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cells_are_quoted_and_formulas_defused() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a, b"), "\"a, b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("-1"), "'-1");
    }
}
