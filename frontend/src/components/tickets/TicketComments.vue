<script setup lang="ts">
/** Public requester conversation with failure-safe reply drafts. */
import { ref } from "vue";
import { storeToRefs } from "pinia";

import { formatTicketTimestamp } from "@/lib/ticket-requester";
import { useTicketsStore } from "@/stores/tickets";
import type { TicketComment, TicketStatus } from "@/types/api";

const props = defineProps<{
  /** Parent ticket identifier. */
  ticketId: string;
  /** Public comments visible to this requester. */
  comments: TicketComment[];
  /** Signed-in requester identifier used for author labels. */
  currentUserId: string;
  /** Current lifecycle state controlling reply availability. */
  status: TicketStatus;
}>();

const ticketsStore = useTicketsStore();
const { isSaving, actionError } = storeToRefs(ticketsStore);
const draft = ref("");
const validationError = ref<string | null>(null);

/** Submits one public reply and clears the draft only after success. */
async function submitReply(): Promise<void> {
  validationError.value = null;
  if (!draft.value.trim()) {
    validationError.value = "Write a reply before sending.";
    return;
  }
  try {
    await ticketsStore.addPublicComment(props.ticketId, draft.value.trim());
    draft.value = "";
  } catch {
    // The visible draft is intentionally retained so the requester can retry.
  }
}
</script>

<template>
  <section aria-labelledby="conversation-heading">
    <div class="flex items-end justify-between gap-4">
      <div>
        <p class="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Conversation</p>
        <h2 id="conversation-heading" class="mt-1 text-xl font-bold">Updates and replies</h2>
      </div>
      <span class="text-xs text-[var(--color-text-tertiary)]">{{ comments.length }} {{ comments.length === 1 ? "message" : "messages" }}</span>
    </div>

    <ol v-if="comments.length" class="mt-5 space-y-3">
      <li
        v-for="comment in comments"
        :key="comment.id"
        class="rounded-2xl border p-4"
        :class="comment.author_id === currentUserId ? 'ml-5 bg-[var(--color-surface-tertiary)] sm:ml-12' : 'mr-5 bg-[var(--color-surface-primary)] sm:mr-12'"
        :style="{ borderColor: 'var(--color-border-default)' }"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <span class="text-xs font-bold uppercase tracking-wider">{{ comment.author_id === currentUserId ? "You" : "IT support" }}</span>
          <time class="text-xs text-[var(--color-text-tertiary)]" :datetime="comment.created_at">{{ formatTicketTimestamp(comment.created_at) }}</time>
        </div>
        <p class="mt-3 whitespace-pre-wrap text-sm leading-6">{{ comment.body }}</p>
      </li>
    </ol>
    <p v-else class="mt-5 rounded-2xl border border-dashed p-5 text-sm leading-6 text-[var(--color-text-secondary)]" :style="{ borderColor: 'var(--color-border-default)' }">
      No replies yet. IT support updates will appear here.
    </p>

    <div v-if="status === 'closed'" class="mt-5 rounded-2xl border bg-[var(--color-surface-tertiary)] p-4" :style="{ borderColor: 'var(--color-border-default)' }">
      <p class="font-bold">Ticket closed</p>
      <p class="mt-1 text-sm text-[var(--color-text-secondary)]">Closed tickets are read-only. Submit a new ticket if the issue has returned.</p>
    </div>
    <form v-else class="mt-5" @submit.prevent="submitReply">
      <label for="public-reply" class="block text-sm font-semibold">Add a public reply</label>
      <textarea
        id="public-reply"
        v-model="draft"
        class="mt-2 min-h-28 w-full resize-y rounded-xl border bg-[var(--color-surface-primary)] px-3 py-3 outline-none focus:border-[var(--color-accent-primary)] focus:ring-2 focus:ring-[var(--color-accent-primary)]/20"
        maxlength="5000"
        placeholder="Add details or answer the technician…"
        :disabled="isSaving"
      />
      <div class="mt-3 flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <p v-if="validationError || actionError" class="text-sm text-red-700 dark:text-red-300" role="alert">{{ validationError || actionError }}</p>
        <span v-else class="text-xs leading-5 text-[var(--color-text-tertiary)]">Visible to you and IT support.</span>
        <button type="submit" class="min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white disabled:opacity-50" :disabled="isSaving">
          {{ isSaving ? "Sending…" : "Send reply" }}
        </button>
      </div>
    </form>
  </section>
</template>
