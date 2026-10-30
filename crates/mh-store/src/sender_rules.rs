//! The user's sender rules: domains that always or never count as journal
//! mail (FR07).

use mh_core::ProposedEvent;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{store::now, Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderRuleKind {
    /// Always journal mail, even without editorial vocabulary.
    Journal,
    /// Never journal mail; wins over every other rule.
    Ignore,
}

impl SenderRuleKind {
    fn as_str(self) -> &'static str {
        match self {
            SenderRuleKind::Journal => "journal",
            SenderRuleKind::Ignore => "ignore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenderRule {
    pub domain: String,
    pub kind: SenderRuleKind,
}

/// `alex@Mail.Example.org`, `@mail.example.org` or `mail.example.org.` →
/// `mail.example.org`.
fn normalize_domain(value: &str) -> Result<String, StoreError> {
    let domain = value
        .trim()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_lowercase();
    let valid = domain.contains('.')
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    if valid {
        Ok(domain)
    } else {
        Err(StoreError::Format(format!(
            "\"{}\" isn't a mail domain. Enter something like journal.example.org.",
            value.trim()
        )))
    }
}

fn domain_matches(domain: &str, rule: &str) -> bool {
    domain == rule || domain.ends_with(&format!(".{rule}"))
}

impl Store {
    pub fn sender_rules(&self) -> Result<Vec<SenderRule>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT domain, kind FROM sender_rules ORDER BY domain")?;
        let rows = statement.query_map([], |row| {
            let kind: String = row.get(1)?;
            Ok(SenderRule {
                domain: row.get(0)?,
                kind: if kind == "journal" {
                    SenderRuleKind::Journal
                } else {
                    SenderRuleKind::Ignore
                },
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Adds or replaces the rule for a domain. An "ignore" rule also sets
    /// aside proposals from that sender that are still waiting for review;
    /// returns how many.
    pub fn set_sender_rule(
        &mut self,
        domain: &str,
        kind: SenderRuleKind,
    ) -> Result<usize, StoreError> {
        let domain = normalize_domain(domain)?;
        self.write(|tx| {
            tx.execute(
                "INSERT INTO sender_rules (domain, kind, created_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (domain) DO UPDATE SET kind = excluded.kind",
                params![domain, kind.as_str(), now()],
            )?;
            if kind != SenderRuleKind::Ignore {
                return Ok(0);
            }
            let pending: Vec<(i64, String)> = tx
                .prepare("SELECT id, extraction_json FROM events WHERE review_state = 'proposed'")?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            let mut dismissed = 0;
            for (id, json) in pending {
                let proposal: Option<ProposedEvent> = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| serde_json::from_value(v.get("proposal")?.clone()).ok());
                let from_domain = proposal
                    .and_then(|p| p.sender_domain)
                    .is_some_and(|d| domain_matches(&d, &domain));
                if from_domain {
                    tx.execute(
                        "UPDATE events SET review_state = 'unrelated', reviewed_at = ?2 WHERE id = ?1",
                        params![id, now()],
                    )?;
                    dismissed += 1;
                }
            }
            Ok(dismissed)
        })
    }

    pub fn remove_sender_rule(&mut self, domain: &str) -> Result<(), StoreError> {
        let domain = normalize_domain(domain)?;
        self.write(|tx| {
            tx.execute(
                "DELETE FROM sender_rules WHERE domain = ?1",
                params![domain],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_addresses_and_domains() {
        assert_eq!(
            normalize_domain(" Alex@Mail.Example.org ").unwrap(),
            "mail.example.org"
        );
        assert_eq!(normalize_domain("@example.org").unwrap(), "example.org");
        assert_eq!(normalize_domain("example.org.").unwrap(), "example.org");
        assert!(normalize_domain("not a domain").is_err());
        assert!(normalize_domain("localhost").is_err());
    }
}
