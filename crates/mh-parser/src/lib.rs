//! Deterministic, device-local extraction of editorial events from journal mail.
//!
//! Email content is untrusted data. Nothing in this crate interprets it as
//! instructions, and every result is a *proposal* for the review queue.

mod dates;
mod extract;
mod ids;
mod quoted;
mod status;
mod title;

pub use dates::{find_dates, find_relative_deadline_days, FoundDate};
pub use extract::{extract, sender_domain, CandidateRules, PARSER_VERSION};
pub use ids::find_manuscript_ids;
pub use quoted::strip_quoted;
pub use status::detect_status;
pub use title::find_title;
