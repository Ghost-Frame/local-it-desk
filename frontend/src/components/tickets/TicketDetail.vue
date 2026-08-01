<script setup lang="ts">
/** Requester ticket detail with public history, attachments, and reopen action. */
import { ref } from "vue";
import { storeToRefs } from "pinia";

import AttachmentPreview from "@/components/common/AttachmentPreview.vue";
import FileUpload from "@/components/common/FileUpload.vue";
import PriorityBadge from "@/components/tickets/PriorityBadge.vue";
import StatusBadge from "@/components/tickets/StatusBadge.vue";
import TicketComments from "@/components/tickets/TicketComments.vue";
import {
  categoryLabel,
  formatTicketTimestamp,
  requesterCanReopen,
} from "@/lib/ticket-requester";
import { useAuthStore } from "@/stores/auth";
import { useTicketsStore } from "@/stores/tickets";
import type { Attachment, PublicCategory, Ticket, TicketComment } from "@/types/api";

const props = defineProps<{
  /** Current requester-visible ticket. */
  ticket: Ticket;
  /** Public conversation entries. */
  comments: TicketComment[];
  /** Requester-visible attachment metadata. */
  attachments: Attachment[];
  /** Runtime category lookup. */
  categories: PublicCategory[];
}>();

const ticketsStore = useTicketsStore();
const authStore = useAuthStore();
const { isSaving, actionError } = storeToRefs(ticketsStore);
const selectedFile = ref<File | null>(null);
const uploadSuccess = ref<string | null>(null);

/** Reopens this resolved request after conflict-safe server validation. */
async function reopen(): Promise<void> {
  uploadSuccess.value = null;
  try {
    await ticketsStore.reopenTicket(props.ticket);
  } catch {
    // Store exposes bounded failure guidance next to the action.
  }
}

/** Adds one ticket-level attachment while retaining selection on failure. */
async function upload(): Promise<void> {
  if (!selectedFile.value) return;
  uploadSuccess.value = null;
  try {
    const name = selectedFile.value.name;
    await ticketsStore.uploadAttachment(props.ticket.id, selectedFile.value);
    selectedFile.value = null;
    uploadSuccess.value = `${name} attached.`;
  } catch {
    // The picker retains the file so the requester can retry.
  }
}
</script>

<template>
  <article>
    <header class="border-b p-5 sm:p-7" :style="{ borderColor: 'var(--color-border-default)' }">
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-mono text-xs font-bold uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Ticket #{{ ticket.number }}</span>
        <StatusBadge :status="ticket.status" />
      </div>
      <h1 class="mt-4 text-2xl font-bold tracking-tight sm:text-3xl">{{ ticket.title }}</h1>
      <div class="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 text-sm text-[var(--color-text-secondary)]">
        <PriorityBadge :priority="ticket.priority" />
        <span>{{ categoryLabel(ticket.category_id, categories) }}</span>
        <time :datetime="ticket.created_at">Opened {{ formatTicketTimestamp(ticket.created_at) }}</time>
      </div>
      <div v-if="requesterCanReopen(ticket)" class="mt-5 rounded-2xl border border-emerald-500/30 bg-emerald-500/10 p-4 sm:flex sm:items-center sm:justify-between sm:gap-4">
        <div>
          <p class="font-bold">Did this solve the issue?</p>
          <p class="mt-1 text-sm text-[var(--color-text-secondary)]">If not, reopen the same ticket and continue the conversation.</p>
        </div>
        <button type="button" class="mt-3 min-h-11 rounded-xl border border-emerald-600/40 bg-[var(--color-surface-elevated)] px-4 text-sm font-bold sm:mt-0" :disabled="isSaving" @click="reopen">
          Reopen ticket
        </button>
      </div>
      <p v-if="actionError" class="mt-4 text-sm text-red-700 dark:text-red-300" role="alert">{{ actionError }}</p>
    </header>

    <div class="space-y-8 p-5 sm:p-7">
      <section aria-labelledby="description-heading">
        <p class="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Original request</p>
        <h2 id="description-heading" class="sr-only">Original request details</h2>
        <p class="mt-3 whitespace-pre-wrap text-sm leading-7 sm:text-base">{{ ticket.description }}</p>
      </section>

      <section class="border-t pt-7" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="attachments-heading">
        <div class="flex items-end justify-between gap-4">
          <div>
            <p class="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Evidence</p>
            <h2 id="attachments-heading" class="mt-1 text-xl font-bold">Attachments</h2>
          </div>
          <span class="text-xs text-[var(--color-text-tertiary)]">{{ attachments.length }} files</span>
        </div>
        <div v-if="attachments.length" class="mt-4 grid gap-3 sm:grid-cols-2">
          <AttachmentPreview v-for="attachment in attachments" :key="attachment.id" :attachment="attachment" />
        </div>
        <p v-else class="mt-3 text-sm text-[var(--color-text-secondary)]">No files attached.</p>
        <form v-if="ticket.status !== 'closed'" class="mt-5 rounded-2xl border bg-[var(--color-surface-secondary)] p-4" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="upload">
          <FileUpload v-model="selectedFile" label="Add another file" :max-bytes="authStore.publicConfig?.max_upload_bytes" :disabled="isSaving" />
          <div class="mt-3 flex items-center justify-between gap-3">
            <p class="text-xs text-[var(--color-text-tertiary)]" aria-live="polite">{{ uploadSuccess }}</p>
            <button type="submit" class="min-h-11 rounded-xl border px-4 text-sm font-bold disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="!selectedFile || isSaving">
              {{ isSaving ? "Uploading…" : "Attach file" }}
            </button>
          </div>
        </form>
      </section>

      <div class="border-t pt-7" :style="{ borderColor: 'var(--color-border-default)' }">
        <TicketComments
          :ticket-id="ticket.id"
          :comments="comments"
          :current-user-id="authStore.user?.id ?? ''"
          :status="ticket.status"
        />
      </div>
    </div>
  </article>
</template>
