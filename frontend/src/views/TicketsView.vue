<script setup lang="ts">
/** Ticket workspace foundation retained for Plan 03 implementation. */
import AppLayout from "@/components/layout/AppLayout.vue";
</script>

<template>
  <AppLayout>
    <section class="space-y-6">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Support queue</p>
        <h1 class="mt-3 text-4xl font-bold tracking-tight">Tickets</h1>
        <p class="mt-3 max-w-2xl text-[var(--color-text-secondary)]">Submit staff requests and follow technician updates. Ticket operations are connected in Plan 03.</p>
      </div>
      <div class="rounded-2xl border bg-[var(--color-surface-secondary)] p-8 text-center" :style="{ borderColor: 'var(--color-border-default)' }">
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-text-tertiary)]">Foundation ready</p>
        <h2 class="mt-3 text-xl font-bold">No ticket records loaded</h2>
        <p class="mx-auto mt-2 max-w-lg text-sm leading-6 text-[var(--color-text-secondary)]">The approved queue, ownership rules, priorities, statuses, comments, and attachment boundaries are established.</p>
      </div>
    </section>
  </AppLayout>
</template>
