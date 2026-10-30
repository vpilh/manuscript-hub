//! Read-only IMAP adapter for GroupWise, Gmail (app password) and other
//! IMAP servers (WP 2.1).
//!
//! Folders are opened with `EXAMINE` and messages fetched with `BODY.PEEK`, so
//! nothing is marked read; [`crate::imap_guard`] rejects anything else before
//! it is sent. Sync resumes after the last seen UID and starts over when the
//! server reports a new `UIDVALIDITY` (FR05).

mod session;
mod transport;

use std::fmt;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use mh_core::{CalendarDate, Provider, SourceRef, TransientMessage};
use serde::{Deserialize, Serialize};

use crate::imap_guard::GuardError;
use crate::{parse_date_time, ChangeBatch, MailProvider, ProviderError, SyncCursor};
pub use session::Folder;
use session::Session;
use transport::Stream;

/// Only the start of each message is downloaded: enough for the letter text,
/// without the attachments that usually follow it.
pub const FETCH_LIMIT: u32 = 64 * 1024;
const BATCH_SIZE: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    /// TLS from the first byte, usually port 993.
    Tls,
    /// Plain connection upgraded with `STARTTLS` before login, usually port 143.
    StartTls,
}

/// Connection settings without the password, which stays in the OS keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImapSettings {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    pub folders: Vec<String>,
    /// How far back the first scan of a folder reaches.
    pub backfill_days: u32,
}

#[derive(Debug)]
pub enum ImapError {
    HostNotFound(String),
    Unreachable {
        host: String,
        port: u16,
        reason: String,
    },
    Certificate(String),
    Tls(String),
    LoginRejected(String),
    LoginDisabled,
    /// User names, passwords and folder names must be printable ASCII for now.
    UnsupportedText,
    Rejected {
        command: String,
        text: String,
    },
    Guard(GuardError),
    Protocol(String),
    Closed,
    Io(io::Error),
}

impl fmt::Display for ImapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImapError::HostNotFound(host) => write!(
                f,
                "The server {host} could not be found. Check the address, and connect to \
                 your university VPN if the server is only reachable from its network."
            ),
            ImapError::Unreachable { host, port, reason } => write!(
                f,
                "Could not connect to {host} on port {port} ({reason}). Check the port, \
                 and connect to your university VPN if needed."
            ),
            ImapError::Certificate(reason) => write!(
                f,
                "The server's certificate could not be verified ({reason}), so the connection \
                 was stopped. Check the server address."
            ),
            ImapError::Tls(reason) => write!(f, "The encrypted connection failed: {reason}"),
            ImapError::LoginRejected(text) => {
                write!(f, "The server rejected the user name or password")?;
                if !text.is_empty() {
                    write!(f, " ({text})")?;
                }
                f.write_str(". For Gmail, use an app password, not your Google password.")
            }
            ImapError::LoginDisabled => {
                f.write_str("The server doesn't allow password login on this connection.")
            }
            ImapError::UnsupportedText => f.write_str(
                "User names, passwords and folder names with accents, umlauts or other \
                 non-ASCII characters aren't supported yet.",
            ),
            ImapError::Rejected { command, text } => {
                write!(f, "The server refused {command}: {text}")
            }
            ImapError::Guard(e) => {
                write!(f, "Blocked a command that could change the mailbox: {e}")
            }
            ImapError::Protocol(reason) => write!(f, "Unexpected server response: {reason}"),
            ImapError::Closed => f.write_str("The server closed the connection."),
            ImapError::Io(e)
                if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock =>
            {
                f.write_str("The server stopped responding.")
            }
            ImapError::Io(e) => write!(f, "The connection failed: {e}"),
        }
    }
}

impl std::error::Error for ImapError {}

impl From<io::Error> for ImapError {
    fn from(e: io::Error) -> Self {
        ImapError::Io(e)
    }
}

impl From<ImapError> for ProviderError {
    fn from(e: ImapError) -> Self {
        match e {
            ImapError::LoginRejected(_) | ImapError::LoginDisabled => {
                ProviderError::PermissionDenied(e.to_string())
            }
            _ => ProviderError::Unavailable(e.to_string()),
        }
    }
}

type Connector = Box<dyn Fn(&ImapSettings) -> Result<Session<Stream>, ImapError> + Send>;

/// Logs in and lists the folders, to check settings before saving an account.
pub fn check_account(settings: &ImapSettings, password: &str) -> Result<Vec<Folder>, ImapError> {
    let mut session = transport::connect(settings)?;
    session.login(&settings.username, password)?;
    let folders = session.list();
    session.logout();
    folders
}

/// One folder of one IMAP account.
pub struct ImapProvider {
    kind: Provider,
    account_alias: String,
    settings: ImapSettings,
    password: String,
    folder: String,
    batch_size: usize,
    connect: Connector,
    session: Option<Session<Stream>>,
}

impl fmt::Debug for ImapProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapProvider")
            .field("account_alias", &self.account_alias)
            .field("host", &self.settings.host)
            .field("folder", &self.folder)
            .finish_non_exhaustive()
    }
}

