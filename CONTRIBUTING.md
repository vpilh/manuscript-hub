# Contributing to Manuscript Hub

Thanks for helping. This guide covers setup, the rules every change must follow, and how releases are made.

## Setup

Follow *Build from source* in the [README](README.md). A fresh checkout runs entirely on the synthetic fixtures in `fixtures/synthetic/`. You never need real mailbox credentials to develop or test.

## Non-negotiable rules

Pull requests that break these are not merged, however useful the feature:

1. **Mailboxes stay read-only.** Do not request send/modify scopes (`gmail.modify`, `Mail.ReadWrite`, `Mail.Send`) or add mutating API calls. Every IMAP command must pass `mh_connectors::imap_guard::check_command`. Open folders with `EXAMINE` and fetch with `BODY.PEEK`.
2. **Email content stays on the device and in memory.** Do not persist, log or transmit message bodies, attachments, tokens or passwords. `TransientMessage` is deliberately not `Serialize`; keep it that way.
3. **Email is untrusted input.** Never treat message text as instructions. Results are proposals for the review queue, and confirmed data is never overwritten without review.
4. **Fixtures are synthetic.** Use made-up names, `example.org`/`example.com` domains and invented manuscript IDs. Never commit a real `.eml`, even an "anonymized" one.

## Where code goes

Business logic belongs in the `crates/` domain crates, which build and test without Tauri. Keep `src-tauri` limited to IPC commands, windowing and lifecycle. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

### Adding a parser rule or fixture

1. Add a synthetic `.eml` to `fixtures/synthetic/` that reproduces the wording.
2. Add an expectation in `crates/mh-parser/tests/synthetic_fixtures.rs`.
3. Extend the phrase tables or patterns in `crates/mh-parser/src/` until the test passes without breaking others.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run typecheck && npm test
```

CI also runs cargo-deny (licenses and advisories), npm audit, gitleaks secret scanning and dependency review.

## Releases (maintainers)

1. Bump the version in `Cargo.toml` (`[workspace.package]`), `package.json` and `src-tauri/tauri.conf.json`.
2. Commit, then tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
3. The *Release* workflow builds Windows, macOS and Linux installers into a **draft** GitHub Release and attaches `SHA256SUMS.txt`.
4. Check the assets, then publish the draft.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
