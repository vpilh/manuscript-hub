//! Allowlist for IMAP commands (FR06, PRD §8).
//!
//! GroupWise credentials can usually modify the mailbox, so the adapter's
//! scopes cannot be trusted to keep it read-only. Every command the IMAP adapter
//! sends must pass [`check_command`] first. Anything not listed is denied.

use std::fmt;

/// Commands that never change mailbox state. `SELECT` is absent on purpose:
/// mailboxes are opened with `EXAMINE`, which is read-only by definition.
/// `CLOSE` is absent because it expunges a selected mailbox; use `UNSELECT`.
const ALLOWED: &[&str] = &[
    "CAPABILITY",
    "NOOP",
    "LOGOUT",
    "STARTTLS",
    "AUTHENTICATE",
    "LOGIN",
    "ID",
    "ENABLE",
    "NAMESPACE",
    "LIST",
    "LSUB",
    "STATUS",
    "EXAMINE",
    "SEARCH",
    "FETCH",
    "UNSELECT",
    "IDLE",
    "DONE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    Empty,
    /// Embedded CR/LF could smuggle a second command.
    LineBreak,
    Denied(String),
    /// A fetch item that would set `\Seen`.
    MarksSeen(String),
}

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::Empty => f.write_str("empty IMAP command"),
            GuardError::LineBreak => f.write_str("IMAP command contains a line break"),
            GuardError::Denied(verb) => write!(f, "IMAP command {verb} is not allowed"),
            GuardError::MarksSeen(item) => write!(f, "fetch item {item} would mark mail as read"),
        }
    }
}

impl std::error::Error for GuardError {}

/// Checks one command line, without its tag, e.g. `UID FETCH 1:* (BODY.PEEK[TEXT])`.
pub fn check_command(command: &str) -> Result<(), GuardError> {
    if command.contains(['\r', '\n']) {
        return Err(GuardError::LineBreak);
    }
    let upper = command.to_ascii_uppercase();
    let mut words = upper.split_ascii_whitespace();
    let mut verb = words.next().ok_or(GuardError::Empty)?;
    if verb == "UID" {
        verb = words.next().ok_or(GuardError::Empty)?;
        if !matches!(verb, "SEARCH" | "FETCH") {
            return Err(GuardError::Denied(format!("UID {verb}")));
        }
    }
    if !ALLOWED.contains(&verb) {
        return Err(GuardError::Denied(verb.to_string()));
    }
    if verb == "FETCH" {
        check_fetch_items(&upper)?;
    }
    Ok(())
}

/// `BODY[...]`, `BINARY[...]`, `RFC822` and `RFC822.TEXT` set `\Seen` (RFC 3501 §6.4.5,
/// RFC 3516). Their `.PEEK` forms, `RFC822.HEADER` and `RFC822.SIZE` do not.
fn check_fetch_items(upper: &str) -> Result<(), GuardError> {
    for item in ["BODY[", "BINARY["] {
        if upper.contains(item) {
            return Err(GuardError::MarksSeen(item.to_string()));
        }
    }
    for (index, _) in upper.match_indices("RFC822") {
        let after = &upper[index + "RFC822".len()..];
        if !(after.starts_with(".SIZE") || after.starts_with(".HEADER")) {
            return Err(GuardError::MarksSeen("RFC822".to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_read_only_commands() {
        for command in [
            "CAPABILITY",
            "EXAMINE INBOX",
            "examine \"Sent Items\"",
            "UID SEARCH SINCE 1-Oct-2024",
            "UID FETCH 1:* (UID FLAGS INTERNALDATE BODY.PEEK[HEADER])",
            "FETCH 4 (BODY.PEEK[TEXT] RFC822.SIZE RFC822.HEADER BINARY.PEEK[1])",
            "STATUS INBOX (UIDVALIDITY UIDNEXT)",
            "UNSELECT",
        ] {
            assert_eq!(check_command(command), Ok(()), "{command}");
        }
    }

    #[test]
    fn denies_mutating_commands() {
        for command in [
            "SELECT INBOX",
            "STORE 1 +FLAGS (\\Seen)",
            "UID STORE 1 +FLAGS (\\Deleted)",
            "UID MOVE 1 Trash",
            "UID EXPUNGE 1",
            "COPY 1 Archive",
            "APPEND INBOX {12}",
            "EXPUNGE",
            "CLOSE",
            "DELETE Old",
            "CREATE New",
            "RENAME A B",
            "SUBSCRIBE A",
            "XGWEXTENSION",
        ] {
            assert!(
                matches!(check_command(command), Err(GuardError::Denied(_))),
                "{command}"
            );
        }
    }

    #[test]
    fn denies_fetches_that_mark_messages_read() {
        for command in [
            "FETCH 1 (BODY[TEXT])",
            "UID FETCH 1 (body[])",
            "FETCH 1 RFC822",
            "FETCH 1 (RFC822.TEXT)",
            "FETCH 1 (BINARY[1])",
        ] {
            assert!(
                matches!(check_command(command), Err(GuardError::MarksSeen(_))),
                "{command}"
            );
        }
    }

    #[test]
    fn denies_smuggled_commands() {
        assert_eq!(
            check_command("NOOP\r\nA2 STORE 1 +FLAGS (\\Deleted)"),
            Err(GuardError::LineBreak)
        );
        assert_eq!(check_command("   "), Err(GuardError::Empty));
    }
}
