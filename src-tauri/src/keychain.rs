//! The database key lives in the OS credential store (Windows Credential
//! Manager, macOS Keychain, Secret Service on Linux), never on disk (PRD §8).
//! Mailbox passwords are kept the same way, one entry per account.

use mh_store::DbKey;

const SERVICE: &str = "io.github.vpilh.manuscripthub";
const DATABASE_KEY: &str = "database-key";

/// Loads the database key, creating one only for a brand-new database. If the
/// database exists but its key is gone, we fail instead of generating a new
/// key, which would make the existing data unreadable.
pub fn database_key(database_exists: bool) -> Result<DbKey, String> {
    let entry = keyring::Entry::new(SERVICE, DATABASE_KEY).map_err(describe)?;
    match entry.get_secret() {
        Ok(bytes) => DbKey::from_bytes(&bytes).map_err(|e| e.to_string()),
        Err(keyring::Error::NoEntry) if !database_exists => {
            let key = DbKey::generate();
            entry.set_secret(key.as_bytes()).map_err(describe)?;
            Ok(key)
        }
        Err(keyring::Error::NoEntry) => Err(
            "The database key is missing from the system keychain, so the local database \
             cannot be opened. Restore the keychain entry or start fresh by removing the \
             database file."
                .into(),
        ),
        Err(e) => Err(describe(e)),
    }
}

fn mail_entry(account_id: i64) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("mail-account-{account_id}")).map_err(describe)
}

pub fn save_mail_password(account_id: i64, password: &str) -> Result<(), String> {
    mail_entry(account_id)?
        .set_password(password)
        .map_err(describe)
}

pub fn mail_password(account_id: i64) -> Result<String, String> {
    match mail_entry(account_id)?.get_password() {
        Ok(password) => Ok(password),
        Err(keyring::Error::NoEntry) => Err(
            "The password for this mailbox is missing from the system keychain.              Disconnect the mailbox and connect it again."
                .into(),
        ),
        Err(e) => Err(describe(e)),
    }
}

/// Removing an entry that is already gone counts as success.
pub fn delete_mail_password(account_id: i64) -> Result<(), String> {
    match mail_entry(account_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(describe(e)),
    }
}

fn describe(e: keyring::Error) -> String {
    format!("The system keychain is unavailable: {e}")
}
