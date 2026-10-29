use mh_core::{
    CalendarDate, Confidence, EditorialStatus, Field, ProposedEvent, ReviewState, TransientMessage,
};

use crate::{
    detect_status, find_dates, find_manuscript_ids, find_relative_deadline_days, strip_quoted,
};

/// Recorded on every proposal for provenance (PRD §6).
pub const PARSER_VERSION: &str = "seed-0.1";

const DEADLINE_CUES: &[&str] = &[
    "due",
    "deadline",
    "by ",
    "no later than",
    "frist",
    "bis zum",
    "spätestens",
];

const BULK_MAIL_MARKERS: &[&str] = &["unsubscribe", "abmelden", "newsletter"];

/// Which messages are worth parsing (FR07). User-defined rules extend the defaults.
#[derive(Debug, Clone)]
pub struct CandidateRules {
    /// Lowercase sender domains; subdomains match too.
    pub sender_domains: Vec<String>,
    /// Lowercase subject keywords.
    pub subject_keywords: Vec<String>,
}

impl Default for CandidateRules {
    fn default() -> Self {
        let keywords = [
            "manuscript",
            "submission",
            "decision",
            "revision",
            "manuskript",
            "einreichung",
            "entscheidung",
            "begutachtung",
            "überarbeitung",
        ];
        Self {
            sender_domains: Vec::new(),
            subject_keywords: keywords.map(String::from).to_vec(),
        }
    }
}

impl CandidateRules {
    fn matches(&self, message: &TransientMessage, has_manuscript_id: bool) -> bool {
        let subject = message.subject.to_lowercase();
        let domain = sender_domain(&message.from);
        let known_sender = self.sender_domains.iter().any(|d| {
            domain
                .as_deref()
                .is_some_and(|s| s == d || s.ends_with(&format!(".{d}")))
        });
        known_sender
            || has_manuscript_id
            || self
                .subject_keywords
                .iter()
                .any(|k| subject.contains(k.as_str()))
    }
}

fn sender_domain(from: &str) -> Option<String> {
    let (_, domain) = from.rsplit_once('@')?;
    let domain = domain.trim_end_matches(|c: char| c == '>' || c.is_whitespace());
    (!domain.is_empty()).then(|| domain.to_lowercase())
}

