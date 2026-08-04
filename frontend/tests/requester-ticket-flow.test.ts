/** Requester ticket workspace contracts for API behavior, navigation, and accessible states. */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { ApiClient, ApiError } from "../src/lib/api.js";
import {
  categoryLabel,
  createRequesterFilters,
  formatTicketTimestamp,
  requesterCanReopen,
  ticketErrorMessage,
  ticketLocation,
} from "../src/lib/ticket-requester.js";
import type { PublicCategory, Ticket } from "../src/types/api.js";

/** Builds a complete requester-owned ticket fixture. */
function makeTicket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: overrides.id ?? "ticket-1",
    number: overrides.number ?? 42,
    title: overrides.title ?? "Projector is offline",
    description: overrides.description ?? "The classroom projector will not power on.",
    requester_id: overrides.requester_id ?? "requester-1",
    assignee_id: overrides.assignee_id ?? null,
    category_id: overrides.category_id ?? "category-1",
    status: overrides.status ?? "new",
    priority: overrides.priority ?? "normal",
    created_at: overrides.created_at ?? "2026-07-30T14:00:00Z",
    updated_at: overrides.updated_at ?? "2026-07-30T14:00:00Z",
    resolved_at: overrides.resolved_at ?? null,
    closed_at: overrides.closed_at ?? null,
  };
}

/** Returns the JSON response shape used by one successful fetch fixture. */
function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

