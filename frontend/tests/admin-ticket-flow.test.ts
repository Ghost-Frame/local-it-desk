/** Administrator ticket queue contracts for visibility, workflow, notes, and conflicts. */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { ApiClient, ApiError } from "../src/lib/api.js";
import {
  activeSupportAccounts,
  availableStatusTransitions,
  createDefaultTicketFilters,
  filterAndSortTickets,
  resolveTicketSelection,
  ticketAdminErrorMessage,
} from "../src/lib/ticket-admin.js";
import type { Ticket, User } from "../src/types/api.js";

/** Builds one complete shared-queue ticket fixture. */
function makeTicket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: overrides.id ?? "ticket-1",
    number: overrides.number ?? 18,
    title: overrides.title ?? "Wireless cart is offline",
    description: overrides.description ?? "The cart in Room 104 cannot reach the staff network.",
    requester_id: overrides.requester_id ?? "requester-1",
    assignee_id: overrides.assignee_id ?? null,
    category_id: overrides.category_id ?? "network",
    priority: overrides.priority ?? "high",
    status: overrides.status ?? "new",
    created_at: overrides.created_at ?? "2026-07-30T10:00:00Z",
    updated_at: overrides.updated_at ?? "2026-07-30T10:00:00Z",
    resolved_at: overrides.resolved_at ?? null,
    closed_at: overrides.closed_at ?? null,
  };
}

/** Builds one active or disabled cumulative-role account fixture. */
function makeUser(id: string, role: User["role"], isActive = true): User {
  return {
    id,
    username: id,
    display_name: id.replaceAll("-", " "),
    email: null,
    role,
    is_active: isActive,
    must_change_password: false,
    created_at: "2026-07-30T10:00:00Z",
    updated_at: "2026-07-30T10:00:00Z",
    last_login_at: null,
  };
}

/** Creates one JSON response fixture. */
function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

test("administrator API lists every ticket page and sends conflict-safe queue mutations", async () => {
  const originalFetch = globalThis.fetch;
  const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
  const ticket = makeTicket();
  const updated = makeTicket({ status: "open", assignee_id: "admin-1", priority: "urgent" });
  const responses = [
    jsonResponse({ user: { id: "admin-1" }, csrf_token: "csrf-admin" }),
    jsonResponse({ items: [ticket], next_cursor: "cursor-2" }),
    jsonResponse({ items: [makeTicket({ id: "ticket-2", number: 17 })], next_cursor: null }),
    jsonResponse(updated),
    jsonResponse({ id: "note-1", ticket_id: ticket.id, visibility: "internal" }, 201),
    jsonResponse({ id: "reply-1", ticket_id: ticket.id, visibility: "public" }, 201),
    jsonResponse([]),
    jsonResponse([]),
  ];
  globalThis.fetch = async (input, init) => {
    calls.push({ input, init });
    return responses.shift() ?? jsonResponse(null, 500);
  };

  try {
    const client = new ApiClient();
    await client.login({ username: "desk.admin", password: "test password" });
    const first = await client.listTickets({ page_size: 25 });
    const second = await client.listTickets({ page_size: 25, cursor: first.next_cursor ?? undefined });
    await client.updateTicket(ticket.id, {
      status: "open",
      priority: "urgent",
      assignee_id: "admin-1",
      category_id: "network",
      expected_updated_at: ticket.updated_at,
    });
    await client.addTicketComment(ticket.id, { body: "Check switch port 12.", visibility: "internal" });
    await client.addTicketComment(ticket.id, { body: "I am checking the network cart.", visibility: "public" });
    await client.listTicketComments(ticket.id);
    await client.listTicketAttachments(ticket.id);

    assert.equal(first.items[0]?.requester_id, "requester-1");
    assert.equal(second.items[0]?.id, "ticket-2");
    assert.ok(String(calls[2]?.input).includes("cursor=cursor-2"));
    assert.deepEqual(JSON.parse(String(calls[3]?.init?.body)), {
      status: "open",
      priority: "urgent",
      assignee_id: "admin-1",
      category_id: "network",
      expected_updated_at: ticket.updated_at,
    });
    assert.equal(JSON.parse(String(calls[4]?.init?.body)).visibility, "internal");
    assert.equal(JSON.parse(String(calls[5]?.init?.body)).visibility, "public");
    assert.equal(new Headers(calls[3]?.init?.headers).get("x-csrf-token"), "csrf-admin");
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("queue helpers filter, preserve visible selection, and exclude inactive assignees", () => {
  const filters = createDefaultTicketFilters();
  filters.search = "wireless";
  filters.assignee = "unassigned";
  filters.priorities = ["high", "urgent"];
  const visible = filterAndSortTickets(
    [
      makeTicket({ id: "visible" }),
      makeTicket({ id: "assigned", assignee_id: "tech-1" }),
      makeTicket({ id: "printer", title: "Printer jam" }),
    ],
    filters,
  );
  assert.deepEqual(visible.map((ticket) => ticket.id), ["visible"]);
  assert.equal(resolveTicketSelection(visible, "visible"), "visible");
  assert.deepEqual(
    activeSupportAccounts([
      makeUser("requester-1", "requester"),
      makeUser("tech-1", "technician"),
      makeUser("admin-1", "administrator"),
      makeUser("disabled-tech", "technician", false),
    ]).map((user) => user.id),
    ["admin-1", "tech-1"],
  );
});

test("workflow options follow technician and administrator lifecycle boundaries", () => {
  assert.deepEqual(availableStatusTransitions("new", false), ["new", "open", "waiting_on_requester", "resolved", "closed"]);
  assert.deepEqual(availableStatusTransitions("resolved", false), ["resolved", "open", "closed"]);
  assert.deepEqual(availableStatusTransitions("closed", false), ["closed"]);
  assert.deepEqual(availableStatusTransitions("closed", true), ["closed", "open"]);
});

test("conflict and privacy failures produce actionable bounded guidance", () => {
  assert.equal(ticketAdminErrorMessage(new ApiError(409, null)), "This ticket changed on the server. Its current values have been reloaded.");
  assert.equal(ticketAdminErrorMessage(new ApiError(404, null)), "That ticket is no longer available.");
  assert.equal(ticketAdminErrorMessage(new Error("sqlite path")), "The ticket operation failed. Try again.");
});

test("administrator queue exposes explicit saves, pagination, internal-note distinction, and recovery", () => {
  const queue = readFileSync(new URL("../../src/components/tickets/AdminTicketQueue.vue", import.meta.url), "utf8");
  const detail = readFileSync(new URL("../../src/components/tickets/AdminTicketDetail.vue", import.meta.url), "utf8");
  const filters = readFileSync(new URL("../../src/components/tickets/TicketFilters.vue", import.meta.url), "utf8");
  const admin = readFileSync(new URL("../../src/views/AdminView.vue", import.meta.url), "utf8");
  const combined = [queue, detail, filters, admin].join("\n");

  for (const phrase of [
    "Save ticket changes",
    "Internal note",
    "Public reply",
    "Load more tickets",
    "Reload current ticket",
    "No queue results",
  ]) {
    assert.ok(combined.includes(phrase), `administrator UI is missing ${phrase}`);
  }
  assert.ok(/expected_updated_at/.test(combined));
  assert.ok(/role="alert"/.test(combined));
  assert.ok(/aria-live="polite"/.test(combined));
  assert.ok(/type="submit"/.test(combined));
});
