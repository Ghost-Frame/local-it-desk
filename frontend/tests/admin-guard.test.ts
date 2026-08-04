/** Unit tests for administrator-only account and configuration controls. */
import test from "node:test";
import assert from "node:assert/strict";

import { canAccessAdministratorControls } from "../src/lib/admin-guard.js";

test("only administrators can access administrator controls", () => {
  assert.equal(canAccessAdministratorControls("administrator"), true);
  assert.equal(canAccessAdministratorControls("technician"), false);
  assert.equal(canAccessAdministratorControls("requester"), false);
  assert.equal(canAccessAdministratorControls(null), false);
  assert.equal(canAccessAdministratorControls(undefined), false);
});
