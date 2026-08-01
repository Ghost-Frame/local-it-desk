/** Contract tests for local setup, login, forced change, and CSRF handling. */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { ApiClient } from "../src/lib/api.js";
import { resolveGuardRedirect, safePostLoginPath } from "../src/lib/router-guards.js";
/** Authentication facts supplied to pure redirect decisions. */
import type { AuthGuardState } from "../src/lib/router-guards.js";
/** Route metadata supplied to pure redirect decisions. */
import type { GuardRoute } from "../src/lib/router-guards.js";

/** Default signed-out state after first-run setup has completed. */
const SIGNED_OUT: AuthGuardState = {
  setupRequired: false,
  isAuthenticated: false,
  mustChangePassword: false,
  isAdministrator: false,
};

/** Builds one route fixture with overridable authorization metadata. */
function route(
  path: string,
  requiresAuth = false,
  requiresAdministrator = false,
): GuardRoute {
  return { path, requiresAuth, requiresAdministrator };
}

/** Loads one frontend source file relative to the compiled test location. */
function source(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8");
}

test("first run always enters setup and completed setup cannot return there", () => {
  assert.equal(
    resolveGuardRedirect(route("/login"), { ...SIGNED_OUT, setupRequired: true }),
    "/setup",
  );
  assert.equal(
    resolveGuardRedirect(route("/setup"), { ...SIGNED_OUT, setupRequired: true }),
    undefined,
  );
  assert.equal(resolveGuardRedirect(route("/setup"), SIGNED_OUT), "/login");
});

test("protected routes preserve a safe intended destination for signed-out staff", () => {
  assert.equal(
    resolveGuardRedirect(route("/tickets/abc-123", true), SIGNED_OUT),
    "/login?redirect=%2Ftickets%2Fabc-123",
  );
  assert.equal(safePostLoginPath("/tickets/abc-123"), "/tickets/abc-123");
  assert.equal(safePostLoginPath("https://example.com/steal"), "/");
  assert.equal(safePostLoginPath("//example.com/steal"), "/");
});

test("forced password change outranks product and administrator navigation", () => {
  const forced: AuthGuardState = {
    ...SIGNED_OUT,
    isAuthenticated: true,
    mustChangePassword: true,
  };
  assert.equal(resolveGuardRedirect(route("/", true), forced), "/change-password");
  assert.equal(
    resolveGuardRedirect(route("/change-password", true), forced),
    undefined,
  );
});

test("administrator routes allow administrators and redirect requesters", () => {
  const requester: AuthGuardState = { ...SIGNED_OUT, isAuthenticated: true };
  const administrator: AuthGuardState = {
    ...requester,
    isAdministrator: true,
  };
  assert.equal(
    resolveGuardRedirect(route("/administration", true, true), requester),
    "/",
  );
  assert.equal(
    resolveGuardRedirect(route("/administration", true, true), administrator),
    undefined,
  );
});

test("API keeps session cookies implicit and adds in-memory CSRF to unsafe requests", async () => {
  const originalFetch = globalThis.fetch;
  const requests: RequestInit[] = [];
  globalThis.fetch = async (_input, init) => {
    requests.push(init ?? {});
    return new Response(
      JSON.stringify({
        user: {
          id: "admin-id",
          username: "desk.admin",
          display_name: "Desk Admin",
          email: null,
          role: "administrator",
          is_active: true,
          must_change_password: false,
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
          last_login_at: null,
        },
        csrf_token: "memory-only-csrf",
      }),
      { status: 200, headers: { "content-type": "application/json" } },
    );
  };

  try {
    const client = new ApiClient();
    await client.login({ username: "desk.admin", password: "test password value" });
    await client.changePassword({
      current_password: "test password value",
      new_password: "replacement password value",
    });
    await client.logout();
    await client.createTicket({
      title: "Projector is offline",
      description: "The classroom projector does not power on.",
      category_id: null,
      priority: "normal",
    });
    const loginHeaders = new Headers(requests[0]?.headers);
    const changeHeaders = new Headers(requests[1]?.headers);
    const logoutHeaders = new Headers(requests[2]?.headers);
    const signedOutHeaders = new Headers(requests[3]?.headers);
    assert.equal(requests[0]?.credentials, "same-origin");
    assert.equal(loginHeaders.has("authorization"), false);
    assert.equal(loginHeaders.has("x-csrf-token"), false);
    assert.equal(changeHeaders.get("x-csrf-token"), "memory-only-csrf");
    assert.equal(logoutHeaders.get("x-csrf-token"), "memory-only-csrf");
    assert.equal(signedOutHeaders.has("x-csrf-token"), false);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("auth sources never persist session or CSRF material", () => {
  const content = [
    source("../../src/lib/api.ts"),
    source("../../src/stores/auth.ts"),
  ]
    .join("\n")
    .toLowerCase();
  for (const forbidden of ["localstorage", "sessionstorage", "indexeddb", "caches.open"]) {
    assert.equal(content.includes(forbidden), false, `auth source contains ${forbidden}`);
  }
});

test("auth forms expose labels, password-manager metadata, and generic login errors", () => {
  const login = source("../../src/views/LoginView.vue");
  const setup = source("../../src/views/SetupView.vue");
  const change = source("../../src/views/ChangePasswordView.vue");
  for (const content of [login, setup, change]) {
    assert.ok(/role="alert"/.test(content));
    assert.ok(/<label/.test(content));
    assert.ok(/autocomplete=/.test(content));
  }
  assert.ok(/We could not sign you in\. Check your details and try again\./.test(login));
  assert.equal(/account not found|disabled account|unknown user/.test(login.toLowerCase()), false);
  assert.ok(/autocomplete="new-password"/.test(setup));
  assert.ok(/autocomplete="current-password"/.test(change));
  assert.ok(/autocomplete="new-password"/.test(change));
});
