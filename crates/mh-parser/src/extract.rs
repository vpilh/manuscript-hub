use mh_core::{
    CalendarDate, Confidence, EditorialStatus, Field, ProposedEvent, ReviewState, TransientMessage,
};

use crate::{
    detect_status, find_dates, find_manuscript_ids, find_relative_deadline_days, find_title,
    strip_quoted,
};

/// Recorded on every proposal for provenance (PRD §6).
pub const PARSER_VERSION: &str = "seed-0.7";

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

/// Editorial systems and publishers whose mail is worth reading even without
/// editorial vocabulary. Subdomains match too.
const KNOWN_SENDER_DOMAINS: &[&str] = &[
    "editorialmanager.com",
    "manuscriptcentral.com",
    "scholarone.com",
    "ejournalpress.com",
    "editorialexpress.com",
    "msubmit.net",
    "elsevier.com",
    "springernature.com",
    "springer.com",
    "nature.com",
    "wiley.com",
    "tandf.co.uk",
    "tandfonline.com",
    "sagepub.com",
    "oup.com",
    "cambridge.org",
    "degruyter.com",
    "emerald.com",
    "mdpi.com",
    "frontiersin.org",
    "plos.org",
    "aeaweb.org",
    "informs.org",
];

/// Words that only editorial correspondence uses. Generic words such as
/// "submission", "decision" or "reviewer" also appear in shop, job and
/// software mail, so they don't count on their own.
const EDITORIAL_TERMS: &[&str] = &[
    "manuscript",
    "manuskript",
    "editorial office",
    "editor-in-chief",
    "handling editor",
    "associate editor",
    "referee report",
    "reviewer comments",
    "reviewers' comments",
    "comments from the reviewers",
    "peer review",
    "peer-review",
    "your submission to",
    "submitted to the journal",
    "begutachtung",
    "gutachten",
    "herausgeber",
    "schriftleitung",
];

/// Sender names of submission systems, which name the system, not the journal.
const PLATFORM_SENDER_NAMES: &[&str] = &[
    "elsevier",
    "editorial manager",
    "scholarone",
    "manuscript central",
    "springer nature",
    "track your",
    "submission system",
    "no-reply",
    "noreply",
    "do not reply",
    "do-not-reply",
];

/// Which messages are worth parsing (FR07). User-defined rules extend the defaults.
#[derive(Debug, Clone)]
pub struct CandidateRules {
    /// Lowercase sender domains; subdomains match too.
    pub sender_domains: Vec<String>,
    /// Lowercase phrases that mark editorial correspondence, searched in the
    /// subject and the unquoted body.
    pub editorial_terms: Vec<String>,
    /// Lowercase addresses of the mailbox owner. Their own messages (such as
    /// Gmail's sent mail in All Mail) are never proposals.
    pub own_addresses: Vec<String>,
    /// Lowercase domains the user marked as journal mail; subdomains match too.
    pub trusted_domains: Vec<String>,
    /// Lowercase domains the user never wants proposals from; these win over
    /// every other rule.
    pub blocked_domains: Vec<String>,
}

impl Default for CandidateRules {
    fn default() -> Self {
        Self {
            sender_domains: KNOWN_SENDER_DOMAINS.iter().map(|d| d.to_string()).collect(),
            editorial_terms: EDITORIAL_TERMS.iter().map(|t| t.to_string()).collect(),
            own_addresses: Vec::new(),
            trusted_domains: Vec::new(),
            blocked_domains: Vec::new(),
        }
    }
}

/// `domain` is `rule` or one of its subdomains.
fn domain_matches(domain: &str, rule: &str) -> bool {
    domain == rule || domain.ends_with(&format!(".{rule}"))
}

/// The lowercase domain of the sender's address, if it has one.
pub fn sender_domain(from: &str) -> Option<String> {
    let address = sender_address(from)?;
    let (_, domain) = address.rsplit_once('@')?;
    (!domain.is_empty()).then(|| domain.to_string())
}

impl CandidateRules {
    /// Why the message counts as editorial mail, or `None` if it doesn't.
    /// Mail from a known editorial sender needs an ID or a status. Any other
    /// mail needs editorial vocabulary *and* a manuscript ID, because order
    /// numbers and phrases like "has been submitted" are everywhere.
    fn reason(
        &self,
        message: &TransientMessage,
        searchable: &str,
        first_id: Option<&str>,
        has_status: bool,
    ) -> Option<String> {
        let address = sender_address(&message.from);
        if address
            .as_deref()
            .is_some_and(|a| self.own_addresses.iter().any(|own| own == a))
        {
            return None;
        }
        let domain = address
            .as_deref()
            .and_then(|a| a.rsplit_once('@'))
            .map(|(_, d)| d);
        let matching = |rules: &[String]| -> Option<String> {
            let domain = domain?;
            rules.iter().find(|r| domain_matches(domain, r)).cloned()
        };
        if matching(&self.blocked_domains).is_some() {
            return None;
        }
        if let Some(trusted) = matching(&self.trusted_domains) {
            return (first_id.is_some() || has_status)
                .then(|| format!("Sent from {trusted}, which you marked as journal mail."));
        }
        if let Some(known) = matching(&self.sender_domains) {
            return (first_id.is_some() || has_status)
                .then(|| format!("Sent from {known}, a known journal or editorial system."));
        }
        let lower = searchable.to_lowercase();
        let id = first_id?;
        let term = self
            .editorial_terms
            .iter()
            .find(|t| lower.contains(t.as_str()))?;
        Some(format!(
            "Mentions the ID {id} together with the word {term}."
        ))
    }
}

