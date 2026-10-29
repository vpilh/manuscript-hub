//! Matching proposals to manuscripts and the review queue (WP 1.5, FR10, FR11, FR13).
//!
//! The current status and deadline of a manuscript are *derived* from its newest
//! confirmed events by message time; nothing is overwritten in place. Confirming
//! an older message therefore adds it to the timeline without changing what is
//! current, and a quoted or stale date can never supersede a newer confirmed one.

use mh_core::{CalendarDate, EditorialStatus, ProposedEvent, Provider, ReviewState, SourceRef};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    matching::{title_similarity, TITLE_MATCH_THRESHOLD},
    store::now,
    Store, StoreError,
};

const UNKNOWN_JOURNAL: &str = "Unknown journal";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Same manuscript ID at the same journal.
    ExactId,
    /// Same manuscript ID; the journal name differs or is unknown.
    IdOnly,
    Title,
    /// Several manuscripts share the ID; the user must choose.
    Ambiguous,
    None,
    /// The user linked the event themselves.
    Manual,
}

impl MatchKind {
    fn as_str(self) -> &'static str {
        match self {
            MatchKind::ExactId => "exact_id",
            MatchKind::IdOnly => "id_only",
            MatchKind::Title => "title",
            MatchKind::Ambiguous => "ambiguous",
            MatchKind::None => "none",
            MatchKind::Manual => "manual",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "exact_id" => MatchKind::ExactId,
            "id_only" => MatchKind::IdOnly,
            "title" => MatchKind::Title,
            "ambiguous" => MatchKind::Ambiguous,
            "manual" => MatchKind::Manual,
            _ => MatchKind::None,
        }
    }
}

fn review_state_str(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Proposed => "proposed",
        ReviewState::Confirmed => "confirmed",
        ReviewState::Edited => "edited",
        ReviewState::Ignored => "ignored",
        ReviewState::Unrelated => "unrelated",
    }
}

fn parse_review_state(value: &str) -> ReviewState {
    match value {
        "confirmed" => ReviewState::Confirmed,
        "edited" => ReviewState::Edited,
        "ignored" => ReviewState::Ignored,
        "unrelated" => ReviewState::Unrelated,
        _ => ReviewState::Proposed,
    }
}

fn parse_status(value: Option<String>) -> Option<EditorialStatus> {
    value.as_deref().and_then(EditorialStatus::parse)
}

