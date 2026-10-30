# Implementation plan

This plan turns the *Manuscript Hub Email Integration PRD* (v0.2, 28 October 2026) into work packages. Requirement IDs (FR01–FR22) and acceptance areas refer to that PRD. The plan is a living document: update it in the same pull request that changes scope, ordering or a decision recorded here.

## 1. What we are building

A desktop app for researchers. It reads journal correspondence from their own mailboxes (Gmail, Microsoft 365, university GroupWise) **read-only**, extracts editorial events (status, decision, deadline, requested action), and shows one submission timeline. The researcher confirms what gets saved. Everything runs on the researcher's computer: the encrypted database, credentials, parsing. Raw email bodies and attachments are never stored and never leave the device.

## 2. Technical decisions

| Topic | Decision | Notes |
|---|---|---|
| Desktop shell | **Tauri 2** | Small installers, a Rust backend for everything that touches secrets or mail. The webview only renders UI. |
| UI | React 18 + TypeScript + Vite | It talks to the backend only through Tauri IPC commands. |
| Process model | Single process: the UI webview plus the Rust core, with a tray icon for background sync | Because there is no separate service process, **no HTTP listener exists**, so FR21's loopback rule is met by construction. If a headless service is split out later, it must bind to `127.0.0.1` only, with per-session token auth and CSRF protection. |
| Async runtime | `tokio` (already used by Tauri) | |
| Local database | Bundled SQLite (`rusqlite`) in memory. After each committed write, the database is serialized and the whole file is sealed with **XChaCha20-Poly1305** (RustCrypto), then written atomically (temp file + rename). | *Changed from SQLCipher (October 2026):* building SQLCipher's vendored OpenSSL needs Strawberry Perl on Windows, which would be an extra prerequisite for every contributor. The metadata database is small, so whole-file sealing keeps the crypto in pure Rust. A random 256-bit key is created on first run and stored in the OS keychain. Backups (WP 1.8) are encrypted with a user passphrase (Argon2id KDF). |
| Credential vault | `keyring` crate (Windows Credential Manager, macOS Keychain, Secret Service on Linux) | Tokens and IMAP secrets never go to the database, logs or error reports. A missing key for an existing database is an error; the app never generates a new key over existing data. |
| Gmail access | **IMAP with an app password** first; the Gmail API with OAuth is optional later | Publishing an OAuth app with the restricted `gmail.readonly` scope needs Google's verification, which this project can't get for now. An unverified Google project in *Testing* mode is capped at 100 manually added test users, and its refresh tokens expire after 7 days. Gmail IMAP (`imap.gmail.com:993`) with an app password needs no Google Cloud project. It does need 2-Step Verification, and Workspace admins can disable IMAP or app passwords. An app password grants full mailbox access, so `imap_guard` enforces read-only exactly as it does for GroupWise. Fallback for every provider: import of `.eml`/`.mbox` files, such as a Google Takeout export. |
| HTTP providers | `reqwest` + `rustls`; `oauth2` crate with PKCE and a loopback redirect | For Microsoft Graph, which doesn't need verification for delegated `Mail.Read` (it shows an "unverified publisher" notice; tenant admins may require consent). Also for the optional Gmail API path. |
| IMAP | `async-imap` over `tokio-rustls`, with platform root certificates | One adapter serves both GroupWise and Gmail. Every command passes through `mh_connectors::imap_guard` (allowlist, default deny). |
| Parsing | Deterministic rules first: a journal rule catalog (TOML) plus English and German phrase tables | The optional local LLM is **out of scope for 1.0**. If added, it gets schema-validated output and goes through review. |
| Notifications / autostart | `tauri-plugin-notification`, `tauri-plugin-autostart` (opt-in) | |
| Distribution | GitHub Releases built by `tauri-action`; `SHA256SUMS.txt` attached | Signing and notarization are added once certificates are available (§7). |

## 3. Repository layout

```
crates/
  mh-core/         Domain model: statuses, source refs, proposals, dates. No I/O.
  mh-parser/       Candidate detection, quoted-text stripping, status/date extraction.
  mh-connectors/   MailProvider trait, mock provider, IMAP command guard, later the real adapters.
  mh-store/        Encrypted SQLite storage, migrations, export/backup.
src-tauri/         Tauri shell: IPC commands, tray, lifecycle. Keep it thin.
ui/                React frontend.
fixtures/synthetic/  Synthetic .eml corpus. Never real mailbox data.
docs/              Architecture, threat model, this plan.
```

The domain crates must build and test **without** Tauri or system webview libraries. Business logic goes there, not in `src-tauri`.

## 4. Phases and work packages

Each work package (WP) is done when its *Done when* evidence is merged and passes CI.

### Phase 1: Foundation (no live mailboxes)

