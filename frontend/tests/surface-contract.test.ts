/** Source-level contract that prevents excluded collaboration surfaces from returning. */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

/** Active files that define routes, navigation, API methods, and public types. */
const SURFACE_FILES = [
  "../../src/router/index.ts",
  "../../src/components/layout/AppSidebar.vue",
  "../../src/lib/api.ts",
  "../../src/types/api.ts",
];

/** Excluded product terms inherited from the collaboration application. */
const EXCLUDED_TERMS = [
  "channel",
  "direct message",
  "dmthread",
  "documentversion",
  "changelog",
  "api token",
  "pushsubscription",
  "auth/callback",
  "tauri",
];

/** Loads one active source file relative to this test module. */
function source(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8").toLowerCase();
}

test("active frontend files expose only the approved help-desk surface", () => {
  for (const file of SURFACE_FILES) {
    const content = source(file);
    for (const term of EXCLUDED_TERMS) {
      assert.equal(content.includes(term), false, `${file} contains ${term}`);
    }
  }
});
