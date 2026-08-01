<script setup lang="ts">
/** Responsive requester ticket list with explicit loading and empty states. */
import PriorityBadge from "./PriorityBadge.vue";
import StatusBadge from "./StatusBadge.vue";
import {
  categoryLabel,
  formatTicketTimestamp,
  ticketLocation,
  type RequesterTicketFilters,
} from "@/lib/ticket-requester";
import type { PublicCategory, Ticket } from "@/types";

defineProps<{
  /** Current visible ticket page. */
  tickets: Ticket[];
  /** Active runtime categories. */
  categories: PublicCategory[];
  /** Selected route ticket identifier. */
  selectedId: string | null;
  /** Current list loading state. */
  loading: boolean;
  /** Current next-page loading state. */
  loadingMore: boolean;
  /** Recoverable list failure. */
  error: string | null;
  /** Whether another stable page is available. */
  hasMore: boolean;
  /** Current URL-backed filter values. */
  filters: RequesterTicketFilters;
}>();

const emit = defineEmits<{
  /** Retries the current list query. */
  retry: [];
  /** Requests the next stable page. */
  more: [];
}>();
</script>

<template>
  <div class="min-h-0">
    <p v-if="loading && tickets.length === 0" class="p-6 text-sm text-[var(--color-text-secondary)]" aria-live="polite">
      Loading tickets…
    </p>
    <div v-else-if="error && tickets.length === 0" class="p-6" role="alert">
      <p class="text-sm leading-6 text-red-700 dark:text-red-300">{{ error }}</p>
      <button type="button" class="mt-4 min-h-11 rounded-xl border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="emit('retry')">
        Try again
      </button>
    </div>
    <div v-else-if="tickets.length === 0" class="p-8 text-center">
      <div class="mx-auto grid h-12 w-12 place-items-center rounded-2xl bg-[var(--color-surface-tertiary)] font-mono text-lg" aria-hidden="true">0</div>
      <h2 class="mt-4 text-lg font-bold">No tickets match</h2>
      <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">Adjust the filters or submit a new staff request.</p>
    </div>
    <ul v-else class="divide-y" :style="{ borderColor: 'var(--color-border-default)' }" aria-label="Your tickets">
      <li v-for="ticket in tickets" :key="ticket.id">
        <router-link
          :to="ticketLocation(ticket.id, filters)"
          class="block border-l-4 px-4 py-4 transition hover:bg-[var(--color-surface-tertiary)] focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-[var(--color-accent-primary)]"
          :class="selectedId === ticket.id ? 'border-l-[var(--color-accent-primary)] bg-[var(--color-surface-tertiary)]' : 'border-l-transparent'"
          :aria-current="selectedId === ticket.id ? 'page' : undefined"
        >
          <div class="flex items-start justify-between gap-3">
            <span class="font-mono text-[0.68rem] font-bold uppercase tracking-[0.15em] text-[var(--color-text-tertiary)]">#{{ ticket.number }}</span>
            <StatusBadge :status="ticket.status" />
          </div>
          <h3 class="mt-2 line-clamp-2 font-bold leading-snug">{{ ticket.title }}</h3>
          <p class="mt-2 line-clamp-2 text-sm leading-5 text-[var(--color-text-secondary)]">{{ ticket.description }}</p>
          <div class="mt-3 flex flex-wrap items-center justify-between gap-2">
            <PriorityBadge :priority="ticket.priority" />
            <time class="text-xs text-[var(--color-text-tertiary)]" :datetime="ticket.updated_at">{{ formatTicketTimestamp(ticket.updated_at) }}</time>
          </div>
          <p class="mt-2 truncate text-xs text-[var(--color-text-tertiary)]">{{ categoryLabel(ticket.category_id, categories) }}</p>
        </router-link>
      </li>
    </ul>
    <div v-if="hasMore" class="border-t p-4" :style="{ borderColor: 'var(--color-border-default)' }">
      <button type="button" class="min-h-11 w-full rounded-xl border text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="loadingMore" @click="emit('more')">
        {{ loadingMore ? "Loading more…" : "Load more tickets" }}
      </button>
    </div>
    <p v-if="error && tickets.length > 0" class="border-t p-4 text-sm text-red-700 dark:text-red-300" role="alert">
      {{ error }} <button type="button" class="ml-1 font-bold underline" @click="emit('retry')">Try again</button>
    </p>
  </div>
</template>
