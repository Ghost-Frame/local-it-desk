# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Contact the repository owner through a private GitHub security advisory after the repository is published. Include the affected commit, reproduction steps, impact, and any proposed fix.

Plan 01 is a development foundation. It has no supported release line.

## Deployment expectations

The operator must place the finished service behind HTTPS before staff submit credentials. Restrict network access to the intended school network, keep the host patched, and protect the persistent data directory with host-level access controls.

Local-only hosting does not remove the need for authentication. The product rejects anonymous ticket submission and uses named accounts. Later plans add Argon2id password hashes, opaque server-managed sessions, CSRF protection, login rate limits, session revocation, and the first-administrator setup transaction.

Uploaded files will live outside the web root under randomized names. The finished service will reject executable and active web content, set download headers, and enforce size limits. Those controls do not replace malware scanning required by school policy.

## Data handling

Do not commit school names, staff records, ticket contents, uploaded files, credentials, database files, or production network details. Tests must use fictional records. Follow school policy for retention, backups, privacy, and incident response.
