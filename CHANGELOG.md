# Changelog

All notable changes to Local IT Desk are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-08-01

### Added

- Staff-only, named local accounts with no anonymous submission or dependency
  on a school email tenant.
- Cumulative requester, technician, and administrator roles for tickets,
  comments, attachments, internal notes, assignment, and status workflows.
- Announcements, notifications, categories, settings, audit history, and CSV
  staff roster import.
- First-run administrator setup, generated temporary passwords, forced first
  password changes, administrator password resets, and session revocation.
- Containerized local-network deployment with Linux AMD64 and ARM64 images,
  digest-pinned operator bundles, checksums, an SPDX software bill of
  materials, and signed build provenance.
- Operator runbooks for installation, trusted LAN HTTPS, backup, restore,
  update, rollback, and administrator recovery.

### Pilot status

- Version 0.1.0 is the first teacher pilot release. Use invented accounts and
  data over plain HTTP. Configure trusted HTTPS before entering real staff
  identities, credentials, tickets, or attachments.
