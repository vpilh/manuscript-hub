//! Review-queue invariants (FR10, FR11, FR13).

use mh_core::{
    CalendarDate, Confidence, EditorialStatus, Field, ProposedEvent, Provider, ReviewState,
    SourceRef,
};
use mh_store::{DbKey, EventEdits, MatchKind, RecordOutcome, ReviewDecision, Store, StoreError};

const DAY: i64 = 86_400;
const T0: i64 = 1_793_275_200; // 2026-10-29T12:00:00Z

fn field<T>(value: T) -> Option<Field<T>> {
    Some(Field {
        value,
        original: "phrase".into(),
        confidence: Confidence::new(0.8),
    })
}

fn proposal(
    message_id: &str,
    received_at: i64,
    journal: Option<&str>,
    manuscript_id: Option<&str>,
    status: Option<EditorialStatus>,
    deadline: Option<(i32, u8, u8)>,
) -> ProposedEvent {
    ProposedEvent {
        source: SourceRef {
            provider: Provider::Mock,
            account_alias: "test".into(),
            message_id: message_id.into(),
            folder: "INBOX".into(),
            received_at,
        },
        journal: journal.and_then(|j| field(j.to_string())),
        manuscript_id: manuscript_id.and_then(|m| field(m.to_string())),
        title: None,
        status: status.and_then(field),
        deadline: deadline.and_then(|(y, m, d)| field(CalendarDate::new(y, m, d).unwrap())),
        requested_action: None,
        parser_version: "test".into(),
        uncertainty: Vec::new(),
        review: ReviewState::Proposed,
    }
}

fn setup() -> (Store, i64) {
    let mut store = Store::open_in_memory().unwrap();
    let account = store.ensure_account(Provider::Mock, "test").unwrap();
    (store, account)
}

fn record(store: &mut Store, account: i64, p: &ProposedEvent) -> i64 {
    match store.record_proposal(account, p).unwrap() {
        RecordOutcome::Proposed { event_id } => event_id,
        RecordOutcome::Duplicate => panic!("unexpected duplicate"),
    }
}

fn confirm() -> ReviewDecision {
    ReviewDecision::Confirm {
        manuscript_id: None,
        edits: None,
    }
}

const JSS: Option<&str> = Some("Journal of Synthetic Studies");

#[test]
fn confirming_a_proposal_creates_a_tracked_manuscript() {
    let (mut store, account) = setup();
    let p = proposal(
        "m1",
        T0,
        JSS,
        Some("JSS-1"),
        Some(EditorialStatus::RevisionRequired),
        Some((2026, 11, 12)),
    );
    let event = record(&mut store, account, &p);

    let queue = store.review_queue().unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].match_kind, MatchKind::None);
    assert!(store.manuscripts().unwrap().is_empty());

    store.review(event, confirm()).unwrap();
    assert!(store.review_queue().unwrap().is_empty());
    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 1);
    assert_eq!(manuscripts[0].external_id.as_deref(), Some("JSS-1"));
    assert_eq!(
        manuscripts[0].status,
        Some(EditorialStatus::RevisionRequired)
    );
    assert_eq!(
        manuscripts[0].deadline,
        Some(CalendarDate::new(2026, 11, 12).unwrap())
    );
}

#[test]
fn the_same_message_is_recorded_once() {
    let (mut store, account) = setup();
    let p = proposal("m1", T0, JSS, Some("JSS-1"), None, None);
    record(&mut store, account, &p);
    assert_eq!(
        store.record_proposal(account, &p).unwrap(),
        RecordOutcome::Duplicate
    );
    assert_eq!(store.review_queue().unwrap().len(), 1);
}

#[test]
fn later_mail_with_the_same_id_is_suggested_for_that_manuscript() {
    let (mut store, account) = setup();
    let first = record(
        &mut store,
        account,
        &proposal(
            "m1",
            T0,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::Submitted),
            None,
        ),
    );
    store.review(first, confirm()).unwrap();
    let manuscript_id = store.manuscripts().unwrap()[0].id;

    let second = record(
        &mut store,
        account,
        &proposal(
            "m2",
            T0 + DAY,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::UnderReview),
            None,
        ),
    );
    let queue = store.review_queue().unwrap();
    assert_eq!(queue[0].match_kind, MatchKind::ExactId);
    assert_eq!(
        queue[0].suggested_manuscript.as_ref().unwrap().id,
        manuscript_id
    );

    store.review(second, confirm()).unwrap();
    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 1);
    assert_eq!(manuscripts[0].status, Some(EditorialStatus::UnderReview));
    assert_eq!(store.timeline(manuscript_id).unwrap().len(), 2);
}

