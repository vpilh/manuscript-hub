//! Connected mailboxes and their sync cursors (WP 2.2, FR04, FR05).
//!
//! Settings hold connection details only; passwords stay in the OS keychain.

use mh_core::Provider;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::{store::now, Store, StoreError};

#[derive(Debug, Clone, Serialize)]
pub struct AccountSummary {
    pub id: i64,
    pub provider: Provider,
    pub alias: String,
    /// Connection settings as JSON, without credentials.
    pub settings_json: String,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
}

impl Store {
    pub fn add_account(
        &mut self,
        provider: Provider,
        alias: &str,
        settings_json: &str,
    ) -> Result<i64, StoreError> {
        self.write(|tx| {
            let taken: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM accounts WHERE provider = ?1 AND alias = ?2)",
                params![provider.as_str(), alias],
                |row| row.get(0),
            )?;
            if taken {
                return Err(StoreError::Format(format!(
                    "A mailbox named \"{alias}\" is already connected. Choose another name."
                )));
            }
            tx.execute(
                "INSERT INTO accounts (provider, alias, settings_json) VALUES (?1, ?2, ?3)",
                params![provider.as_str(), alias, settings_json],
            )?;
            Ok(tx.last_insert_rowid())
        })
    }

    pub fn accounts(&self) -> Result<Vec<AccountSummary>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT id, provider, alias, settings_json, last_sync_at, last_error
             FROM accounts ORDER BY alias",
        )?;
        let rows = statement.query_map([], |row| {
            let provider: String = row.get(1)?;
            Ok(AccountSummary {
                id: row.get(0)?,
                provider: Provider::parse(&provider).unwrap_or(Provider::Mock),
                alias: row.get(2)?,
                settings_json: row.get(3)?,
                last_sync_at: row.get(4)?,
                last_error: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn account(&self, id: i64) -> Result<AccountSummary, StoreError> {
        self.accounts()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(StoreError::NotFound {
                entity: "account",
                id,
            })
    }

    pub fn update_account_settings(
        &mut self,
        id: i64,
        settings_json: &str,
    ) -> Result<(), StoreError> {
        self.write(|tx| {
            let updated = tx.execute(
                "UPDATE accounts SET settings_json = ?2 WHERE id = ?1",
                params![id, settings_json],
            )?;
            if updated == 0 {
                return Err(StoreError::NotFound {
                    entity: "account",
                    id,
                });
            }
            Ok(())
        })
    }

    /// Removes an account with its cursors and the proposals still waiting for
    /// review. Confirmed events stay in their manuscripts' timelines; they only
    /// lose the link to the source message.
    pub fn remove_account(&mut self, id: i64) -> Result<(), StoreError> {
        self.write(|tx| {
            tx.execute(
                "DELETE FROM events
                 WHERE review_state NOT IN ('confirmed', 'edited')
                   AND source_message_id IN
                       (SELECT id FROM source_messages WHERE account_id = ?1)",
                params![id],
            )?;
            let removed = tx.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
            if removed == 0 {
                return Err(StoreError::NotFound {
                    entity: "account",
                    id,
                });
            }
            Ok(())
        })
    }

    pub fn sync_cursor(&self, account_id: i64, folder: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT cursor_json FROM sync_cursors WHERE account_id = ?1 AND folder = ?2",
                params![account_id, folder],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn save_sync_cursor(
        &mut self,
        account_id: i64,
        folder: &str,
        cursor_json: &str,
    ) -> Result<(), StoreError> {
        self.write(|tx| {
            tx.execute(
                "INSERT INTO sync_cursors (account_id, folder, cursor_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (account_id, folder)
                 DO UPDATE SET cursor_json = excluded.cursor_json, updated_at = excluded.updated_at",
                params![account_id, folder, cursor_json, now()],
            )?;
            Ok(())
        })
    }

    pub fn clear_sync_cursor(&mut self, account_id: i64, folder: &str) -> Result<(), StoreError> {
        self.write(|tx| {
            tx.execute(
                "DELETE FROM sync_cursors WHERE account_id = ?1 AND folder = ?2",
                params![account_id, folder],
            )?;
            Ok(())
        })
    }

    /// Records the outcome of a sync: success clears the last error.
    pub fn record_sync(&mut self, account_id: i64, error: Option<&str>) -> Result<(), StoreError> {
        self.write(|tx| {
            match error {
                None => tx.execute(
                    "UPDATE accounts SET last_sync_at = ?2, last_error = NULL WHERE id = ?1",
                    params![account_id, now()],
                )?,
                Some(error) => tx.execute(
                    "UPDATE accounts SET last_error = ?2 WHERE id = ?1",
                    params![account_id, error],
                )?,
            };
            Ok(())
        })
    }
}
