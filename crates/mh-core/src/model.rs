use serde::{Deserialize, Serialize};

use crate::{CalendarDate, EditorialStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Gmail,
    MicrosoftGraph,
    GroupwiseImap,
    /// Synthetic fixtures and manual paste-email import.
    Mock,
}

/// Enough to reopen the original message in the user's mail client. No body copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub provider: Provider,
    pub account_alias: String,
    /// Provider-stable identifier (Gmail message ID, Graph immutable ID, IMAP `UIDVALIDITY:UID`).
    pub message_id: String,
    pub folder: String,
    /// Unix seconds, UTC.
    pub received_at: i64,
}

/// Messages are deduplicated per provider account (FR10).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DedupKey {
    pub provider: Provider,
    pub account_alias: String,
    pub message_id: String,
}

impl SourceRef {
    pub fn dedup_key(&self) -> DedupKey {
        DedupKey {
            provider: self.provider,
            account_alias: self.account_alias.clone(),
            message_id: self.message_id.clone(),
        }
    }
}

/// Extraction confidence in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Confidence(f32);

impl Confidence {
    pub fn new(value: f32) -> Self {
        Self(if value.is_nan() {
            0.0
        } else {
            value.clamp(0.0, 1.0)
        })
    }

    pub fn value(self) -> f32 {
        self.0
    }
}

/// An extracted value together with the phrase it came from and how sure the parser is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field<T> {
    pub value: T,
    pub original: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Proposed,
    Confirmed,
    Edited,
    Ignored,
    Unrelated,
}

/// A parser result awaiting the researcher's decision (PRD journeys B and C).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedEvent {
    pub source: SourceRef,
    pub journal: Option<Field<String>>,
    pub manuscript_id: Option<Field<String>>,
    pub title: Option<Field<String>>,
    pub status: Option<Field<EditorialStatus>>,
    pub deadline: Option<Field<CalendarDate>>,
    pub requested_action: Option<Field<String>>,
    /// Version of the rule set that produced this proposal, for provenance.
    pub parser_version: String,
    /// Why this message needs a human decision, if anything is uncertain.
    pub uncertainty: Vec<String>,
    pub review: ReviewState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_is_clamped() {
        assert_eq!(Confidence::new(1.7).value(), 1.0);
        assert_eq!(Confidence::new(-0.2).value(), 0.0);
        assert_eq!(Confidence::new(f32::NAN).value(), 0.0);
    }

    #[test]
    fn dedup_key_ignores_folder_and_time() {
        let a = SourceRef {
            provider: Provider::Gmail,
            account_alias: "work".into(),
            message_id: "18c2".into(),
            folder: "INBOX".into(),
            received_at: 1,
        };
        let b = SourceRef {
            folder: "Archive".into(),
            received_at: 2,
            ..a.clone()
        };
        assert_eq!(a.dedup_key(), b.dedup_key());
    }
}
