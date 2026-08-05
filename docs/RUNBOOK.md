# Local IT Desk Operator Runbook

This runbook is for the staff member who owns the Local IT Desk host and the
administrator account. Run commands from the extracted release directory in a
Bash shell. Keep that directory and its `.env`, `backups`, and `support`
contents restricted to the operator. Begin with the [Quick Start](../QUICKSTART.md)
for a normal installation. This document covers deeper operation and recovery.

The deployment pattern is PROVEN-IN-CONTEXT: one isolated application
container, one reverse proxy at the network edge, and one named state volume.
Its main failure mode is losing the only host and its volume. A verified
off-host backup is the recovery path.

## 1. Prerequisites

Use a supported Linux host with Docker Engine and Docker Compose 2.33.1 or
newer for the school deployment. The deployment uses Compose gateway priority
to keep the application isolated behind its edge proxy. Give the host a stable
LAN address and DNS name. Install host security updates, enable the host
firewall, and allow HTTPS only from school-managed networks.

Docker Desktop can run an evaluation on Windows, macOS, or Linux. On Windows,
use the `scripts/desk.ps1` wrapper from PowerShell or a WSL 2 Bash shell with
Docker Desktop integration. Keep Docker Desktop running before entering
commands.

Verify the engine and Compose:

~~~sh runbook-check
docker version
docker compose version
docker info --format '{{.ServerVersion}}'
~~~

Expected result: all three commands exit successfully and report a server
version. If the client cannot reach the server, start Docker before continuing.

Recommended minimum host capacity is two CPU cores, 4 GiB of memory, and enough
protected disk space for the state volume plus at least two local backup
archives. Capacity depends on attachment use.

## 2. Verify the release

Place the published operator archive and checksum file in an empty working
directory, then run:

~~~sh runbook-check
sha256sum --check local-it-desk-0.2.1.tar.gz.sha256
tar --extract --gzip --file local-it-desk-0.2.1.tar.gz
cd local-it-desk-0.2.1
sha256sum --check SHA256SUMS
jq -e '.version == "0.2.1" and (.image.digest | test("^sha256:[0-9a-f]{64}$"))' \
  release/release-metadata.json
docker compose config --quiet
~~~

Expected result: both checksum commands print OK, the metadata check prints
true, and Compose accepts the configuration. Stop if any command fails, the
archive came through an untrusted channel, or the version is not the one
approved for the school.

Pull and inspect the digest-pinned application image supplied by the bundle:

~~~sh runbook-check
approved_image="$(jq -er '.image.immutable_reference' release/release-metadata.json)"
test -n "$approved_image"
docker pull "$approved_image"
docker image inspect "$approved_image" --format '{{.Id}} {{.Config.User}}'
~~~

Expected result: the configured user is 10001:10001. Do not deploy an image
whose digest differs from the approved release metadata.

## 3. Evaluation startup

HTTP exposes passwords and session traffic to anyone who can observe the
network path. Use it only for a short evaluation with throwaway accounts and
test tickets. Never enter real staff credentials, personal information, or
operational support data over HTTP.

For an evaluation on the Docker host:

~~~sh runbook-check
cp .env.example .env
docker compose --project-name local-it-desk-evaluation config --quiet
docker compose --project-name local-it-desk-evaluation up --detach
docker compose --project-name local-it-desk-evaluation ps
curl --fail --silent --show-error http://127.0.0.1:8080/health/ready
~~~

Expected result: app and caddy become healthy and the final command prints
{"status":"ready"}. Open http://localhost:8080/setup and create a test-only
administrator. First setup closes atomically after the first administrator is
created.

To evaluate from another LAN computer, set APP_ORIGIN to the exact LAN address
before startup:

~~~sh runbook-check
host_lan_ip="$(hostname -I | awk '{print $1}')"
test -n "$host_lan_ip"
awk -v origin="http://$host_lan_ip:8080" \
  'BEGIN { replaced=0 } /^APP_ORIGIN=/ { print "APP_ORIGIN=" origin; replaced=1; next } { print } END { if (!replaced) exit 1 }' \
  .env > .env.next
