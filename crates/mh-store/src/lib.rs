//! Encrypted, device-local storage (FR15, PRD §7).
//!
//! The database file is sealed with XChaCha20-Poly1305 under a [`DbKey`] that
//! the app keeps in the OS keychain, never next to the database. Tables hold
//! normalized metadata, source references and sync cursors only: no message
//! bodies, attachments or credentials.

mod accounts;
mod crypto;
mod error;
mod matching;
mod merge;
mod review;
mod store;

pub use accounts::AccountSummary;
pub use crypto::DbKey;
pub use error::StoreError;
pub use matching::{normalize_title, title_similarity, TITLE_MATCH_THRESHOLD};
pub use review::{
    EventEdits, ManuscriptRef, ManuscriptSummary, MatchKind, QueueItem, RecordOutcome,
    ReviewDecision, TimelineEntry,
};
pub use store::Store;

/// Ordered schema migrations; `PRAGMA user_version` counts the applied ones.
pub const MIGRATIONS: &[(&str, &str)] =
    &[("0001_init", include_str!("../migrations/0001_init.sql"))];

#[cfg(test)]
mod tests {
    use super::MIGRATIONS;

    #[test]
    fn migrations_are_ordered_and_unique() {
        let names: Vec<_> = MIGRATIONS.iter().map(|(name, _)| *name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn schema_has_no_body_or_secret_columns() {
        for (_, sql) in MIGRATIONS {
            let lower = sql.to_lowercase();
            for forbidden in ["body", "attachment", "password", "token", "secret"] {
                assert!(
                    !lower.contains(forbidden),
                    "migration mentions forbidden column `{forbidden}`"
                );
            }
        }
    }
}
