use serde::{Deserialize, Serialize};

/// Canonical editorial statuses (PRD §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorialStatus {
    Submitted,
    WithEditor,
    UnderReview,
    RevisionRequired,
    Revised,
    Accepted,
    Rejected,
    Withdrawn,
}

impl EditorialStatus {
    pub const ALL: [EditorialStatus; 8] = [
        EditorialStatus::Submitted,
        EditorialStatus::WithEditor,
        EditorialStatus::UnderReview,
        EditorialStatus::RevisionRequired,
        EditorialStatus::Revised,
        EditorialStatus::Accepted,
        EditorialStatus::Rejected,
        EditorialStatus::Withdrawn,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EditorialStatus::Submitted => "submitted",
            EditorialStatus::WithEditor => "with_editor",
            EditorialStatus::UnderReview => "under_review",
            EditorialStatus::RevisionRequired => "revision_required",
            EditorialStatus::Revised => "revised",
            EditorialStatus::Accepted => "accepted",
            EditorialStatus::Rejected => "rejected",
            EditorialStatus::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }

    /// Terminal statuses end a submission; a later event belongs to a resubmission chain.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            EditorialStatus::Accepted | EditorialStatus::Rejected | EditorialStatus::Withdrawn
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_str() {
        for status in EditorialStatus::ALL {
            assert_eq!(EditorialStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(EditorialStatus::parse("in_press"), None);
    }

    #[test]
    fn only_decisions_and_withdrawal_are_terminal() {
        let terminal: Vec<_> = EditorialStatus::ALL
            .into_iter()
            .filter(|s| s.is_terminal())
            .collect();
        assert_eq!(
            terminal,
            [
                EditorialStatus::Accepted,
                EditorialStatus::Rejected,
                EditorialStatus::Withdrawn
            ]
        );
    }
}
