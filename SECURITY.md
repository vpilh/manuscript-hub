# Security policy

Manuscript Hub handles access to people's mailboxes, so we take vulnerability reports seriously.

## Reporting a vulnerability

Please report privately through GitHub: **[Report a vulnerability](https://github.com/vpilh/manuscript-hub/security/advisories/new)** (Security tab → *Report a vulnerability*). Do not open a public issue, and do not include real email content or credentials in your report.

Include the affected version, your OS, steps to reproduce, and the impact you expect. We aim to acknowledge reports within 7 days and will keep you updated until a fix is released. With your permission, we credit reporters in the advisory.

## Supported versions

Until 1.0, only the latest release gets security fixes.

## In scope

Especially relevant:

- Any way the app could modify, send, move or mark mail as read
- Email bodies, attachments, tokens or passwords being written to disk, logs or crash reports, or sent off the device
- Weaknesses in local database encryption, backup encryption or credential storage
- Content in an email that can make the app take actions (injection through untrusted mail content)
- Exposure of local IPC or listeners to other users, other processes or web pages
- TLS validation problems in mail connections, and tampering with release artifacts

See [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) for the assets and trust boundaries.
