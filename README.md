# Local IT Desk

Local IT Desk is a self-hosted support desk for staff on a local network. Named users submit tickets, add public comments and attachments, and follow technician updates. Administrators manage accounts, categories, announcements, settings, and audit history.

The project uses built-in usernames and passwords. It does not require access to a school email tenant or an outside identity provider. The server stores sessions and application data on the host running the container.

## Current status

Plan 01 establishes the repository, server boundary, database schema, domain policy, and browser shell. Retained business endpoints return `501 Not Implemented` until later plans connect authentication and ticket persistence.

Do not treat this commit as a public release or school deployment. It does not include the container image, production Compose file, first-run account setup, or operator runbook.

## Product boundary

The approved browser navigation contains Dashboard, Tickets, Announcements, Settings, and Administration. The three cumulative roles are `requester`, `technician`, and `administrator`.

The application makes no required outbound requests. Production serves the compiled browser application and REST API from one origin. SQLite, uploaded files, and runtime branding stay on the operator's host.

See [Architecture](docs/ARCHITECTURE.md) and [Excluded surfaces](docs/EXCLUDED-SURFACES.md) for the enforced boundaries.

## Development

Install Rust 1.88, pnpm, and a Node.js release supported by Vite 8. The checked-in Rust toolchain file installs `rustfmt` and Clippy.

Run the server:

```sh
cargo run -p local-it-desk-server
```

Run the browser development server in another terminal:

```sh
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend dev
```

Vite proxies `/api` and `/health` to `http://127.0.0.1:3000`. Production uses relative same-origin URLs.

## Verification

```sh
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm --dir frontend check
bash scripts/check-dependencies.sh
bash scripts/check-forbidden-surfaces.sh
bash scripts/check-private-terms.sh
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before changing the product boundary. Report security issues through the process in [SECURITY.md](SECURITY.md).
