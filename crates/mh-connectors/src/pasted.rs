//! Messages the user pastes in (FR17): a full source with headers, as from
//! "Show original" in Gmail, or just the text of a letter. Parsed in memory
//! like synced mail and never stored.

use mh_core::{Provider, SourceRef, TransientMessage};

pub const PASTED_ACCOUNT: &str = "Pasted mail";

/// `received_at` is used when the text has no `Date:` header.
pub fn parse_pasted(text: &str, received_at: i64) -> TransientMessage {
    let parsed = has_headers(text)
        .then(|| mail_parser::MessageParser::default().parse(text.as_bytes()))
        .flatten();
    let source = |message_id: Option<&str>, date: Option<i64>| SourceRef {
        provider: Provider::Mock,
        account_alias: PASTED_ACCOUNT.to_string(),
        message_id: match message_id {
            Some(id) => format!("<{id}>"),
            // Pasting the same text twice yields the same ID, so it's recorded once.
            None => format!("<pasted-{:016x}@manuscripthub.invalid>", fnv1a(text.trim())),
        },
        folder: "Pasted".to_string(),
        received_at: date.unwrap_or(received_at),
    };
    match parsed {
        Some(message) => TransientMessage {
            source: source(
                message.message_id(),
                message.date().map(|d| d.to_timestamp()),
            ),
            from: message
                .from()
                .and_then(|a| a.first())
                .map(|addr| match (addr.name(), addr.address()) {
                    (Some(name), Some(address)) => format!("\"{name}\" <{address}>"),
                    (_, Some(address)) => format!("<{address}>"),
                    (Some(name), None) => name.to_string(),
                    (None, None) => String::new(),
                })
                .unwrap_or_default(),
            subject: message.subject().unwrap_or_default().to_string(),
            body: message
                .body_text(0)
                .map(|b| b.into_owned())
                .unwrap_or_default(),
        },
        None => TransientMessage {
            source: source(None, None),
            from: String::new(),
            subject: String::new(),
            body: text.to_string(),
        },
    }
}

/// A header block: the first line is a `Name: value` header and the block
/// before the first blank line names a sender or a subject.
fn has_headers(text: &str) -> bool {
    let text = text.trim_start();
    let first = text.lines().next().unwrap_or_default();
    let header_line = first.split_once(':').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    let block = text
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    header_line
        && (block.contains("\nfrom:") || block.starts_with("from:") || block.contains("subject:"))
}

/// A stable 64-bit hash (FNV-1a); std's hasher may change between releases.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_full_source_with_headers() {
        let message = parse_pasted(
            "Message-ID: <p1@jss.example.org>\nFrom: \"Journal of Synthetic Studies\" <editor@jss.example.org>\n\
             Subject: Decision on JSS-2026-0142\nDate: Thu, 29 Oct 2026 12:00:00 +0000\n\nmajor revision\n",
            0,
        );
        assert_eq!(message.source.message_id, "<p1@jss.example.org>");
        assert_eq!(message.subject, "Decision on JSS-2026-0142");
        assert_eq!(
            message.from,
            "\"Journal of Synthetic Studies\" <editor@jss.example.org>"
        );
        assert_eq!(message.source.received_at, 1_793_275_200);
        assert!(message.body.contains("major revision"));
    }

    #[test]
    fn plain_text_becomes_the_body_with_a_stable_id() {
        let text = "Dear Dr. Example,\nyour manuscript JSS-2026-0142: major revision.";
        let first = parse_pasted(text, 42);
        assert_eq!(first.body, text);
        assert_eq!(first.source.received_at, 42);
        assert!(first.subject.is_empty());
        assert_eq!(
            parse_pasted(&format!("  {text}\n"), 7).source.message_id,
            first.source.message_id
        );
        assert_ne!(
            parse_pasted("Another letter entirely.", 42)
                .source
                .message_id,
            first.source.message_id
        );
    }

    #[test]
    fn a_colon_in_the_first_line_alone_is_not_a_header() {
        let message = parse_pasted("Manuscript ID: JSS-2026-0142\n\nwas accepted.", 0);
        assert!(message.body.starts_with("Manuscript ID"));
    }
}
