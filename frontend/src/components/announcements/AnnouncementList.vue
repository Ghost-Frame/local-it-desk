<script setup lang="ts">
/** Safe announcement feed with administrator lifecycle actions. */
import { renderSafeMarkdown } from "@/lib/safe-markdown";
import { formatTicketTimestamp } from "@/lib/ticket-requester";
import type { Announcement } from "@/types/api";

/** Announcement records and administrator control state. */
defineProps<{
  /** Server-authorized announcement records in display order. */
  announcements: Announcement[];
  /** Whether lifecycle controls should be rendered. */
  administrator: boolean;
  /** Record currently being mutated. */
  busyId: string | null;
}>();

/** Explicit content edit and lifecycle events. */
const emit = defineEmits<{
  /** Opens one editable announcement in the editor. */
  edit: [announcement: Announcement];
  /** Publishes one private draft. */
  publish: [announcement: Announcement];
  /** Archives one draft or published record. */
  archive: [announcement: Announcement];
}>();
</script>

<template>
  <div v-if="announcements.length" class="space-y-4">
    <article v-for="announcement in announcements" :key="announcement.id" class="rounded-2xl border bg-[var(--color-surface-primary)] p-5 sm:p-6" :class="announcement.is_pinned ? 'border-l-4 border-l-[var(--color-accent-primary)]' : ''" :style="{ borderColor: 'var(--color-border-default)' }">
      <div class="flex flex-wrap items-center gap-2">
        <span v-if="announcement.is_pinned" class="rounded-full bg-[var(--color-surface-tertiary)] px-2 py-1 text-[0.65rem] font-bold uppercase tracking-wider text-[var(--color-accent-primary)]">Pinned</span>
        <span v-if="administrator" class="rounded-full bg-[var(--color-surface-tertiary)] px-2 py-1 text-[0.65rem] font-bold uppercase tracking-wider">{{ announcement.state }}</span>
        <time class="ml-auto text-xs text-[var(--color-text-tertiary)]" :datetime="announcement.published_at ?? announcement.created_at">{{ formatTicketTimestamp(announcement.published_at ?? announcement.created_at) }}</time>
      </div>
      <h2 class="mt-4 text-xl font-bold sm:text-2xl">{{ announcement.title }}</h2>
      <div class="safe-markdown mt-4 space-y-3 text-sm leading-7 text-[var(--color-text-secondary)]" v-html="renderSafeMarkdown(announcement.body)" />
      <div v-if="administrator && announcement.state !== 'archived'" class="mt-5 flex flex-wrap gap-2 border-t pt-4" :style="{ borderColor: 'var(--color-border-default)' }">
        <button type="button" class="min-h-11 rounded-xl border px-4 text-sm font-bold" :disabled="busyId === announcement.id" @click="emit('edit', announcement)">Edit</button>
        <button v-if="announcement.state === 'draft'" type="button" class="min-h-11 rounded-xl bg-emerald-700 px-4 text-sm font-bold text-white" :disabled="busyId === announcement.id" @click="emit('publish', announcement)">Publish announcement</button>
        <button type="button" class="min-h-11 rounded-xl px-4 text-sm font-bold text-red-700 dark:text-red-300" :disabled="busyId === announcement.id" @click="emit('archive', announcement)">Archive announcement</button>
      </div>
    </article>
  </div>
  <div v-else class="rounded-2xl border bg-[var(--color-surface-secondary)] p-8 text-center" :style="{ borderColor: 'var(--color-border-default)' }"><h2 class="text-xl font-bold">No published announcements</h2><p class="mt-2 text-sm text-[var(--color-text-secondary)]">There are no current staff notices.</p></div>
</template>

<style scoped>
.safe-markdown :deep(h2),
.safe-markdown :deep(h3),
.safe-markdown :deep(h4) { color: var(--color-text-primary); font-weight: 700; margin-top: 1rem; }
.safe-markdown :deep(ul) { list-style: disc; padding-left: 1.25rem; }
.safe-markdown :deep(code) { background: var(--color-surface-tertiary); border-radius: 0.35rem; padding: 0.1rem 0.3rem; }
.safe-markdown :deep(a) { color: var(--color-accent-primary); font-weight: 700; text-decoration: underline; }
</style>
