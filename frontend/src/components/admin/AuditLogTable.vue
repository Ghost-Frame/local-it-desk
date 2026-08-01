<script setup lang="ts">
/** Privacy-bounded administrator audit history table. */
import type { AuditEntry } from "@/types/api";

defineProps<{
  /** Newest-first audit entries returned by the server. */
  entries: AuditEntry[];
  /** Whether the parent is loading audit history. */
  loading: boolean;
}>();
</script>

<template>
  <section class="overflow-hidden rounded-2xl border bg-[var(--color-surface-secondary)]" :style="{ borderColor: 'var(--color-border-default)' }">
    <div class="border-b p-5" :style="{ borderColor: 'var(--color-border-default)' }"><p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Accountability</p><h2 class="mt-2 text-xl font-bold">Audit history</h2><p class="mt-1 text-sm text-[var(--color-text-secondary)]">Recent administrative activity. Passwords and session secrets are never included.</p></div>
    <p v-if="loading" class="p-5 text-sm text-[var(--color-text-secondary)]" role="status">Loading audit history…</p>
    <p v-else-if="entries.length === 0" class="p-5 text-sm text-[var(--color-text-secondary)]">No audit activity yet.</p>
    <div v-else class="overflow-x-auto"><table class="w-full min-w-[48rem] text-left text-sm"><thead class="bg-[var(--color-surface-tertiary)] text-xs uppercase tracking-wider text-[var(--color-text-secondary)]"><tr><th class="p-3">When</th><th class="p-3">Action</th><th class="p-3">Summary</th><th class="p-3">Target</th></tr></thead><tbody><tr v-for="entry in entries" :key="entry.id" class="border-t" :style="{ borderColor: 'var(--color-border-default)' }"><td class="whitespace-nowrap p-3">{{ new Date(entry.created_at).toLocaleString() }}</td><td class="p-3 font-mono text-xs">{{ entry.action }}</td><td class="p-3">{{ entry.summary }}</td><td class="p-3 font-mono text-xs">{{ entry.target_type }}{{ entry.target_id ? ` / ${entry.target_id}` : "" }}</td></tr></tbody></table></div>
  </section>
</template>
