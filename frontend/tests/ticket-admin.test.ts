/** Ticket queue helper tests for approved filters and urgency order. */
import test from "node:test";
import assert from "node:assert/strict";

import type { Ticket } from "../src/types/api.js";
import {
  createDefaultTicketFilters,
  filterAndSortTickets,
  resolveTicketSelection,
} from "../src/lib/ticket-admin.js";

/** Builds a complete ticket fixture with stable defaults. */
function makeTicket(overrides: Partial<Ticket>): Ticket {
  return {
    id: overrides.id ?? crypto.randomUUID(),
    number: overrides.number ?? 1,
    title: overrides.title ?? "Printer request",
    description: overrides.description ?? "The staff printer needs attention.",
    requester_id: overrides.requester_id ?? "requester-1",
    assignee_id: overrides.assignee_id ?? null,
    category_id: overrides.category_id ?? "general",
    priority: overrides.priority ?? "normal",
    status: overrides.status ?? "new",
    created_at: overrides.created_at ?? "2026-07-30T10:00:00Z",
    updated_at: overrides.updated_at ?? "2026-07-30T10:00:00Z",
  };
}

test("filterAndSortTickets keeps urgent work first and respects filters", () => {
  const filters = createDefaultTicketFilters();
  filters.statuses = ["new", "open"];
  filters.categoryIds = ["network", "email"];
  const tickets = [
    makeTicket({ id: "low-network", category_id: "network", priority: "low" }),
    makeTicket({ id: "urgent-email", category_id: "email", priority: "urgent" }),
    makeTicket({ id: "resolved-email", category_id: "email", status: "resolved" }),
  ];
  assert.deepEqual(
    filterAndSortTickets(tickets, filters).map((ticket) => ticket.id),
    ["urgent-email", "low-network"],
  );
});

test("filterAndSortTickets can order by creation time", () => {
  const filters = createDefaultTicketFilters();
  filters.sort = "created";
  const tickets = [
    makeTicket({ id: "older", created_at: "2026-07-29T09:00:00Z" }),
    makeTicket({ id: "newer", created_at: "2026-07-30T09:00:00Z" }),
  ];
  assert.deepEqual(
    filterAndSortTickets(tickets, filters).map((ticket) => ticket.id),
    ["newer", "older"],
  );
});

test("resolveTicketSelection chooses the first visible fallback", () => {
  const visible = [makeTicket({ id: "alpha" }), makeTicket({ id: "beta" })];
  assert.equal(resolveTicketSelection(visible, "missing"), "alpha");
  assert.equal(resolveTicketSelection(visible, "beta"), "beta");
  assert.equal(resolveTicketSelection([], "beta"), null);
});
