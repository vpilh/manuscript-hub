# Threat model (draft)

This draft is refined in WP 4.2 before the first release. Update it whenever a change adds a trust boundary, a new kind of stored data, or a network destination.

## Assets

| Asset | Where it lives | Why it matters |
|---|---|---|
| Mailbox access (OAuth refresh tokens, IMAP credentials) | OS keychain | Gives access to the user's entire mailbox, not just journal mail |
| Email content (bodies, attachments) | Memory only, during parsing | Confidential correspondence, possibly unpublished research and third-party personal data |
| Submission metadata | SQLCipher database in per-user app data | Reveals the research pipeline, and rejections are sensitive |
| Database key | OS keychain | Decrypts the metadata |
| Release artifacts | GitHub Releases | A tampered installer compromises everything above |

## Trust boundaries

1. **App ↔ mail providers** (network, TLS). Certificates are validated, with no plaintext fallback.
2. **Email content ↔ app logic.** Every message is attacker-controllable input, since anyone can email the user.
3. **Webview ↔ Rust core** (Tauri IPC). Only explicitly registered commands and capabilities are exposed, and a strict CSP is set.
4. **App ↔ other local users and processes.** Data sits in the per-user directory, secrets in the OS keychain, and there are no network listeners.
5. **Source ↔ build ↔ user** (supply chain). This covers dependencies, CI and release signing.

## Threats and mitigations

| Threat | Mitigation | Status |
|---|---|---|
| The app modifies mail (bug or compromised dependency) | Read-only scopes; IMAP allowlist with default deny; integrity tests on mocks and a monitored inbox | Guard implemented; adapter tests in WP 2–3 |
| A phishing or spoofed "journal" email creates false deadlines or decisions | Everything is a proposal; confirmed data is never auto-overwritten; sender authenticity is treated as a signal, not proof | Proposal model in place; matching in WP 1.5 |
| Email content injects instructions (e.g. into a future local LLM) | Deterministic parsing first; LLM output schema-validated and reviewed; no tool access from parsed content | Policy; LLM deferred |
| Quoted old messages overwrite newer deadlines | Quote stripping; no unreviewed overwrite of confirmed deadlines | Stripping implemented |
| Email content or tokens leak through logs or crash reports | `TransientMessage` has no serde and a redacted `Debug`; no telemetry; secrets only in the keychain | Partly implemented |
| Theft of the device or database file | SQLCipher at rest; key in the OS keychain; encrypted backups | WP 1.3 |
| A malicious local web page or process calls app APIs | No HTTP listener; Tauri capabilities limited to the main window | In place |
| XSS in the webview from rendered email text | React escaping; CSP without remote sources or inline scripts; email HTML is never rendered | In place |
| A tampered release or dependency | cargo-deny, npm audit, Dependabot, gitleaks, dependency review; SHA-256 checksums; signing and notarization when certificates are available | CI in place; signing pending |
| Weak or missing TLS on GroupWise servers | Refuse connections without validated TLS; diagnostic wizard explains why | WP 3.1–3.2 |
