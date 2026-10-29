use std::fmt;

use mh_core::{Provider, TransientMessage};

/// Where incremental sync resumes (FR05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncCursor {
    GmailHistory {
        history_id: u64,
    },
    GraphDelta {
        folder: String,
        delta_link: String,
    },
    /// A changed `uid_validity` invalidates `last_uid` and forces a rescan of the folder.
    Imap {
        folder: String,
        uid_validity: u32,
        last_uid: u32,
    },
    Mock {
        position: usize,
    },
}

#[derive(Debug)]
pub struct ChangeBatch {
    pub messages: Vec<TransientMessage>,
    pub next_cursor: SyncCursor,
    /// More changes are available right away.
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// The cursor is no longer valid (Gmail 404, Graph 410, IMAP `UIDVALIDITY` change).
    /// The caller rescans from the backfill window; dedup keys prevent duplicate events.
    CursorExpired,
    RateLimited {
        retry_after_secs: u64,
    },
    PermissionDenied(String),
    Unavailable(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::CursorExpired => f.write_str("sync cursor expired; a rescan is needed"),
            ProviderError::RateLimited { retry_after_secs } => {
                write!(f, "rate limited; retry after {retry_after_secs} s")
            }
            ProviderError::PermissionDenied(reason) => write!(f, "permission denied: {reason}"),
            ProviderError::Unavailable(reason) => write!(f, "mailbox unavailable: {reason}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// A read-only mailbox. Implementations must never issue a mutating request.
pub trait MailProvider {
    fn kind(&self) -> Provider;

    fn account_alias(&self) -> &str;

    /// Messages changed since `cursor`, or the initial backfill when `cursor` is `None`.
    fn fetch_changes(&mut self, cursor: Option<&SyncCursor>) -> Result<ChangeBatch, ProviderError>;
}
