# Contributing

Local IT Desk has a narrow product boundary: local account authentication, staff tickets, attachments, announcements, in-app notifications, administration, branding, backup, and recovery.

Discuss scope changes before adding a dependency or route family. Keep browser requests same-origin and avoid required outbound services. Store runtime data outside the source tree.

## Development rules

- Add a failing contract test before changing a boundary or business rule.
- Comment exported declarations and non-obvious internal helpers.
- Keep requester, technician, and administrator permissions cumulative.
- Preserve requester ownership checks and staff-only internal notes on the server.
- Avoid secrets, identifying school data, and private infrastructure terms in fixtures or documentation.
- Update architecture and exclusion documentation when an approved boundary changes.

Run the complete verification block from [README.md](README.md) before opening a pull request. Include the commands you ran and their results in the pull request description.

Do not add automated-author attribution or unrelated formatting changes to a contribution.

## License

By submitting a contribution, you license it under the PolyForm Noncommercial License 1.0.0 and grant the project owner a perpetual, worldwide, irrevocable, royalty-free right to use, modify, sublicense, and relicense your contribution under any terms, including commercial terms.
