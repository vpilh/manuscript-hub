//! A provider backed by synthetic `.eml` fixtures, for development and tests (FR22).

use mh_core::{Provider, SourceRef, TransientMessage};

use crate::{parse_date_time, ChangeBatch, MailProvider, ProviderError, SyncCursor};

pub struct MockProvider {
    account_alias: String,
    messages: Vec<TransientMessage>,
    batch_size: usize,
}

impl MockProvider {
    pub fn new(account_alias: impl Into<String>, messages: Vec<TransientMessage>) -> Self {
        Self {
            account_alias: account_alias.into(),
            messages,
            batch_size: 50,
        }
    }

    /// Builds a provider from raw `.eml` texts. Fixtures that cannot be parsed are skipped.
    pub fn from_eml(account_alias: impl Into<String>, emls: &[&str]) -> Self {
        let account_alias = account_alias.into();
        let messages = emls
            .iter()
            .filter_map(|eml| parse_eml(&account_alias, "INBOX", eml))
            .collect();
        Self::new(account_alias, messages)
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }
}

impl MailProvider for MockProvider {
    fn kind(&self) -> Provider {
        Provider::Mock
    }

    fn account_alias(&self) -> &str {
        &self.account_alias
    }

    fn fetch_changes(&mut self, cursor: Option<&SyncCursor>) -> Result<ChangeBatch, ProviderError> {
        let start = match cursor {
            None => 0,
            Some(SyncCursor::Mock { position }) if *position <= self.messages.len() => *position,
            Some(_) => return Err(ProviderError::CursorExpired),
        };
        let end = (start + self.batch_size).min(self.messages.len());
        Ok(ChangeBatch {
            messages: self.messages[start..end].to_vec(),
            next_cursor: SyncCursor::Mock { position: end },
            more: end < self.messages.len(),
        })
    }
}

/// Minimal RFC 5322 reader for plain-text fixtures: unfolds headers and takes
/// the body verbatim. MIME decoding belongs to the real adapters.
pub fn parse_eml(account_alias: &str, folder: &str, eml: &str) -> Option<TransientMessage> {
    let eml = eml.replace("\r\n", "\n");
    let (head, body) = eml.split_once("\n\n").unwrap_or((eml.as_str(), ""));

    let mut headers: Vec<(String, String)> = Vec::new();
    for line in head.lines() {
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };

    Some(TransientMessage {
        source: SourceRef {
            provider: Provider::Mock,
            account_alias: account_alias.to_string(),
            message_id: header("message-id")?,
            folder: folder.to_string(),
            received_at: header("date")
                .and_then(|d| parse_date_time(&d))
                .unwrap_or(0),
        },
        from: header("from").unwrap_or_default(),
        subject: header("subject").unwrap_or_default(),
        body: body.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EML: &str = "Message-ID: <1@example.org>\r\n\
                       From: Editor <ed@example.org>\r\n\
                       Subject: Decision on\r\n manuscript JSS-1\r\n\
                       Date: Thu, 29 Oct 2026 12:00:00 +0000\r\n\
                       \r\n\
                       Body text\r\n";

    #[test]
    fn parses_folded_headers() {
        let msg = parse_eml("test", "INBOX", EML).unwrap();
        assert_eq!(msg.subject, "Decision on manuscript JSS-1");
        assert_eq!(msg.source.message_id, "<1@example.org>");
        assert_eq!(msg.source.received_at, 1_793_275_200);
        assert_eq!(msg.body, "Body text\n");
    }

    #[test]
    fn pages_through_messages_with_cursor() {
        let mut provider = MockProvider::from_eml("test", &[EML, EML, EML]).with_batch_size(2);
        let first = provider.fetch_changes(None).unwrap();
        assert_eq!(first.messages.len(), 2);
        assert!(first.more);
        let second = provider.fetch_changes(Some(&first.next_cursor)).unwrap();
        assert_eq!(second.messages.len(), 1);
        assert!(!second.more);
    }

    #[test]
    fn foreign_cursor_forces_rescan() {
        let mut provider = MockProvider::from_eml("test", &[EML]);
        let cursor = SyncCursor::GmailHistory { history_id: 7 };
        assert_eq!(
            provider.fetch_changes(Some(&cursor)).unwrap_err(),
            ProviderError::CursorExpired
        );
    }
}
