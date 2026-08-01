# Backup and Restore

Local IT Desk stores the active database, attachments, branding, backup
archives, staging generations, and quarantined generations in one named state
volume. The maintenance command uses SQLite online backup semantics. Never copy
the live database file as a backup.

## Create and verify an online backup

The application can stay online during backup. Run from the release directory:

~~~sh runbook-check
compose=(docker compose -f compose.yaml -f compose.https.yaml)
backup_name="local-it-desk-$(date -u +%Y%m%dT%H%M%SZ).tar.gz"
"${compose[@]}" exec -T app \
  /app/local-it-desk-admin backup \
  --database /state/current/data/local-it-desk.db \
  --attachments /state/current/attachments \
  --branding /state/current/branding \
  --output "/state/backups/$backup_name"
"${compose[@]}" exec -T app \
  /app/local-it-desk-admin verify-backup \
  --archive "/state/backups/$backup_name"
printf 'Verified in-volume archive: %s\n' "$backup_name"
~~~

Expected result: backup reports its payload count, byte count, and schema
version, then verify reports the same values. The output path refuses to
overwrite an existing archive.

For an HTTP evaluation, define compose=(docker compose -f compose.yaml) instead.
Do not put production staff data into the HTTP profile.

## Copy and verify off host

Copy the verified archive to the operator's protected backup directory, verify
the exported copy through the application command, and record its checksum:

~~~sh runbook-check
test -n "$backup_name"
mkdir -p backups
chmod 0700 backups
"${compose[@]}" cp "app:/state/backups/$backup_name" "backups/$backup_name"
"${compose[@]}" run --rm --no-deps \
  --volume "$PWD/backups:/operator-backups:ro" \
  --entrypoint /app/local-it-desk-admin app \
  verify-backup --archive "/operator-backups/$backup_name"
(
  cd backups
  sha256sum "$backup_name" |
    tee "$backup_name.sha256"
)
chmod 0600 "backups/$backup_name" "backups/$backup_name.sha256"
~~~

Next copy both files to a mounted destination on a different device or managed
storage system:

~~~sh runbook-check
read -r -p 'Mounted off-host backup directory: ' off_host_backup_dir
test -d "$off_host_backup_dir"
cp "backups/$backup_name" "$off_host_backup_dir/$backup_name"
cp "backups/$backup_name.sha256" "$off_host_backup_dir/$backup_name.sha256"
(
  cd "$off_host_backup_dir"
  sha256sum --check "$backup_name.sha256"
)
~~~

Expected result: the destination checksum prints OK. A local archive alone is
not an accepted backup because the host and its volume share one failure
domain.

Record the archive name, UTC creation time, application version, destination,
verification result, and operator in the school's backup log. Protect backups
as confidential school data.

## Restore dry-run

Place the selected archive back in the active project's backup directory if
needed:

~~~sh runbook-check
read -r -p 'Backup archive filename: ' backup_name
case "$backup_name" in
  ''|*/*|*[!A-Za-z0-9._-]*|*.tar.gz.tar.gz) printf 'Enter one plain .tar.gz filename.\n' >&2; exit 1;;
  *.tar.gz) ;;
  *) printf 'The archive must end in .tar.gz.\n' >&2; exit 1;;
esac
test -f "backups/$backup_name"
(
  cd backups
  sha256sum --check "$backup_name.sha256"
)
compose=(docker compose -f compose.yaml -f compose.https.yaml)
"${compose[@]}" cp "backups/$backup_name" "app:/state/backups/$backup_name"
USE_HTTPS=true bash scripts/restore-compose.sh "$backup_name" --dry-run
~~~

Expected result: the wrapper prints the exact target container, volume,
archive, and active generation, then reports that the target stayed unchanged.
Stop if the project name or volume is not the intended production target.

For an HTTP evaluation restore, omit USE_HTTPS=true.

## Apply a production restore

A restore discards application changes made after the selected archive. Get
the service owner's approval for that data boundary. Confirm the dry-run,
archive timestamp, project, volume, and expected lost interval before apply.

~~~sh runbook-check
test -n "$backup_name"
USE_HTTPS=true bash scripts/restore-compose.sh "$backup_name" --apply
docker compose -f compose.yaml -f compose.https.yaml ps
~~~

The wrapper stops only app. Apply creates a new pre-restore backup, stages and
validates the selected archive, quarantines the old active generation, activates
the restored generation, starts app, and waits for healthy status. If
activation fails, the maintenance command restores the original generation.

After health succeeds, verify directly in the browser:

1. The expected accounts exist and can sign in.
2. A known ticket has the expected public comments.
3. Internal comments remain visible only to support staff.
4. A known attachment downloads and opens.
5. Announcements and notification state match the selected backup time.

Keep the automatic pre-restore archive and quarantined generation until the
restore has passed the school's observation period.

## Restore rehearsal

Use a separate host or an explicitly isolated Compose project. Start a clean
instance, copy the archive into that project's state volume, run dry-run and
apply, then perform the direct browser checks above. The rehearsal must not
reuse the production project name, ports, or volume.

The repository's automated equivalent is:

~~~sh runbook-check
bash scripts/smoke-compose.sh
~~~

The smoke journey creates a unique project, verifies clean setup and account
flows, creates a backup, updates the image, mutates data, restores the archive,
checks the restored records and attachment checksum, rolls the image back, and
tests stop-start recovery.

## Backup failure handling

- If backup reports a lock or I/O error, leave the application running, check
  free disk space and container health, then retry with a new archive name.
- If verification fails, quarantine that archive and create a new one. Do not
  copy or restore the failed archive.
- If the off-host checksum fails, replace the destination copy and verify
  again. Do not mark it accepted.
- If restore health fails, preserve the printed pre-restore and quarantine
  paths. Restart app if needed and collect a sanitized support bundle from the
  [operator runbook](RUNBOOK.md).
