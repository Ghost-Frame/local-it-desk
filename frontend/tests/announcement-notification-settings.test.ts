/** Frontend contracts for staff bulletins, private notices, and live runtime branding. */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { ApiClient } from "../src/lib/api.js";
import { renderSafeMarkdown } from "../src/lib/safe-markdown.js";

/** Creates one JSON response fixture. */
function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

test("announcement, notification, settings, category, and logo APIs use exact local routes", async () => {
  const originalFetch = globalThis.fetch;
  const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
  const announcement = {
    id: "notice-1", title: "Wireless maintenance", body: "Staff network work",
    author_id: "admin-1", state: "draft", is_pinned: true, published_at: null,
    created_at: "2026-07-30T10:00:00Z", updated_at: "2026-07-30T10:00:00Z",
  };
  const settings = {
    app_name: "Vocational IT Desk", support_contact: "Room 104",
    logo_url: "/api/branding/logo", default_category_id: "category-1",
    default_priority: "normal",
  };
  const responses = [
    jsonResponse({ user: { id: "admin-1" }, csrf_token: "csrf-admin" }),
    jsonResponse([announcement]), jsonResponse([announcement]),
    jsonResponse(announcement, 201), jsonResponse(announcement),
    jsonResponse({ ...announcement, state: "published" }),
    jsonResponse({ ...announcement, state: "archived" }),
    jsonResponse([]), jsonResponse({ count: 2 }),
    new Response(null, { status: 204 }), new Response(null, { status: 204 }),
    jsonResponse(settings), jsonResponse(settings), jsonResponse([]),
    jsonResponse({ id: "category-1", name: "Network" }, 201),
    jsonResponse({ id: "category-1", name: "Network", is_active: false }),
    jsonResponse(settings), jsonResponse(settings),
  ];
  globalThis.fetch = async (input, init) => {
    calls.push({ input, init });
    return responses.shift() ?? jsonResponse(null, 500);
  };

  try {
    const client = new ApiClient();
    await client.login({ username: "desk.admin", password: "test password" });
    await client.listAnnouncements();
    await client.listAdminAnnouncements();
    await client.createAnnouncement({ title: "Wireless maintenance", body: "Staff network work", is_pinned: true });
    await client.updateAnnouncement("notice-1", { is_pinned: false });
    await client.publishAnnouncement("notice-1");
    await client.archiveAnnouncement("notice-1");
    await client.listNotifications();
    assert.equal(await client.getUnreadNotificationCount(), 2);
    await client.markNotificationRead("notification-1");
    await client.markAllNotificationsRead();
    await client.getAdminSettings();
    await client.updateAdminSettings({ app_name: "Vocational IT Desk", support_contact: "Room 104", default_priority: "normal" });
    await client.listCategories();
    await client.createCategory({ name: "Network", description: null, sort_order: 0 });
    await client.updateCategory("category-1", { is_active: false });
    await client.selectDefaultCategory("category-1");
    await client.uploadLogo(new File(["png"], "desk.png", { type: "image/png" }));

    assert.deepEqual(
      calls.slice(1).map((call) => (call.init?.method ?? "GET") + " " + String(call.input)),
      [
        "GET /api/announcements", "GET /api/admin/announcements",
        "POST /api/admin/announcements", "PATCH /api/admin/announcements/notice-1",
        "POST /api/admin/announcements/notice-1/publish",
        "POST /api/admin/announcements/notice-1/archive",
        "GET /api/notifications", "GET /api/notifications/unread-count",
        "POST /api/notifications/notification-1/read",
        "POST /api/notifications/read-all", "GET /api/admin/settings",
        "PATCH /api/admin/settings", "GET /api/admin/categories",
        "POST /api/admin/categories", "PATCH /api/admin/categories/category-1",
        "POST /api/admin/categories/category-1/default",
        "POST /api/admin/settings/logo",
      ],
    );
    assert.equal(new Headers(calls[3]?.init?.headers).get("x-csrf-token"), "csrf-admin");
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("local Markdown renderer formats safe text without remote media or active HTML", () => {
  /** Mixed trusted-local and rejected-remote link fixture. */
  const rendered = renderSafeMarkdown(
    "# Notice\n\nUse **staff Wi-Fi** and [tickets](/tickets/1).\n\n![remote](https://example.com/pixel.png) [remote](https://example.com) [protocol](//example.com)\n<script>alert(1)</script>",
  );
  assert.ok(/<h2>Notice<\/h2>/.test(rendered));
  assert.ok(/<strong>staff Wi-Fi<\/strong>/.test(rendered));
  assert.ok(/href="\/tickets\/1"/.test(rendered));
  assert.ok(!/href="(?:https?:)?\/\//i.test(rendered));
  assert.ok(!/<img|<script/i.test(rendered));
  assert.ok(/&lt;script&gt;/.test(rendered));
});

test("announcement, notification, and settings surfaces expose lifecycle and accessibility controls", () => {
  const paths = [
    "../../src/views/AnnouncementsView.vue",
    "../../src/components/announcements/AnnouncementEditor.vue",
    "../../src/components/announcements/AnnouncementList.vue",
    "../../src/components/layout/NotificationMenu.vue",
    "../../src/components/layout/AppHeader.vue",
    "../../src/components/layout/AppSidebar.vue",
    "../../src/views/SettingsView.vue",
  ];
  const source = paths.map((path) => readFileSync(new URL(path, import.meta.url), "utf8")).join("\n");
  /** Router contract paired with server-generated announcement notification targets. */
  const routerSource = readFileSync(new URL("../../src/router/index.ts", import.meta.url), "utf8");
  for (const phrase of [
    "Create draft", "Publish announcement", "Archive announcement", "Pinned",
    "Mark all read", "Unread notifications", "Application name", "Support contact",
    "Default priority", "Default category", "Upload logo", "Disable category",
  ]) {
    assert.ok(source.includes(phrase), "product UI is missing " + phrase);
  }
  assert.ok(/aria-expanded/.test(source));
  assert.ok(/aria-live="polite"/.test(source));
  assert.ok(/role="dialog"/.test(source));
  assert.ok(/@keydown="trapDialogFocus"/.test(source));
  assert.ok(/path:\s*"\/announcements\/:id"/.test(routerSource));
  assert.ok(/focusTargetAnnouncement/.test(source));
  assert.ok(/watch\(\s*\(\) => route\.params\.id/.test(source));
  assert.ok(/type="submit"/.test(source));
  assert.ok(!/target="_blank"/.test(source));
});
