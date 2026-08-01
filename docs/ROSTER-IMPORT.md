# Staff Roster Import

Local IT Desk can create named staff accounts from a CSV file. Import is an
administrator-only operation with separate preview and apply steps. Preview
never changes the database. Apply creates every account in one transaction or
creates none of them.

## File format

Save the file as UTF-8 CSV with this exact first line:

```csv
username,display_name,role,email
```

Each later non-empty line contains one account:

```csv
teacher.one,Teacher One,requester,teacher.one@example.invalid
helpdesk.tech,Help Desk Technician,technician,
backup.admin,Backup Administrator,administrator,
```

The fields are:

- `username`: 3 to 32 characters using lowercase letters, numbers, `.`, `_`,
  or `-`. Uppercase input is normalized to lowercase.
- `display_name`: 2 to 80 Unicode characters.
- `role`: exactly `requester`, `technician`, or `administrator`.
- `email`: optional metadata. It is not used for login or recovery.

The parser accepts quoted CSV fields and ignores empty lines. Give each row
exactly four columns. Do not begin a cell with `=`, `+`, `-`, or `@` after
leading whitespace; the parser rejects those spreadsheet formula prefixes.

## Limits

The server rejects a request before import when its raw body exceeds
`MAX_ROSTER_BYTES`. The default is 1 MiB. It reports a preview error when the
number of non-empty staff rows exceeds `MAX_ROSTER_ROWS`. The default is 500.

The supported configuration ranges are:

- `MAX_ROSTER_BYTES`: 1 KiB through 10 MiB
- `MAX_ROSTER_ROWS`: 1 through 10,000

## Preview and apply

The browser sends the selected file as a raw `text/csv` request to:

```text
POST /api/admin/users/import/preview
```

The response contains normalized rows, a `valid` flag, and safe row-specific
errors. Errors identify the line and field but do not echo rejected cell
contents. Preview also reports usernames that already exist in the database.

Only apply a roster after preview reports `valid: true`:

```text
POST /api/admin/users/import/apply
```

Apply parses and validates the file again. This prevents a changed file from
bypassing preview. A malformed row, duplicate normalized username, existing
account, or concurrent database conflict cancels the complete transaction.

## One-time password handling

A successful apply response contains one generated temporary password for each
created account. Each account must change that password before using normal
help-desk features. The server sets `Cache-Control: no-store` on the response.

Download or print the result immediately, give each password only to its named
staff member, and then close the result screen. Local IT Desk stores only the
Argon2id password hashes. It does not store or write the temporary passwords to
the audit log, and it cannot show the same passwords again. If you lose one, use
the administrator password-reset action to generate a replacement.