impl ImapProvider {
    pub fn new(
        kind: Provider,
        account_alias: impl Into<String>,
        settings: ImapSettings,
        password: impl Into<String>,
        folder: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            account_alias: account_alias.into(),
            settings,
            password: password.into(),
            folder: folder.into(),
            batch_size: BATCH_SIZE,
            connect: Box::new(transport::connect),
            session: None,
        }
    }

    fn session(&mut self) -> Result<&mut Session<Stream>, ImapError> {
        if self.session.is_none() {
            let mut session = (self.connect)(&self.settings)?;
            session.login(&self.settings.username, &self.password)?;
            self.session = Some(session);
        }
        Ok(self.session.as_mut().expect("session was just opened"))
    }

    fn next_batch(&mut self, cursor: Option<&SyncCursor>) -> Result<ChangeBatch, ProviderError> {
        let folder = self.folder.clone();
        let since = imap_date(unix_now() - i64::from(self.settings.backfill_days) * 86_400);
        let batch_size = self.batch_size;
        let session = self.session()?;

        let info = session.examine(&folder)?;
        let uid_validity = info.uid_validity.ok_or_else(|| {
            ProviderError::Unavailable("the server didn't report UIDVALIDITY".into())
        })?;
        let last_uid = match cursor {
            None => 0,
            Some(SyncCursor::Imap {
                folder: f,
                uid_validity: v,
                last_uid,
            }) if *f == folder => {
                if *v != uid_validity {
                    return Err(ProviderError::CursorExpired);
                }
                *last_uid
            }
            Some(_) => return Err(ProviderError::CursorExpired),
        };

        // `n:*` still matches the newest message when n is past the end, so
        // filter on our side as well.
        let mut uids: Vec<u32> = session
            .uid_search(&format!(
                "UID {}:* SINCE {since}",
                last_uid.saturating_add(1)
            ))?
            .into_iter()
            .filter(|uid| *uid > last_uid)
            .collect();
        let more = uids.len() > batch_size;
        uids.truncate(batch_size);

        let mut messages = Vec::new();
        if !uids.is_empty() {
            let items = format!("UID INTERNALDATE BODY.PEEK[]<0.{FETCH_LIMIT}>");
            for fetched in session.uid_fetch(&uids, &items)? {
                if let Some(message) = to_message(
                    &fetched,
                    self.kind,
                    &self.account_alias,
                    &folder,
                    uid_validity,
                ) {
                    messages.push(message);
                }
            }
        }
        let next_uid = match uids.last() {
            Some(uid) => *uid,
            // Nothing new: skip ahead so the next search starts at new mail.
            None => info
                .uid_next
                .map_or(last_uid, |next| next.saturating_sub(1).max(last_uid)),
        };
        Ok(ChangeBatch {
            messages,
            next_cursor: SyncCursor::Imap {
                folder,
                uid_validity,
                last_uid: next_uid,
            },
            more,
        })
    }
}

impl MailProvider for ImapProvider {
    fn kind(&self) -> Provider {
        self.kind
    }

    fn account_alias(&self) -> &str {
        &self.account_alias
    }

    fn fetch_changes(&mut self, cursor: Option<&SyncCursor>) -> Result<ChangeBatch, ProviderError> {
        let result = self.next_batch(cursor);
        if matches!(
            result,
            Err(ProviderError::Unavailable(_) | ProviderError::PermissionDenied(_))
        ) {
            // Reconnect on the next call rather than reuse a broken session.
            self.session = None;
        }
        result
    }
}

impl Drop for ImapProvider {
    fn drop(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.logout();
        }
    }
}

/// Decodes a fetched message in memory. Bodies never leave this struct (FR15).
fn to_message(
    fetched: &session::Fetched,
    kind: Provider,
    account_alias: &str,
    folder: &str,
    uid_validity: u32,
) -> Option<TransientMessage> {
    let parsed = mail_parser::MessageParser::default().parse(&fetched.raw)?;
    let from = parsed
        .from()
        .and_then(|address| address.first())
        .map(|addr| match (addr.name(), addr.address()) {
            (Some(name), Some(address)) => format!("\"{name}\" <{address}>"),
            (None, Some(address)) => format!("<{address}>"),
            (Some(name), None) => name.to_string(),
            (None, None) => String::new(),
        })
        .unwrap_or_default();
    let message_id = match parsed.message_id() {
        Some(id) => format!("<{id}>"),
        // Without a Message-ID the UID is the only stable identity.
        None => format!("<{uid_validity}.{}@{folder}.imap.invalid>", fetched.uid),
    };
    let received_at = parsed
        .date()
        .map(|date| date.to_timestamp())
        .or_else(|| fetched.internal_date.as_deref().and_then(parse_date_time))
        .unwrap_or(0);
    Some(TransientMessage {
        source: SourceRef {
            provider: kind,
            account_alias: account_alias.to_string(),
            message_id,
            folder: folder.to_string(),
            received_at,
        },
        from,
        subject: parsed.subject().unwrap_or_default().to_string(),
        body: parsed
            .body_text(0)
            .map(|text| text.into_owned())
            .unwrap_or_default(),
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// IMAP search dates look like `29-Oct-2026`.
fn imap_date(unix_seconds: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = CalendarDate::from_unix_seconds(unix_seconds);
    format!(
        "{}-{}-{}",
        date.day(),
        MONTHS[usize::from(date.month()) - 1],
        date.year()
    )
}

#[cfg(test)]
mod tests;
