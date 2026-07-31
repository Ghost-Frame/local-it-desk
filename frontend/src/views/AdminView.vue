<script setup lang="ts">
/** Administrator workspace foundation for local operations. */
import AppLayout from "@/components/layout/AppLayout.vue";

/** Approved administrator work areas. */
const areas = [
  "Accounts and sessions",
  "Categories and branding",
  "Announcements",
  "Audit history",
  "Backup and recovery",
];
</script>

<template>
  <AppLayout>
    <section class="space-y-6">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Operator access</p>
        <h1 class="mt-3 text-4xl font-bold tracking-tight">Administration</h1>
        <p class="mt-3 max-w-2xl text-[var(--color-text-secondary)]">Configure the local help desk and manage named staff accounts.</p>
      </div>
      <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <article v-for="(area, index) in areas" :key="area" class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5" :style="{ borderColor: 'var(--color-border-default)' }">
          <p class="font-mono text-xs text-[var(--color-text-tertiary)]">0{{ index + 1 }}</p>
          <h2 class="mt-5 font-bold">{{ area }}</h2>
          <p class="mt-2 text-sm text-[var(--color-text-secondary)]">Foundation route reserved for a later implementation plan.</p>
        </article>
      </div>
    </section>
  </AppLayout>
</template>
