/** Requester ticket state, navigation helpers, and failure-safe API actions. */

import { computed, ref } from "vue";
import { defineStore } from "pinia";

import { api } from "@/lib/api";
import {
  createRequesterFilters,
  ticketErrorMessage,
} from "@/lib/ticket-requester";
/** URL-backed requester filter contract used by store state. */
import type { RequesterTicketFilters } from "@/lib/ticket-requester";
import type {
  Attachment,
  CreateTicketRequest,
  Ticket,
  TicketComment,
} from "@/types/api";

/** Shared requester ticket workspace with stale-response protection. */
export const useTicketsStore = defineStore("tickets", () => {
  const tickets = ref<Ticket[]>([]);
  const selectedTicket = ref<Ticket | null>(null);
  const comments = ref<TicketComment[]>([]);
  const attachments = ref<Attachment[]>([]);
  const filters = ref<RequesterTicketFilters>(createRequesterFilters());
  const nextCursor = ref<string | null>(null);
  const isLoading = ref(false);
  const isLoadingMore = ref(false);
  const isLoadingDetail = ref(false);
  const isSaving = ref(false);
  const error = ref<string | null>(null);
  const detailError = ref<string | null>(null);
  const actionError = ref<string | null>(null);
  let listRequest = 0;
  let detailRequest = 0;

  /** Whether another stable page can be requested. */
  const hasMore = computed(() => nextCursor.value !== null);

  /** Applies URL-derived filters without starting network activity. */
  function setFilters(next: RequesterTicketFilters): void {
    filters.value = { ...next };
  }

  /** Builds the server query for the current requester filters. */
  function listParams(cursor?: string) {
    return {
      status: filters.value.status || undefined,
      category_id: filters.value.categoryId || undefined,
      search: filters.value.search || undefined,
      cursor,
      page_size: 25,
    };
  }

  /** Replaces the visible list while retaining the previous successful result on failure. */
  async function loadTickets(): Promise<void> {
    const request = ++listRequest;
    isLoading.value = true;
    error.value = null;
    try {
      const page = await api.listTickets(listParams());
      if (request !== listRequest) return;
      tickets.value = page.items;
      nextCursor.value = page.next_cursor;
    } catch (failure) {
      if (request === listRequest) error.value = ticketErrorMessage(failure);
    } finally {
      if (request === listRequest) isLoading.value = false;
    }
  }

  /** Appends the next stable page without losing records already on screen. */
  async function loadMore(): Promise<void> {
    if (!nextCursor.value || isLoadingMore.value) return;
    const request = listRequest;
    isLoadingMore.value = true;
    error.value = null;
    try {
      const page = await api.listTickets(listParams(nextCursor.value));
      if (request !== listRequest) return;
      const known = new Set(tickets.value.map((ticket) => ticket.id));
      tickets.value.push(...page.items.filter((ticket) => !known.has(ticket.id)));
      nextCursor.value = page.next_cursor;
    } catch (failure) {
      if (request === listRequest) error.value = ticketErrorMessage(failure);
    } finally {
      isLoadingMore.value = false;
    }
  }

  /** Loads one visible ticket and its public conversation in parallel. */
  async function loadTicket(ticketId: string): Promise<void> {
    const request = ++detailRequest;
    isLoadingDetail.value = true;
    detailError.value = null;
    actionError.value = null;
    try {
      const [ticket, nextComments, nextAttachments] = await Promise.all([
        api.getTicket(ticketId),
        api.listTicketComments(ticketId),
        api.listTicketAttachments(ticketId),
      ]);
      if (request !== detailRequest) return;
      selectedTicket.value = ticket;
      comments.value = nextComments;
      attachments.value = nextAttachments;
      upsertTicket(ticket);
    } catch (failure) {
      if (request !== detailRequest) return;
      selectedTicket.value = null;
      comments.value = [];
      attachments.value = [];
      detailError.value = ticketErrorMessage(failure);
    } finally {
      if (request === detailRequest) isLoadingDetail.value = false;
    }
  }

  /** Creates one named requester ticket and places it at the top of the workspace. */
  async function createTicket(details: CreateTicketRequest): Promise<Ticket> {
    isSaving.value = true;
    actionError.value = null;
    try {
      const ticket = await api.createTicket(details);
      upsertTicket(ticket);
      selectedTicket.value = ticket;
      comments.value = [];
      attachments.value = [];
      return ticket;
    } catch (failure) {
      actionError.value = ticketErrorMessage(failure);
      throw failure;
    } finally {
      isSaving.value = false;
    }
  }

  /** Uploads one file and reflects its returned metadata without a page refresh. */
  async function uploadAttachment(parentId: string, file: File): Promise<Attachment> {
    isSaving.value = true;
    actionError.value = null;
    try {
      const attachment = await api.uploadAttachment("ticket", parentId, file);
      if (selectedTicket.value?.id === parentId) attachments.value.push(attachment);
      return attachment;
    } catch (failure) {
      actionError.value = ticketErrorMessage(failure);
      throw failure;
    } finally {
      isSaving.value = false;
    }
  }

  /** Adds one requester-visible reply and keeps the draft outside the store on failure. */
  async function addPublicComment(ticketId: string, body: string): Promise<TicketComment> {
    isSaving.value = true;
    actionError.value = null;
    try {
      const comment = await api.addTicketComment(ticketId, { body, visibility: "public" });
      if (selectedTicket.value?.id === ticketId) {
        comments.value.push(comment);
        selectedTicket.value = { ...selectedTicket.value, updated_at: comment.updated_at };
        upsertTicket(selectedTicket.value);
      }
      return comment;
    } catch (failure) {
      actionError.value = ticketErrorMessage(failure);
      throw failure;
    } finally {
      isSaving.value = false;
    }
  }

  /** Reopens one resolved requester ticket using conflict-safe server state. */
  async function reopenTicket(ticket: Ticket): Promise<Ticket> {
    isSaving.value = true;
    actionError.value = null;
    try {
      const updated = await api.updateTicket(ticket.id, {
        status: "open",
        expected_updated_at: ticket.updated_at,
      });
      if (selectedTicket.value?.id === ticket.id) selectedTicket.value = updated;
      upsertTicket(updated);
      return updated;
    } catch (failure) {
      actionError.value = ticketErrorMessage(failure);
      throw failure;
    } finally {
      isSaving.value = false;
    }
  }

  /** Inserts or replaces one ticket while preserving newest-update ordering. */
  function upsertTicket(ticket: Ticket): void {
    tickets.value = [ticket, ...tickets.value.filter((item) => item.id !== ticket.id)].sort(
      (left, right) => right.updated_at.localeCompare(left.updated_at),
    );
  }

  /** Clears detail state when the browser returns to the collection route. */
  function clearSelection(): void {
    detailRequest += 1;
    selectedTicket.value = null;
    comments.value = [];
    attachments.value = [];
    detailError.value = null;
    actionError.value = null;
    isLoadingDetail.value = false;
  }

  return {
    tickets,
    selectedTicket,
    comments,
    attachments,
    filters,
    nextCursor,
    isLoading,
    isLoadingMore,
    isLoadingDetail,
    isSaving,
    error,
    detailError,
    actionError,
    hasMore,
    setFilters,
    loadTickets,
    loadMore,
    loadTicket,
    createTicket,
    uploadAttachment,
    addPublicComment,
    reopenTicket,
    clearSelection,
  };
});
