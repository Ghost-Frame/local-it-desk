<script setup lang="ts">
/** Staff announcement empty state retained for Plan 03 implementation. */
import AppLayout from "@/components/layout/AppLayout.vue";
</script>

<template>
  <AppLayout>
    <section class="space-y-6">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Staff notices</p>
        <h1 class="mt-3 text-4xl font-bold tracking-tight">Announcements</h1>
      </div>
      <div class="rounded-2xl border bg-[var(--color-surface-secondary)] p-8" :style="{ borderColor: 'var(--color-border-default)' }">
        <h2 class="text-xl font-bold">No published announcements</h2>
        <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">The administrator-managed announcement board arrives with the ticket workflow in Plan 03.</p>
      </div>
    </section>
  </AppLayout>
</template>
