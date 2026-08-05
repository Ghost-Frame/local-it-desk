# Local IT Desk Quick Start

This is the normal setup path for one school-owned computer and named staff
accounts. It starts the desk with trusted local HTTPS, stores its data in Docker
volumes, and does not require access to the school's email system.

Keep this folder on the host computer. Run every command from this folder.

## 1. Prepare the host

Give the host a stable local-network address. Create a local DNS record such as
`helpdesk.local` that points to that address. Confirm the name resolves from a
staff computer before creating accounts.

Install one of these supported environments:

- Linux with Docker Engine and the Docker Compose plugin.
- Windows with Docker Desktop, WSL 2, and Docker Desktop's WSL integration.

Keep Docker running. On Linux or in WSL, verify it:

~~~sh runbook-check
docker version
docker compose version
~~~

## 2. Install the desk

On Linux or in WSL, run:

~~~sh runbook-check
./scripts/desk install --host helpdesk.local --name 'School IT Desk' --support 'Call the main office'
~~~

On Windows PowerShell, run the equivalent wrapper:

```powershell
.\scripts\desk.ps1 install --host helpdesk.local --name "School IT Desk" --support "Call the main office"
```

The installer uses the configured image, starts the app, waits for both
containers to become healthy, and exports the public trust certificate to
`exports/local-it-desk-root.crt`. A published release downloads its immutable
image when needed. A source checkout builds its reviewed local image.

If `.env` already exists, installation stops without replacing it. Use
`./scripts/desk start` for an existing installation.

## 3. Trust HTTPS on staff devices

Staff devices must trust `exports/local-it-desk-root.crt`. This file is public
and contains no private key.

For a few managed Windows computers, open PowerShell as an administrator in
this folder and run:

```powershell
certutil.exe -addstore -f Root .\exports\local-it-desk-root.crt
```

For a Windows domain, deploy the same certificate to Trusted Root
Certification Authorities with Group Policy. For other managed platforms,
use the school's certificate profile or device-management system. The complete
platform notes and verification steps are in [Trusted HTTPS](docs/TLS.md).

Open `https://helpdesk.local` from one staff device. Stop if the browser shows
a certificate warning. Never tell staff to bypass the warning.

## 4. Complete browser setup

The first browser visit opens a four-step setup:

1. Name the desk and enter a short support contact.
2. Create the one administrator and technician account.
3. Paste staff names to create named requester accounts.
4. Print or download the one-time login cards and finish.

Each staff member gets a separate username and temporary password. The QR code
contains only the desk address. Staff must choose a new password at first
login. Anonymous requests are not available.

Give staff the [Staff Guide](docs/STAFF-GUIDE.md) with their individual login
card.

## 5. Use the everyday commands

~~~sh runbook-check
./scripts/desk status
./scripts/desk backup
./scripts/desk stop
./scripts/desk start
~~~

Copy every backup archive and its `.sha256` file from `backups/` to a different
managed device. A backup left only on the host is not protection from host
failure.

Use these less often:

~~~sh runbook-check
./scripts/desk certificate
./scripts/desk support
~~~

Use the update procedure in the operator runbook only with an exact approved
versioned image tag or digest. The launcher creates a verified backup first and
restores the previous image setting if health checks fail.

## If something goes wrong

Run `./scripts/desk status`. If support is needed, run
`./scripts/desk support`, open every generated text file, and review it before
sharing the archive. The bundle is designed to omit the database, attachments,
backups, and passwords.

Use the [Operator Runbook](docs/RUNBOOK.md) for password recovery, restore,
rollback, migration, security boundaries, and detailed troubleshooting.
