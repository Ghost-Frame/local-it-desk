/** Deterministic accessibility and responsive-layout contracts for the local help desk. */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

/** Reads one frontend source file relative to the emitted test module. */
function source(path: string): string {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

test("authenticated shell exposes landmarks, skip navigation, and keyboard focus containment", () => {
  /** Responsive application shell source. */
  const layout = source("../../src/components/layout/AppLayout.vue");
  /** Primary navigation source. */
  const sidebar = source("../../src/components/layout/AppSidebar.vue");
  /** Modal notice source. */
  const notifications = source("../../src/components/layout/NotificationMenu.vue");
  assert.ok(/href="#main-content"/.test(layout));
  assert.ok(/<main id="main-content"/.test(layout));
  assert.ok(/<nav/.test(sidebar));
  assert.ok(/<aside[^>]+aria-label="Primary navigation"/s.test(sidebar));
  assert.ok(/aria-modal="true"/.test(notifications));
  assert.ok(/@keydown="trapDialogFocus"/.test(notifications));
  assert.ok(/closeMenu\(\)/.test(notifications));
});

test("credential and ticket forms keep visible labels, status text, and native submission semantics", () => {
  /** All form-heavy screens and shared workflow components. */
  const forms = [
    "../../src/views/SetupView.vue",
    "../../src/views/LoginView.vue",
    "../../src/views/ChangePasswordView.vue",
    "../../src/components/tickets/TicketForm.vue",
    "../../src/components/tickets/TicketComments.vue",
    "../../src/components/announcements/AnnouncementEditor.vue",
    "../../src/views/SettingsView.vue",
    "../../src/components/admin/StaffQuickAdd.vue",
  ].map(source).join("\n");
  /** Text-bearing status badge implementations. */
  const statuses = source("../../src/components/tickets/StatusBadge.vue")
    + source("../../src/components/tickets/PriorityBadge.vue");
  assert.ok(/<label/g.test(forms));
  assert.ok(/type="submit"/g.test(forms));
  assert.ok(/role="alert"/.test(forms));
  assert.ok(/aria-live="polite"/.test(forms));
  assert.ok(/\{\{\s*label\s*\}\}/.test(statuses));
  assert.ok(!/placeholder="(?:Username|Password)"[^>]*aria-label=/i.test(forms));
  assert.ok(/min-h-11/.test(forms));
  assert.ok(/sm:grid-cols-2/.test(forms));
});

test("global interaction styles preserve focus, touch targets, and reduced-motion preferences", () => {
  /** Global visual-system source applied to every route. */
  const styles = source("../../src/assets/main.css");
  assert.ok(/:focus-visible/.test(styles));
  assert.ok(/min-(?:block-size|height):\s*2\.75rem/.test(styles));
  assert.ok(/prefers-reduced-motion:\s*reduce/.test(styles));
  assert.ok(/transition-duration:\s*0\.01ms/.test(styles));
  assert.ok(!/outline:\s*(?:none|0)/.test(styles));
});

test("dense workspaces retain narrow-screen fallbacks and bounded horizontal overflow", () => {
  /** Ticket, administration, and notification workspace sources. */
  const responsive = [
    "../../src/views/TicketsView.vue",
    "../../src/views/AdminView.vue",
    "../../src/components/admin/AuditLogTable.vue",
    "../../src/components/layout/NotificationMenu.vue",
  ].map(source).join("\n");
  assert.ok(/overflow-x-auto/.test(responsive));
  assert.ok(/fixed inset-0/.test(responsive));
  assert.ok(/sm:absolute/.test(responsive));
  assert.ok(/(?:sm|md|lg|xl):grid-cols/.test(responsive));
});
