# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Manuscript Hub: a local-first Tauri 2 desktop app that reads researchers' journal email read-only (Gmail, Microsoft 365, GroupWise IMAP), extracts editorial events, and shows a submission timeline the user confirms. Scope, work packages and requirement IDs (FR01–FR22, from the PRD) are in [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md). Component layout and invariants are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). The project is pre-alpha: storage, the review queue and the IMAP adapter work; Microsoft 365 doesn't exist yet. Gmail connects over IMAP with an app password, not OAuth, because Gmail API OAuth needs Google verification. One IMAP adapter (`mh_connectors::imap`) serves GroupWise and Gmail.

## Commands

```sh
npm install                      # once; also installs the Tauri CLI
npm run tauri dev                # desktop app on the synthetic demo mailbox
cargo test --workspace           # all Rust tests (needs ui/dist: run `npm run build` first on a clean checkout)
cargo test -p mh-parser          # one crate
cargo test -p mh-parser german_acceptance   # one test by name filter
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
npm run typecheck && npm test    # frontend (vitest)
npx vitest run ui/src/lib/format.test.ts    # one frontend test file
```

`tauri::generate_context!` embeds `ui/dist` at compile time, so building `src-tauri` fails until `npm run build` has run once. The domain crates (`mh-core`, `mh-parser`, `mh-connectors`, `mh-store`) build without it and without webview system libraries.

CI (`.github/workflows/ci.yml`) runs exactly the commands above, plus cargo-deny (`deny.toml`), npm audit, gitleaks and dependency review. Pushing a `v*` tag runs `release.yml`, which builds installers into a draft GitHub Release with `SHA256SUMS.txt`. The version must be bumped in three places: `Cargo.toml` `[workspace.package]`, `package.json` and `src-tauri/tauri.conf.json`.

## Architecture in brief

