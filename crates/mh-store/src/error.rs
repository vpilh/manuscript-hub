use std::fmt;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    /// Wrong key or a corrupted file. The file is never overwritten in this case.
    Decrypt,
    Format(String),
    NotFound {
        entity: &'static str,
        id: i64,
    },
    /// Reviewed events are final; confirmed data is never changed silently (FR11).
    AlreadyReviewed {
        event_id: i64,
    },
    /// Several manuscripts share the ID, so the user has to pick one.
    NeedsManuscriptChoice {
        event_id: i64,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "database error: {e}"),
            StoreError::Io(e) => write!(f, "file error: {e}"),
            StoreError::Json(e) => write!(f, "data encoding error: {e}"),
            StoreError::Decrypt => {
                f.write_str("the database could not be decrypted (wrong key or damaged file)")
            }
            StoreError::Format(reason) => f.write_str(reason),
            StoreError::NotFound { entity, id } => write!(f, "{entity} {id} not found"),
            StoreError::AlreadyReviewed { event_id } => {
                write!(f, "event {event_id} has already been reviewed")
            }
            StoreError::NeedsManuscriptChoice { event_id } => write!(
                f,
                "several manuscripts share the ID in event {event_id}; choose which one it belongs to"
            ),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Sqlite(e) => Some(e),
            StoreError::Io(e) => Some(e),
            StoreError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Json(e)
    }
}
