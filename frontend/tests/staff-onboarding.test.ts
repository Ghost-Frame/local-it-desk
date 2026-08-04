/** Contract tests for pasted-name staff onboarding utilities. */

import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_QUICK_STAFF,
  buildRequesterRosterCsv,
  loginCardText,
  loginQrPayload,
  parseStaffNames,
  suggestUsername,
} from "../src/lib/staff-onboarding.js";
import type { OneTimeCredential } from "../src/types/api.js";

/** Builds one complete transient credential fixture. */
function credential(): OneTimeCredential {
  return {
    user: {
      id: "staff-id",
      username: "renee.oconnor",
      display_name: "Renée O'Connor",
      email: null,
      role: "requester",
      is_active: true,
      must_change_password: true,
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
      last_login_at: null,
    },
    temporary_password: "temporary-value",
  };
}

test("pasted names normalize Unicode and ignore blank lines", () => {
  const result = parseStaffNames("  Rene\u0301e   O'Connor  \n\nMary Jane Watson\r\n");

  assert.equal(result.valid, true);
  assert.deepEqual(result.rows, [
    { displayName: "Renée O'Connor", username: "renee.oconnor" },
    { displayName: "Mary Jane Watson", username: "mary.watson" },
  ]);
});

test("username suggestions strip punctuation, truncate, and resolve collisions deterministically", () => {
  const longName = "Alexanderthelongfirstname Maximilianthelonglastname";
  const base = suggestUsername(longName, new Set<string>());
  assert.equal(base?.length, 32);

  const used = new Set([base ?? ""]);
  const duplicate = suggestUsername(longName, used);
  assert.equal(duplicate?.length, 32);
  assert.ok(duplicate?.endsWith("2"));

  const result = parseStaffNames("Alex Smith\nAlex Smith\nAlex Smith", ["alex.smith"]);
  assert.deepEqual(result.rows.map((row) => row.username), ["alex.smith2", "alex.smith3", "alex.smith4"]);
});

test("invalid lines and the 500-row bound return structured errors", () => {
  const invalid = parseStaffNames("李 小龍\n=FORMULA");
  assert.equal(invalid.valid, false);
  assert.deepEqual(invalid.errors.map((error) => error.line), [1, 2]);

  const tooMany = parseStaffNames(Array.from({ length: MAX_QUICK_STAFF + 1 }, (_, index) => `Staff ${index}`).join("\n"));
  assert.equal(tooMany.valid, false);
  assert.equal(tooMany.rows.length, MAX_QUICK_STAFF);
  assert.ok(tooMany.errors[0]?.message.includes("500"));
});

test("generated CSV quotes every field and grants requester role only", () => {
  const csv = buildRequesterRosterCsv([
    { username: "renee.oconnor", displayName: 'Renée "Rae" O\'Connor' },
  ]);

  assert.equal(
    csv,
    'username,display_name,role,email\n"renee.oconnor","Renée ""Rae"" O\'Connor","requester",""',
  );
  assert.equal(csv.includes("administrator"), false);
  assert.equal(csv.includes("technician"), false);
});

test("login card text includes the URL while QR data contains only the URL", () => {
  const entry = credential();
  const url = "https://helpdesk.example.com";

  assert.equal(
    loginCardText(entry, url),
    "Renée O'Connor\nDesk address: https://helpdesk.example.com\nUsername: renee.oconnor\nTemporary password: temporary-value\nSign in, then create a new private password when asked.",
  );
  assert.equal(loginQrPayload(url), url);
  assert.equal(loginQrPayload(url).includes(entry.user.username), false);
  assert.equal(loginQrPayload(url).includes(entry.temporary_password), false);
});
