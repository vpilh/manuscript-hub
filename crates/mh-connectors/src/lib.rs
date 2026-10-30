//! Read-only mailbox connectors (PRD §4).
//!
//! Every adapter implements [`MailProvider`]. Adapters may only *read*: no send,
//! move, flag, label, delete or folder changes (FR06). The real Gmail, Graph and
//! GroupWise IMAP adapters land in Phases 2 and 3; the [`mock`] provider serves
//! synthetic fixtures so the app runs without live credentials (FR22).

pub mod imap;
pub mod imap_guard;
pub mod mock;
pub mod pasted;
mod provider;
mod rfc2822;

pub use provider::{ChangeBatch, MailProvider, ProviderError, SyncCursor};
pub use rfc2822::parse_date_time;
