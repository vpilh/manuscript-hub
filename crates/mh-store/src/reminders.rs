//! Deadline reminders (FR14): a week before, a day before, and once overdue.
//! Each stage is shown once per manuscript and deadline; a new deadline
//! starts over.

use mh_core::CalendarDate;
use rusqlite::params;
use serde::Serialize;

use crate::{store::now, ManuscriptSummary, Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStage {
    Week,
    Day,
    Overdue,
}

impl ReminderStage {
    fn as_str(self) -> &'static str {
        match self {
            ReminderStage::Week => "week",
            ReminderStage::Day => "day",
            ReminderStage::Overdue => "overdue",
        }
    }

    /// The stage a deadline `days_left` days away is in, if any.
    pub fn for_days_left(days_left: i64) -> Option<Self> {
        match days_left {
            ..=-1 => Some(ReminderStage::Overdue),
            0..=1 => Some(ReminderStage::Day),
            2..=7 => Some(ReminderStage::Week),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Reminder {
    pub manuscript: ManuscriptSummary,
    pub deadline: CalendarDate,
    pub days_left: i64,
    pub stage: ReminderStage,
}

impl Store {
    /// Reminders due on `today` that haven't been shown. Only the current
    /// stage counts: a deadline first seen two days before it is due gets the
    /// "day" reminder, not a late "week" one.
    pub fn due_reminders(&self, today: CalendarDate) -> Result<Vec<Reminder>, StoreError> {
        let mut due = Vec::new();
        for manuscript in self.manuscripts()? {
            let Some(deadline) = manuscript.deadline else {
                continue;
            };
            let days_left = deadline.days_since_epoch() - today.days_since_epoch();
            let Some(stage) = ReminderStage::for_days_left(days_left) else {
                continue;
            };
            let sent: bool = self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM reminders_sent
                 WHERE manuscript_id = ?1 AND deadline = ?2 AND stage = ?3)",
                params![manuscript.id, deadline.to_string(), stage.as_str()],
                |row| row.get(0),
            )?;
            if !sent {
                due.push(Reminder {
                    manuscript,
                    deadline,
                    days_left,
                    stage,
                });
            }
        }
        Ok(due)
    }

    pub fn mark_reminder_sent(&mut self, reminder: &Reminder) -> Result<(), StoreError> {
        self.write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO reminders_sent (manuscript_id, deadline, stage, sent_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    reminder.manuscript.id,
                    reminder.deadline.to_string(),
                    reminder.stage.as_str(),
                    now()
                ],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_by_days_left() {
        assert_eq!(ReminderStage::for_days_left(8), None);
        assert_eq!(ReminderStage::for_days_left(7), Some(ReminderStage::Week));
        assert_eq!(ReminderStage::for_days_left(2), Some(ReminderStage::Week));
        assert_eq!(ReminderStage::for_days_left(1), Some(ReminderStage::Day));
        assert_eq!(ReminderStage::for_days_left(0), Some(ReminderStage::Day));
        assert_eq!(
            ReminderStage::for_days_left(-1),
            Some(ReminderStage::Overdue)
        );
    }
}
