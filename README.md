# Local IT Desk

Local IT Desk is a self-hosted support desk for staff on a local network. Named
users submit tickets, comments, and attachments. The administrator manages the
shared queue, staff accounts, announcements, categories, settings, and audit
history.

The application uses local usernames and passwords. It does not need a school
email tenant, an outside identity provider, or internet access while running.
Application data stays in one Docker volume on the host.

## Start an evaluation

Plain HTTP is for evaluation with throwaway accounts.
Do not enter real staff credentials or operational ticket data until trusted
HTTPS is working.

From the repository root:

~~~sh runbook-check
cp .env.example .env
docker build --tag local-it-desk:0.1.0 .
docker compose --project-name local-it-desk-evaluation config --quiet
docker compose --project-name local-it-desk-evaluation up --detach
docker compose --project-name local-it-desk-evaluation ps
~~~

Open http://localhost:8080/setup on the Docker host and create a test
administrator. Stop it with
docker compose --project-name local-it-desk-evaluation stop. Its isolated
named volume remains intact and is never reused by the school deployment.

For a school deployment, follow the [operator runbook](docs/RUNBOOK.md), then
complete the [trusted HTTPS procedure](docs/TLS.md) before creating staff
accounts. Backup and recovery procedures are in
[Backup and Restore](docs/BACKUP-RESTORE.md).

## Product boundary

The browser contains Dashboard, Tickets, Announcements, Settings, and
Administration. Roles are cumulative:

- A requester can create and follow their own tickets.
- A technician can work the shared ticket queue.
- An administrator can also manage accounts, settings, and audit history.

Anonymous submission is not available. New accounts receive generated
temporary passwords and must change them at first login.

The application makes no required outbound requests. Production serves the
browser application and REST API from one origin. SQLite, uploaded files,
backups, and runtime branding stay on the operator's host.

See [Architecture](docs/ARCHITECTURE.md), [staff roster import](docs/ROSTER-IMPORT.md),
and [excluded surfaces](docs/EXCLUDED-SURFACES.md) for the detailed contracts.

## Development

Install Rust 1.88, pnpm, and a Node.js release supported by Vite 8. The checked
in Rust toolchain installs rustfmt and Clippy.

Run the server:

~~~sh runbook-check
cargo run -p local-it-desk-server
~~~

Run the browser development server in another terminal:

~~~sh runbook-check
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend dev
~~~

Vite proxies /api and /health to http://127.0.0.1:3000. Production uses
relative same-origin URLs.

## Verification

~~~sh runbook-check
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm --dir frontend check
bash scripts/container-contract.sh
bash scripts/compose-contract.sh
bash scripts/check-runbook.sh
bash scripts/smoke-compose.sh
bash scripts/check-dependencies.sh
bash scripts/check-release-rights.sh
bash scripts/check-forbidden-surfaces.sh
bash scripts/check-private-terms.sh
~~~

Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing the product boundary.
Report security issues through the process in [SECURITY.md](SECURITY.md).

## License and provenance

Local IT Desk is licensed under the [Apache License 2.0](LICENSE). Source and
asset classifications are documented in [Provenance](docs/PROVENANCE.md), and
dependency license families are listed in
[Third-Party Notices](THIRD-PARTY-NOTICES.md).
