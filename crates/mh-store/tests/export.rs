//! Export → wipe → import round-trips (WP 1.8).

use mh_core::{
    CalendarDate, Confidence, EditorialStatus, Field, ProposedEvent, Provider, ReviewState,
    SourceRef,
};
use mh_store::{
    open_backup, seal_backup, EventEdits, ManualTarget, ManualUpdate, RecordOutcome,
    ReviewDecision, SenderRuleKind, Store,
};

const T0: i64 = 1_793_275_200;

fn proposal(message_id: &str, at: i64, status: EditorialStatus) -> ProposedEvent {
    let field = |value: String| {
        Some(Field {
            value,
            original: "phrase".into(),
            confidence: Confidence::new(0.9),
        })
    };
    ProposedEvent {
        source: SourceRef {
            provider: Provider::Gmail,
            account_alias: "Gmail".into(),
            message_id: message_id.into(),
            folder: "INBOX".into(),
            received_at: at,
        },
        journal: field("Journal of Synthetic Studies".into()),
        manuscript_id: field("JSS-2026-0142".into()),
        title: field("Invented Methods for Example Data".into()),
        status: Some(Field {
            value: status,
            original: "sent out for review".into(),
            confidence: Confidence::new(0.7),
        }),
        deadline: None,
        requested_action: None,
        parser_version: "test".into(),
        reason: "test".into(),
        sender_domain: Some("jss.example.org".into()),
        uncertainty: Vec::new(),
        review: ReviewState::Proposed,
    }
}

fn confirm(store: &mut Store, account: i64, p: &ProposedEvent, edits: Option<EventEdits>) {
    let RecordOutcome::Proposed { event_id } = store.record_proposal(account, p).unwrap() else {
        panic!("duplicate");
    };
    store
        .review(
            event_id,
            ReviewDecision::Confirm {
                manuscript_id: None,
                edits,
            },
        )
        .unwrap();
}

/// A store with mail-confirmed, corrected and hand-entered updates, a
/// pending proposal and a sender rule.
fn populated() -> (Store, i64) {
    let mut store = Store::open_in_memory().unwrap();
    let account = store.add_account(Provider::Gmail, "Gmail", "{}").unwrap();
    confirm(
        &mut store,
        account,
        &proposal("m1", T0, EditorialStatus::Submitted),
        None,
    );
    confirm(
        &mut store,
        account,
        &proposal("m2", T0 + 86_400, EditorialStatus::Accepted),
        Some(EventEdits {
            status: Some(EditorialStatus::UnderReview),
            deadline: Some(CalendarDate::new(2026, 12, 1).unwrap()),
            requested_action: Some("Wait for the referee reports".into()),
        }),
    );
    store
        .add_manual_update(
            ManualTarget::New {
                journal: "Zeitschrift für Beispielstudien".into(),
                external_id: None,
                title: Some("Beispielhafte Studien".into()),
            },
            ManualUpdate {
                occurred_on: CalendarDate::new(2026, 10, 1).unwrap(),
                status: Some(EditorialStatus::Rejected),
                deadline: None,
                requested_action: None,
            },
        )
        .unwrap();
    store
        .record_proposal(
            account,
            &proposal("m3", T0 + 2 * 86_400, EditorialStatus::Revised),
        )
        .unwrap();
    store
        .set_sender_rule("shop.example.com", SenderRuleKind::Ignore)
        .unwrap();
    (store, account)
}

fn snapshot(store: &Store) -> Vec<String> {
    let mut lines: Vec<String> = store
        .manuscripts()
        .unwrap()
        .into_iter()
        .map(|m| {
            let steps: Vec<String> = store
                .timeline(m.id)
                .unwrap()
                .into_iter()
                .filter(|e| e.review_state != ReviewState::Proposed)
                .map(|e| {
                    format!(
                        "{} {:?} {:?} {:?} {:?} {:?} {:?}",
                        e.occurred_at,
                        e.review_state,
                        e.status,
                        e.deadline.map(|d| d.to_string()),
                        e.requested_action,
                        e.original_phrase,
                        e.source.map(|s| s.message_id)
                    )
                })
                .collect();
            format!(
                "{} {:?} {:?} {:?} {:?} | {}",
                m.journal,
                m.external_id,
                m.title,
                m.status,
                m.deadline.map(|d| d.to_string()),
                steps.join(" ; ")
            )
        })
        .collect();
    lines.sort();
    lines
}

#[test]
fn export_wipe_import_round_trips() {
    let (mut store, _) = populated();
    let before = snapshot(&store);
    let export = store.export().unwrap();
    assert_eq!(export.manuscripts.len(), 2);
    let json = serde_json::to_string(&export).unwrap();
    assert!(!json.contains("m3"), "pending proposals aren't exported");

    store.wipe().unwrap();
    assert!(store.manuscripts().unwrap().is_empty());
    assert!(store.accounts().unwrap().is_empty());
    assert!(store.sender_rules().unwrap().is_empty());

    // The account exists again before the import, as after reconnecting.
    let account = store.add_account(Provider::Gmail, "Gmail", "{}").unwrap();
    let summary = store.import(&serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(summary.manuscripts_added, 2);
    assert_eq!(summary.updates_added, 3);
    assert_eq!(snapshot(&store), before);
    assert_eq!(store.sender_rules().unwrap().len(), 1);

    // Message references came back, so a rescan doesn't propose m1 again.
    assert!(matches!(
        store
            .record_proposal(account, &proposal("m1", T0, EditorialStatus::Submitted))
            .unwrap(),
        RecordOutcome::Duplicate
    ));
}

#[test]
fn importing_twice_changes_nothing() {
    let (mut store, _) = populated();
    let export = store.export().unwrap();
    let before = snapshot(&store);
    let summary = store.import(&export).unwrap();
    assert_eq!(summary.manuscripts_added, 0);
    assert_eq!(summary.updates_added, 0);
    assert_eq!(summary.updates_skipped, 3);
    assert_eq!(snapshot(&store), before);
}

#[test]
fn backups_restore_on_another_store() {
    let (store, _) = populated();
    let file = seal_backup(&store.export().unwrap(), "a long backup password").unwrap();
    let mut elsewhere = Store::open_in_memory().unwrap();
    let export = open_backup(&file, "a long backup password").unwrap();
    elsewhere.import(&export).unwrap();
    assert_eq!(snapshot(&elsewhere).len(), 2);
}

#[test]
fn csv_has_one_row_per_manuscript() {
    let (store, _) = populated();
    let csv = store.export_csv().unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("Journal,Manuscript ID,Title,Status"));
    assert!(csv.contains(
        "Journal of Synthetic Studies,JSS-2026-0142,Invented Methods for Example Data,under_review"
    ));
}
