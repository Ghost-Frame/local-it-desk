<script setup lang="ts">
/** Named-requester ticket form with retry-safe draft and attachment handling. */
import { computed, ref, watch } from "vue";
import { storeToRefs } from "pinia";

import FileUpload from "@/components/common/FileUpload.vue";
import { useAuthStore } from "@/stores/auth";
import { useTicketsStore } from "@/stores/tickets";
import type { Ticket, TicketPriority } from "@/types/api";

const emit = defineEmits<{
  /** Reports a fully submitted ticket to the containing workspace. */
  created: [ticket: Ticket];
  /** Closes the form without changing its current draft. */
  cancel: [];
}>();

const authStore = useAuthStore();
const ticketsStore = useTicketsStore();
const { publicConfig } = storeToRefs(authStore);
const { isSaving, actionError } = storeToRefs(ticketsStore);
const title = ref("");
const description = ref("");
const location = ref("");
const categoryId = ref("");
const priority = ref<TicketPriority>("normal");
const urgent = ref(false);
const file = ref<File | null>(null);
const validationError = ref<string | null>(null);
const createdTicket = ref<Ticket | null>(null);

/** Active categories supplied by the runtime server configuration. */
const categories = computed(() => publicConfig.value?.categories ?? []);
/** Whether a category exists for valid ticket creation. */
const canSubmit = computed(() => categories.value.length > 0 && !isSaving.value);

watch(
  publicConfig,
  (config) => {
    if (!categoryId.value || !config?.categories.some((item) => item.id === categoryId.value)) {
      categoryId.value = config?.default_category_id ?? config?.categories[0]?.id ?? "";
    }
    if (!createdTicket.value && config) priority.value = config.default_priority;
  },
  { immediate: true },
);

/** Creates the ticket once, then retries only a failed attachment upload. */
async function submit(): Promise<void> {
  validationError.value = null;
  if (title.value.trim().length < 3) {
    validationError.value = "Add a short title with at least 3 characters.";
    return;
  }
  if (description.value.trim().length < 10) {
    validationError.value = "Tell us a little more so the technician knows where to start.";
    return;
  }
  if (!categoryId.value) {
    validationError.value = "Choose a category before submitting.";
    return;
  }
  try {
    const requestDescription = location.value.trim()
      ? `Location or device: ${location.value.trim()}\n\n${description.value.trim()}`
      : description.value.trim();
    const ticket =
      createdTicket.value ??
      (await ticketsStore.createTicket({
        title: title.value.trim(),
        description: requestDescription,
        category_id: categoryId.value,
        priority: urgent.value ? "urgent" : priority.value,
      }));
    createdTicket.value = ticket;
    if (file.value) await ticketsStore.uploadAttachment(ticket.id, file.value);
    title.value = "";
    description.value = "";
    location.value = "";
    urgent.value = false;
    file.value = null;
    createdTicket.value = null;
    emit("created", ticket);
  } catch {
    // The visible draft and any already-created ticket are intentionally retained for retry.
  }
}
</script>

<template>
  <form class="space-y-5" @submit.prevent="submit">
    <div>
      <p class="font-mono text-[0.68rem] uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">New request</p>
      <h2 class="mt-2 text-2xl font-bold tracking-tight">What can we help with?</h2>
      <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">
        Your name is attached automatically. Add enough detail for the technician to get started.
      </p>
    </div>

    <div>
      <label for="ticket-title" class="block text-sm font-semibold">What is wrong?</label>
      <input
        id="ticket-title"
        v-model="title"
        class="mt-2 min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 py-2 outline-none focus:border-[var(--color-accent-primary)] focus:ring-2 focus:ring-[var(--color-accent-primary)]/20"
        maxlength="160"
        autocomplete="off"
        autofocus
        placeholder="Example: Projector will not turn on"
        :disabled="isSaving"
        required
      />
    </div>

    <div>
      <label for="ticket-location" class="block text-sm font-semibold">Where is it?</label>
      <input
        id="ticket-location"
        v-model="location"
        class="mt-2 min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 py-2 outline-none focus:border-[var(--color-accent-primary)] focus:ring-2 focus:ring-[var(--color-accent-primary)]/20"
        maxlength="200"
        autocomplete="off"
        placeholder="Room, office, or device name"
        :disabled="isSaving"
      />
    </div>

    <div>
      <label for="ticket-description" class="block text-sm font-semibold">What happened?</label>
      <textarea
        id="ticket-description"
        v-model="description"
        class="mt-2 min-h-36 w-full resize-y rounded-xl border bg-[var(--color-surface-primary)] px-3 py-3 outline-none focus:border-[var(--color-accent-primary)] focus:ring-2 focus:ring-[var(--color-accent-primary)]/20"
        maxlength="10000"
        placeholder="What did you see, and what have you already tried?"
        :disabled="isSaving"
        required
      />
    </div>

    <label class="flex min-h-12 cursor-pointer items-center gap-3 rounded-xl border bg-[var(--color-surface-secondary)] px-4 py-3 text-sm font-semibold" :style="{ borderColor: 'var(--color-border-default)' }">
      <input v-model="urgent" type="checkbox" class="h-5 w-5 accent-[var(--color-accent-primary)]" :disabled="isSaving" />
      <span>This is stopping a class</span>
    </label>

    <details v-if="categories.length > 1" class="rounded-xl border bg-[var(--color-surface-secondary)]" :style="{ borderColor: 'var(--color-border-default)' }">
      <summary class="min-h-11 cursor-pointer px-4 py-3 text-sm font-semibold">Change request category</summary>
      <div class="border-t p-4" :style="{ borderColor: 'var(--color-border-default)' }">
        <label for="ticket-category" class="block text-sm font-semibold">Category</label>
        <select
          id="ticket-category"
          v-model="categoryId"
          class="mt-2 min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 outline-none focus:border-[var(--color-accent-primary)]"
          :disabled="isSaving || categories.length === 0"
          required
        >
          <option value="" disabled>Choose a category</option>
          <option v-for="category in categories" :key="category.id" :value="category.id">{{ category.name }}</option>
        </select>
      </div>
    </details>

    <FileUpload v-model="file" :max-bytes="publicConfig?.max_upload_bytes" :disabled="isSaving" />

    <p v-if="categories.length === 0" class="rounded-xl border border-amber-500/30 bg-amber-500/10 p-3 text-sm" role="alert">
      Ticket submission is paused until an administrator creates an active category.
    </p>
    <p v-if="validationError || actionError" class="rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-800 dark:text-red-200" role="alert">
      {{ validationError || actionError }}
      <span v-if="createdTicket" class="mt-1 block">Your ticket was created. Submit again to retry only the attachment.</span>
    </p>

    <div class="flex flex-col-reverse gap-3 border-t pt-5 sm:flex-row sm:justify-end" :style="{ borderColor: 'var(--color-border-default)' }">
      <button type="button" class="min-h-11 rounded-xl border px-5 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="isSaving" @click="emit('cancel')">
        Cancel
      </button>
      <button type="submit" class="min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white disabled:cursor-not-allowed disabled:opacity-50" :disabled="!canSubmit">
        {{ isSaving ? "Sending…" : createdTicket ? "Retry attachment" : "Send request" }}
      </button>
    </div>
  </form>
</template>
