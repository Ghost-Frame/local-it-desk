/** Unit tests for administrator-only account and configuration controls. */
import test from "node:test";
import assert from "node:assert/strict";

import { canAccessAdministration } from "../src/lib/admin-guard.js";

test("only administrators can access administrator controls", () => {
  assert.equal(canAccessAdministration("administrator"), true);
  assert.equal(canAccessAdministration("technician"), false);
  assert.equal(canAccessAdministration("requester"), false);
  assert.equal(canAccessAdministration(null), false);
  assert.equal(canAccessAdministration(undefined), false);
});