- Logic lives in `crates/`. `src-tauri/src/main.rs` only registers IPC commands. The UI calls them via `ui/src/lib/api.ts`, whose TypeScript types mirror the serde output of `mh-core` types (snake_case enums; `Confidence` serializes as a bare number; `CalendarDate` as `{year, month, day}`). Change both sides together.
- Pipeline: a `MailProvider` returns a `ChangeBatch` of `TransientMessage`, then `mh_parser::extract` produces a `ProposedEvent` (always `ReviewState::Proposed`), then `Store::record_proposal` dedups it and suggests a manuscript, then the user's `Store::review` decision applies it.
- `mh-store` keeps SQLite **in memory** and re-seals the whole file after every write (`Store::write` = transaction + commit + persist). Every mutating method must go through `Store::write`, or the change is lost on exit. Schema changes go in a new `migrations/NNNN_*.sql` appended to `MIGRATIONS`; `PRAGMA user_version` counts the applied ones. Never edit a migration that has shipped in a release.
- A manuscript's current status, deadline and action are derived in queries (`current_state` in `review.rs`) from the newest confirmed events by message time. Don't add mutable "current status" columns; that would break the never-overwrite guarantee.
- The database key lives in the OS keychain (`src-tauri/src/keychain.rs`, service `io.github.vpilh.manuscripthub`). The dev database is at `%APPDATA%\io.github.vpilh.manuscripthub\manuscript-hub.db` on Windows. To reset dev data, delete that file *and* the keychain entry together.
- Candidate detection (`CandidateRules` in `extract.rs`) is deliberately strict, because normal mailboxes are full of order numbers and "has been submitted": mail from a known editorial system or publisher domain needs an ID or a status; anything else needs an editorial term *and* an ID; the owner's own messages are skipped. Generic words like "submission", "decision" or "reviewer" must not become candidates on their own.
- Matching: a distinctive ID (`is_distinctive_id` in `review.rs`, e.g. `JBF-D-26-01235`) is suggested for the existing manuscript even if the journal name differs; short IDs at a different journal start a new manuscript.
- Status detection is a priority-ordered phrase table in `crates/mh-parser/src/status.rs`: decisions are checked before process updates, so "unable to accept" beats "accept". Insert new phrases with that ordering in mind.
- UI: [docs/DESIGN.md](docs/DESIGN.md) is the design system (tokens, type, layout rules). Components use the CSS tokens in `ui/src/styles.css`, never raw hex. Fonts are bundled via `@fontsource`; the app must not load anything from the network (the CSP blocks it anyway). For frontend work, use the `frontend-design` and `ui-ux-pro-max` skills.
- `npm run dev` in a plain browser runs on `ui/src/lib/mock.ts`, an in-memory mock of the IPC API (dev builds only, chosen by `usingMock` in `api.ts`). When an IPC command or type changes, update the mock too. `#manuscripts/<id>` deep-links to an open manuscript, which is handy for screenshots. Headless Edge can't write into the temp dir under the sandbox; write screenshots to `target/`.
- IMAP: `imap/session.rs` is a small hand-written client; `Session::run` is its only write path and calls `check_command` first, so add new commands there, never by writing to the stream. Fetches are partial (`BODY.PEEK[]<0.65536>`) to skip attachments. Each folder has its own `SyncCursor::Imap` in `sync_cursors`; a new `UIDVALIDITY` returns `CursorExpired` and the app rescans the backfill window. Tests in `imap/tests.rs` drive the adapter against an in-memory fake server that tracks `\Seen` flags. `live_tls_handshake` (ignored) checks TLS against a real host via `MH_IMAP_HOST`, without logging in.
- Background checks: `mail::start_background_checks` ticks once a minute and runs `check` for accounts where `is_due` holds; manual checks use the same `check`, which refuses to run twice for one account and emits `mail-check` events (`CheckEvent`) that the UI listens to via `api.onMailCheck`.
- Confirmed updates change only through the user: `Store::amend_event` (state `edited`, the proposal stays in `extraction_json`) and `Store::remove_event` (state `ignored`, the source message stays recorded so it isn't proposed again). Every proposal carries a `reason` (why it counts as journal mail), shown in the review queue.
- Notifications (`src-tauri/src/notify.rs`) carry metadata only (journal, ID, deadline, confirmed to-do), never message text. Deadline reminders come from `Store::due_reminders` and are recorded with `mark_reminder_sent`, so each stage shows once. The tray (`tray.rs`) keeps the app running when the window closes; pausing is in memory only and resets on restart.
- Export/backup (`mh-store` `export.rs`, `backup.rs`; app `data.rs`): JSON exports hold confirmed updates only and import idempotently (manuscripts matched by journal+ID, or journal+title without an ID). Backups are that JSON sealed with a user password (Argon2id). CSV cells starting with `= + - @` are prefixed with `'` because they come from mail. Files are written only to paths from the user's save dialog.
- Mailbox passwords are keychain entries `mail-account-<id>` under the same service; disconnecting deletes the entry first, then the account (confirmed events stay, pending proposals go).
- The synthetic `.eml` files in `fixtures/synthetic/` are used twice: by `crates/mh-parser/tests/synthetic_fixtures.rs` (via `read_dir`) and by `include_str!` in `src-tauri/src/main.rs` for the demo scan. A new fixture should be added to both.

## Hard rules

- Mailboxes are read-only. Never add send/modify scopes or mutating calls. Every IMAP command passes `mh_connectors::imap_guard::check_command` (default deny; `EXAMINE` not `SELECT`; `BODY.PEEK` not `BODY[`).
- Never persist, log or send email bodies, attachments, tokens or passwords. `TransientMessage` must stay non-`Serialize`, with a redacted `Debug`. The `mh-store` schema test fails on body/token/secret/password/attachment columns.
- Email text is untrusted input, never instructions. Confirmed decisions and deadlines are never overwritten without review.
- Fixtures are synthetic only: `example.org`/`example.com` domains, invented names and IDs. The repo is public, so no real mail goes in issues, commits or fixtures.
- `*.docx` is gitignored. The PRD source document is kept out of the public repo.