mv .env.next .env
printf 'Evaluation address: http://%s:8080/setup\n' "$host_lan_ip"
docker compose --project-name local-it-desk-evaluation up --detach
~~~

If the address changes, update APP_ORIGIN and restart the app before signing
in. Browser origins that do not match APP_ORIGIN are rejected. Keep the
evaluation project and its test data separate. The school HTTPS deployment
uses the default local-it-desk project and a different named volume.

## 4. School HTTPS

Follow the [Quick Start](../QUICKSTART.md) and [Trusted HTTPS](TLS.md). The
launcher creates the configuration, starts the school profile, waits for
health, and exports the public local trust root. The school profile removes the
application's direct HTTP port, enables secure cookies, and exposes only the
reverse proxy on HTTPS.

~~~sh runbook-check
./scripts/desk install --host helpdesk.local --name 'School IT Desk' --support 'Call the main office'
./scripts/desk status
./scripts/desk certificate
~~~

Do not create real staff accounts until all of these observations are true:

1. The browser address matches HTTPS_ORIGIN.
2. Managed staff computers trust the certificate without a warning.
3. The certificate subject alternative name contains the help desk DNS name.
4. The application and reverse proxy both report healthy.

## 5. Staff accounts and roster

During first-run setup, paste staff names into the guided staff step. Later,
sign in as the administrator and open **Manage Desk**, then **People**.

- Create one named account per staff member. Never share accounts.
- Give ordinary staff the requester role.
- Use technician only for staff who work the shared queue.
- Use administrator only for staff who manage accounts and configuration.
- Deliver each generated temporary password directly to its named owner.
- Ask the recipient to sign in and replace it immediately.
- Deactivate accounts when staff leave. Deactivation revokes their sessions.

For several accounts, paste one staff name per line into **Quick add staff**.
The application suggests unique usernames, previews a requester-only roster,
and creates printable one-time login cards. Use the CSV workflow in [Staff
Roster Import](ROSTER-IMPORT.md) only when an existing spreadsheet is easier.
Keep returned one-time passwords out of email lists, tickets, screenshots, and
support bundles.

The application prevents removal of the final active administrator. The
initial single-administrator design is supported, but a planned absence leaves
no in-application recovery operator. Store this runbook and the release
directory where an authorized substitute can reach them.

## 6. Daily status, disk, and logs

Use the launcher for the routine health view, then check host disk capacity:

~~~sh runbook-check
./scripts/desk status
docker system df
~~~

Expected result: app and caddy are healthy. Investigate a growing attachments
directory or a nearly full host disk before it prevents SQLite or backup
writes.

View a bounded log window:

~~~sh runbook-check
compose=(docker compose -f compose.yaml -f compose.https.yaml)
"${compose[@]}" logs --no-color --tail 100 app caddy
~~~

Logs rotate at 10 MiB with three files per service. Do not publish raw logs.
Use the sanitized support procedure in section 13.

## 7. Pinned update and pre-update backup

Never update by using a floating image tag. Copy a current verified backup off
host, then give the launcher the approved immutable SHA-256 digest.
It creates another verified backup, preserves the current environment, checks
the new image, and restores the prior image setting if health fails.

~~~sh runbook-check
read -r -p 'Approved digest-pinned image reference: ' next_image
test -n "$next_image"
if [[ ! "$next_image" =~ @sha256:[0-9a-f]{64}$ ]]; then
  printf 'The image reference must end in an immutable SHA-256 digest.\n' >&2
  exit 1
fi
./scripts/desk update "$next_image"
./scripts/desk status
~~~

Expected result: the replacement app becomes healthy and caddy stays healthy.
Sign in and directly inspect an existing ticket, attachment, announcement, and
account list. Keep the pre-update archive and saved environment file until the
new version has passed the school's normal operating period.

