//! Whole-file authenticated encryption for the database (XChaCha20-Poly1305).
//!
//! File layout: `MHDB` magic, format version byte, 24-byte random nonce,
//! ciphertext + 16-byte tag. The header is bound as associated data, so it
//! cannot be altered without failing authentication.

use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use zeroize::Zeroizing;

use crate::StoreError;

const MAGIC: &[u8; 4] = b"MHDB";
const FORMAT_VERSION: u8 = 1;
const HEADER_LEN: usize = MAGIC.len() + 1;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// The 256-bit database key. Kept in the OS keychain, zeroed from memory on drop.
pub struct DbKey(Zeroizing<[u8; 32]>);

impl DbKey {
    pub const LEN: usize = 32;

    pub fn generate() -> Self {
        let key = XChaCha20Poly1305::generate_key(&mut OsRng);
        let mut bytes = Zeroizing::new([0u8; Self::LEN]);
        bytes.copy_from_slice(&key);
        Self(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StoreError> {
        let array: [u8; Self::LEN] = bytes
            .try_into()
            .map_err(|_| StoreError::Format("database key must be 32 bytes".into()))?;
        Ok(Self(Zeroizing::new(array)))
    }

    /// For storing the key in the OS keychain. Never log or persist this elsewhere.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(Key::from_slice(self.0.as_slice()))
    }
}

impl std::fmt::Debug for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DbKey(..)")
    }
}

fn header() -> [u8; HEADER_LEN] {
    let mut header = [0u8; HEADER_LEN];
    header[..MAGIC.len()].copy_from_slice(MAGIC);
    header[MAGIC.len()] = FORMAT_VERSION;
    header
}

pub fn seal(key: &DbKey, plaintext: &[u8]) -> Result<Vec<u8>, StoreError> {
    let header = header();
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = key
        .cipher()
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &header,
            },
        )
        .map_err(|_| StoreError::Format("encryption failed".into()))?;

    let mut sealed = Vec::with_capacity(HEADER_LEN + NONCE_LEN + ciphertext.len());
    sealed.extend_from_slice(&header);
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

pub fn open(key: &DbKey, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, StoreError> {
    if sealed.len() < HEADER_LEN + NONCE_LEN + TAG_LEN || !sealed.starts_with(MAGIC) {
        return Err(StoreError::Format(
            "not a Manuscript Hub database file".into(),
        ));
    }
    let (header, rest) = sealed.split_at(HEADER_LEN);
    if header[MAGIC.len()] != FORMAT_VERSION {
        return Err(StoreError::Format(format!(
            "unsupported database format version {}",
            header[MAGIC.len()]
        )));
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    key.cipher()
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: header,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| StoreError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_wrong_key_or_tampering() {
        let key = DbKey::generate();
        let sealed = seal(&key, b"SQLite format 3\0 example").unwrap();
        assert!(!sealed.windows(b"SQLite".len()).any(|w| w == b"SQLite"));
        assert_eq!(
            open(&key, &sealed).unwrap().as_slice(),
            b"SQLite format 3\0 example"
        );

        assert!(matches!(
            open(&DbKey::generate(), &sealed),
            Err(StoreError::Decrypt)
        ));

        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(matches!(open(&key, &tampered), Err(StoreError::Decrypt)));

        let mut bad_version = sealed;
        bad_version[MAGIC.len()] = 9;
        assert!(matches!(
            open(&key, &bad_version),
            Err(StoreError::Format(_))
        ));
    }

    #[test]
    fn nonces_are_fresh() {
        let key = DbKey::generate();
        assert_ne!(seal(&key, b"x").unwrap(), seal(&key, b"x").unwrap());
    }

    #[test]
    fn key_debug_is_redacted() {
        assert_eq!(format!("{:?}", DbKey::generate()), "DbKey(..)");
    }
}