/// `"Journal of X" <editor@x.example>` → `Journal of X`
fn sender_display_name(from: &str) -> Option<String> {
    let (name, _) = from.split_once('<')?;
    let name = name.trim().trim_matches('"').trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn field<T>(value: T, original: impl Into<String>, confidence: f32) -> Field<T> {
    Field {
        value,
        original: original.into(),
        confidence: Confidence::new(confidence),
    }
}

/// Turns one message into a proposal for the review queue, or `None` if it is
/// not editorial correspondence. Nothing here is applied automatically.
pub fn extract(message: &TransientMessage, rules: &CandidateRules) -> Option<ProposedEvent> {
    let body = strip_quoted(&message.body);
    let searchable = format!("{}\n{}", message.subject, body);
    let ids = find_manuscript_ids(&searchable);
    let status = detect_status(&searchable);

    if !rules.matches(message, !ids.is_empty()) {
        return None;
    }
    let lower_body = body.to_lowercase();
    if ids.is_empty() && BULK_MAIL_MARKERS.iter().any(|m| lower_body.contains(m)) {
        return None;
    }
    if ids.is_empty() && status.is_none() {
        return None;
    }

    let mut uncertainty = Vec::new();

    let manuscript_id = match ids.as_slice() {
        [] => {
            uncertainty.push("No manuscript ID found.".to_string());
            None
        }
        [only] => Some(field(only.clone(), only.clone(), 0.9)),
        [first, ..] => {
            uncertainty.push(format!(
                "Message mentions {} possible manuscript IDs.",
                ids.len()
            ));
            Some(field(first.clone(), first.clone(), 0.4))
        }
    };

    let status = match status {
        Some((value, original)) => Some(field(value, original, 0.7)),
        None => {
            uncertainty.push("No editorial status recognized.".to_string());
            None
        }
    };

    let deadline = find_deadline(&body, message.source.received_at, &mut uncertainty);

    let requested_action = match &status {
        Some(Field {
            value: EditorialStatus::RevisionRequired,
            original,
            confidence,
        }) => Some(Field {
            value: "Submit revised manuscript".to_string(),
            original: original.clone(),
            confidence: *confidence,
        }),
        _ => None,
    };

    Some(ProposedEvent {
        source: message.source.clone(),
        journal: sender_display_name(&message.from).map(|name| field(name.clone(), name, 0.5)),
        manuscript_id,
        title: None,
        status,
        deadline,
        requested_action,
        parser_version: PARSER_VERSION.to_string(),
        uncertainty,
        review: ReviewState::Proposed,
    })
}

fn find_deadline(
    body: &str,
    received_at: i64,
    uncertainty: &mut Vec<String>,
) -> Option<Field<CalendarDate>> {
    let mut candidates = Vec::new();
    for line in body.lines() {
        let lower = line.to_lowercase();
        if DEADLINE_CUES.iter().any(|cue| lower.contains(cue)) {
            for found in find_dates(line) {
                if !candidates.iter().any(|(d, _)| *d == found.date) {
                    candidates.push((found.date, found.original));
                }
            }
        }
    }

    if let Some((date, original)) = candidates.first().cloned() {
        if candidates.len() > 1 {
            uncertainty.push(format!(
                "Message contains {} different deadline dates.",
                candidates.len()
            ));
            return Some(field(date, original, 0.4));
        }
        return Some(field(date, original, 0.8));
    }

    let days = find_relative_deadline_days(body)?;
    uncertainty.push(format!(
        "Deadline computed as {days} days after the received date (UTC)."
    ));
    let date = CalendarDate::from_unix_seconds(received_at).add_days(i64::from(days));
    Some(field(date, format!("within {days} days"), 0.5))
}

#[cfg(test)]
mod tests {
    use mh_core::{Provider, SourceRef};

    use super::*;

    fn message(from: &str, subject: &str, body: &str) -> TransientMessage {
        TransientMessage {
            source: SourceRef {
                provider: Provider::Mock,
                account_alias: "test".into(),
                message_id: "1".into(),
                folder: "INBOX".into(),
                // 2026-10-29T12:00:00Z
                received_at: 1_793_275_200,
            },
            from: from.into(),
            subject: subject.into(),
            body: body.into(),
        }
    }

    const EDITOR: &str = "\"Journal of Synthetic Studies\" <editorial@jss.example.org>";

    #[test]
    fn extracts_revision_request_with_deadline() {
        let msg = message(
            EDITOR,
            "Decision on manuscript JSS-2026-0142",
            "Dear Dr. Example,\nthe reviewers recommend a major revision.\n\
             Please upload your revision by 12 November 2026.",
        );
        let event = extract(&msg, &CandidateRules::default()).unwrap();
        assert_eq!(
            event.status.unwrap().value,
            EditorialStatus::RevisionRequired
        );
        assert_eq!(event.manuscript_id.unwrap().value, "JSS-2026-0142");
        assert_eq!(event.deadline.unwrap().value.to_string(), "2026-11-12");
        assert_eq!(event.journal.unwrap().value, "Journal of Synthetic Studies");
        assert!(event.requested_action.is_some());
        assert!(event.uncertainty.is_empty());
        assert_eq!(event.review, ReviewState::Proposed);
    }

    #[test]
    fn resolves_relative_deadline_and_flags_it() {
        let msg = message(
            EDITOR,
            "Manuscript JSS-2026-0142",
            "Please submit a revised version within 14 days.",
        );
        let event = extract(&msg, &CandidateRules::default()).unwrap();
        assert_eq!(event.deadline.unwrap().value.to_string(), "2026-11-12");
        assert_eq!(event.uncertainty.len(), 1);
    }

    #[test]
    fn ignores_newsletters() {
        let msg = message(
            EDITOR,
            "Submission tips for authors",
            "Read our newsletter. Click to unsubscribe.",
        );
        assert!(extract(&msg, &CandidateRules::default()).is_none());
    }

    #[test]
    fn quoted_decisions_do_not_count() {
        let msg = message(
            EDITOR,
            "Re: manuscript JSS-2026-0142",
            "Thanks, noted.\n> We are pleased to accept your manuscript.",
        );
        let event = extract(&msg, &CandidateRules::default()).unwrap();
        assert!(event.status.is_none());
    }

    #[test]
    fn flags_messages_about_several_manuscripts() {
        let msg = message(
            EDITOR,
            "Your submissions",
            "JSS-2026-0142 and JSS-2026-0177 are now under review.",
        );
        let event = extract(&msg, &CandidateRules::default()).unwrap();
        assert_eq!(event.manuscript_id.unwrap().confidence.value(), 0.4);
        assert!(!event.uncertainty.is_empty());
    }

    #[test]
    fn known_sender_domains_match_subdomains() {
        let rules = CandidateRules {
            sender_domains: vec!["example.org".into()],
            subject_keywords: Vec::new(),
        };
        let msg = message(EDITOR, "Hello", "Your paper is under review.");
        assert!(extract(&msg, &rules).is_some());
    }
}
