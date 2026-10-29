use crate::SourceRef;

/// A fetched message held in memory only while it is being parsed (FR15).
///
/// Deliberately not `Serialize`: bodies must never reach storage, logs or IPC.
/// Treat every field as untrusted input, never as instructions.
#[derive(Clone)]
pub struct TransientMessage {
    pub source: SourceRef,
    pub from: String,
    pub subject: String,
    pub body: String,
}

impl std::fmt::Debug for TransientMessage {
    // Keep bodies out of debug output so they cannot leak into logs or error reports.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransientMessage")
            .field("source", &self.source)
            .field("body_len", &self.body.len())
            .finish_non_exhaustive()
    }
}
