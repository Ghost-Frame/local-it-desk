<script setup lang="ts">
/** Staff ticket detail editor with conflict-safe workflow and separated note visibility. */
import { computed, ref, watch } from "vue";

import AttachmentPreview from "@/components/common/AttachmentPreview.vue";
import FileUpload from "@/components/common/FileUpload.vue";
import PriorityBadge from "@/components/tickets/PriorityBadge.vue";
import StatusBadge from "@/components/tickets/StatusBadge.vue";
import { ApiError, api } from "@/lib/api";
import { availableStatusTransitions, ticketAdminErrorMessage } from "@/lib/ticket-admin";
import { categoryLabel, formatTicketTimestamp } from "@/lib/ticket-requester";
import type { Attachment, PublicCategory, Ticket, TicketComment, TicketPriority, TicketStatus, UpdateTicketRequest, User } from "@/types/api";

/** Inputs supplied by the owning shared-queue pane. */
const props = defineProps<{
  /** Ticket selected from the shared staff queue. */
  ticket: Ticket;
  /** Accounts used for requester, author, and assignee labels. */
  users: User[];
  /** Active runtime categories. */
  categories: PublicCategory[];
  /** Active technician and administrator assignees. */
  supportAccounts: User[];
  /** Whether the current operator may reopen a closed ticket. */
  isAdministrator: boolean;
  /** Runtime single-file upload ceiling. */
  maxUploadBytes: number;
}>();

/** Server-confirmed updates and narrow-screen navigation events. */
const emit = defineEmits<{
  /** Replaces the matching queue record after a server-confirmed change. */
  updated: [ticket: Ticket];
  /** Returns to the queue list on narrow screens. */
  back: [];
}>();

/** Editable lifecycle draft. */
const status = ref<TicketStatus>(props.ticket.status);
/** Editable urgency draft. */
const priority = ref<TicketPriority>(props.ticket.priority);
/** Editable assignment draft using an empty string for unassigned. */
const assigneeId = ref(props.ticket.assignee_id ?? "");
/** Editable active category draft. */
const categoryId = ref(props.ticket.category_id ?? "");
/** Authorized conversation entries for the selected ticket. */
const comments = ref<TicketComment[]>([]);
/** Authorized attachment metadata for the selected ticket. */
const attachments = ref<Attachment[]>([]);
/** Requester-visible reply draft. */
const publicReply = ref("");
/** Staff-only note draft. */
const internalNote = ref("");
/** Ticket-level file awaiting upload. */
const selectedFile = ref<File | null>(null);
/** Whether related ticket records are loading. */
const loadingRelated = ref(false);
/** Whether one mutation is in flight. */
const saving = ref(false);
/** Bounded operator-facing error guidance. */
const error = ref("");
/** Successful operation feedback. */
const message = ref("");
/** Monotonic identity used to discard stale related-record responses. */
let relatedGeneration = 0;

/** Requester account when it still exists. */
const requester = computed(() => props.users.find((user) => user.id === props.ticket.requester_id));
/** Current assignee account, including a historical inactive account. */
const currentAssignee = computed(() => props.users.find((user) => user.id === props.ticket.assignee_id));
/** Whether the historical assignment needs an extra disabled option. */
const hasHistoricalAssignee = computed(() => currentAssignee.value && !props.supportAccounts.some((user) => user.id === currentAssignee.value?.id));
/** Whether the historical category needs an extra disabled option. */
const hasHistoricalCategory = computed(() => props.ticket.category_id && !props.categories.some((category) => category.id === props.ticket.category_id));
/** Allowed lifecycle choices for the selected ticket and operator. */
const statusOptions = computed(() => availableStatusTransitions(props.ticket.status, props.isAdministrator));
/** Whether the ticket is immutable until an administrator reopens it. */
const isClosed = computed(() => props.ticket.status === "closed");
/** Whether any persisted ticket field differs from its current server value. */
const hasChanges = computed(() => status.value !== props.ticket.status || priority.value !== props.ticket.priority || assigneeId.value !== (props.ticket.assignee_id ?? "") || categoryId.value !== (props.ticket.category_id ?? ""));

/** Returns a safe display label for one account identifier. */
function accountName(userId: string): string {
  return props.users.find((user) => user.id === userId)?.display_name ?? "Former staff account";
}