test("requester API covers filtered pages, creation, detail, comments, attachments, and reopen", async () => {
  const originalFetch = globalThis.fetch;
  const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
  const ticket = makeTicket();
  const responses = [
    jsonResponse({ user: { id: "requester-1" }, csrf_token: "csrf-ticket" }),
    jsonResponse({ items: [ticket], next_cursor: "next-page" }),
    jsonResponse(ticket, 201),
    jsonResponse(ticket),
    jsonResponse([], 200),
    jsonResponse([], 200),
    jsonResponse({ id: "comment-1", ticket_id: ticket.id }, 201),
    jsonResponse({ id: "attachment-1", parent_kind: "ticket", parent_id: ticket.id }, 201),
    jsonResponse({ ...ticket, status: "open" }),
  ];
  globalThis.fetch = async (input, init) => {
    calls.push({ input, init });
    return responses.shift() ?? jsonResponse(null, 500);
  };

  try {
    const client = new ApiClient();
    await client.login({ username: "staff.member", password: "test password" });
    const page = await client.listTickets({
      status: "open",
      priority: "high",
      category_id: "category 1",
      search: "projector & cart",
      page_size: 20,
    });
    await client.createTicket({
      title: ticket.title,
      description: ticket.description,
      category_id: "category-1",
      priority: "normal",
    });
    await client.getTicket(ticket.id);
    await client.listTicketComments(ticket.id);
    await client.listTicketAttachments(ticket.id);
    await client.addTicketComment(ticket.id, { body: "Still happening.", visibility: "public" });
    await client.uploadAttachment("ticket", ticket.id, new File(["log"], "details.txt"));
    await client.updateTicket(ticket.id, {
      status: "open",
      expected_updated_at: ticket.updated_at,
    });

    assert.deepEqual(page, { items: [ticket], next_cursor: "next-page" });
    assert.ok(/status=open/.test(String(calls[1]?.input)));
    assert.ok(/priority=high/.test(String(calls[1]?.input)));
    assert.ok(/category_id=category\+1/.test(String(calls[1]?.input)));
    assert.ok(/search=projector\+%26\+cart/.test(String(calls[1]?.input)));
    assert.ok(/page_size=20/.test(String(calls[1]?.input)));
    const upload = calls[7]?.init;
    assert.ok(upload?.body instanceof FormData);
    assert.equal(new Headers(upload?.headers).get("x-csrf-token"), "csrf-ticket");
    assert.equal(new Headers(upload?.headers).has("content-type"), false);
    assert.deepEqual(JSON.parse(String(calls[8]?.init?.body)), {
      status: "open",
      expected_updated_at: ticket.updated_at,
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("URL helpers preserve requester selection and filter context", () => {
  assert.deepEqual(createRequesterFilters({ q: "  printer  ", status: "resolved", category: "cat-1" }), {
    search: "printer",
    status: "resolved",
    categoryId: "cat-1",
  });
  assert.deepEqual(createRequesterFilters({ status: "bogus", category: ["first", "second"] }), {
    search: "",
    status: "",
    categoryId: "first",
  });
  assert.deepEqual(ticketLocation("ticket-1", { search: "wifi", status: "open", categoryId: "" }), {
    name: "ticket",
    params: { id: "ticket-1" },
    query: { q: "wifi", status: "open" },
  });
  assert.deepEqual(ticketLocation(null, createRequesterFilters({})), {
    name: "tickets",
    query: {},
  });
});

test("requester presentation handles local time, removed categories, and lifecycle boundaries", () => {
  const categories: PublicCategory[] = [
    { id: "category-1", name: "Classroom technology", description: null, sort_order: 1 },
  ];
  assert.equal(categoryLabel("category-1", categories), "Classroom technology");
  assert.equal(categoryLabel("removed-category", categories), "Unknown category");
  assert.equal(categoryLabel(null, categories), "Unknown category");
  assert.ok(formatTicketTimestamp("2026-07-30T14:00:00Z") !== "2026-07-30T14:00:00Z");
  assert.equal(requesterCanReopen(makeTicket({ status: "resolved" })), true);
  assert.equal(requesterCanReopen(makeTicket({ status: "closed" })), false);
  assert.equal(requesterCanReopen(makeTicket({ status: "open" })), false);
});

test("requester failures distinguish missing resources while keeping messages safe", () => {
  assert.equal(ticketErrorMessage(new ApiError(404, null)), "That ticket is unavailable or you do not have access to it.");
  assert.equal(ticketErrorMessage(new ApiError(403, null)), "You do not have permission to do that.");
  assert.equal(ticketErrorMessage(new ApiError(409, null)), "This ticket changed while you were viewing it. Refresh and try again.");
  assert.equal(ticketErrorMessage(new Error("database secret")), "The help desk could not complete that request. Try again.");
});

test("requester components expose complete loading, empty, error, and keyboard-accessible states", () => {
  const view = readFileSync(new URL("../../src/views/TicketsView.vue", import.meta.url), "utf8");
  const form = readFileSync(new URL("../../src/components/tickets/TicketForm.vue", import.meta.url), "utf8");
  const list = readFileSync(new URL("../../src/components/tickets/TicketList.vue", import.meta.url), "utf8");
  const detail = readFileSync(new URL("../../src/components/tickets/TicketDetail.vue", import.meta.url), "utf8");
  const comments = readFileSync(new URL("../../src/components/tickets/TicketComments.vue", import.meta.url), "utf8");
  const combined = [view, form, list, detail, comments].join("\n");

  for (const phrase of ["Loading tickets", "No tickets match", "Try again", "Ticket closed", "Reopen ticket"]) {
    assert.ok(combined.includes(phrase), `requester UI is missing ${phrase}`);
  }
  assert.ok(/role="alert"/.test(combined));
  assert.ok(/aria-live="polite"/.test(combined));
  assert.ok(/<label/.test(combined));
  assert.ok(/type="submit"/.test(combined));
  assert.ok(/:disabled=/.test(combined));
});

test("new request form keeps ordinary staff choices short and explicit", () => {
  const form = readFileSync(new URL("../../src/components/tickets/TicketForm.vue", import.meta.url), "utf8");
  const view = readFileSync(new URL("../../src/views/TicketsView.vue", import.meta.url), "utf8");

  for (const phrase of ["What is wrong?", "Where is it?", "What happened?", "This is stopping a class", "Send request"]) {
    assert.ok(form.includes(phrase), `request form is missing ${phrase}`);
  }
  assert.ok(/urgent\.value \? "urgent" : priority\.value/.test(form));
  assert.ok(/Location or device:/.test(form));
  assert.equal(/id="ticket-priority"/.test(form), false);
  assert.ok(/<details/.test(form));
  assert.ok(/Search and filter requests/.test(view));
  assert.ok(/\+ New request/.test(view));
});
