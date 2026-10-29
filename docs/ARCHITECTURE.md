# Architecture

Manuscript Hub runs entirely on the researcher's computer. There is no Manuscript Hub server. The only network traffic is between the app and the user's own mail providers.

```
┌──────────────────────── researcher's computer ────────────────────────┐
│                                                                        │
│  ui/ (React, in the Tauri webview)                                     │
│        │  Tauri IPC commands only (no HTTP listener)                   │
│        ▼                                                               │
│  src-tauri/  ── thin shell: commands, tray, lifecycle, notifications   │
│        │                                                               │
│        ├── mh-connectors ── read-only adapters ──────────────┐         │
│        │     MailProvider trait, imap_guard, mock            │         │
│        │                                                     │  TLS    │
│        ├── mh-parser ── TransientMessage → ProposedEvent      │         │
│        │     (bodies exist only in memory here)               │         │
│        │                                                     │         │
│        └── mh-store ── sealed SQLite file in per-user app data│         │
│              key + tokens in the OS keychain                 │         │
└──────────────────────────────────────────────────────────────┼─────────┘
                                                               ▼
                                  Gmail API · Microsoft Graph · GroupWise IMAP
```

## Crates

| Crate | Responsibility | Depends on |
|---|---|---|
| `mh-core` | Domain types: `EditorialStatus`, `SourceRef`/`DedupKey`, `Field<T>` with confidence and original phrase, `ProposedEvent`, `ReviewState`, `CalendarDate`, `TransientMessage`. No I/O. | `serde` |
| `mh-parser` | Candidate detection, quoted-reply stripping, manuscript-ID candidates, English and German status phrases and dates, assembly of a `ProposedEvent`. | `mh-core` |
| `mh-connectors` | The `MailProvider` trait and sync cursors, the IMAP command allowlist, RFC 2822 / INTERNALDATE parsing, the mock provider for fixtures. Real adapters land here. | `mh-core` |
| `mh-store` | Encrypted persistence (in-memory SQLite, sealed to disk with XChaCha20-Poly1305 after each write), migrations, matching and the review queue. | `mh-core`, `rusqlite`, `chacha20poly1305` |
| `src-tauri` | Wires the crates together behind IPC commands; keeps the database key in the OS keychain (`keychain.rs`). No business logic. | all of the above, `tauri`, `keyring` |

The domain crates compile and test without Tauri or system webview libraries. CI runs them on Windows and macOS as well as Linux.

## Data flow

1. A provider returns a `ChangeBatch` of `TransientMessage`s plus the next `SyncCursor`. An expired cursor (`ProviderError::CursorExpired`) triggers a rescan of the backfill window.
2. `mh_parser::extract` strips quoted history, finds IDs, status, dates and deadlines, and returns a `ProposedEvent` with per-field confidence, provenance (`parser_version`, original phrases) and a list of uncertainty reasons. The message body is then dropped.
3. `Store::record_proposal` skips messages already processed (`source_messages`, unique per account and message ID), then suggests a manuscript. The rules, in order: same ID at the same journal; same ID when the sender's journal is unknown; a similar title. A known *different* journal with the same ID means a different manuscript. Conflicts with confirmed data are added to the proposal as notes.
4. The review queue shows proposals. `Store::review` confirms, edits, ignores or marks them unrelated, and reviewed events are final. An ID shared across journals needs an explicit choice. The only other planned path is an opt-in rule for non-conflicting, high-confidence updates (FR18).
5. A manuscript's current status, deadline and requested action are **derived** from its newest confirmed events by message time, never stored as mutable fields. Confirming an older message adds it to the timeline without changing what is current.

## Invariants

These are enforced in code and tests, and reviewers check them on every PR:

- **No mailbox writes.** OAuth scopes are read-only, and every IMAP command passes `imap_guard::check_command` (default deny; `EXAMINE`, `BODY.PEEK`).
- **No content at rest.** `TransientMessage` is not `Serialize`, and its `Debug` output omits headers and body. The schema test rejects body, attachment and secret columns.
- **Proposals, not mutations.** Parser output is always `ReviewState::Proposed`.
- **No listener.** The UI talks to Rust only through Tauri IPC. If a separate background service is ever added, its API binds to loopback with per-session authentication.

## Frontend

`ui/` is a Vite + React + TypeScript app. `ui/src/lib/api.ts` mirrors the serde shapes of the Rust types and wraps `invoke` calls. Opened in a plain browser (`npm run dev`), the UI only shows a preview notice, because the commands exist only inside the desktop shell.
