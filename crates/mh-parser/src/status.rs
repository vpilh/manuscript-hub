use mh_core::EditorialStatus;

/// Seed phrase table, checked in order: decisions before process updates, so
/// "we are unable to accept" never reads as an acceptance and "thank you for
/// submitting your revised manuscript" never reads as a first submission.
/// Phrases are lowercase. The journal rule catalog (WP 1.4) will extend this.
const RULES: &[(EditorialStatus, &[&str])] = &[
    (
        EditorialStatus::Withdrawn,
        &[
            "has been withdrawn",
            "withdrawal of your manuscript",
            "zurückgezogen",
        ],
    ),
    (
        EditorialStatus::Rejected,
        &[
            "unable to accept",
            "not able to accept",
            "cannot accept",
            "decided to reject",
            "has been rejected",
            "must decline your",
            "nicht zur veröffentlichung",
            "nicht annehmen",
            "abgelehnt",
        ],
    ),
    (
        EditorialStatus::Accepted,
        &[
            "pleased to accept",
            "accepted for publication",
            "has been accepted",
            "zur veröffentlichung angenommen",
            "zur publikation angenommen",
        ],
    ),
    (
        EditorialStatus::RevisionRequired,
        &[
            "major revision",
            "minor revision",
            "revise and resubmit",
            "submit a revised",
            "revise your manuscript",
            "überarbeitung ihres manuskripts",
            "überarbeitete fassung einzureichen",
            "überarbeiten sie",
        ],
    ),
    (
        EditorialStatus::Revised,
        &[
            "submitting your revised",
            "received your revised",
            "revised manuscript has been received",
            "überarbeitete fassung erhalten",
            "überarbeitete fassung ist eingegangen",
        ],
    ),
    (
        EditorialStatus::UnderReview,
        &[
            "under review",
            "sent out for review",
            "sent to reviewers",
            "in begutachtung",
            "an die gutachter",
            "accepted for review",
            "accepted for peer review",
        ],
    ),
    (
        EditorialStatus::WithEditor,
        &[
            "assigned to an editor",
            "assigned to the handling editor",
            "dem herausgeber zugewiesen",
            "accepted for consideration",
        ],
    ),
    (
        EditorialStatus::Submitted,
        &[
            "successfully submitted",
            "received your submission",
            "has been submitted",
            "thank you for submitting",
            "erfolgreich eingereicht",
            "eingang ihres manuskripts",
        ],
    ),
];

/// Words that make the rest of their clause hypothetical: "if accepted for
/// publication", "once your manuscript has been accepted", "falls angenommen".
const CONDITIONALS: &[&str] = &[
    "if", "once", "when", "whether", "unless", "until", "before", "upon", "should", "would",
    "wenn", "falls", "sobald", "ob", "bevor",
];

/// Plural subjects that make a phrase general advice rather than news about
/// this manuscript: "articles accepted for publication are …".
const GENERIC_SUBJECTS: &[&str] = &[
    "articles",
    "papers",
    "manuscripts",
    "submissions",
    "contributions",
    "beiträge",
    "artikel",
];

/// Generic phrases that only count when their clause is about the
/// manuscript: a PDF, a declaration or a payment "has been accepted" too.
const GENERIC_PHRASES: &[&str] = &[
    "has been withdrawn",
    "has been rejected",
    "has been accepted",
    "has been submitted",
];

const MANUSCRIPT_NOUNS: &[&str] = &[
    "manuscript",
    "paper",
    "article",
    "submission",
    "contribution",
    "manuskript",
    "beitrag",
    "artikel",
];

/// What follows "has been accepted" when it isn't a decision.
const NOT_A_DECISION: &[&str] = &[
    " for review",
    " for peer review",
    " for consideration",
    " for evaluation",
    " for further review",
];

