# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Contact the repository owner through a private GitHub security advisory after the repository is published. Include the affected commit, reproduction steps, impact, and any proposed fix.

Security fixes target the latest published semantic-version release. Release
notes identify any older version that also receives a backport.

## Deployment expectations

The operator must place the finished service behind HTTPS before staff submit credentials. Restrict network access to the intended school network, keep the host patched, and protect the persistent data directory with host-level access controls.

Local-only hosting does not remove the need for authentication. The product
rejects anonymous ticket submission and uses named accounts. The service uses
Argon2id password hashes, opaque server-managed sessions, CSRF protection,
login rate limits, session revocation, and an atomic first-administrator setup
transaction.

The service stores uploads outside the web root under randomized names. It
rejects executable and active web content, sets defensive download headers,
and enforces size limits. School policy may still require malware scanning.

## Release security gates

The release gate scans the complete Git history and runtime image for secrets,
private infrastructure, excluded product surfaces, and unreviewed binaries.
It blocks fixed HIGH or CRITICAL vulnerabilities reported by Trivy. Unfixed
findings remain visible for review because an image rebuild cannot resolve
them. Version tags are immutable, so a security correction receives a new
patch release.

## Data handling

Do not commit school names, staff records, ticket contents, uploaded files, credentials, database files, or production network details. Tests must use fictional records. Follow school policy for retention, backups, privacy, and incident response.
