<script setup lang="ts">
/** Responsive administrator queue with stable cursor pagination and local triage filters. */
import { computed, onMounted, ref, watch } from "vue";

import AdminTicketDetail from "@/components/tickets/AdminTicketDetail.vue";
import PriorityBadge from "@/components/tickets/PriorityBadge.vue";
import StatusBadge from "@/components/tickets/StatusBadge.vue";
import TicketFilters from "@/components/tickets/TicketFilters.vue";
import { api } from "@/lib/api";
import {
  activeSupportAccounts,
  createDefaultTicketFilters,
  filterAndSortTickets,
  resolveTicketSelection,
  ticketAdminErrorMessage,
  type TicketFilters as QueueFilters,
} from "@/lib/ticket-admin";
import { categoryLabel, formatTicketTimestamp } from "@/lib/ticket-requester";
import type { PublicCategory, Ticket, User } from "@/types/api";

/** Runtime accounts, categories, privileges, and upload policy for the queue. */
const props = defineProps<{
  /** Account records used for requester and assignment labels. */
  users: User[];
  /** Active runtime categories. */
  categories: PublicCategory[];
  /** Whether the operator has administrator-only workflow authority. */
  isAdministrator: boolean;
  /** Runtime single-file upload ceiling. */
  maxUploadBytes: number;
}>();

/** Loaded newest-first shared queue pages. */
const tickets = ref<Ticket[]>([]);
/** Opaque cursor for the next stable server page. */
const nextCursor = ref<string | null>(null);
/** Current local filter and sort state. */
const filters = ref<QueueFilters>(createDefaultTicketFilters());
/** Selected queue record identifier. */
const selectedId = ref<string | null>(null);
/** Whether narrow screens are displaying the detail pane. */
const showMobileDetail = ref(false);
/** Whether the first queue page is loading. */
const loading = ref(true);
/** Whether an additional queue page is loading. */
const loadingMore = ref(false);
/** Bounded queue-loading failure guidance. */
const error = ref("");
/** Monotonic request identity used to ignore stale first-page responses. */
let loadGeneration = 0;

/** Active accounts eligible for ticket assignment. */
const supportAccounts = computed(() => activeSupportAccounts(props.users));
/** Tickets matching the current local queue controls. */
const visibleTickets = computed(() => filterAndSortTickets(tickets.value, filters.value));
/** Full selected ticket record when it remains visible. */
const selectedTicket = computed(() => tickets.value.find((ticket) => ticket.id === selectedId.value) ?? null);

/** Returns a safe account label without rendering optional email metadata. */
function accountName(userId: string | null): string {
  if (!userId) return "Unassigned";
  return props.users.find((user) => user.id === userId)?.display_name ?? "Former staff account";
}

