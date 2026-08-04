/**
 * Router guard helper tests -- verifies redirect logic for auth-protected
 * routes and the already-logged-in bounce, with no DOM or Pinia required.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { landingPath, resolveGuardRedirect } from "../src/lib/router-guards.js";
import type { GuardRoute } from "../src/lib/router-guards.js";

/** Builds a minimal GuardRoute fixture. */
function makeRoute(path: string, requiresAuth: boolean): GuardRoute {
  return { path, requiresAuth };
}

test("resolveGuardRedirect allows unauthenticated access to public routes", () => {
  const result = resolveGuardRedirect(makeRoute("/login", false), false);
  assert.equal(result, undefined);
});

test("resolveGuardRedirect redirects unauthenticated user from protected route to /login", () => {
  const result = resolveGuardRedirect(makeRoute("/tickets", true), false);
  assert.equal(result, "/login?redirect=%2Ftickets");
});

test("resolveGuardRedirect preserves the nested intended ticket path", () => {
  const result = resolveGuardRedirect(makeRoute("/tickets/abc-123", true), false);
  assert.equal(result, "/login?redirect=%2Ftickets%2Fabc-123");
});

test("resolveGuardRedirect allows authenticated access to protected routes", () => {
  const result = resolveGuardRedirect(makeRoute("/tickets", true), true);
  assert.equal(result, undefined);
});

test("resolveGuardRedirect sends authenticated requesters from login to tickets", () => {
  const result = resolveGuardRedirect(makeRoute("/login", false), true);
  assert.equal(result, "/tickets");
});

test("resolveGuardRedirect replaces the compatibility root with the role landing page", () => {
  const result = resolveGuardRedirect(makeRoute("/", false), true);
  assert.equal(result, "/tickets");
});

test("landingPath opens each role at its active work", () => {
  assert.equal(landingPath("requester"), "/tickets");
  assert.equal(landingPath("technician"), "/administration");
  assert.equal(landingPath("administrator"), "/administration");
});

/**
 * Redirect-loop guard: if /login were ever misconfigured with requiresAuth=true,
 * the guard must NOT redirect /login -> /login?redirect=%2Flogin (infinite loop).
 * Expected: allow the navigation (return undefined).
 */
test("resolveGuardRedirect does not produce a self-redirect loop for requiresAuth+/login when unauthenticated", () => {
  /** Simulate a misconfigured route: /login marked requiresAuth=true. */
  const result = resolveGuardRedirect(makeRoute("/login", true), false);
  /** Must not redirect to /login again -- return undefined to allow through. */
  assert.equal(result, undefined);
});
