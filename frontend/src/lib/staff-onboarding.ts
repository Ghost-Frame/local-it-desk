/** Pure staff-name onboarding utilities shared by setup and administration. */

import type { OneTimeCredential } from "../types/api.js";

/** Maximum number of accounts accepted by one server roster transaction. */
export const MAX_QUICK_STAFF = 500;

/** One generated requester account row ready for roster preview. */
export interface StaffRosterDraft {
  /** Human-facing staff name after safe whitespace normalization. */
  displayName: string;
  /** Deterministic local login suggestion. */
  username: string;
}

/** One actionable problem tied to a pasted source line. */
export interface StaffNameError {
  /** One-based source line, or zero for a whole-input limit. */
  line: number;
  /** Operator-facing validation message. */
  message: string;
}

/** Complete non-throwing result for pasted staff names. */
export interface StaffNameParseResult {
  /** Whether every nonblank source line can be previewed by the server. */
  valid: boolean;
  /** Generated requester rows in source order. */
  rows: StaffRosterDraft[];
  /** Validation errors that block server preview. */
  errors: StaffNameError[];
}

/** Collapses compatible Unicode and repeated whitespace for display. */
function normalizeDisplayName(value: string): string {
  return value.normalize("NFKC").trim().replace(/\s+/gu, " ");
}

/** Converts one name fragment into conservative ASCII username characters. */
function usernameFragment(value: string): string {
  return value
    .normalize("NFKD")
    .replace(/\p{Mark}/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/gu, "");
}

/** Fits a collision suffix while preserving the server's 32-character limit. */
function withSuffix(base: string, suffix: string): string {
  return `${base.slice(0, 32 - suffix.length)}${suffix}`;
}

/** Suggests one unique first-and-last-name username without throwing. */
export function suggestUsername(displayName: string, usedUsernames: ReadonlySet<string>): string | null {
  const normalized = normalizeDisplayName(displayName);
  const fragments = normalized.split(" ").map(usernameFragment).filter(Boolean);
  if (fragments.length === 0) return null;

  const first = fragments[0] ?? "";
  const last = fragments.length > 1 ? fragments.at(-1) ?? "" : "";
  let base = last ? `${first}.${last}` : first;
  if (base.length < 3) base = `${base}.staff`;
  base = base.slice(0, 32);

  if (!usedUsernames.has(base)) return base;
  for (let suffixNumber = 2; suffixNumber <= 9999; suffixNumber += 1) {
    const candidate = withSuffix(base, String(suffixNumber));
    if (!usedUsernames.has(candidate)) return candidate;
  }
  return null;
}

/** Parses pasted names into bounded, unique requester roster rows. */
export function parseStaffNames(
  input: string,
  existingUsernames: readonly string[] = [],
): StaffNameParseResult {
  const errors: StaffNameError[] = [];
  const rows: StaffRosterDraft[] = [];
  const usedUsernames = new Set(existingUsernames.map((username) => username.trim().toLowerCase()));
  const sourceLines = input.split(/\r?\n/u);
  const nonblankCount = sourceLines.filter((line) => normalizeDisplayName(line).length > 0).length;

  if (nonblankCount > MAX_QUICK_STAFF) {
    errors.push({
      line: 0,
      message: `Paste no more than ${MAX_QUICK_STAFF} staff names at one time.`,
    });
  }

  for (const [index, sourceLine] of sourceLines.entries()) {
    const displayName = normalizeDisplayName(sourceLine);
    if (!displayName) continue;
    if (rows.length >= MAX_QUICK_STAFF) continue;
    if (displayName.length > 80) {
      errors.push({ line: index + 1, message: "Name must be 80 characters or fewer." });
      continue;
    }
    if (/^[=+\-@]/u.test(displayName) || /\p{Control}/u.test(displayName)) {
      errors.push({ line: index + 1, message: "Name contains a disallowed leading or control character." });
      continue;
    }
    const username = suggestUsername(displayName, usedUsernames);
    if (!username) {
      errors.push({ line: index + 1, message: "Name needs at least one Latin letter or number for a username." });
      continue;
    }
    usedUsernames.add(username);
    rows.push({ displayName, username });
  }

  if (nonblankCount === 0) {
    errors.push({ line: 0, message: "Paste at least one staff name." });
  }
  return { valid: errors.length === 0, rows, errors };
}

/** Quotes one field so generated roster CSV cannot change shape. */
function csvField(value: string): string {
  return `"${value.replaceAll('"', '""')}"`;
}

/** Serializes quick-add rows as requester-only roster CSV with empty emails. */
export function buildRequesterRosterCsv(rows: readonly StaffRosterDraft[]): string {
  const lines = rows.map((row) =>
    [row.username, row.displayName, "requester", ""].map(csvField).join(","),
  );
  return ["username,display_name,role,email", ...lines].join("\n");
}

/** Formats one complete printed or copied login card. */
export function loginCardText(credential: OneTimeCredential, deskUrl: string): string {
  return [
    credential.user.display_name,
    `Desk address: ${deskUrl}`,
    `Username: ${credential.user.username}`,
    `Temporary password: ${credential.temporary_password}`,
    "Sign in, then create a new private password when asked.",
  ].join("\n");
}

/** Returns the URL-only QR payload so printed codes never contain credentials. */
export function loginQrPayload(deskUrl: string): string {
  return deskUrl;
}
