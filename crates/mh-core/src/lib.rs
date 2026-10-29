//! Domain model shared by connectors, parser, storage and UI.
//!
//! This crate performs no I/O. Raw email content never appears in these types;
//! only normalized metadata and stable source references (PRD §6).

mod date;
mod message;
mod model;
mod status;

pub use date::{CalendarDate, DateError};
pub use message::TransientMessage;
pub use model::{Confidence, DedupKey, Field, ProposedEvent, Provider, ReviewState, SourceRef};
pub use status::EditorialStatus;
