//! Encrypted, device-local storage (FR15, PRD §7).
//!
//! The database is SQLCipher-encrypted; its key lives in the OS keychain, never
//! on disk next to the database. Tables hold normalized metadata, source
//! references and sync cursors only: no message bodies, attachments or secrets.
//!
//! Status: schema draft. The SQLCipher connection, key management and
//! migration runner are work package 1.3 in `docs/IMPLEMENTATION_PLAN.md`.

/// Ordered schema migrations, applied once each inside a transaction.
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