fn parse_date(value: Option<String>) -> Option<CalendarDate> {
    value.and_then(|s| s.parse().ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    Proposed {
        event_id: i64,
    },
    /// This message was processed before (FR10).
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManuscriptRef {
    pub id: i64,
    pub journal: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueItem {
    pub event_id: i64,
    pub proposal: ProposedEvent,
    pub match_kind: MatchKind,
    pub match_rationale: String,
    pub suggested_manuscript: Option<ManuscriptRef>,
}

/// The researcher's decision on a proposed event (PRD journey C).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewDecision {
    /// Link to `manuscript_id`, or to the suggestion, or to a new manuscript
    /// when there is none. With `edits`, the corrected values are saved instead.
    Confirm {
        #[serde(default)]
        manuscript_id: Option<i64>,
        #[serde(default)]
        edits: Option<EventEdits>,
    },
    Ignore,
    Unrelated,
}

/// Replacement values for an edited event; `None` clears the field.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct EventEdits {
    pub status: Option<EditorialStatus>,
    pub deadline: Option<CalendarDate>,
    pub requested_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManuscriptSummary {
    pub id: i64,
    pub journal: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub status: Option<EditorialStatus>,
    pub status_since: Option<i64>,
    pub deadline: Option<CalendarDate>,
    pub requested_action: Option<String>,
    pub pending_reviews: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEntry {
    pub event_id: i64,
    pub occurred_at: i64,
    pub review_state: ReviewState,
    pub status: Option<EditorialStatus>,
    pub original_phrase: Option<String>,
    pub deadline: Option<CalendarDate>,
    pub requested_action: Option<String>,
    pub source: Option<SourceRef>,
}

/// Stored in `events.extraction_json` for provenance.
#[derive(Serialize, Deserialize)]
struct StoredExtraction {
    proposal: ProposedEvent,
    match_rationale: String,
}

struct Match {
    kind: MatchKind,
    manuscript_id: Option<i64>,
    rationale: String,
}

impl Store {
    pub fn ensure_account(&mut self, provider: Provider, alias: &str) -> Result<i64, StoreError> {
        self.write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO accounts (provider, alias) VALUES (?1, ?2)",
                params![provider.as_str(), alias],
            )?;
            Ok(tx.query_row(
                "SELECT id FROM accounts WHERE provider = ?1 AND alias = ?2",
                params![provider.as_str(), alias],
                |row| row.get(0),
            )?)
        })
    }

    /// Adds a parser proposal to the review queue with a suggested manuscript,
    /// unless the same message was recorded before.
    pub fn record_proposal(
        &mut self,
        account_id: i64,
        proposal: &ProposedEvent,
    ) -> Result<RecordOutcome, StoreError> {
        self.write(|tx| {
            let source = &proposal.source;
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO source_messages
                     (account_id, message_id, folder, received_at, parser_version)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    account_id,
                    source.message_id,
                    source.folder,
                    source.received_at,
                    proposal.parser_version
                ],
            )?;
            if inserted == 0 {
                return Ok(RecordOutcome::Duplicate);
            }
            let source_message_id = tx.last_insert_rowid();

            let journal = proposal.journal.as_ref().map(|f| f.value.as_str());
            let external_id = proposal.manuscript_id.as_ref().map(|f| f.value.as_str());
            let title = proposal.title.as_ref().map(|f| f.value.as_str());
            let found = find_match(tx, journal, external_id, title)?;

            let mut proposal = proposal.clone();
            proposal.review = ReviewState::Proposed;
            if found.kind == MatchKind::Ambiguous {
                proposal
                    .uncertainty
                    .push("Several manuscripts share this ID; choose one.".into());
            }
            if let Some(manuscript_id) = found.manuscript_id {
                proposal.uncertainty.extend(conflicts_with_confirmed(
                    tx,
                    manuscript_id,
                    &proposal,
                )?);
            }

            let status = proposal.status.as_ref();
            let extraction = serde_json::to_string(&StoredExtraction {
                proposal: proposal.clone(),
                match_rationale: found.rationale,
            })?;
            tx.execute(
                "INSERT INTO events (source_message_id, manuscript_id, match_kind, journal,
                     external_id, title, status, original_phrase, deadline, requested_action,
                     extraction_json, review_state, occurred_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'proposed', ?12)",
                params![
                    source_message_id,
                    found.manuscript_id,
                    found.kind.as_str(),
                    journal,
                    external_id,
                    title,
                    status.map(|s| s.value.as_str()),
                    status.map(|s| s.original.as_str()),
                    proposal.deadline.as_ref().map(|d| d.value.to_string()),
                    proposal.requested_action.as_ref().map(|a| a.value.as_str()),
                    extraction,
                    proposal.source.received_at,
                ],
            )?;
            Ok(RecordOutcome::Proposed {
                event_id: tx.last_insert_rowid(),
            })
        })
    }

    /// Applies the researcher's decision. Only proposed events can be reviewed;
    /// reviewed events are final (FR11).
    pub fn review(&mut self, event_id: i64, decision: ReviewDecision) -> Result<(), StoreError> {
        self.write(|tx| {
            let (state, suggested, kind, journal, external_id, title) = tx
                .query_row(
                    "SELECT review_state, manuscript_id, match_kind, journal, external_id, title
                     FROM events WHERE id = ?1",
                    [event_id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<i64>>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                        ))
                    },
                )
                .optional()?
                .ok_or(StoreError::NotFound {
                    entity: "event",
                    id: event_id,
                })?;
            if parse_review_state(&state) != ReviewState::Proposed {
                return Err(StoreError::AlreadyReviewed { event_id });
            }
            let kind = MatchKind::parse(&kind);
            let reviewed_at = now();

            let dismiss = |state: ReviewState| -> Result<(), StoreError> {
                tx.execute(
                    "UPDATE events SET review_state = ?2, reviewed_at = ?3 WHERE id = ?1",
                    params![event_id, review_state_str(state), reviewed_at],
                )?;
                Ok(())
            };
            let (edits, manuscript_choice) = match decision {
                ReviewDecision::Ignore => return dismiss(ReviewState::Ignored),
                ReviewDecision::Unrelated => return dismiss(ReviewState::Unrelated),
                ReviewDecision::Confirm {
                    manuscript_id,
                    edits,
                } => (edits, manuscript_id),
            };

            let (manuscript_id, kind) = match manuscript_choice {
                Some(chosen) => {
                    ensure_manuscript_exists(tx, chosen)?;
                    let kind = if Some(chosen) == suggested && kind != MatchKind::Ambiguous {
                        kind
                    } else {
                        MatchKind::Manual
                    };
                    (chosen, kind)
                }
                None => match suggested {
                    _ if kind == MatchKind::Ambiguous => {
                        return Err(StoreError::NeedsManuscriptChoice { event_id })
                    }
                    Some(id) => (id, kind),
                    None => (
                        create_manuscript(
                            tx,
                            journal.as_deref(),
                            external_id.as_deref(),
                            title.as_deref(),
                        )?,
                        kind,
                    ),
                },
            };

            match edits {
                None => tx.execute(
                    "UPDATE events SET manuscript_id = ?2, match_kind = ?3,
                         review_state = 'confirmed', reviewed_at = ?4
                     WHERE id = ?1",
                    params![event_id, manuscript_id, kind.as_str(), reviewed_at],
                )?,
                Some(edits) => tx.execute(
                    "UPDATE events SET manuscript_id = ?2, match_kind = ?3,
                         review_state = 'edited', reviewed_at = ?4,
                         status = ?5, deadline = ?6, requested_action = ?7
                     WHERE id = ?1",
                    params![
                        event_id,
                        manuscript_id,
                        kind.as_str(),
                        reviewed_at,
                        edits.status.map(EditorialStatus::as_str),
                        edits.deadline.map(|d| d.to_string()),
                        edits.requested_action,
                    ],
                )?,
            };
            Ok(())
        })
    }

    /// Proposed events, newest message first.
    pub fn review_queue(&self) -> Result<Vec<QueueItem>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.extraction_json, e.match_kind,
                    m.id, m.journal, m.external_id, m.title
             FROM events e LEFT JOIN manuscripts m ON m.id = e.manuscript_id
             WHERE e.review_state = 'proposed'
             ORDER BY e.occurred_at DESC, e.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let manuscript = match r.get::<_, Option<i64>>(3)? {
                Some(id) => Some(ManuscriptRef {
                    id,
                    journal: r.get(4)?,
                    external_id: r.get(5)?,
                    title: r.get(6)?,
                }),
                None => None,
            };
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                manuscript,
            ))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (event_id, json, kind, suggested_manuscript) = row?;
            let stored: StoredExtraction = serde_json::from_str(&json)?;
            items.push(QueueItem {
                event_id,
                proposal: stored.proposal,
                match_kind: MatchKind::parse(&kind),
                match_rationale: stored.match_rationale,
                suggested_manuscript,
            });
        }
        Ok(items)
    }

    /// All tracked manuscripts with their current state, most recent activity first.
    pub fn manuscripts(&self) -> Result<Vec<ManuscriptSummary>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.journal, m.external_id, m.title,
                    (SELECT count(*) FROM events e
                     WHERE e.manuscript_id = m.id AND e.review_state = 'proposed'),
                    (SELECT max(e.occurred_at) FROM events e
                     WHERE e.manuscript_id = m.id AND e.review_state IN ('confirmed', 'edited'))
             FROM manuscripts m
             ORDER BY 6 DESC, m.id DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ManuscriptSummary {
                    id: r.get(0)?,
                    journal: r.get(1)?,
                    external_id: r.get(2)?,
                    title: r.get(3)?,
                    pending_reviews: r.get(4)?,
                    status: None,
                    status_since: None,
                    deadline: None,
                    requested_action: None,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        rows.into_iter()
            .map(|mut summary| {
                current_state(&self.conn, &mut summary)?;
                Ok(summary)
            })
            .collect()
    }

    /// A manuscript's events, newest first. Proposed events linked to it are included.
    pub fn timeline(&self, manuscript_id: i64) -> Result<Vec<TimelineEntry>, StoreError> {
        ensure_manuscript_exists(&self.conn, manuscript_id)?;
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.occurred_at, e.review_state, e.status, e.original_phrase,
                    e.deadline, e.requested_action,
                    a.provider, a.alias, s.message_id, s.folder, s.received_at
             FROM events e
             LEFT JOIN source_messages s ON s.id = e.source_message_id
             LEFT JOIN accounts a ON a.id = s.account_id
             WHERE e.manuscript_id = ?1
               AND e.review_state IN ('proposed', 'confirmed', 'edited')
             ORDER BY e.occurred_at DESC, e.id DESC",
        )?;
        let entries = stmt
            .query_map([manuscript_id], |r| {
                let source = match (
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(9)?,
                ) {
                    (Some(provider), Some(message_id)) => {
                        Provider::parse(&provider).map(|provider| SourceRef {
                            provider,
                            account_alias: r.get(8).unwrap_or_default(),
                            message_id,
                            folder: r.get(10).unwrap_or_default(),
                            received_at: r.get(11).unwrap_or_default(),
                        })
                    }
                    _ => None,
                };
                Ok(TimelineEntry {
                    event_id: r.get(0)?,
                    occurred_at: r.get(1)?,
                    review_state: parse_review_state(&r.get::<_, String>(2)?),
                    status: parse_status(r.get(3)?),
                    original_phrase: r.get(4)?,
                    deadline: parse_date(r.get(5)?),
                    requested_action: r.get(6)?,
                    source,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }
}

/// Exact manuscript ID first, then title similarity (FR10).
fn find_match(
    conn: &Connection,
    journal: Option<&str>,
    external_id: Option<&str>,
    title: Option<&str>,
) -> Result<Match, StoreError> {
    if let Some(id) = external_id {
        let mut stmt =
            conn.prepare("SELECT id, journal FROM manuscripts WHERE external_id = ?1")?;
        let rows = stmt
            .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(journal) = journal {
            // IDs issued by different journals are different manuscripts (PRD §6).
            return Ok(
                match rows.iter().find(|(_, j)| j.eq_ignore_ascii_case(journal)) {
                    Some((manuscript_id, _)) => Match {
                        kind: MatchKind::ExactId,
                        manuscript_id: Some(*manuscript_id),
                        rationale: format!("Same manuscript ID {id} at {journal}."),
                    },
                    None if rows.is_empty() => Match {
                        kind: MatchKind::None,
                        manuscript_id: None,
                        rationale: format!(
                            "No tracked manuscript has ID {id}; confirming adds it."
                        ),
                    },
                    None => Match {
                        kind: MatchKind::None,
                        manuscript_id: None,
                        rationale: format!(
                            "ID {id} is tracked at another journal; confirming adds a \
                             separate manuscript at {journal}. Link it manually if it is \
                             the same paper."
                        ),
                    },
                },
            );
        }
        // The sender's journal is unknown, so only the ID can match.
        return Ok(match rows.as_slice() {
            [] => Match {
                kind: MatchKind::None,
                manuscript_id: None,
                rationale: format!("No tracked manuscript has ID {id}; confirming adds it."),
            },
            [(manuscript_id, other_journal)] => Match {
                kind: MatchKind::IdOnly,
                manuscript_id: Some(*manuscript_id),
                rationale: format!(
                    "Same manuscript ID {id} as the manuscript at {other_journal}; \
                     the sender's journal is unknown."
                ),
            },
            _ => Match {
                kind: MatchKind::Ambiguous,
                manuscript_id: None,
                rationale: format!("{} tracked manuscripts use ID {id}.", rows.len()),
            },
        });
    }

    if let Some(title) = title {
        let mut stmt =
            conn.prepare("SELECT id, journal, title FROM manuscripts WHERE title IS NOT NULL")?;
        let best = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(id, journal, known)| (title_similarity(title, &known), id, journal))
            .filter(|(score, _, _)| *score >= TITLE_MATCH_THRESHOLD)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((score, manuscript_id, journal)) = best {
            return Ok(Match {
                kind: MatchKind::Title,
                manuscript_id: Some(manuscript_id),
                rationale: format!(
                    "No manuscript ID; title is {:.0}% similar to a manuscript at {journal}.",
                    score * 100.0
                ),
            });
        }
    }

    Ok(Match {
        kind: MatchKind::None,
        manuscript_id: None,
        rationale: "No manuscript ID or similar title; confirming adds a new manuscript.".into(),
    })
}

/// Explains how a proposal relates to what is already confirmed for the manuscript.
fn conflicts_with_confirmed(
    conn: &Connection,
    manuscript_id: i64,
    proposal: &ProposedEvent,
) -> Result<Vec<String>, StoreError> {
    let mut notes = Vec::new();
    let latest_status_at: Option<i64> = conn
        .query_row(
            "SELECT occurred_at FROM events
             WHERE manuscript_id = ?1 AND review_state IN ('confirmed', 'edited')
               AND status IS NOT NULL
             ORDER BY occurred_at DESC, id DESC LIMIT 1",
            [manuscript_id],
            |r| r.get(0),
        )
        .optional()?;
    if proposal.status.is_some()
        && latest_status_at.is_some_and(|at| at > proposal.source.received_at)
    {
        notes.push(
            "Older than the latest confirmed update; confirming adds it to the timeline \
             without changing the current status."
                .to_string(),
        );
    }

    let confirmed_deadline: Option<String> = conn
        .query_row(
            "SELECT deadline FROM events
             WHERE manuscript_id = ?1 AND review_state IN ('confirmed', 'edited')
               AND deadline IS NOT NULL
             ORDER BY occurred_at DESC, id DESC LIMIT 1",
            [manuscript_id],
            |r| r.get(0),
        )
        .optional()?;
    if let (Some(proposed), Some(confirmed)) = (&proposal.deadline, confirmed_deadline) {
        if proposed.value.to_string() != confirmed {
            notes.push(format!("Differs from the confirmed deadline {confirmed}."));
        }
    }
    Ok(notes)
}

/// Status from the newest confirmed event that has one; deadline and requested
/// action from the newest confirmed event that has a status or a deadline, so a
/// later update (e.g. "revision received") retires an earlier deadline.
fn current_state(conn: &Connection, summary: &mut ManuscriptSummary) -> Result<(), StoreError> {
    if let Some((status, since)) = conn
        .query_row(
            "SELECT status, occurred_at FROM events
             WHERE manuscript_id = ?1 AND review_state IN ('confirmed', 'edited')
               AND status IS NOT NULL
             ORDER BY occurred_at DESC, id DESC LIMIT 1",
            [summary.id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        summary.status = parse_status(status);
        summary.status_since = Some(since);
    }
    if let Some((deadline, action)) = conn
        .query_row(
            "SELECT deadline, requested_action FROM events
             WHERE manuscript_id = ?1 AND review_state IN ('confirmed', 'edited')
               AND (status IS NOT NULL OR deadline IS NOT NULL)
             ORDER BY occurred_at DESC, id DESC LIMIT 1",
            [summary.id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()?
    {
        summary.deadline = parse_date(deadline);
        summary.requested_action = action;
    }
    Ok(())
}

fn ensure_manuscript_exists(conn: &Connection, id: i64) -> Result<(), StoreError> {
    conn.query_row("SELECT 1 FROM manuscripts WHERE id = ?1", [id], |_| Ok(()))
        .optional()?
        .ok_or(StoreError::NotFound {
            entity: "manuscript",
            id,
        })
}

fn create_manuscript(
    conn: &Connection,
    journal: Option<&str>,
    external_id: Option<&str>,
    title: Option<&str>,
) -> Result<i64, StoreError> {
    let journal = journal.unwrap_or(UNKNOWN_JOURNAL);
    conn.execute(
        "INSERT OR IGNORE INTO manuscripts (journal, external_id, title, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![journal, external_id, title, now()],
    )?;
    match external_id {
        // The insert may have been ignored because the manuscript already exists.
        Some(external_id) => Ok(conn.query_row(
            "SELECT id FROM manuscripts WHERE journal = ?1 AND external_id = ?2",
            params![journal, external_id],
            |r| r.get(0),
        )?),
        None => Ok(conn.last_insert_rowid()),
    }
}