/** Loads the first complete shared-queue page and discards stale responses. */
async function loadQueue(): Promise<void> {
  const generation = ++loadGeneration;
  loading.value = true;
  error.value = "";
  try {
    const page = await api.listTickets({ page_size: 25 });
    if (generation !== loadGeneration) return;
    tickets.value = page.items;
    nextCursor.value = page.next_cursor;
  } catch (failure) {
    if (generation === loadGeneration) error.value = ticketAdminErrorMessage(failure);
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

/** Appends the next stable cursor page without duplicating existing records. */
async function loadMore(): Promise<void> {
  if (!nextCursor.value || loadingMore.value) return;
  const cursor = nextCursor.value;
  loadingMore.value = true;
  error.value = "";
  try {
    const page = await api.listTickets({ page_size: 25, cursor });
    if (cursor !== nextCursor.value) return;
    const known = new Set(tickets.value.map((ticket) => ticket.id));
    tickets.value = [...tickets.value, ...page.items.filter((ticket) => !known.has(ticket.id))];
    nextCursor.value = page.next_cursor;
  } catch (failure) {
    error.value = ticketAdminErrorMessage(failure);
  } finally {
    loadingMore.value = false;
  }
}

/** Selects one visible ticket and opens detail on narrow screens. */
function selectTicket(ticketId: string): void {
  selectedId.value = ticketId;
  showMobileDetail.value = true;
}

/** Replaces one server-confirmed queue record while retaining selection. */
function acceptUpdatedTicket(updated: Ticket): void {
  tickets.value = tickets.value.map((ticket) => (ticket.id === updated.id ? updated : ticket));
}

watch(
  () => visibleTickets.value.map((ticket) => ticket.id).join(","),
  () => {
    selectedId.value = resolveTicketSelection(visibleTickets.value, selectedId.value);
    if (!selectedId.value) showMobileDetail.value = false;
  },
);

onMounted(() => void loadQueue());
</script>

<template>
  <section class="space-y-4" aria-labelledby="queue-heading">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Shared support workload</p>
        <h2 id="queue-heading" class="mt-2 text-2xl font-bold">Ticket queue</h2>
      </div>
      <div class="flex items-center gap-3">
        <p class="text-sm text-[var(--color-text-secondary)]">{{ visibleTickets.length }} shown · {{ tickets.length }} loaded</p>
        <button type="button" class="min-h-11 px-2 text-sm font-bold text-[var(--color-accent-primary)]" :disabled="loading" @click="loadQueue">Refresh queue</button>
      </div>
    </div>

    <TicketFilters v-model="filters" :categories="categories" :support-accounts="supportAccounts" />
    <p v-if="error" class="rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-800 dark:text-red-200" role="alert">{{ error }}</p>

    <div class="overflow-hidden rounded-2xl border bg-[var(--color-surface-primary)] lg:grid lg:grid-cols-[minmax(18rem,0.72fr)_minmax(0,1.6fr)]" :style="{ borderColor: 'var(--color-border-default)' }">
      <aside class="border-r lg:max-h-[72rem] lg:overflow-y-auto" :class="showMobileDetail ? 'hidden lg:block' : 'block'" :style="{ borderColor: 'var(--color-border-default)' }" aria-label="Ticket queue results">
        <p v-if="loading" class="p-6 text-sm" role="status">Loading all visible tickets…</p>
        <div v-else-if="visibleTickets.length" class="divide-y" :style="{ borderColor: 'var(--color-border-default)' }">
          <button
            v-for="ticket in visibleTickets"
            :key="ticket.id"
            type="button"
            class="block min-h-32 w-full p-4 text-left transition hover:bg-[var(--color-surface-secondary)] focus-visible:outline-2 focus-visible:outline-inset focus-visible:outline-[var(--color-accent-primary)]"
            :class="selectedId === ticket.id ? 'bg-[var(--color-surface-tertiary)]' : ''"
            :aria-current="selectedId === ticket.id ? 'true' : undefined"
            @click="selectTicket(ticket.id)"
          >
            <div class="flex items-center justify-between gap-3"><span class="font-mono text-xs font-bold">#{{ ticket.number }}</span><time class="text-xs text-[var(--color-text-tertiary)]" :datetime="ticket.updated_at">{{ formatTicketTimestamp(ticket.updated_at) }}</time></div>
            <p class="mt-2 line-clamp-2 font-bold">{{ ticket.title }}</p>
            <div class="mt-3 flex flex-wrap gap-2"><StatusBadge :status="ticket.status" /><PriorityBadge :priority="ticket.priority" /></div>
            <p class="mt-3 truncate text-xs text-[var(--color-text-secondary)]">{{ accountName(ticket.requester_id) }} · {{ accountName(ticket.assignee_id) }} · {{ categoryLabel(ticket.category_id, categories) }}</p>
          </button>
        </div>
        <div v-else class="p-8 text-center"><p class="font-bold">No queue results</p><p class="mt-2 text-sm text-[var(--color-text-secondary)]">Adjust the filters or reload the queue.</p></div>
        <div class="border-t p-4 text-center" :style="{ borderColor: 'var(--color-border-default)' }">
          <button v-if="nextCursor" type="button" class="min-h-11 rounded-xl border px-4 text-sm font-bold disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="loadingMore" @click="loadMore">{{ loadingMore ? "Loading…" : "Load more tickets" }}</button>
          <p v-else-if="tickets.length" class="text-xs text-[var(--color-text-tertiary)]">End of loaded queue</p>
        </div>
      </aside>

      <div :class="showMobileDetail ? 'block' : 'hidden lg:block'">
        <AdminTicketDetail
          v-if="selectedTicket"
          :key="selectedTicket.id"
          :ticket="selectedTicket"
          :users="users"
          :categories="categories"
          :support-accounts="supportAccounts"
          :is-administrator="isAdministrator"
          :max-upload-bytes="maxUploadBytes"
          @updated="acceptUpdatedTicket"
          @back="showMobileDetail = false"
        />
        <div v-else class="grid min-h-80 place-items-center p-8 text-center text-[var(--color-text-secondary)]">Select a visible ticket to review its history and workflow.</div>
      </div>
    </div>
  </section>
</template>