/// `"Journal of X" <Editor@X.example>` → `editor@x.example`
fn sender_address(from: &str) -> Option<String> {
    let address = match from.rsplit_once('<') {
        Some((_, rest)) => rest.trim_end_matches(|c: char| c == '>' || c.is_whitespace()),
        None => from.trim(),
    };
    address.contains('@').then(|| address.to_lowercase())
}

/// `"Journal of X" <editor@x.example>` → `Journal of X`
fn sender_display_name(from: &str) -> Option<String> {
    let (name, _) = from.split_once('<')?;
    let name = name.trim().trim_matches('"').trim();
    (!name.is_empty()).then(|| name.to_string())
}

const TITLE_WORDS: &[&str] = &[
    "journal",
    "zeitschrift",
    "review",
    "annals",
    "quarterly",
    "proceedings",
    "transactions",
    "letters",
];

fn looks_like_journal_title(name: &str) -> bool {
    name.split_whitespace()
        .any(|w| TITLE_WORDS.contains(&w.to_lowercase().as_str()))
}

/// The journal, best evidence first: a sender name that reads like a journal
/// title, a title such as "Journal of Banking and Finance" in the text, then
/// any sender name that isn't a submission system's.
fn find_journal(from: &str, searchable: &str) -> Option<Field<String>> {
    let sender = sender_display_name(from);
    if let Some(name) = sender.as_deref().filter(|n| looks_like_journal_title(n)) {
        let name = name.trim_end_matches("Editorial Office").trim();
        return Some(field(name.to_string(), name, 0.6));
    }
    if let Some(title) = find_journal_title(searchable) {
        return Some(field(title.clone(), title, 0.5));
    }
    let name = sender?;
    let lower = name.to_lowercase();
    if PLATFORM_SENDER_NAMES.iter().any(|p| lower.contains(p)) {
        return None;
    }
    Some(field(name.clone(), name, 0.3))
}

