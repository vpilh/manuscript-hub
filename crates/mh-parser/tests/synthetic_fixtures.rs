//! End-to-end: synthetic `.eml` fixtures → mock provider → parser.

use std::{fs, path::Path};

use mh_connectors::{mock::MockProvider, MailProvider};
use mh_core::{EditorialStatus, ProposedEvent};
use mh_parser::{extract, CandidateRules};

fn scan() -> Vec<(String, Option<ProposedEvent>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .expect("fixtures directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "eml"))
        .collect();
    paths.sort();
    let emls: Vec<String> = paths
        .iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect();
    let refs: Vec<&str> = emls.iter().map(String::as_str).collect();

    let mut provider = MockProvider::from_eml("fixtures", &refs);
    let batch = provider.fetch_changes(None).unwrap();
    assert_eq!(batch.messages.len(), paths.len(), "every fixture parses");

    let rules = CandidateRules::default();
    paths
        .iter()
        .zip(&batch.messages)
        .map(|(path, message)| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            (name, extract(message, &rules))
        })
        .collect()
}

fn event<'a>(
    results: &'a [(String, Option<ProposedEvent>)],
    name: &str,
) -> Option<&'a ProposedEvent> {
    results
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("missing fixture {name}"))
        .1
        .as_ref()
}

#[test]
fn english_revision_request() {
    let results = scan();
    let e = event(&results, "jss-revision-requested").unwrap();
    assert_eq!(
        e.status.as_ref().unwrap().value,
        EditorialStatus::RevisionRequired
    );
    assert_eq!(e.manuscript_id.as_ref().unwrap().value, "JSS-2026-0142");
    assert_eq!(e.deadline.as_ref().unwrap().value.to_string(), "2026-11-12");
}

#[test]
fn german_acceptance() {
    let results = scan();
    let e = event(&results, "zss-accepted-de").unwrap();
    assert_eq!(e.status.as_ref().unwrap().value, EditorialStatus::Accepted);
    assert_eq!(e.manuscript_id.as_ref().unwrap().value, "ZSS-2026-077");
    assert_eq!(e.deadline.as_ref().unwrap().value.to_string(), "2026-11-09");
}

#[test]
fn submission_confirmation() {
    let results = scan();
    let e = event(&results, "jss-submitted").unwrap();
    assert_eq!(e.status.as_ref().unwrap().value, EditorialStatus::Submitted);
    assert!(e.deadline.is_none());
}

#[test]
fn newsletter_is_not_a_candidate() {
    let results = scan();
    assert!(event(&results, "publisher-newsletter").is_none());
}

#[test]
fn quoted_decision_is_ignored() {
    let results = scan();
    let e = event(&results, "reply-quoting-old-decision").unwrap();
    assert!(e.status.is_none());
    assert!(e.deadline.is_none());
}

#[test]
fn digest_about_several_manuscripts_needs_review() {
    let results = scan();
    let e = event(&results, "multiple-manuscripts").unwrap();
    assert_eq!(
        e.status.as_ref().unwrap().value,
        EditorialStatus::UnderReview
    );
    assert!(e
        .uncertainty
        .iter()
        .any(|u| u.contains("2 possible manuscript IDs")));
}

fn value<T: Clone>(field: &Option<mh_core::Field<T>>) -> T {
    field
        .as_ref()
        .expect("field was not extracted")
        .value
        .clone()
}

#[test]
fn submission_confirmation_with_conditional_boilerplate() {
    let results = scan();
    let e = event(&results, "em-submission-confirmation").unwrap();
    assert_eq!(value(&e.status), EditorialStatus::Submitted);
    assert_eq!(value(&e.manuscript_id), "JEE-D-26-00412");
    assert_eq!(value(&e.journal), "Journal of Example Economics");
    assert_eq!(value(&e.title), "Invented Methods for Example Data");
}

#[test]
fn pdf_approval_without_an_id_is_not_a_proposal() {
    let results = scan();
    assert!(event(&results, "em-pdf-approval").is_none());
}

#[test]
fn revision_received_keeps_the_revision_id() {
    let results = scan();
    let e = event(&results, "em-revision-received").unwrap();
    assert_eq!(value(&e.status), EditorialStatus::Revised);
    assert_eq!(value(&e.manuscript_id), "JEE-D-26-00412R1");
}

#[test]
fn scholarone_minor_revision_with_deadline() {
    let results = scan();
    let e = event(&results, "scholarone-minor-revision").unwrap();
    assert_eq!(value(&e.status), EditorialStatus::RevisionRequired);
    assert_eq!(value(&e.manuscript_id), "ERF-2026-0311");
    assert_eq!(value(&e.deadline).to_string(), "2026-12-15");
    assert_eq!(value(&e.title), "A Note on Synthetic Portfolio Returns");
    assert_eq!(value(&e.journal), "Example Review of Finance");
}

#[test]
fn tracking_service_names_the_journal_in_the_text() {
    let results = scan();
    let e = event(&results, "tracker-under-review").unwrap();
    assert_eq!(value(&e.status), EditorialStatus::UnderReview);
    assert_eq!(value(&e.journal), "Example Review of Finance");
}

#[test]
fn acceptance_with_title_and_relative_deadline() {
    let results = scan();
    let e = event(&results, "annals-accepted").unwrap();
    assert_eq!(value(&e.status), EditorialStatus::Accepted);
    assert_eq!(value(&e.title), "Testing Invented Hypotheses at Scale");
    assert_eq!(value(&e.deadline).to_string(), "2026-11-06");
}

#[test]
fn german_rejection_with_title() {
    let results = scan();
    let e = event(&results, "redaktion-abgelehnt-de").unwrap();
    assert_eq!(value(&e.status), EditorialStatus::Rejected);
    assert_eq!(value(&e.manuscript_id), "ZBS-2026-019");
    assert_eq!(value(&e.title), "Beispielhafte Studien zu Testdaten");
}

#[test]
fn everyday_mail_is_not_journal_mail() {
    let results = scan();
    assert!(event(&results, "shop-order").is_none());
    assert!(event(&results, "ticket-review-request").is_none());
}
