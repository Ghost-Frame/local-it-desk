/** Contract tests for administrator account and roster controls. */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { isFinalActiveAdministrator } from "../src/lib/account-admin.js";
import { ApiClient } from "../src/lib/api.js";
import type { User } from "../src/types/api.js";

/** Builds one complete account fixture with optional field overrides. */
function user(overrides: Partial<User> = {}): User {
  return {
    id: "administrator-id",
    username: "desk.admin",
    display_name: "Desk Admin",
    email: null,
    role: "administrator",
    is_active: true,
    must_change_password: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    last_login_at: null,
    ...overrides,
  };
}

/** Loads one frontend source file relative to the compiled test location. */
function source(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8");
}

test("final active administrator guard permits changes only when another active admin exists", () => {
  const first = user();
  const second = user({ id: "second-admin", username: "second.admin" });
  const inactive = user({ id: "inactive-admin", username: "inactive.admin", is_active: false });
  const requester = user({ id: "requester-id", username: "requester", role: "requester" });

  assert.equal(isFinalActiveAdministrator([first], first.id), true);
  assert.equal(isFinalActiveAdministrator([first, inactive, requester], first.id), true);
  assert.equal(isFinalActiveAdministrator([first, second], first.id), false);
  assert.equal(isFinalActiveAdministrator([first], "missing-id"), false);
});

test("CSV preview and atomic apply send same-origin CSRF-protected text requests", async () => {
  const originalFetch = globalThis.fetch;
  const requests: Array<{ input: RequestInfo | URL; init: RequestInit }> = [];
  let responseIndex = 0;
  const responses = [
    {
      user: user(),
      csrf_token: "admin-csrf",
    },
    {
      valid: false,
      rows: [],
      errors: [{ row_number: 2, field: "role", message: "Use a supported role." }],
    },
    {
      created: [
        {
          user: user({ id: "staff-id", username: "casey", role: "requester" }),
          temporary_password: "one-time-password",
        },
      ],
    },
  ];
  globalThis.fetch = async (input, init) => {
    requests.push({ input, init: init ?? {} });
    const body = responses[responseIndex];
    responseIndex += 1;
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  };

  try {
    const client = new ApiClient();
    await client.login({ username: "desk.admin", password: "test-password" });
    const csv = "username,display_name,role,email\ncasey,Casey Smith,requester,";
    const preview = await client.previewRoster(csv);
    const applied = await client.applyRoster(csv);

    assert.equal(preview.valid, false);
    assert.equal(preview.errors[0]?.row_number, 2);
    assert.equal(applied.created[0]?.temporary_password, "one-time-password");
    assert.equal(requests[1]?.input, "/api/admin/users/import/preview");
    assert.equal(requests[2]?.input, "/api/admin/users/import/apply");
    for (const request of requests.slice(1)) {
      const headers = new Headers(request.init.headers);
      assert.equal(request.init.credentials, "same-origin");
      assert.equal(headers.get("content-type"), "text/csv");
      assert.equal(headers.get("x-csrf-token"), "admin-csrf");
      assert.equal(request.init.body, csv);
    }
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("Manage Desk groups routine work and keeps advanced lifecycle controls", () => {
  const admin = source("../../src/views/AdminView.vue");
  const row = source("../../src/components/admin/UserRow.vue");

  for (const label of ["Tickets", "People", "Desk", "Advanced", "Sessions", "Audit log"]) {
    assert.ok(admin.includes(label), `missing ${label} tab`);
  }
  assert.ok(/Import a CSV roster instead/.test(admin));
  assert.ok(/visibleTabs/.test(admin));
  assert.ok(/isAdministrator/.test(admin));
  assert.ok(/<dialog/.test(row));
  assert.ok(/showModal/.test(row));
  assert.ok(/current-password/.test(row));
  assert.ok(/\.focus\(\)/.test(row));
});

test("one-time onboarding material can be copied or printed without browser persistence", () => {
  const panel = source("../../src/components/admin/OnboardingPanel.vue");
  const normalized = panel.toLowerCase();

  assert.ok(/temporary_password/.test(panel));
  assert.ok(/clipboard\.writeText/.test(panel));
  assert.ok(/window\.print/.test(panel));
  assert.ok(/loginQrPayload\(props\.deskUrl\)/.test(panel));
  assert.ok(/QR code for the desk address only/.test(panel));
  for (const forbidden of ["localstorage", "sessionstorage", "indexeddb"]) {
    assert.equal(normalized.includes(forbidden), false, `onboarding source contains ${forbidden}`);
  }
});

test("guided setup keeps branding, administrator, staff, and finish as explicit stages", () => {
  const setup = source("../../src/views/SetupView.vue");
  const quickAdd = source("../../src/components/admin/StaffQuickAdd.vue");

  for (const phrase of ["Step {{ step }} of 4", "Name the desk", "Create the technician account", "Add staff who can submit requests", "The desk is ready."]) {
    assert.ok(setup.includes(phrase), `setup is missing ${phrase}`);
  }
  assert.ok(/authStore\.setup/.test(setup));
  assert.ok(/api\.updateAdminSettings/.test(setup));
  assert.ok(/authStore\.refreshPublicConfig/.test(setup));
  assert.ok(/StaffQuickAdd/.test(setup));
  assert.ok(/buildRequesterRosterCsv/.test(quickAdd));
  assert.ok(/previewRoster/.test(quickAdd));
  assert.ok(/applyRoster/.test(quickAdd));
  assert.ok(/requester/.test(quickAdd));
});

test("roster UI displays preview errors and waits for a valid preview before apply", () => {
  const roster = source("../../src/components/admin/RosterImport.vue");

  assert.ok(/preview\.errors/.test(roster));
  assert.ok(/preview\.valid/.test(roster));
  assert.ok(/previewedCsv/.test(roster));
  assert.ok(/csvText\.value !== previewedCsv\.value/.test(roster));
  assert.ok(/applyRoster/.test(roster));
  assert.ok(/:disabled=.*preview/.test(roster));
});
