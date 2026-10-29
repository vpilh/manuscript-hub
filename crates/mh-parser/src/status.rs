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
        ],
    ),
    (
        EditorialStatus::WithEditor,
        &[
            "assigned to an editor",
            "assigned to the handling editor",
            "dem herausgeber zugewiesen",
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

/// The highest-priority status phrase in `text`, with the phrase as written.
pub fn detect_status(text: &str) -> Option<(EditorialStatus, String)> {
    let lower = text.to_lowercase();
    for (status, phrases) in RULES {
        for phrase in *phrases {
            if let Some(start) = lower.find(phrase) {
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
