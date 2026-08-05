# Teacher pilot checklist

This checklist is for the first staff-only pilot of Local IT Desk. Give the
pilot administrator only the public `v0.2.1` GitHub release and the runbook
inside its downloaded bundle. Do not use a private development checkout.

Plain HTTP is limited to evaluation with invented accounts, tickets, comments,
and attachments. Configure trusted LAN HTTPS from a staff device before using
real identities, credentials, ticket contents, or files.

Record a timestamp and pass or fail for every item. Stop and investigate a
failed security, recovery, or data-isolation check before continuing.

## Release and installation

- [ ] Record the `v0.2.1` GitHub release URL.
- [ ] Verify the archive against its adjacent SHA-256 checksum before
  extraction.
- [ ] Follow the bundled runbook on a fresh host without consulting the source
  repository.
- [ ] Record the image manifest digest and confirm it matches the digest-pinned
  Compose file.
- [ ] Confirm the host starts the service after a normal power cycle.
- [ ] Open `/healthz` and the sign-in page from a separate staff device.

## Staff workflow

- [ ] Complete first-run setup and create the single administrator and
  technician account.
- [ ] Create one invented requester account and complete its required first
  password change.
- [ ] Sign in as the requester and create a ticket with a comment and harmless
  test attachment.
- [ ] Sign in as the administrator and verify the shared queue, assignment,
  internal note, status change, and requester-visible reply.
- [ ] Publish an announcement and verify the requester notification state.
- [ ] Confirm the requester cannot view another requester's ticket or internal
  note.
- [ ] Disable the requester, confirm sign-in is denied, then restore access.
- [ ] Reset the requester password and confirm the required password change.
- [ ] Revoke the requester's sessions and confirm the active session ends.
- [ ] Review the audit history for the administrative actions above.

## Operations and recovery

- [ ] Create a backup using the bundled procedure.
- [ ] Restore that backup into the documented isolated recovery project and
  verify the invented ticket and attachment.
- [ ] Rehearse an update with the documented backup checkpoint.
- [ ] Rehearse rollback to the prior digest without changing the data volume.
- [ ] Rehearse administrator recovery from the Docker host.
- [ ] Confirm HTTPS is trusted by the staff device and HTTP redirects as
  documented before any real-data phase.

## Pilot record

Record only the release version, image digest, host platform, browser and
version, elapsed time, pass or fail results, and privacy-scrubbed defect notes.
Do not record staff or school identities, passwords, IP addresses, ticket text,
attachment contents, or production configuration values in the public issue
tracker.

A release-blocking defect becomes a privacy-scrubbed public issue and a new
patch release. Never move, replace, or delete an existing version tag.