/// The words of the clause before `start`: back to the last sentence mark,
/// at most 120 bytes.
fn clause_words(lower: &str, start: usize) -> Vec<&str> {
    let mut from = lower[..start]
        .rfind(['.', '!', '?', ';', ':', '\n'])
        .map_or(0, |i| i + 1)
        .max(start.saturating_sub(120));
    while !lower.is_char_boundary(from) {
        from += 1;
    }
    lower[from..start]
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

/// True when the phrase starting at `start` sits in a conditional or generic
/// clause. Submission confirmations are full of such boilerplate.
fn is_hypothetical(lower: &str, start: usize) -> bool {
    let words = clause_words(lower, start);
    words.iter().any(|w| CONDITIONALS.contains(w))
        || words.last().is_some_and(|w| GENERIC_SUBJECTS.contains(w))
}

/// For a generic phrase at `start..end`: its clause names the manuscript, and
/// it isn't "accepted for review" or the like.
fn is_about_manuscript(lower: &str, start: usize, end: usize) -> bool {
    clause_words(lower, start)
        .iter()
        .any(|w| MANUSCRIPT_NOUNS.contains(w))
        && !NOT_A_DECISION.iter().any(|n| lower[end..].starts_with(n))
}

/// The highest-priority status phrase in `text` that states a fact (not a
/// condition), with the phrase as written.
pub fn detect_status(text: &str) -> Option<(EditorialStatus, String)> {
    let lower = text.to_lowercase();
    for (status, phrases) in RULES {
        for phrase in *phrases {
            let found = lower
                .match_indices(phrase)
                .map(|(start, _)| start)
                .find(|&start| {
                    !is_hypothetical(&lower, start)
                        && (!GENERIC_PHRASES.contains(phrase)
                            || is_about_manuscript(&lower, start, start + phrase.len()))
                });
            if let Some(start) = found {
                let end = start + phrase.len();
                // Lowercasing can change byte lengths for some scripts; fall back to the rule text.
                let original = if lower.len() == text.len()
                    && text.is_char_boundary(start)
                    && text.is_char_boundary(end)
                {
                    text[start..end].to_string()
                } else {
                    (*phrase).to_string()
                };
                return Some((*status, original));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(text: &str) -> Option<EditorialStatus> {
        detect_status(text).map(|(s, _)| s)
    }

    #[test]
    fn conditional_boilerplate_is_not_a_decision() {
        let confirmation = "Your manuscript JBF-D-26-01235 has been successfully submitted. \
            By submitting, you confirm that the work is original and that, if accepted for \
            publication, it will not be published elsewhere. Once your manuscript has been \
            accepted, you will receive proofs. Articles accepted for publication appear online \
            first.";
        assert_eq!(status(confirmation), Some(EditorialStatus::Submitted));
        assert_eq!(
            status(
                "Falls Ihr Beitrag zur Veröffentlichung angenommen wird, erhalten Sie Fahnen. \
                    Ihr Manuskript wurde erfolgreich eingereicht."
            ),
            Some(EditorialStatus::Submitted)
        );
    }

    #[test]
    fn generic_phrases_need_the_manuscript_as_subject() {
        assert_eq!(status("Your PDF has been accepted. Thank you."), None);
        assert_eq!(
            status("Your declaration has been accepted. Thank you for submitting your work."),
            Some(EditorialStatus::Submitted)
        );
        assert_eq!(
            status("Your manuscript has been accepted."),
            Some(EditorialStatus::Accepted)
        );
        assert_eq!(
            status("Your submission has been accepted for consideration."),
            Some(EditorialStatus::WithEditor)
        );
        assert_eq!(
            status("Your paper has been accepted for review."),
            Some(EditorialStatus::UnderReview)
        );
    }

    #[test]
    fn a_real_decision_after_boilerplate_still_counts() {
        assert_eq!(
            status("If you have questions, contact us. Your manuscript has been accepted for publication."),
            Some(EditorialStatus::Accepted)
        );
    }

    #[test]
    fn decisions_take_priority_over_process_phrases() {
        assert_eq!(
            status("Thank you for submitting to JSS. Unfortunately we are unable to accept it."),
            Some(EditorialStatus::Rejected)
        );
        assert_eq!(
            status("Thank you for submitting your revised manuscript."),
            Some(EditorialStatus::Revised)
        );
        assert_eq!(
            status("Ihr Beitrag wurde leider nicht zur Veröffentlichung angenommen."),
            Some(EditorialStatus::Rejected)
        );
    }

    #[test]
    fn recognizes_german_phrases() {
        assert_eq!(
            status("Ihr Manuskript wurde zur Veröffentlichung angenommen."),
            Some(EditorialStatus::Accepted)
        );
        assert_eq!(
            status("Das Manuskript befindet sich in Begutachtung."),
            Some(EditorialStatus::UnderReview)
        );
    }

    #[test]
    fn returns_original_phrase() {
        let (_, original) = detect_status("We are PLEASED TO ACCEPT your paper").unwrap();
        assert_eq!(original, "PLEASED TO ACCEPT");
    }

    #[test]
    fn ignores_unrelated_text() {
        assert_eq!(status("Our autumn newsletter is here"), None);
    }
}