| WP | Scope | FRs | Done when |
|---|---|---|---|
| 1.1 | Public repo, MIT license, README, contributing, security policy, issue templates, CI (build, tests, licenses, advisories, secret scanning), release workflow | FR19, FR22 | A fresh clone builds and runs the mock scan; CI is green. *(Delivered in the initial commit, apart from a first green release run.)* |
| 1.2 | Canonical domain model: manuscripts (per-journal IDs, resubmission chains), events, deadlines/actions, provenance, per-field confidence, confirmation state | FR08, FR10 | Types in `mh-core`, with unit tests for invariants such as terminal statuses and dedup keys. |
| 1.3 | Encrypted store: sealed SQLite file, migrations, keychain-held DB key, per-user app-data path, account/cursor tables | FR15, FR21 | The DB file is unreadable without the key (test). Migrations run on a clean install. No secrets in tables (schema review). **Done.** |
| 1.4 | Parser v1: candidate detection (domains, senders, subjects, manuscript-ID patterns, user rules); quoted-reply/forward stripping; English and German status phrases; dates (ISO, `DD.MM.YYYY`, month names in English and German, relative deadlines such as "within 30 days"); timezone assumption recorded | FR07, FR08, FR09 | Fixture tests per journal family. Quoted older decisions never produce new events. |
| 1.5 | Matching and review queue: exact manuscript ID, then journal + title similarity; conflict detection; state machine `proposed → confirmed / edited / ignored / unrelated` | FR10, FR11 | Tests show a confirmed decision or deadline is never overwritten without review. Ambiguous IDs never auto-apply. **Done**: the current status and deadline are derived from the newest confirmed events, never overwritten in place. |
| 1.6 | Mock provider and integrity harness: synthetic corpus (≥10 journal families, English and German, newsletters, phishing look-alikes, multi-paper messages); a recording fake for each provider API that fails on any mutating call | FR06, FR22 | `cargo test` exercises full scans on mocks with zero mutating calls recorded. |
| 1.7 | UI: onboarding wizard (local storage, permissions, notifications), manuscript list, event timeline with source links, review queue, account health, manual entry and paste-email import | FR13, FR17 | Every PRD journey can be clicked through on mock data. |
| 1.8 | Data control: CSV/JSON export, re-import, encrypted backup/restore, local-data deletion; importer for the hosted MVP's D1 export | FR16 | Export → wipe → import round-trips. A D1 sample export imports cleanly (needs a sample, see §7). |
| 1.9 | Lifecycle: tray, sync indicator, pause/resume/quit, opt-in autostart, desktop notifications (review requests, decisions, approaching deadlines) | FR14, FR21 | Manual test checklist on Windows, macOS and Linux. Autostart is off by default. **Partly done**: tray icon (open, check now, pause/resume, quit; closing the window keeps the app in the tray), notifications for new proposals after automatic checks and for deadlines (a week before, a day before, overdue; each once, recorded in `reminders_sent`). Autostart and the manual checklist are open. |

### Phase 2: IMAP — GroupWise and Gmail

One IMAP adapter covers university GroupWise and Gmail (app password), so neither needs OAuth verification. Live testing uses a real GroupWise account and a real Gmail account, with credentials entered in the app and kept in the OS keychain, never in the repo.

| WP | Scope | FRs | Done when |
|---|---|---|---|
| 2.1 | IMAP adapter: validated TLS (implicit TLS or STARTTLS, no plaintext fallback), `EXAMINE`, `UID SEARCH`, `BODY.PEEK` fetches, MIME decoding of text parts in memory, UID checkpoints with `UIDVALIDITY` reset handling, configurable host/port/folders | FR03, FR05, FR06 | Every command goes through `imap_guard`. Tests against a local IMAP test server show flags are unchanged after a scan. **Done**: tests use an in-memory fake server; fetches are partial (64 KiB) so attachments aren't downloaded. Non-ASCII user names, passwords and folder names aren't supported yet (they need IMAP literals). |
| 2.2 | Account setup: presets for Gmail (`imap.gmail.com:993`, app password, Gmail folder names such as `[Gmail]/All Mail`) and GroupWise (host/port from the university); credentials in the keychain; disconnect removes them | FR03, FR16 | Disconnect leaves no keychain entry (test). The Gmail preset explains 2-Step Verification and app passwords. **Done** apart from a keychain test (needs an OS keychain in CI): the Mailboxes view connects, checks and disconnects accounts; Gmail reads the `\All` folder found via `LIST`, whatever its localized name. |
| 2.3 | Connection diagnostic wizard: DNS, TCP, TLS certificate, capability and auth-method checks; VPN hints; safe failure messages that never suggest disabling MFA | FR03, FR17 | Each failure mode maps to a specific message (tests). |
| 2.4 | Scheduler and account health: per-account frequency, last-successful-sync timestamp, catch-up after sleep or offline, actionable error states | FR04, freshness target | New mail is detected within 15 minutes on the default schedule (integration test with a fake clock). **Done**: a background thread checks due accounts once a minute (`is_due` in `src-tauri/src/mail.rs`, tested with a fake clock); per-account interval or manual only; failed checks retry at the interval; checks report to the window via `mail-check` events. |
| 2.5 | Live validation on the project's GroupWise and Gmail test accounts; then a pilot with ≥2 cooperating German university IT departments | GroupWise pilot gate | Report covering TLS, EXAMINE, UID reset, folder naming, VPN recovery and Gmail labels. |
| 2.6 | File import: `.eml` and `.mbox` (e.g. Google Takeout), for any provider that can't connect | FR17 | Imported messages go through the same parser and dedup as synced mail. |

