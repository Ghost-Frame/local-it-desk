/** Pure technician queue filtering, sorting, and selection helpers. */

import type { Ticket, TicketStatus } from "@/types";

/** Sort keys supported by the shared ticket queue. */
export type TicketSortKey = "priority" | "created" | "updated";

/** Filter and sort settings applied to the shared ticket queue. */
export interface TicketFilters {
  /** Included lifecycle states. */
  statuses: TicketStatus[];
  /** Included category identifiers. */
  categoryIds: string[];
  /** Active queue sort order. */
  sort: TicketSortKey;
}

/** Severity ranking used for priority-first queue ordering. */
const PRIORITY_RANK: Record<Ticket["priority"], number> = {
  urgent: 0,
  high: 1,
  normal: 2,
  low: 3,
};

/** Creates default filters that include every approved lifecycle state. */
export function createDefaultTicketFilters(): TicketFilters {
  return {
    statuses: ["new", "open", "waiting_on_requester", "resolved", "closed"],
    categoryIds: [],
    sort: "priority",
  };
}

/** Applies ticket filters and returns a newly sorted visible list. */
export function filterAndSortTickets(tickets: Ticket[], filters: TicketFilters): Ticket[] {
  return tickets
    .filter((ticket) => filters.statuses.includes(ticket.status))
    .filter((ticket) =>
      filters.categoryIds.length === 0
        ? true
        : ticket.category_id !== null && filters.categoryIds.includes(ticket.category_id),
    )
    .sort((left, right) => compareTickets(left, right, filters.sort));
}

/** Resolves a safe selected ticket identifier for the visible queue. */
export function resolveTicketSelection(
  visibleTickets: Ticket[],
  requestedTicketId: string | null,
): string | null {
  if (requestedTicketId && visibleTickets.some((ticket) => ticket.id === requestedTicketId)) {
    return requestedTicketId;
  }
  return visibleTickets[0]?.id ?? null;
}

/** Compares two tickets according to the selected queue sort mode. */
function compareTickets(left: Ticket, right: Ticket, sort: TicketSortKey): number {
  if (sort === "created") return right.created_at.localeCompare(left.created_at);
  if (sort === "updated") return right.updated_at.localeCompare(left.updated_at);
  const priorityDifference = PRIORITY_RANK[left.priority] - PRIORITY_RANK[right.priority];
  return priorityDifference || right.updated_at.localeCompare(left.updated_at);
}
