# Local IT Desk

Local IT Desk is a self-hosted support desk for staff on a local network. Named
users submit tickets, comments, and attachments. The administrator manages the
shared queue, staff accounts, announcements, categories, settings, and audit
history.

The application uses local usernames and passwords. It does not need a school
email tenant, an outside identity provider, or internet access while running.
Application data stays in one Docker volume on the host.

## Start here

For a real local-network installation, use the
[five-part Quick Start](QUICKSTART.md). It covers the host, one-command install,
trusted HTTPS, the guided first run, named staff accounts, and everyday backup
commands. Keep the [Operator Runbook](docs/RUNBOOK.md) beside it for recovery
and maintenance. The printable [Staff Guide](docs/STAFF-GUIDE.md) explains the
day-to-day workflow without administrator details.

The normal Linux or WSL installation command is:

~~~sh runbook-check
./scripts/desk install --host helpdesk.local --name 'School IT Desk' --support 'Call the main office'
~~~

## Optional evaluation

Plain HTTP is for evaluation with throwaway accounts.
Do not enter real staff credentials or operational ticket data until trusted
HTTPS is working.

From the repository root:

~~~sh runbook-check
cp .env.example .env
docker build --tag local-it-desk:0.1.1 .
export LOCAL_IT_DESK_IMAGE=local-it-desk:0.1.1
docker compose --project-name local-it-desk-evaluation config --quiet
docker compose --project-name local-it-desk-evaluation up --detach
docker compose --project-name local-it-desk-evaluation ps
~~~

Open http://localhost:8080/setup on the Docker host and create a test
administrator. Stop it with
docker compose --project-name local-it-desk-evaluation stop. Its isolated
named volume remains intact and is never reused by the school deployment.

For a school deployment, use the Quick Start above rather than this HTTP
evaluation. Backup and recovery procedures are in [Backup and
Restore](docs/BACKUP-RESTORE.md).

## Install a published release

Download the versioned archive and matching `.sha256` file from
[GitHub Releases](https://github.com/Ghost-Frame/local-it-desk/releases). Verify
the outer checksum before extraction, then verify the bundle manifest:

~~~sh runbook-check
sha256sum --check local-it-desk-0.1.1.tar.gz.sha256
tar --extract --gzip --file local-it-desk-0.1.1.tar.gz
cd local-it-desk-0.1.1
sha256sum --check SHA256SUMS
~~~

The packaged Compose file pins the application image by digest. Follow the
included operator runbook and configure trusted HTTPS before using real staff
accounts or ticket data.

## Product boundary

The browser contains Tickets, Announcements, Settings, and a worker-only Manage
Desk area. Roles are cumulative:

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
bash scripts/https-smoke.sh
bash scripts/check-runbook.sh
bash scripts/smoke-compose.sh
bash scripts/check-dependencies.sh
bash scripts/check-release-rights.sh
bash scripts/check-history.sh
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
