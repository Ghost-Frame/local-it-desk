/** Pure technician queue filtering, sorting, and selection helpers. */

import { ApiError } from "./api.js";
import type { Ticket, TicketPriority, TicketStatus, User } from "../types/api.js";

/** Sort keys supported by the shared ticket queue. */
export type TicketSortKey = "priority" | "created" | "updated";

/** Filter and sort settings applied to the shared ticket queue. */
export interface TicketFilters {
  /** Case-insensitive title and description search. */
  search: string;
  /** Included lifecycle states. */
  statuses: TicketStatus[];
  /** Included urgency values. */
  priorities: TicketPriority[];
  /** Included category identifiers. */
  categoryIds: string[];
  /** Exact support assignment, unassigned sentinel, or all assignments. */
  assignee: "all" | "unassigned" | string;
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
    search: "",
    statuses: ["new", "open", "waiting_on_requester", "resolved", "closed"],
    priorities: ["low", "normal", "high", "urgent"],
    categoryIds: [],
    assignee: "all",
    sort: "priority",
  };
}

/** Applies ticket filters and returns a newly sorted visible list. */
export function filterAndSortTickets(tickets: Ticket[], filters: TicketFilters): Ticket[] {
  const search = filters.search.trim().toLocaleLowerCase();
  return tickets
    .filter((ticket) =>
      search
        ? ticket.title.toLocaleLowerCase().includes(search) ||
          ticket.description.toLocaleLowerCase().includes(search) ||
          String(ticket.number).includes(search)
        : true,
    )
    .filter((ticket) => filters.statuses.includes(ticket.status))
    .filter((ticket) => filters.priorities.includes(ticket.priority))
    .filter((ticket) =>
      filters.categoryIds.length === 0
        ? true
        : ticket.category_id !== null && filters.categoryIds.includes(ticket.category_id),
    )
    .filter((ticket) => {
      if (filters.assignee === "all") return true;
      if (filters.assignee === "unassigned") return ticket.assignee_id === null;
      return ticket.assignee_id === filters.assignee;
    })
    .sort((left, right) => compareTickets(left, right, filters.sort));
}

/** Returns active cumulative-role accounts that may receive a ticket assignment. */
export function activeSupportAccounts(users: User[]): User[] {
  return users
    .filter(
      (user) =>
        user.is_active && (user.role === "technician" || user.role === "administrator"),
    )
    .sort((left, right) => left.display_name.localeCompare(right.display_name));
}

/** Returns server-supported workflow choices for the current staff role and state. */
export function availableStatusTransitions(
  current: TicketStatus,
  isAdministrator: boolean,
): TicketStatus[] {
  if (current === "new") return ["new", "open", "waiting_on_requester", "resolved", "closed"];
  if (current === "open") return ["open", "waiting_on_requester", "resolved", "closed"];
  if (current === "waiting_on_requester") {
    return ["waiting_on_requester", "open", "resolved", "closed"];
  }
  if (current === "resolved") return ["resolved", "open", "closed"];
  return isAdministrator ? ["closed", "open"] : ["closed"];
}

/** Converts queue API failures into bounded operator recovery guidance. */
export function ticketAdminErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.status === 409) {
      return "This ticket changed on the server. Its current values have been reloaded.";
    }
    if (error.status === 404) return "That ticket is no longer available.";
    if (error.status === 403) return "Your account cannot perform that ticket operation.";
  }
  return "The ticket operation failed. Try again.";
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