/** Restores every editor control from the latest selected ticket record. */
function resetDraft(): void {
  status.value = props.ticket.status;
  priority.value = props.ticket.priority;
  assigneeId.value = props.ticket.assignee_id ?? "";
  categoryId.value = props.ticket.category_id ?? "";
}

/** Loads authorized comments and attachments for the selected ticket. */
async function loadRelated(): Promise<void> {
  const generation = ++relatedGeneration;
  const ticketId = props.ticket.id;
  loadingRelated.value = true;
  error.value = "";
  try {
    const [nextComments, nextAttachments] = await Promise.all([
      api.listTicketComments(ticketId),
      api.listTicketAttachments(ticketId),
    ]);
    if (generation !== relatedGeneration || ticketId !== props.ticket.id) return;
    comments.value = nextComments;
    attachments.value = nextAttachments;
  } catch (failure) {
    if (generation === relatedGeneration && ticketId === props.ticket.id) {
      error.value = ticketAdminErrorMessage(failure);
    }
  } finally {
    if (generation === relatedGeneration && ticketId === props.ticket.id) {
      loadingRelated.value = false;
    }
  }
}

/** Reloads the canonical server ticket and related records. */
async function reloadTicket(): Promise<void> {
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    const current = await api.getTicket(props.ticket.id);
    emit("updated", current);
    await loadRelated();
    message.value = "Current ticket reloaded.";
  } catch (failure) {
    error.value = ticketAdminErrorMessage(failure);
  } finally {
    saving.value = false;
  }
}

/** Builds the smallest conflict-safe mutation supported by the server. */
function updateRequest(): UpdateTicketRequest {
  const request: UpdateTicketRequest = { expected_updated_at: props.ticket.updated_at };
  if (status.value !== props.ticket.status) request.status = status.value;
  if (priority.value !== props.ticket.priority) request.priority = priority.value;
  if (assigneeId.value !== (props.ticket.assignee_id ?? "")) {
    request.assignee_id = assigneeId.value || null;
  }
  if (categoryId.value && categoryId.value !== (props.ticket.category_id ?? "")) {
    request.category_id = categoryId.value;
  }
  return request;
}

/** Saves explicit workflow changes and reloads canonical state after a conflict. */
async function saveTicket(): Promise<void> {
  if (!hasChanges.value) return;
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    const updated = await api.updateTicket(props.ticket.id, updateRequest());
    emit("updated", updated);
    message.value = "Ticket changes saved.";
  } catch (failure) {
    if (failure instanceof ApiError && failure.status === 409) {
      try {
        emit("updated", await api.getTicket(props.ticket.id));
      } catch {
        // Retain the bounded conflict guidance if the follow-up read also fails.
      }
    }
    error.value = ticketAdminErrorMessage(failure);
  } finally {
    saving.value = false;
  }
}

/** Adds one conversation entry while preserving the draft on failure. */
async function addComment(visibility: "public" | "internal"): Promise<void> {
  const draft = visibility === "public" ? publicReply : internalNote;
  const body = draft.value.trim();
  if (!body) return;
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    const comment = await api.addTicketComment(props.ticket.id, { body, visibility });
    comments.value = [...comments.value, comment];
    draft.value = "";
    emit("updated", { ...props.ticket, updated_at: comment.updated_at });
    message.value = visibility === "public" ? "Public reply posted." : "Internal note added.";
  } catch (failure) {
    error.value = ticketAdminErrorMessage(failure);
  } finally {
    saving.value = false;
  }
}

/** Uploads one ticket-level file and retains the selection if upload fails. */
async function uploadAttachment(): Promise<void> {
  if (!selectedFile.value) return;
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    const attachment = await api.uploadAttachment("ticket", props.ticket.id, selectedFile.value);
    attachments.value = [...attachments.value, attachment];
    const filename = selectedFile.value.name;
    selectedFile.value = null;
    message.value = filename + " attached.";
  } catch (failure) {
    error.value = ticketAdminErrorMessage(failure);
  } finally {
    saving.value = false;
  }
}

watch(
  () => [props.ticket.id, props.ticket.updated_at] as const,
  ([ticketId], previous) => {
    resetDraft();
    error.value = "";
    if (ticketId !== previous?.[0]) {
      comments.value = [];
      attachments.value = [];
      publicReply.value = "";
      internalNote.value = "";
      selectedFile.value = null;
      void loadRelated();
    }
  },
  { immediate: true },
);
</script>

