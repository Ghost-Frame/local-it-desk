<script setup lang="ts">
/** Staff dashboard foundation for local help-desk work. */
import AppLayout from "@/components/layout/AppLayout.vue";

/** Dashboard cards retained for the approved product surface. */
const workAreas = [
  { label: "My open tickets", value: "0", detail: "Requests awaiting resolution" },
  { label: "Waiting on me", value: "0", detail: "Tickets needing requester input" },
  { label: "Unread updates", value: "0", detail: "Ticket and staff notices" },
];
</script>

<template>
  <AppLayout>
    <section class="space-y-8">
      <div class="max-w-3xl">
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">
          Dashboard
        </p>
        <h1 class="mt-3 text-4xl font-bold tracking-tight sm:text-5xl">Support work, in one place.</h1>
        <p class="mt-4 max-w-2xl text-base leading-7 text-[var(--color-text-secondary)]">
          Submit staff IT requests, follow their progress, and read service announcements from the local school network.
        </p>
      </div>

      <div class="grid gap-4 md:grid-cols-3">
        <article
          v-for="area in workAreas"
          :key="area.label"
          class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5"
          :style="{ borderColor: 'var(--color-border-default)' }"
        >
          <p class="text-sm font-semibold text-[var(--color-text-secondary)]">{{ area.label }}</p>
          <p class="mt-5 font-mono text-4xl font-bold text-[var(--color-text-primary)]">{{ area.value }}</p>
          <p class="mt-2 text-sm text-[var(--color-text-tertiary)]">{{ area.detail }}</p>
        </article>
      </div>

      <div
        class="flex flex-col gap-4 rounded-2xl border border-l-4 bg-[var(--color-surface-elevated)] p-6 sm:flex-row sm:items-center sm:justify-between"
        :style="{ borderColor: 'var(--color-border-default)', borderLeftColor: 'var(--color-accent-primary)' }"
      >
        <div>
          <h2 class="text-lg font-bold">Need help with a school system?</h2>
          <p class="mt-1 text-sm text-[var(--color-text-secondary)]">Open a named staff request so the technician can follow up.</p>
        </div>
        <router-link
          to="/tickets"
          class="inline-flex min-h-11 items-center justify-center rounded-lg bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white"
        >
          Go to tickets
        </router-link>
      </div>
    </section>
  </AppLayout>
</template>
