## What and why

<!-- Link the issue or work package, e.g. "WP 1.4 (FR07–FR09)". -->

## Checklist

- [ ] Tests cover the change (`cargo test --workspace`, `npm test`)
- [ ] No mailbox-mutating calls, scopes or IMAP commands were added
- [ ] No email bodies, attachments, credentials or tokens are persisted, logged or sent off-device
- [ ] Fixtures are synthetic (no real names, addresses or manuscript IDs)
- [ ] `docs/` updated if architecture, scope or a decision changed
