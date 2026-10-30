//! Updates entered by hand (FR17): for journals that write from addresses
//! the app doesn't read, or news that came by phone or on paper.

use mh_core::{CalendarDate, EditorialStatus};
use rusqlite::params;
use serde::Deserialize;

use crate::review::{create_manuscript, ensure_manuscript_exists};
use crate::{store::now, Store, StoreError};

/// Which manuscript a manual update belongs to.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManualTarget {
    Existing {
        manuscript_id: i64,
    },
    New {
        journal: String,
        external_id: Option<String>,
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManualUpdate {
    /// The day it happened, e.g. the date on the decision letter.
    pub occurred_on: CalendarDate,
    pub status: Option<EditorialStatus>,
    pub deadline: Option<CalendarDate>,
    pub requested_action: Option<String>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Store {
    /// Adds a confirmed update typed in by the user and returns the
    /// manuscript it belongs to. Like every confirmed update, it takes part
    /// in deriving the current status by date.
    pub fn add_manual_update(
        &mut self,
        target: ManualTarget,
        update: ManualUpdate,
    ) -> Result<i64, StoreError> {
        let action = non_empty(update.requested_action);
        if update.status.is_none() && update.deadline.is_none() && action.is_none() {
            return Err(StoreError::Format(
                "Enter a status, a deadline or something to do.".into(),
            ));
        }
        self.write(|tx| {
            let manuscript_id = match target {
                ManualTarget::Existing { manuscript_id } => {
                    ensure_manuscript_exists(tx, manuscript_id)?;
                    manuscript_id
                }
                ManualTarget::New {
                    journal,
                    external_id,
                    title,
                } => {
                    let journal = journal.trim();
                    if journal.is_empty() {
                        return Err(StoreError::Format("Enter the journal's name.".into()));
                    }
                    create_manuscript(
                        tx,
                        Some(journal),
                        non_empty(external_id).as_deref(),
                        non_empty(title).as_deref(),
                    )?
                }
            };
            let (journal, external_id, title): (String, Option<String>, Option<String>) = tx
                .query_row(
                    "SELECT journal, external_id, title FROM manuscripts WHERE id = ?1",
                    [manuscript_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
            // Noon UTC keeps the calendar day in every time zone near Europe.
            let occurred_at = update.occurred_on.days_since_epoch() * 86_400 + 12 * 3_600;
            tx.execute(
                "INSERT INTO events (source_message_id, manuscript_id, match_kind, journal,
                     external_id, title, status, deadline, requested_action, extraction_json,
                     review_state, occurred_at, reviewed_at)
                 VALUES (NULL, ?1, 'manual', ?2, ?3, ?4, ?5, ?6, ?7, '{\"manual\":true}',
                     'confirmed', ?8, ?9)",
                params![
                    manuscript_id,
                    journal,
                    external_id,
                    title,
                    update.status.map(EditorialStatus::as_str),
                    update.deadline.map(|d| d.to_string()),
                    action,
                    occurred_at,
                    now(),
                ],
            )?;
            Ok(manuscript_id)
        })
    }
}
