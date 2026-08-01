<script setup lang="ts">
/** Administrator announcement draft and edit form with explicit lifecycle separation. */
import { ref, watch } from "vue";

import type { Announcement, CreateAnnouncementRequest } from "@/types/api";

/** Existing editable announcement, busy state, and parent operation error. */
const props = defineProps<{
  /** Existing non-archived record or null for a new draft. */
  announcement: Announcement | null;
  /** Whether one announcement mutation is in flight. */
  busy: boolean;
}>();

/** Validated draft save and edit-cancellation events. */
const emit = defineEmits<{
  /** Saves a new draft or applies content changes to the selected record. */
  save: [details: CreateAnnouncementRequest];
  /** Leaves edit mode without mutating the server. */
  cancel: [];
}>();

/** Editable announcement heading. */
const title = ref("");
/** Editable unrendered Markdown body. */
const body = ref("");
/** Editable pinned display state. */
const pinned = ref(false);

/** Restores the form from the selected record or a blank draft. */
function reset(): void {
  title.value = props.announcement?.title ?? "";
  body.value = props.announcement?.body ?? "";
  pinned.value = props.announcement?.is_pinned ?? false;
}

/** Emits trimmed bounded form fields for server validation. */
function submit(): void {
  if (!title.value.trim() || !body.value.trim()) return;
  emit("save", {
    title: title.value.trim(),
    body: body.value.trim(),
    is_pinned: pinned.value,
  });
}

watch(() => props.announcement, reset, { immediate: true });
</script>

<template>
  <form class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="submit">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div><p class="font-mono text-xs uppercase tracking-[0.16em] text-[var(--color-accent-primary)]">{{ announcement ? "Edit bulletin" : "New bulletin" }}</p><h2 class="mt-2 text-xl font-bold">{{ announcement ? "Update announcement" : "Create draft" }}</h2></div>
      <button v-if="announcement" type="button" class="min-h-11 px-3 text-sm font-bold text-[var(--color-text-secondary)]" :disabled="busy" @click="emit('cancel')">Cancel edit</button>
    </div>
    <label class="mt-5 grid gap-2 text-sm font-bold">Title<input v-model="title" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 font-normal" maxlength="160" required :disabled="busy" /></label>
    <label class="mt-4 grid gap-2 text-sm font-bold">Announcement text <span class="text-xs font-normal text-[var(--color-text-secondary)]">Local Markdown supports headings, lists, bold text, code, and same-site links. Images and remote links stay plain text.</span><textarea v-model="body" class="min-h-44 rounded-xl border bg-[var(--color-surface-primary)] p-3 font-normal leading-6" maxlength="10000" required :disabled="busy" /></label>
    <label class="mt-4 flex min-h-11 items-center gap-3 text-sm font-bold"><input v-model="pinned" type="checkbox" :disabled="busy" /> Pinned</label>
    <button type="submit" class="mt-4 min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white disabled:opacity-50" :disabled="busy || !title.trim() || !body.trim()">{{ busy ? "Saving…" : announcement ? "Save changes" : "Create draft" }}</button>
  </form>
</template>
