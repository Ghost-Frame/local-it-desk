# Local IT Desk Architecture

Local IT Desk is one Rust web server, one compiled Vue application, and one
SQLite database. This boundary keeps installation and recovery practical for a
small local network and avoids required cloud services.

## Request Flow

1. A browser connects to the Rust server over the operator-configured LAN URL.
2. The server handles `/api/*` and `/health/*` requests directly.
3. In production, the same server returns compiled frontend assets for all
   other paths and falls back to `index.html` for browser navigation.
4. In development, Vite serves the frontend and proxies API and health requests
   to the Rust server.
5. Route handlers use one shared SQLite pool. Startup runs migrations before
   the network listener accepts traffic.

The production application is same-origin. Cross-origin browser access is not
part of the deployment contract.

## Module Boundary

- `config` validates runtime settings and exposes only non-secret branding.
- `db` owns SQLite connection setup, migrations, and blocking database work.
- `auth` reserves the local account, role, and session boundary completed in
  Plan 02.
- `models` contains help-desk domain types and persistence code.
- `routes` mounts health, configuration, local-auth placeholders, tickets,
  attachments, users, and administration endpoints.
- `middleware` contains request-scoped behavior such as audit context.

The server does not include a second application process, message broker,
external database, or required outbound integration.

## Persistent Directories

| Path | Purpose |
|---|---|
| `data/` | SQLite database and its journal files |
| `uploads/` | Randomly named ticket and announcement attachments |
| `branding/` | Optional operator-provided logo and visual assets |
| `backups/` | Verified operator-created backup archives |

These paths are runtime data, not source assets. They are ignored by Git and
must be preserved across container replacement.

## Local Dependency Contract

After installation, normal operation requires no internet access, hosted
identity service, email service, analytics endpoint, remote font, remote
script, model provider, or update service. The browser communicates only with
the configured Local IT Desk origin.

## Foundation State

Plan 01 establishes the boundary and compilation contracts. Local account
creation, login, and session enforcement are intentionally incomplete until
Plan 02. Ticket, attachment, announcement, and administrator workflows are
completed in later plans. This foundation is not a public release.
