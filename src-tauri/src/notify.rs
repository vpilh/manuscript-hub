//! Desktop notifications (FR14): new mail to review and deadline reminders.
//!
//! Notifications carry metadata only (journal, ID, deadline, the to-do the
//! user confirmed), never text from a message.

use std::time::{SystemTime, UNIX_EPOCH};

use mh_core::CalendarDate;
use mh_store::{Reminder, ReminderStage};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

fn show(app: &AppHandle, title: &str, body: &str) {
    // A missing notification service (e.g. on a minimal Linux desktop) must
    // not stop checks, so failures are ignored.
    let _ = app.notification().builder().title(title).body(body).show();
}

/// After an automatic check found mail for the review queue.
pub fn new_mail(app: &AppHandle, count: usize) {
    let title = if count == 1 {
        "1 new journal message to review".to_string()
    } else {
        format!("{count} new journal messages to review")
    };
    show(
        app,
        &title,
        "Open Manuscript Hub to add them to your manuscripts.",
    );
}

/// The first time the window is closed while the app keeps running.
pub fn still_running(app: &AppHandle) {
    show(
        app,
        "Manuscript Hub is still running",
        "It keeps checking your mailboxes. Open or quit it from the tray icon.",
    );
}

/// Shows reminders that are due and records them, so each appears once.
pub fn deadline_reminders(app: &AppHandle) {
    let Ok(store) = app.state::<AppState>().handle() else {
        return;
    };
    let today = CalendarDate::from_unix_seconds(unix_now());
    let Ok(mut store) = store.lock() else {
        return;
    };
    let Ok(due) = store.due_reminders(today) else {
        return;
    };
    for reminder in due {
        let (title, body) = reminder_text(&reminder);
        show(app, &title, &body);
        let _ = store.mark_reminder_sent(&reminder);
    }
}

fn reminder_text(reminder: &Reminder) -> (String, String) {
    let title = match (reminder.stage, reminder.days_left) {
        (ReminderStage::Overdue, -1) => "A deadline passed yesterday".to_string(),
        (ReminderStage::Overdue, days) => format!("A deadline passed {} days ago", -days),
        (_, 0) => "A deadline is today".to_string(),
        (_, 1) => "A deadline is tomorrow".to_string(),
        (_, days) => format!("A deadline is in {days} days"),
    };
    let m = &reminder.manuscript;
    let paper = match &m.external_id {
        Some(id) => format!("{}, {id}", m.journal),
        None => m.journal.clone(),
    };
    let task = m.requested_action.as_deref().unwrap_or("Due");
    (
        title,
        format!("{paper}: {task}, {}.", long_date(reminder.deadline)),
    )
}

fn long_date(date: CalendarDate) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    format!(
        "{} {} {}",
        date.day(),
        MONTHS[usize::from(date.month()) - 1],
        date.year()
    )
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mh_store::ManuscriptSummary;

    fn reminder(days_left: i64, stage: ReminderStage) -> Reminder {
        Reminder {
            manuscript: ManuscriptSummary {
                id: 1,
                journal: "Journal of Synthetic Studies".into(),
                external_id: Some("JSS-2026-0142".into()),
                title: None,
                status: None,
                status_since: None,
                deadline: None,
                requested_action: Some("Submit revised manuscript".into()),
                pending_reviews: 0,
            },
            deadline: CalendarDate::new(2026, 11, 12).unwrap(),
            days_left,
            stage,
        }
    }

    #[test]
    fn reminder_texts_name_the_paper_and_the_task() {
        let (title, body) = reminder_text(&reminder(6, ReminderStage::Week));
        assert_eq!(title, "A deadline is in 6 days");
        assert_eq!(
            body,
            "Journal of Synthetic Studies, JSS-2026-0142: Submit revised manuscript, 12 November 2026."
        );
        assert_eq!(
            reminder_text(&reminder(1, ReminderStage::Day)).0,
            "A deadline is tomorrow"
        );
        assert_eq!(
            reminder_text(&reminder(0, ReminderStage::Day)).0,
            "A deadline is today"
        );
        assert_eq!(
            reminder_text(&reminder(-3, ReminderStage::Overdue)).0,
            "A deadline passed 3 days ago"
        );
    }
}