#[test]
fn a_long_id_matches_even_when_the_journal_name_differs() {
    let (mut store, account) = setup();
    let first = record(
        &mut store,
        account,
        &proposal(
            "m1",
            T0,
            Some("Journal of Banking and Finance"),
            Some("JBF-D-26-01235"),
            Some(EditorialStatus::Submitted),
            None,
        ),
    );
    store.review(first, confirm()).unwrap();
    let manuscript_id = store.manuscripts().unwrap()[0].id;

    let update = record(
        &mut store,
        account,
        &proposal(
            "m2",
            T0 + DAY,
            Some("Track your Elsevier submission"),
            Some("JBF-D-26-01235"),
            Some(EditorialStatus::UnderReview),
            None,
        ),
    );
    let queue = store.review_queue().unwrap();
    assert_eq!(queue[0].match_kind, MatchKind::IdOnly);
    assert_eq!(
        queue[0].suggested_manuscript.as_ref().unwrap().id,
        manuscript_id
    );

    store.review(update, confirm()).unwrap();
    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 1);
    assert_eq!(manuscripts[0].journal, "Journal of Banking and Finance");
    assert_eq!(manuscripts[0].status, Some(EditorialStatus::UnderReview));
}

#[test]
fn an_older_message_never_replaces_the_current_status_or_deadline() {
    let (mut store, account) = setup();
    let newer = record(
        &mut store,
        account,
        &proposal(
            "accept",
            T0 + 10 * DAY,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::Accepted),
            None,
        ),
    );
    store.review(newer, confirm()).unwrap();

    let older = record(
        &mut store,
        account,
        &proposal(
            "revise",
            T0,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::RevisionRequired),
            Some((2026, 11, 12)),
        ),
    );
    let queue = store.review_queue().unwrap();
    assert!(queue[0]
        .proposal
        .uncertainty
        .iter()
        .any(|u| u.contains("Older than the latest confirmed update")));

    store.review(older, confirm()).unwrap();
    let summary = &store.manuscripts().unwrap()[0];
    assert_eq!(summary.status, Some(EditorialStatus::Accepted));
    assert_eq!(summary.deadline, None);
}

#[test]
fn a_differing_deadline_is_flagged() {
    let (mut store, account) = setup();
    let first = record(
        &mut store,
        account,
        &proposal(
            "m1",
            T0,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::RevisionRequired),
            Some((2026, 11, 12)),
        ),
    );
    store.review(first, confirm()).unwrap();
    record(
        &mut store,
        account,
        &proposal(
            "m2",
            T0 + DAY,
            JSS,
            Some("JSS-1"),
            None,
            Some((2026, 11, 30)),
        ),
    );
    let queue = store.review_queue().unwrap();
    assert!(queue[0]
        .proposal
        .uncertainty
        .iter()
        .any(|u| u.contains("Differs from the confirmed deadline 2026-11-12")));
}

#[test]
fn reviewed_events_are_final() {
    let (mut store, account) = setup();
    let event = record(
        &mut store,
        account,
        &proposal(
            "m1",
            T0,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::Submitted),
            None,
        ),
    );
    store.review(event, confirm()).unwrap();
    assert!(matches!(
        store.review(event, ReviewDecision::Ignore),
        Err(StoreError::AlreadyReviewed { .. })
    ));
    assert!(matches!(
        store.review(999, confirm()),
        Err(StoreError::NotFound { .. })
    ));
}

#[test]
fn a_shared_id_requires_an_explicit_choice() {
    let (mut store, account) = setup();
    for (message, journal) in [("a", "Journal A"), ("b", "Journal B")] {
        let event = record(
            &mut store,
            account,
            &proposal(message, T0, Some(journal), Some("MS-001"), None, None),
        );
        store.review(event, confirm()).unwrap();
    }
    let unclear = record(
        &mut store,
        account,
        &proposal(
            "c",
            T0 + DAY,
            None,
            Some("MS-001"),
            Some(EditorialStatus::Accepted),
            None,
        ),
    );
    let queue = store.review_queue().unwrap();
    assert_eq!(queue[0].match_kind, MatchKind::Ambiguous);
    assert!(queue[0].suggested_manuscript.is_none());
    assert!(matches!(
        store.review(unclear, confirm()),
        Err(StoreError::NeedsManuscriptChoice { .. })
    ));

    let chosen = store
        .manuscripts()
        .unwrap()
        .into_iter()
        .find(|m| m.journal == "Journal B")
        .unwrap()
        .id;
    store
        .review(
            unclear,
            ReviewDecision::Confirm {
                manuscript_id: Some(chosen),
                edits: None,
            },
        )
        .unwrap();
    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 2);
    let b = manuscripts.iter().find(|m| m.id == chosen).unwrap();
    assert_eq!(b.status, Some(EditorialStatus::Accepted));
}

