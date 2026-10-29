# Manuscript Hub

Manuscript Hub is a desktop app that keeps track of your journal submissions by reading your editorial email. It finds decision letters, revision requests and deadlines in your mailboxes (Gmail, Microsoft 365, or university GroupWise) and puts them on one timeline. You confirm every change before it is saved.

- **Local-first.** Your mail is read and parsed on your computer. Only the extracted facts are kept, in an encrypted database on your device. Email bodies and attachments are not stored and are never sent to a server.
- **Read-only.** The app requests read-only access and never sends, moves, deletes, labels or marks mail as read.
- **You decide.** Uncertain matches go to a review queue. A confirmed decision or deadline is never overwritten silently.

> **Status: early development (pre-alpha).** The app runs on a built-in *synthetic* demo mailbox, with a working review queue and encrypted local storage. Connecting real accounts is next: GroupWise and Gmail over IMAP, then Microsoft 365. See the [implementation plan](docs/IMPLEMENTATION_PLAN.md).

## Install (researchers)

When releases are available, download the installer for your system from [GitHub Releases](https://github.com/vpilh/manuscript-hub/releases). You won't need Git, Node.js or Rust.

1. Download the file for your system: `.msi`/`.exe` (Windows), `.dmg` (macOS), or `.AppImage`/`.deb`/`.rpm` (Linux).
2. Check the download against `SHA256SUMS.txt` on the same release page:
   - Windows (PowerShell): `Get-FileHash .\<file> -Algorithm SHA256`
   - macOS/Linux: `shasum -a 256 <file>`
3. Install and open it. Early builds are not code-signed yet, so Windows SmartScreen or macOS Gatekeeper may ask you to confirm.

## Build from source (contributors)

Prerequisites:

- Node.js 20 or newer
- Rust (stable) via [rustup](https://rustup.rs)
- The Tauri system dependencies for your OS: see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). On Windows that means the Microsoft C++ Build Tools and WebView2. On Linux it means `webkit2gtk-4.1` and related packages.

```sh
git clone https://github.com/vpilh/manuscript-hub.git
cd manuscript-hub
npm install
npm run tauri dev       # start the desktop app on the synthetic demo mailbox
```

No mailbox account or cloud credentials are needed for development.

| Task | Command |
|---|---|
| Run the desktop app (dev) | `npm run tauri dev` |
| UI only, in a browser (sample data, no Rust needed) | `npm run dev` |
| All Rust tests | `cargo test --workspace` |
| One crate / one test | `cargo test -p mh-parser` · `cargo test -p mh-parser german_acceptance` |
| Frontend tests | `npm test` |
| Lint | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm run typecheck` |
| Build installers | `npm run tauri build` |

## Project layout

```
crates/mh-core/        domain model (statuses, events, provenance)
crates/mh-parser/      detection and extraction (English + German)
crates/mh-connectors/  read-only mail providers, IMAP command guard, mock provider
crates/mh-store/       encrypted local storage, matching, review queue
src-tauri/             desktop shell
ui/                    React UI
fixtures/synthetic/    synthetic test emails
docs/                  architecture, threat model, implementation plan
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the pieces fit together.

## Contributing and security

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) first. Please never put real email content in issues, pull requests or fixtures.

To report a vulnerability, follow [SECURITY.md](SECURITY.md). Do not open a public issue.

## License

[MIT](LICENSE)
