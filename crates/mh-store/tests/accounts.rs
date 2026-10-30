//! Connected accounts and sync cursors (WP 2.2).

use mh_core::{
    Confidence, EditorialStatus, Field, ProposedEvent, Provider, ReviewState, SourceRef,
};
use mh_store::{RecordOutcome, ReviewDecision, Store};

fn proposal(message_id: &str) -> ProposedEvent {
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
            received_at: 1_793_275_200,
        },
        journal: field("Journal of Synthetic Studies".into()),
        manuscript_id: field(format!("JSS-{message_id}")),
        title: None,
        status: Some(Field {
            value: EditorialStatus::UnderReview,
            original: "under review".into(),
            confidence: Confidence::new(0.9),
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

fn event_id(outcome: RecordOutcome) -> i64 {
    match outcome {
        RecordOutcome::Proposed { event_id } => event_id,
        RecordOutcome::Duplicate => panic!("unexpected duplicate"),
    }
}

#[test]
fn account_names_are_unique_per_provider() {
    let mut store = Store::open_in_memory().unwrap();
    store.add_account(Provider::Gmail, "Gmail", "{}").unwrap();
    let error = store
        .add_account(Provider::Gmail, "Gmail", "{}")
        .unwrap_err();
    assert!(error.to_string().contains("already connected"));
    store
        .add_account(Provider::GroupwiseImap, "Gmail", "{}")
        .unwrap();
    assert_eq!(store.accounts().unwrap().len(), 2);
}

#[test]
fn cursors_are_kept_per_folder() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store.add_account(Provider::Gmail, "Gmail", "{}").unwrap();
    assert_eq!(store.sync_cursor(id, "INBOX").unwrap(), None);
    store.save_sync_cursor(id, "INBOX", "{\"a\":1}").unwrap();
    store.save_sync_cursor(id, "INBOX", "{\"a\":2}").unwrap();
    store.save_sync_cursor(id, "Archive", "{\"b\":1}").unwrap();
    assert_eq!(
        store.sync_cursor(id, "INBOX").unwrap().as_deref(),
        Some("{\"a\":2}")
    );
    store.clear_sync_cursor(id, "INBOX").unwrap();
    assert_eq!(store.sync_cursor(id, "INBOX").unwrap(), None);
    assert!(store.sync_cursor(id, "Archive").unwrap().is_some());
}

#[test]
fn sync_errors_are_recorded_and_cleared() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store.add_account(Provider::Gmail, "Gmail", "{}").unwrap();
    store
        .record_sync(id, Some("The server stopped responding."))
        .unwrap();
    let account = store.account(id).unwrap();
    assert_eq!(
        account.last_error.as_deref(),
        Some("The server stopped responding.")
    );
    assert_eq!(account.last_sync_at, None);
    store.record_sync(id, None).unwrap();
    let account = store.account(id).unwrap();
    assert_eq!(account.last_error, None);
    assert!(account.last_sync_at.is_some());
}

#[test]
fn removing_an_account_keeps_confirmed_history() {
    let mut store = Store::open_in_memory().unwrap();
    let id = store.add_account(Provider::Gmail, "Gmail", "{}").unwrap();
    store.save_sync_cursor(id, "INBOX", "{}").unwrap();
    let confirmed = event_id(store.record_proposal(id, &proposal("1")).unwrap());
    store
        .review(
            confirmed,
            ReviewDecision::Confirm {
                manuscript_id: None,
                edits: None,
            },
        )
        .unwrap();
    event_id(store.record_proposal(id, &proposal("2")).unwrap());
    assert_eq!(store.review_queue().unwrap().len(), 1);

    store.remove_account(id).unwrap();

    assert!(store.accounts().unwrap().is_empty());
    assert!(
        store.review_queue().unwrap().is_empty(),
        "pending proposals go with the account"
    );
    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 1);
    let timeline = store.timeline(manuscripts[0].id).unwrap();
    assert_eq!(timeline.len(), 1);
    assert!(timeline[0].source.is_none());
}