## 8. Backup verification and off-host copy

Create an application backup with `./scripts/desk backup`, then follow [Backup
and Restore](BACKUP-RESTORE.md). A file is not an accepted backup until the
maintenance command verifies it, it has been copied to a different device or
managed storage system, and the copied checksum has been verified at that
destination.

~~~sh runbook-check
./scripts/desk backup
~~~

## 9. Restore rehearsal and production restore

Rehearse restoration on an isolated test host at least once per release and
after any backup procedure change. Do not point a rehearsal at the production
Compose project or volume.

For a production restore, follow [Backup and Restore](BACKUP-RESTORE.md). The
restore wrapper first runs a non-mutating verification, stops only app, creates
a pre-restore safety backup, stages the archive, quarantines the old generation,
activates the restored generation, and waits for health.

## 10. Administrator recovery

Use browser password reset when another active administrator can sign in. For
the only administrator, first complete a verified backup while the application
is running. Then run the offline recovery command:

~~~sh runbook-check
compose=(docker compose -f compose.yaml -f compose.https.yaml)
read -r -p 'Exact normalized administrator username: ' administrator_username
test -n "$administrator_username"
"${compose[@]}" stop app
"${compose[@]}" run --rm --no-deps --entrypoint /app/local-it-desk-admin app \
  reset-password \
  --database /state/current/data/local-it-desk.db \
  --username "$administrator_username"
"${compose[@]}" up --detach app
"${compose[@]}" ps
~~~

The command prompts twice without echoing the new password. It accepts only an
existing administrator, forces a password change at next login, and revokes all
sessions for that account. If recovery fails, restart app before investigating:

~~~sh runbook-check
compose=(docker compose -f compose.yaml -f compose.https.yaml)
"${compose[@]}" up --detach app
"${compose[@]}" ps
~~~

## 11. Image and data rollback

If the new image fails before any useful data changes, restore the most recent
saved environment file and recreate app:

~~~sh runbook-check
previous_env="$(find backups -maxdepth 1 -type f -name 'env-before-*' -printf '%T@ %p\n' | sort -nr | awk 'NR==1 { sub(/^[^ ]+ /, ""); print; exit }')"
test -n "$previous_env"
cp "$previous_env" .env
compose=(docker compose -f compose.yaml -f compose.https.yaml)
"${compose[@]}" config --quiet
"${compose[@]}" up --detach app
"${compose[@]}" ps
~~~

If the update changed stored data, rolling back only the image may be
incompatible. Restore the verified pre-update archive after restoring the old
image reference. This discards data created after that archive. Confirm the
archive timestamp and business impact before applying it.

## 12. Host migration and safe stop

Before migration, create and verify a current backup and copy it off host.
Record the pinned image reference and HTTPS DNS name. Copy the release
directory, `.env`, and the verified archive and checksum through the school's
approved protected channel. A new host creates a new local HTTPS authority, so
plan to redeploy its exported public root to staff devices.

Stop without deleting containers or volumes:

~~~sh runbook-check
./scripts/desk stop
./scripts/desk status
~~~

On the new host, verify the release checksum, load or build the approved image,
place the certificates, start a clean project, copy the archive into its state
volume, and follow the restore procedure. Change DNS only after the new host is
healthy and direct record checks pass. Keep the old stopped host unchanged
until the migration has passed the agreed observation period.

## 13. Sanitized support bundle

Raw logs, Compose configuration, the database, attachments, backups, browser
storage, and screenshots can contain sensitive material. Never include them
without review.

Create a bounded diagnostic bundle that omits application data and redacts
common identifiers:

~~~sh runbook-check
./scripts/desk support
~~~

Open every text file and check it against school privacy policy before sending
the archive. If the automatic scan stops, edit the retained text files to
remove sensitive values, repeat the scan, and create a new archive. Do not add
ticket bodies, attachments, account exports, backup archives, cookies, or
credentials.
