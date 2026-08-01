<script setup lang="ts">
/** URL-backed requester ticket workspace for named local staff accounts. */
import { computed, reactive, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { useRoute, useRouter } from "vue-router";

import AppLayout from "@/components/layout/AppLayout.vue";
import TicketDetail from "@/components/tickets/TicketDetail.vue";
import TicketForm from "@/components/tickets/TicketForm.vue";
import TicketList from "@/components/tickets/TicketList.vue";
import {
  createRequesterFilters,
  ticketLocation,
  type RequesterTicketFilters,
} from "@/lib/ticket-requester";
import { useAuthStore } from "@/stores/auth";
import { useTicketsStore } from "@/stores/tickets";
import type { Ticket } from "@/types/api";

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();
const ticketsStore = useTicketsStore();
const {
  tickets,
  selectedTicket,
  comments,
  attachments,
  filters,
  isLoading,
  isLoadingMore,
  isLoadingDetail,
  error,
  detailError,
  hasMore,
} = storeToRefs(ticketsStore);
const formOpen = ref(false);
const filterDraft = reactive<RequesterTicketFilters>(createRequesterFilters(route.query));

/** Active runtime categories used for ticket creation and historical labels. */
const categories = computed(() => authStore.publicConfig?.categories ?? []);
/** Stable selected identifier derived from the route. */
const selectedId = computed(() => (typeof route.params.id === "string" ? route.params.id : null));

watch(
  () => JSON.stringify(route.query),
  async () => {
    const next = createRequesterFilters(route.query);
    Object.assign(filterDraft, next);
    ticketsStore.setFilters(next);
    await ticketsStore.loadTickets();
  },
  { immediate: true },
);

watch(
  selectedId,
  async (ticketId) => {
    if (ticketId) await ticketsStore.loadTicket(ticketId);
    else ticketsStore.clearSelection();
  },
  { immediate: true },
);

/** Writes the current filter draft into browser history without losing selection. */
async function applyFilters(): Promise<void> {
  const normalized = createRequesterFilters({
    q: filterDraft.search,
    status: filterDraft.status,
    category: filterDraft.categoryId,
  });
  await router.push(ticketLocation(selectedId.value, normalized));
}

/** Clears every filter and returns to the unfiltered collection context. */
async function clearFilters(): Promise<void> {
  Object.assign(filterDraft, createRequesterFilters());
  await router.push(ticketLocation(selectedId.value, filterDraft));
}

/** Closes the form and opens the newly created ticket in the same filter context. */
async function ticketCreated(ticket: Ticket): Promise<void> {
  formOpen.value = false;
  await router.push(ticketLocation(ticket.id, filters.value));
}
</script>

<template>
  <AppLayout>
    <section class="space-y-5">
      <header class="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Staff help desk</p>
          <h1 class="mt-2 text-3xl font-bold tracking-tight sm:text-4xl">Your support tickets</h1>
          <p class="mt-2 max-w-2xl text-sm leading-6 text-[var(--color-text-secondary)]">
            Submit a named request, follow the technician’s progress, and keep all details with the ticket.
          </p>
        </div>
        <button
          type="button"
          class="min-h-12 rounded-xl bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white shadow-sm hover:bg-[var(--color-accent-primary-hover)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--color-accent-primary)]"
          @click="formOpen = true"
        >
          + New ticket
        </button>
      </header>

      <form
        class="grid gap-3 rounded-2xl border bg-[var(--color-surface-secondary)] p-4 sm:grid-cols-[minmax(12rem,1fr)_auto_auto_auto] sm:items-end"
        :style="{ borderColor: 'var(--color-border-default)' }"
        aria-label="Filter your tickets"
        @submit.prevent="applyFilters"
      >
        <div>
          <label for="ticket-search" class="block text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">Search</label>
          <input
            id="ticket-search"
            v-model="filterDraft.search"
            type="search"
            maxlength="100"
            class="mt-2 min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 outline-none focus:border-[var(--color-accent-primary)]"
            placeholder="Title or details"
          />
        </div>
        <div>
          <label for="ticket-status-filter" class="block text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">Status</label>
          <select id="ticket-status-filter" v-model="filterDraft.status" class="mt-2 min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 outline-none focus:border-[var(--color-accent-primary)]">
            <option value="">All statuses</option>
            <option value="new">New</option>
            <option value="open">In progress</option>
            <option value="waiting_on_requester">Waiting on you</option>
            <option value="resolved">Resolved</option>
            <option value="closed">Closed</option>
          </select>
        </div>
        <div>
          <label for="ticket-category-filter" class="block text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">Category</label>
          <select id="ticket-category-filter" v-model="filterDraft.categoryId" class="mt-2 min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 outline-none focus:border-[var(--color-accent-primary)]">
            <option value="">All categories</option>
            <option v-for="category in categories" :key="category.id" :value="category.id">{{ category.name }}</option>
          </select>
        </div>
        <div class="flex gap-2">
          <button type="submit" class="min-h-11 flex-1 rounded-xl border bg-[var(--color-surface-elevated)] px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-strong)' }">Apply</button>
          <button type="button" class="min-h-11 rounded-xl px-3 text-sm font-semibold text-[var(--color-text-secondary)]" @click="clearFilters">Clear</button>
        </div>
      </form>

      <div
        class="overflow-hidden rounded-2xl border bg-[var(--color-surface-elevated)] shadow-[var(--shadow-sm)] lg:grid lg:min-h-[42rem] lg:grid-cols-[23rem_minmax(0,1fr)]"
        :style="{ borderColor: 'var(--color-border-default)' }"
      >
        <aside class="border-r lg:block" :class="selectedId ? 'hidden' : 'block'" :style="{ borderColor: 'var(--color-border-default)' }" aria-label="Ticket results">
          <div class="flex items-center justify-between border-b px-4 py-3" :style="{ borderColor: 'var(--color-border-default)' }">
            <span class="text-xs font-bold uppercase tracking-[0.14em] text-[var(--color-text-tertiary)]">Recent requests</span>
            <span class="font-mono text-xs text-[var(--color-text-tertiary)]">{{ tickets.length }}</span>
          </div>
          <TicketList
            :tickets="tickets"
            :categories="categories"
            :selected-id="selectedId"
            :loading="isLoading"
            :loading-more="isLoadingMore"
            :error="error"
            :has-more="hasMore"
            :filters="filters"
            @retry="ticketsStore.loadTickets"
            @more="ticketsStore.loadMore"
          />
        </aside>

        <main class="min-w-0" :class="selectedId ? 'block' : 'hidden lg:block'" aria-live="polite">
          <router-link v-if="selectedId" :to="ticketLocation(null, filters)" class="flex min-h-12 items-center border-b px-4 text-sm font-bold text-[var(--color-accent-primary)] lg:hidden" :style="{ borderColor: 'var(--color-border-default)' }">
            ← Back to your tickets
          </router-link>
          <p v-if="isLoadingDetail" class="p-8 text-sm text-[var(--color-text-secondary)]">Loading ticket…</p>
          <div v-else-if="detailError" class="p-8" role="alert">
            <p class="font-bold">Ticket unavailable</p>
            <p class="mt-2 max-w-lg text-sm leading-6 text-[var(--color-text-secondary)]">{{ detailError }}</p>
            <div class="mt-5 flex flex-wrap gap-3">
              <button v-if="selectedId" type="button" class="min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-4 text-sm font-bold text-white" @click="ticketsStore.loadTicket(selectedId)">Try again</button>
              <router-link :to="ticketLocation(null, filters)" class="inline-flex min-h-11 items-center rounded-xl border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }">Return to tickets</router-link>
            </div>
          </div>
          <TicketDetail
            v-else-if="selectedTicket"
            :ticket="selectedTicket"
            :comments="comments"
            :attachments="attachments"
            :categories="categories"
          />
          <div v-else class="grid min-h-[38rem] place-items-center p-8 text-center">
            <div class="max-w-sm">
              <div class="mx-auto grid h-16 w-16 place-items-center rounded-3xl bg-[var(--color-surface-tertiary)] font-mono text-xl" aria-hidden="true">#</div>
              <h2 class="mt-5 text-xl font-bold">Choose a ticket</h2>
              <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">Select a request to read its history, add details, or attach another file.</p>
            </div>
          </div>
        </main>
      </div>
    </section>

    <div v-if="formOpen" class="fixed inset-0 z-50 grid place-items-end bg-black/55 p-0 sm:place-items-center sm:p-6" role="dialog" aria-modal="true" aria-labelledby="new-ticket-heading" @keydown.esc="formOpen = false">
      <div class="max-h-[95vh] w-full overflow-y-auto rounded-t-3xl border bg-[var(--color-surface-elevated)] p-5 shadow-2xl sm:max-w-2xl sm:rounded-3xl sm:p-7" :style="{ borderColor: 'var(--color-border-default)' }">
        <h2 id="new-ticket-heading" class="sr-only">New ticket</h2>
        <TicketForm @created="ticketCreated" @cancel="formOpen = false" />
      </div>
    </div>
  </AppLayout>
</template>