### Phase 3: Microsoft 365 (and optional Gmail API)

| WP | Scope | FRs | Done when |
|---|---|---|---|
| 3.1 | Microsoft Entra app registration (multi-tenant + personal accounts, public client); publisher verification optional | Release gate | App registered; consent behaviour documented for school tenants. |
| 3.2 | OAuth for installed apps: PKCE, loopback redirect on an ephemeral `127.0.0.1` port, tokens in the keychain, revoke on disconnect | FR02, FR16 | No write or send scope can be requested (test on scope constants). Disconnect removes keychain entries. |
| 3.3 | Graph adapter: per-folder message delta, `410 Gone` resync, tenant-consent-blocked diagnostics | FR02, FR04, FR05 | Recorded-response tests. A clear warning appears when admin consent is required. |
| 3.4 | *Optional:* Gmail API adapter (`gmail.readonly`, `history.list` cursor) for users who bring their own Google OAuth client, or once verification is obtained | FR01 | Only built if IMAP proves insufficient (e.g. Workspace accounts with app passwords disabled). |

### Phase 4: Hardening and first release

| WP | Scope | Done when |
|---|---|---|
| 4.1 | Evaluation harness: precision/recall on a synthetic or consented corpus (≥10 journal families, English and German) | Detection precision ≥ 90%; exact-ID matching ≥ 95%; report published in `docs/`. |
| 4.2 | Security and privacy: final threat model, dependency and secret scanning, network inspection (no bodies, attachments or credentials to vendor endpoints), GDPR and university policy review | Review sign-off recorded in an issue. |
| 4.3 | Packaging: Windows (MSI/NSIS), macOS (DMG, notarized), Linux (AppImage/deb/rpm); checksums; signed update manifest if auto-update is enabled (user-controlled) | A non-developer installs and launches the app on all three OS families without dev tools. |
| 4.4 | Documentation and the `v1.0.0` MIT release with tagged source | The release has assets, checksums and matching source tags. |

## 5. Test strategy (PRD §9)

| Acceptance area | How it is tested |
|---|---|
| Mailbox integrity | `imap_guard` unit tests; IMAP integration test comparing flags before and after; recording fakes for Graph that fail on mutating endpoints; the GroupWise and Gmail test inboxes checked for unchanged flags after live scans. |
| Detection / matching / dates | Fixture-driven tests in `mh-parser`; evaluation harness in WP 4.1. |
| Sync reliability | Recorded provider responses for cursor expiry, rate limits and network failure; duplicate-event assertions. |
| Local storage | Encryption-at-rest test; outbound-traffic capture during a full scan (WP 4.2). |
| User control | End-to-end disconnect, delete and export/import tests. |
| Installation / reproducibility | Release workflow on clean runners; manual install checklist on clean VMs. |

## 6. Critical path

1. **IMAP adapter (WP 2.1)** unlocks both GroupWise and Gmail, and test accounts for both exist.
2. **GroupWise pilot sites (WP 2.5)** depend on university IT cooperation. Recruit them early.
3. **Microsoft tenant consent (WP 3.1)** may need admin approval at universities. Ask early.

## 7. Open questions (owner decision needed)

- **Hosted MVP export format.** We need a sample D1 export (schema plus anonymized rows) for WP 1.8.
- **Code-signing certificates.** Apple Developer Program membership for notarization, and a Windows code-signing certificate. Until then, releases are unsigned and the README must say so.
- **Gmail API verification.** Not pursued for now; Gmail connects over IMAP. Revisit only if app passwords turn out to be unavailable for many users.
- **OAuth client ownership.** Who owns the Microsoft app registration (and a Google one, if WP 3.4 is ever built). Installed-app client IDs ship inside the open-source binary, so builds by contributors use their own IDs via configuration.
- **Local LLM assistance.** Deferred until after 1.0 unless the evaluation in WP 4.1 shows that deterministic rules miss too much.
