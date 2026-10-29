use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, Transaction, MAIN_DB};

use crate::{crypto, DbKey, StoreError, MIGRATIONS};

/// The encrypted local database.
///
/// SQLite runs in memory; after every committed write the whole database is
/// serialized, sealed with [`DbKey`] and atomically replaces the file. The
/// metadata is small (no mail content), so this keeps the crypto in audited
/// pure-Rust code without a C/OpenSSL build dependency.
pub struct Store {
    pub(crate) conn: Connection,
    file: Option<(PathBuf, DbKey)>,
}

impl Store {
    /// Opens or creates the database at `path`. A wrong key fails with
    /// [`StoreError::Decrypt`] and leaves the file untouched.
    pub fn open(path: impl Into<PathBuf>, key: DbKey) -> Result<Self, StoreError> {
        let path = path.into();
        let mut conn = Connection::open_in_memory()?;
        let existing = path.exists();
        if existing {
            let sealed = fs::read(&path)?;
            let plain = crypto::open(&key, &sealed)?;
            conn.deserialize_read_exact(MAIN_DB, &plain[..], plain.len(), false)?;
        } else if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut store = Self {
            conn,
            file: Some((path, key)),
        };
        let migrated = store.init()?;
        if migrated || !existing {
            store.persist()?;
        }
        Ok(store)
    }

    /// An unpersisted database, for tests and previews.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let mut store = Self {
            conn: Connection::open_in_memory()?,
            file: None,
        };
        store.init()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    /// Applies pending migrations; returns whether any ran.
    fn init(&mut self) -> Result<bool, StoreError> {
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let applied = usize::try_from(self.schema_version()?)
            .map_err(|_| StoreError::Format("invalid schema version".into()))?;
        if applied > MIGRATIONS.len() {
            return Err(StoreError::Format(
                "this database was created by a newer version of Manuscript Hub".into(),
            ));
        }
        for (index, (_, sql)) in MIGRATIONS.iter().enumerate().skip(applied) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute_batch(&format!("PRAGMA user_version = {};", index + 1))?;
            tx.commit()?;
        }
        Ok(applied < MIGRATIONS.len())
    }

    /// Runs `f` in a transaction, commits, then persists the encrypted file.
    pub(crate) fn write<T>(
        &mut self,
        f: impl FnOnce(&Transaction<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let tx = self.conn.transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        self.persist()?;
        Ok(value)
    }

    fn persist(&mut self) -> Result<(), StoreError> {
        let Some((path, key)) = &self.file else {
            return Ok(());
        };
        let data = self.conn.serialize(MAIN_DB)?;
        let sealed = crypto::seal(key, &data)?;
        drop(data);
        write_atomically(path, &sealed)
    }
}

/// Write to a sibling temp file, flush it to disk, then rename over the target,
/// so a crash never leaves a half-written database.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_is_encrypted_and_reopens_with_the_right_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("hub.db");
        let key_bytes = DbKey::generate().as_bytes().to_vec();

        {
            let mut store = Store::open(&path, DbKey::from_bytes(&key_bytes).unwrap()).unwrap();
            store
                .ensure_account(mh_core::Provider::Mock, "Demo mailbox")
                .unwrap();
        }

        let raw = fs::read(&path).unwrap();
        assert!(raw.starts_with(b"MHDB"));
        for needle in [&b"SQLite format"[..], b"Demo mailbox", b"CREATE TABLE"] {
            assert!(
                !raw.windows(needle.len()).any(|w| w == needle),
                "plaintext leaked into the database file"
            );
        }

        let store = Store::open(&path, DbKey::from_bytes(&key_bytes).unwrap()).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as i64);
        let alias: String = store
            .conn
            .query_row("SELECT alias FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(alias, "Demo mailbox");
    }

    #[test]
    fn wrong_key_fails_without_touching_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hub.db");
        drop(Store::open(&path, DbKey::generate()).unwrap());
        let before = fs::read(&path).unwrap();

        assert!(matches!(
            Store::open(&path, DbKey::generate()),
            Err(StoreError::Decrypt)
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
