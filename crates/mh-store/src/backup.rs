//! Password-protected backups (FR16): an [`Export`] sealed with a key derived
//! from a password the user chooses, so it can be restored on another
//! computer, where the keychain-held database key doesn't exist.
//!
//! File layout: `MHBK` magic, format version byte, 16-byte salt, 24-byte
//! nonce, ciphertext + tag. Argon2id derives the key; XChaCha20-Poly1305
//! seals the JSON with the header as associated data.

use argon2::Argon2;
use chacha20poly1305::{
    aead::{rand_core::RngCore, Aead, AeadCore, KeyInit, OsRng, Payload},
    Key, XChaCha20Poly1305, XNonce,
};

use zeroize::Zeroizing;

use crate::{Export, StoreError};

const MAGIC: &[u8; 4] = b"MHBK";
const VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const HEADER_LEN: usize = MAGIC.len() + 1 + SALT_LEN;

/// Passwords shorter than this are refused; the file may leave the computer.
pub const MIN_PASSWORD_CHARS: usize = 10;

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, StoreError> {
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, key.as_mut_slice())
        .map_err(|e| StoreError::Format(format!("key derivation failed: {e}")))?;
    Ok(key)
}

pub fn seal_backup(export: &Export, password: &str) -> Result<Vec<u8>, StoreError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(StoreError::Format(format!(
            "Choose a backup password of at least {MIN_PASSWORD_CHARS} characters."
        )));
    }
    let plain = Zeroizing::new(serde_json::to_vec(export)?);
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_slice()));
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.push(VERSION);
    header.extend_from_slice(&salt);
    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &plain,
                aad: &header,
            },
        )
        .map_err(|_| StoreError::Format("encryption failed".into()))?;

    let mut file = header;
    file.extend_from_slice(&nonce);
    file.extend_from_slice(&sealed);
    Ok(file)
}

/// A wrong password and a damaged file both fail authentication, and are
/// reported the same way.
pub fn open_backup(file: &[u8], password: &str) -> Result<Export, StoreError> {
    if file.len() < HEADER_LEN + NONCE_LEN || &file[..MAGIC.len()] != MAGIC {
        return Err(StoreError::Format(
            "This file isn't a Manuscript Hub backup.".into(),
        ));
    }
    if file[MAGIC.len()] != VERSION {
        return Err(StoreError::Format(
            "This backup was made by a newer version of Manuscript Hub. Update the app first."
                .into(),
        ));
    }
    let (header, rest) = file.split_at(HEADER_LEN);
    let (nonce, sealed) = rest.split_at(NONCE_LEN);
    let key = derive_key(password, &header[MAGIC.len() + 1..])?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_slice()));
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: header,
                },
            )
            .map_err(|_| {
                StoreError::Format("The password is wrong, or the backup file is damaged.".into())
            })?,
    );
    Ok(serde_json::from_slice(&plain)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EXPORT_FORMAT, EXPORT_VERSION};

    fn export() -> Export {
        Export {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            exported_at: 1,
            manuscripts: Vec::new(),
            sender_rules: Vec::new(),
        }
    }

    #[test]
    fn round_trips_with_the_right_password_only() {
        let file = seal_backup(&export(), "correct horse battery").unwrap();
        assert_eq!(&file[..4], b"MHBK");
        assert_eq!(
            open_backup(&file, "correct horse battery")
                .unwrap()
                .exported_at,
            1
        );
        let wrong = open_backup(&file, "correct horse battery!").unwrap_err();
        assert!(wrong.to_string().contains("password is wrong"));
    }

    #[test]
    fn tampering_is_detected() {
        let mut file = seal_backup(&export(), "correct horse battery").unwrap();
        let last = file.len() - 1;
        file[last] ^= 1;
        assert!(open_backup(&file, "correct horse battery").is_err());
        let mut file = seal_backup(&export(), "correct horse battery").unwrap();
        file[6] ^= 1; // a salt byte, bound as associated data
        assert!(open_backup(&file, "correct horse battery").is_err());
    }

    #[test]
    fn short_passwords_and_foreign_files_are_refused() {
        assert!(seal_backup(&export(), "short").is_err());
        assert!(open_backup(b"MHDB\x01not a backup at all, really", "x")
            .unwrap_err()
            .to_string()
            .contains("isn't a Manuscript Hub backup"));
    }
}
