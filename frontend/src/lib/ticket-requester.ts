/** Pure requester ticket helpers shared by browser state and contract tests. */

import { ApiError } from "./api.js";
import type { PublicCategory, Ticket, TicketStatus } from "../types/api.js";

/** URL-backed filters available in the requester workspace. */
export interface RequesterTicketFilters {
  /** Free-text title and description search. */
  search: string;
  /** Exact lifecycle filter or all states when blank. */
  status: TicketStatus | "";
  /** Exact category filter or every category when blank. */
  categoryId: string;
}

/** Minimal route location returned without coupling helpers to a router instance. */
export interface TicketLocation {
  /** Named requester route. */
  name: "ticket" | "tickets";
  /** Selected ticket parameter for the detail route. */
  params?: { id: string };
  /** Non-empty URL-backed filters. */
  query: Record<string, string>;
}

/** Query values accepted from Vue Router. */
export type RequesterFilterQuery = Record<
  string,
  string | null | Array<string | null> | undefined
>;

/** Approved ticket statuses accepted from an untrusted browser URL. */
const TICKET_STATUSES: TicketStatus[] = [
  "new",
  "open",
  "waiting_on_requester",
  "resolved",
  "closed",
];

/** Selects the first scalar value from a router query field. */
function queryValue(value: string | null | Array<string | null> | undefined): string {
  const scalar = Array.isArray(value) ? value.find((item): item is string => item !== null) : value;
  return scalar?.trim() ?? "";
}

/** Converts untrusted router query values into supported requester filters. */
export function createRequesterFilters(query: RequesterFilterQuery = {}): RequesterTicketFilters {
  const status = queryValue(query.status);
  return {
    search: queryValue(query.q),
    status: TICKET_STATUSES.includes(status as TicketStatus) ? (status as TicketStatus) : "",
    categoryId: queryValue(query.category),
  };
}

/** Builds a stable requester route that preserves non-empty filter context. */
export function ticketLocation(
  ticketId: string | null,
  filters: RequesterTicketFilters,
): TicketLocation {
  const query: Record<string, string> = {};
  if (filters.search) query.q = filters.search;
  if (filters.status) query.status = filters.status;
  if (filters.categoryId) query.category = filters.categoryId;
  return ticketId
    ? { name: "ticket", params: { id: ticketId }, query }
    : { name: "tickets", query };
}

/** Formats one server UTC timestamp in the browser's current timezone. */
export function formatTicketTimestamp(timestamp: string): string {
  const value = new Date(timestamp);
  if (Number.isNaN(value.getTime())) return "Unknown time";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(value);
}

/** Resolves an active category name while preserving historical ticket readability. */
export function categoryLabel(
  categoryId: string | null,
  categories: PublicCategory[],
): string {
  return categories.find((category) => category.id === categoryId)?.name ?? "Unknown category";
}

/** Returns whether a requester may reopen the current server lifecycle state. */
export function requesterCanReopen(ticket: Ticket): boolean {
  return ticket.status === "resolved";
}

/** Converts API failures into bounded requester-facing recovery guidance. */
export function ticketErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.status === 404) return "That ticket is unavailable or you do not have access to it.";
    if (error.status === 403) return "You do not have permission to do that.";
    if (error.status === 409) {
      return "This ticket changed while you were viewing it. Refresh and try again.";
    }
    if (error.status === 413) return "That file is larger than this help desk allows.";
  }
  return "The help desk could not complete that request. Try again.";
}