<template>
  <article class="min-w-0 bg-[var(--color-surface-primary)]">
    <header class="border-b p-5 sm:p-7" :style="{ borderColor: 'var(--color-border-default)' }">
      <button type="button" class="mb-4 min-h-11 text-sm font-bold text-[var(--color-accent-primary)] lg:hidden" @click="emit('back')">← Back to queue</button>
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-mono text-xs font-bold uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Ticket #{{ ticket.number }}</span>
        <StatusBadge :status="ticket.status" />
        <PriorityBadge :priority="ticket.priority" />
      </div>
      <h2 class="mt-4 text-2xl font-bold tracking-tight sm:text-3xl">{{ ticket.title }}</h2>
      <div class="mt-4 flex flex-wrap gap-x-5 gap-y-2 text-sm text-[var(--color-text-secondary)]">
        <span>Requested by {{ requester?.display_name ?? "Former staff account" }}</span>
        <span>{{ categoryLabel(ticket.category_id, categories) }}</span>
        <time :datetime="ticket.created_at">Opened {{ formatTicketTimestamp(ticket.created_at) }}</time>
      </div>
      <p class="mt-5 whitespace-pre-wrap text-sm leading-7">{{ ticket.description }}</p>
      <div class="mt-5 flex flex-wrap items-center gap-3">
        <button type="button" class="min-h-11 rounded-xl border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="saving" @click="reloadTicket">Reload current ticket</button>
        <p class="text-sm text-[var(--color-text-secondary)]" aria-live="polite">{{ message }}</p>
      </div>
      <p v-if="error" class="mt-4 rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-800 dark:text-red-200" role="alert">{{ error }}</p>
    </header>

    <div class="space-y-8 p-5 sm:p-7">
      <section aria-labelledby="workflow-heading">
        <div class="flex flex-wrap items-end justify-between gap-3">
          <div><p class="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Ownership</p><h3 id="workflow-heading" class="mt-1 text-xl font-bold">Workflow</h3></div>
          <span v-if="isClosed" class="text-xs font-bold uppercase tracking-wider text-[var(--color-text-tertiary)]">Closed ticket</span>
        </div>
        <form class="mt-4 grid gap-4 rounded-2xl border bg-[var(--color-surface-secondary)] p-4 sm:grid-cols-2" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="saveTicket">
          <label class="grid gap-2 text-sm font-semibold">Status<select v-model="status" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3" :disabled="saving || (isClosed && !isAdministrator)"><option v-for="option in statusOptions" :key="option" :value="option">{{ option.replaceAll("_", " ") }}</option></select></label>
          <label class="grid gap-2 text-sm font-semibold">Priority<select v-model="priority" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3" :disabled="saving || isClosed"><option value="low">Low</option><option value="normal">Normal</option><option value="high">High</option><option value="urgent">Urgent</option></select></label>
          <label class="grid gap-2 text-sm font-semibold">Assigned to<select v-model="assigneeId" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3" :disabled="saving || isClosed"><option value="">Unassigned</option><option v-if="hasHistoricalAssignee" :value="currentAssignee?.id" disabled>{{ currentAssignee?.display_name }} (inactive)</option><option v-for="account in supportAccounts" :key="account.id" :value="account.id">{{ account.display_name }}</option></select></label>
          <label class="grid gap-2 text-sm font-semibold">Category<select v-model="categoryId" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3" :disabled="saving || isClosed"><option v-if="hasHistoricalCategory" :value="ticket.category_id" disabled>Inactive category</option><option v-for="category in categories" :key="category.id" :value="category.id">{{ category.name }}</option></select></label>
          <div class="flex flex-wrap items-center justify-end gap-3 sm:col-span-2">
            <button type="button" class="min-h-11 px-3 text-sm font-bold text-[var(--color-text-secondary)]" :disabled="saving || !hasChanges" @click="resetDraft">Discard changes</button>
            <button type="submit" class="min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white disabled:opacity-50" :disabled="saving || !hasChanges">{{ saving ? "Saving…" : "Save ticket changes" }}</button>
          </div>
        </form>
      </section>

      <section class="border-t pt-7" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="conversation-heading">
        <div><p class="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Timeline</p><h3 id="conversation-heading" class="mt-1 text-xl font-bold">Conversation and notes</h3></div>
        <p v-if="loadingRelated" class="mt-4 text-sm" role="status">Loading ticket activity…</p>
        <div v-else-if="comments.length" class="mt-4 space-y-3">
          <article v-for="comment in comments" :key="comment.id" class="rounded-2xl border p-4" :class="comment.visibility === 'internal' ? 'border-amber-500/40 bg-amber-500/10' : 'bg-[var(--color-surface-secondary)]'">
            <div class="flex flex-wrap items-center justify-between gap-2"><p class="text-sm font-bold">{{ accountName(comment.author_id) }} <span v-if="comment.visibility === 'internal'" class="ml-2 rounded-full bg-amber-200 px-2 py-1 text-[0.65rem] uppercase tracking-wider text-amber-900">Internal note</span></p><time class="text-xs text-[var(--color-text-tertiary)]" :datetime="comment.created_at">{{ formatTicketTimestamp(comment.created_at) }}</time></div>
            <p class="mt-3 whitespace-pre-wrap text-sm leading-6">{{ comment.body }}</p>
          </article>
        </div>
        <p v-else class="mt-4 text-sm text-[var(--color-text-secondary)]">No conversation entries yet.</p>

        <div v-if="!isClosed" class="mt-5 grid gap-4 xl:grid-cols-2">
          <form class="rounded-2xl border border-sky-500/30 bg-sky-500/10 p-4" @submit.prevent="addComment('public')">
            <label class="block font-bold" for="public-reply">Public reply</label><p class="mt-1 text-xs text-[var(--color-text-secondary)]">The requester can read this message.</p>
            <textarea id="public-reply" v-model="publicReply" class="mt-3 min-h-28 w-full rounded-xl border bg-[var(--color-surface-primary)] p-3 text-sm" maxlength="10000" :disabled="saving" />
            <button type="submit" class="mt-3 min-h-11 rounded-xl bg-sky-700 px-4 text-sm font-bold text-white disabled:opacity-50" :disabled="saving || !publicReply.trim()">Post public reply</button>
          </form>
          <form class="rounded-2xl border border-amber-500/40 bg-amber-500/10 p-4" @submit.prevent="addComment('internal')">
            <label class="block font-bold" for="internal-note">Internal note</label><p class="mt-1 text-xs text-[var(--color-text-secondary)]">Visible only to technicians and administrators.</p>
            <textarea id="internal-note" v-model="internalNote" class="mt-3 min-h-28 w-full rounded-xl border bg-[var(--color-surface-primary)] p-3 text-sm" maxlength="10000" :disabled="saving" />
            <button type="submit" class="mt-3 min-h-11 rounded-xl bg-amber-700 px-4 text-sm font-bold text-white disabled:opacity-50" :disabled="saving || !internalNote.trim()">Add internal note</button>
          </form>
        </div>
        <p v-else class="mt-5 rounded-xl bg-[var(--color-surface-secondary)] p-4 text-sm">Reopen this ticket before adding replies, notes, files, or workflow changes.</p>
      </section>

      <section class="border-t pt-7" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="staff-attachments-heading">
        <div class="flex items-end justify-between gap-3"><div><p class="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">Evidence</p><h3 id="staff-attachments-heading" class="mt-1 text-xl font-bold">Attachments</h3></div><span class="text-xs text-[var(--color-text-tertiary)]">{{ attachments.length }} files</span></div>
        <div v-if="attachments.length" class="mt-4 grid gap-3 sm:grid-cols-2"><AttachmentPreview v-for="attachment in attachments" :key="attachment.id" :attachment="attachment" /></div>
        <p v-else class="mt-3 text-sm text-[var(--color-text-secondary)]">No files attached.</p>
        <form v-if="!isClosed" class="mt-5 rounded-2xl border bg-[var(--color-surface-secondary)] p-4" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="uploadAttachment">
          <FileUpload v-model="selectedFile" label="Add ticket attachment" :max-bytes="maxUploadBytes" :disabled="saving" />
          <button type="submit" class="mt-3 min-h-11 rounded-xl border px-4 text-sm font-bold disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="saving || !selectedFile">Attach file</button>
        </form>
      </section>
    </div>
  </article>
</template>