/// Finds "Journal of …", "Zeitschrift für …" and similar titles: the title
/// word, a connector, then capitalized words (joined by "and", "&", "of", …).
fn find_journal_title(text: &str) -> Option<String> {
    const HEADS: &[&str] = &[
        "Journal",
        "Zeitschrift",
        "Annals",
        "Review",
        "Proceedings",
        "Transactions",
    ];
    const CONNECTORS: &[&str] = &["of", "for", "für", "on", "in"];
    const JOINERS: &[&str] = &["and", "&", "of", "the", "for", "in", "und", "für"];
    const ENDINGS: &[char] = &['.', ',', ':', ';', ')', '"', '\''];

    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, pair) in words.windows(2).enumerate() {
        if !HEADS.contains(&pair[0]) || !CONNECTORS.contains(&pair[1]) {
            continue;
        }
        // Up to two capitalized words just before the title word belong to
        // it, as in "Quarterly Journal of Economics".
        let mut first = i;
        while first > 0 && i - first < 2 {
            let previous = words[first - 1];
            if previous.starts_with(char::is_uppercase)
                && !previous.ends_with(ENDINGS)
                && previous != "The"
            {
                first -= 1;
            } else {
                break;
            }
        }
        let mut title: Vec<&str> = words[first..=i + 1].to_vec();
        for word in &words[i + 2..] {
            let clean = word.trim_end_matches(ENDINGS);
            if !clean.starts_with(char::is_uppercase) && !JOINERS.contains(&clean) {
                break;
            }
            title.push(clean);
            if clean.len() != word.len() {
                break;
            }
        }
        while title.last().is_some_and(|w| JOINERS.contains(w)) {
            title.pop();
        }
        if title.len() > 2 {
            return Some(title.join(" "));
        }
    }
    None
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

    let reason = rules.reason(
        message,
        &searchable,
        ids.first().map(String::as_str),
        status.is_some(),
    )?;
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
        journal: find_journal(&message.from, &searchable),
        manuscript_id,
        title: find_title(&body).map(|t| field(t.clone(), t, 0.7)),
        status,
        deadline,
        requested_action,
        parser_version: PARSER_VERSION.to_string(),
        reason,
        sender_domain: sender_domain(&message.from),
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
            "Manuscripts JSS-2026-0142 and JSS-2026-0177 are now under review.",
        );
        let event = extract(&msg, &CandidateRules::default()).unwrap();
        assert_eq!(event.manuscript_id.unwrap().confidence.value(), 0.4);
        assert!(!event.uncertainty.is_empty());
    }

    #[test]
    fn known_sender_domains_match_subdomains() {
        let rules = CandidateRules {
            sender_domains: vec!["example.org".into()],
            ..CandidateRules::default()
        };
        let msg = message(EDITOR, "Hello", "Your paper is under review.");
        assert!(extract(&msg, &rules).is_some());
        assert!(extract(&msg, &CandidateRules::default()).is_none());
    }

    #[test]
    fn everyday_mail_with_references_is_not_journal_mail() {
        for (from, subject, body) in [
            (
                "\"Shop\" <orders@shop.example.com>",
                "Your order ORD-2026-55123 has been submitted",
                "Thank you, your order has been submitted and is under review by our team.",
            ),
            (
                "\"Careers\" <jobs@company.example.com>",
                "Decision on your application",
                "Your application REQ-2026-0042 has been accepted for the next round.",
            ),
            (
                "\"Tracker\" <issues@tracker.example.com>",
                "PROJ-12345 needs review",
                "Reviewer requested on PROJ-12345. The change has been submitted.",
            ),
        ] {
            let msg = message(from, subject, body);
            assert!(
                extract(&msg, &CandidateRules::default()).is_none(),
                "{subject}"
            );
        }
    }

    #[test]
    fn editorial_systems_count_without_vocabulary() {
        let msg = message(
            "\"Track your Elsevier submission\" <no-reply@track.elsevier.com>",
            "Status update JBF-D-26-01235",
            "Your submission to Journal of Banking and Finance is now under review.",
        );
        let event = extract(&msg, &CandidateRules::default()).unwrap();
        assert_eq!(event.manuscript_id.unwrap().value, "JBF-D-26-01235");
        assert_eq!(
            event.journal.unwrap().value,
            "Journal of Banking and Finance"
        );
        assert_eq!(
            event.reason,
            "Sent from elsevier.com, a known journal or editorial system."
        );
    }

    #[test]
    fn user_rules_trust_and_block_senders() {
        let small_journal = message(
            "\"Small Journal\" <office@small-journal.example.org>",
            "Your paper SJ-2026-0042",
            "Your paper SJ-2026-0042 is under review.",
        );
        assert!(extract(&small_journal, &CandidateRules::default()).is_none());
        let trusting = CandidateRules {
            trusted_domains: vec!["small-journal.example.org".into()],
            ..CandidateRules::default()
        };
        let event = extract(&small_journal, &trusting).unwrap();
        assert_eq!(
            event.reason,
            "Sent from small-journal.example.org, which you marked as journal mail."
        );
        assert_eq!(
            event.sender_domain.as_deref(),
            Some("small-journal.example.org")
        );

        let blocking = CandidateRules {
            blocked_domains: vec!["example.org".into()],
            ..CandidateRules::default()
        };
        let editor = message(
            EDITOR,
            "Decision on manuscript JSS-2026-0142",
            "major revision",
        );
        assert!(extract(&editor, &CandidateRules::default()).is_some());
        assert!(extract(&editor, &blocking).is_none(), "blocking wins");
    }

    #[test]
    fn own_messages_are_skipped() {
        let rules = CandidateRules {
            own_addresses: vec!["alex@example.org".into()],
            ..CandidateRules::default()
        };
        let msg = message(
            "Alex Example <Alex@example.org>",
            "Fwd: manuscript JSS-2026-0142",
            "FYI, the manuscript is under review.",
        );
        assert!(extract(&msg, &rules).is_none());
        assert!(extract(&msg, &CandidateRules::default()).is_some());
    }

    #[test]
    fn finds_journal_titles_in_text() {
        let title = |text| find_journal_title(text);
        assert_eq!(
            title("submitted to the Quarterly Journal of Economics. Thanks").as_deref(),
            Some("Quarterly Journal of Economics")
        );
        assert_eq!(
            title("Ihr Beitrag für die Zeitschrift für Synthetische Studien wurde").as_deref(),
            Some("Zeitschrift für Synthetische Studien")
        );
        assert_eq!(
            title("Journal of Banking & Finance, Elsevier").as_deref(),
            Some("Journal of Banking & Finance")
        );
        assert_eq!(title("see the journal of record"), None);
    }

    #[test]
    fn journal_name_prefers_titles_over_people_and_systems() {
        let journal = find_journal(
            "Alex Example <alex@example.org>",
            "Your manuscript for the Journal of Synthetic Studies",
        );
        assert_eq!(journal.unwrap().value, "Journal of Synthetic Studies");
        let journal = find_journal("Alex Example <alex@example.org>", "no title here");
        assert_eq!(journal.unwrap().confidence.value(), 0.3);
        assert!(find_journal("\"Elsevier\" <no-reply@elsevier.com>", "no title").is_none());
    }
}