#[test]
fn edits_replace_the_extracted_values() {
    let (mut store, account) = setup();
    let event = record(
        &mut store,
        account,
        &proposal(
            "m1",
            T0,
            JSS,
            Some("JSS-1"),
            Some(EditorialStatus::UnderReview),
            Some((2026, 11, 12)),
        ),
    );
    store
        .review(
            event,
            ReviewDecision::Confirm {
                manuscript_id: None,
                edits: Some(EventEdits {
                    status: Some(EditorialStatus::RevisionRequired),
                    deadline: Some(CalendarDate::new(2026, 12, 1).unwrap()),
                    requested_action: Some("Submit revised manuscript".into()),
                }),
            },
        )
        .unwrap();
    let summary = &store.manuscripts().unwrap()[0];
    assert_eq!(summary.status, Some(EditorialStatus::RevisionRequired));
    assert_eq!(
        summary.deadline,
        Some(CalendarDate::new(2026, 12, 1).unwrap())
    );
    assert_eq!(
        summary.requested_action.as_deref(),
        Some("Submit revised manuscript")
    );
    let timeline = store.timeline(summary.id).unwrap();
    assert_eq!(timeline[0].review_state, ReviewState::Edited);
}

#[test]
fn ignored_and_unrelated_mail_leaves_no_trace_in_manuscripts() {
    let (mut store, account) = setup();
    let a = record(
        &mut store,
        account,
        &proposal("a", T0, JSS, Some("JSS-1"), None, None),
    );
    let b = record(
        &mut store,
        account,
        &proposal("b", T0, JSS, Some("JSS-2"), None, None),
    );
    store.review(a, ReviewDecision::Ignore).unwrap();
    store.review(b, ReviewDecision::Unrelated).unwrap();
    assert!(store.review_queue().unwrap().is_empty());
    assert!(store.manuscripts().unwrap().is_empty());
}

#[test]
fn review_state_survives_reopening_the_encrypted_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let key = DbKey::generate();
    let key_bytes = key.as_bytes().to_vec();
    {
        let mut store = Store::open(&path, key).unwrap();
        let account = store.ensure_account(Provider::Mock, "test").unwrap();
        let event = record(
            &mut store,
            account,
            &proposal(
                "m1",
                T0,
                JSS,
                Some("JSS-1"),
                Some(EditorialStatus::Submitted),
                None,
            ),
        );
        store.review(event, confirm()).unwrap();
        record(
            &mut store,
            account,
            &proposal("m2", T0 + DAY, JSS, Some("JSS-1"), None, None),
        );
    }
    let store = Store::open(&path, DbKey::from_bytes(&key_bytes).unwrap()).unwrap();
    assert_eq!(store.manuscripts().unwrap().len(), 1);
    assert_eq!(store.review_queue().unwrap().len(), 1);
}

#[test]
fn decisions_deserialize_from_the_ui_and_reject_impossible_dates() {
    let decision: ReviewDecision = serde_json::from_str(
        r#"{"kind":"confirm","edits":{"status":"accepted","deadline":{"year":2026,"month":11,"day":9},"requested_action":null}}"#,
    )
    .unwrap();
    assert!(matches!(
        decision,
        ReviewDecision::Confirm {
            manuscript_id: None,
            edits: Some(_)
        }
    ));
    assert!(serde_json::from_str::<ReviewDecision>(r#"{"kind":"ignore"}"#).is_ok());
    assert!(serde_json::from_str::<ReviewDecision>(
        r#"{"kind":"confirm","edits":{"status":null,"deadline":{"year":2026,"month":2,"day":31},"requested_action":null}}"#
    )
    .is_err());
}

#[test]
fn merging_combines_timelines_and_derives_the_newest_state() {
    let (mut store, account) = setup();
    let submitted = record(
        &mut store,
        account,
        &proposal(
            "m1",
            T0,
            Some("Journal of Banking and Finance"),
            Some("JBF-D-26-01235"),
            Some(EditorialStatus::Submitted),
            None,
        ),
    );
    store.review(submitted, confirm()).unwrap();
    let under_review = record(
        &mut store,
        account,
        &proposal(
            "m2",
            T0 + DAY,
            Some("Track your Elsevier submission"),
            Some("JBF-D-26-99999"),
            Some(EditorialStatus::UnderReview),
            None,
        ),
    );
    store.review(under_review, confirm()).unwrap();
    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 2);
    let id_of = |journal: &str| {
        manuscripts
            .iter()
            .find(|m| m.journal == journal)
            .unwrap()
            .id
    };
    let (wrong, right) = (
        id_of("Track your Elsevier submission"),
        id_of("Journal of Banking and Finance"),
    );

    store.merge_manuscripts(wrong, right).unwrap();

    let manuscripts = store.manuscripts().unwrap();
    assert_eq!(manuscripts.len(), 1);
    assert_eq!(manuscripts[0].id, right);
    assert_eq!(
        manuscripts[0].external_id.as_deref(),
        Some("JBF-D-26-01235")
    );
    assert_eq!(manuscripts[0].status, Some(EditorialStatus::UnderReview));
    assert_eq!(store.timeline(right).unwrap().len(), 2);
    assert!(store.merge_manuscripts(right, right).is_err());
    assert!(matches!(
        store.merge_manuscripts(wrong, right),
        Err(StoreError::NotFound { .. })
    ));
}
