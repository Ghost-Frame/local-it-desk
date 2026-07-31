<script setup lang="ts">
/** Personal settings foundation for password and appearance controls. */
import AppLayout from "@/components/layout/AppLayout.vue";
</script>

<template>
  <AppLayout>
    <section class="space-y-6">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Personal account</p>
        <h1 class="mt-3 text-4xl font-bold tracking-tight">Settings</h1>
        <p class="mt-3 max-w-2xl text-[var(--color-text-secondary)]">Manage your local password and browser appearance.</p>
      </div>
      <div class="grid gap-4 md:grid-cols-2">
        <article class="rounded-2xl border bg-[var(--color-surface-secondary)] p-6" :style="{ borderColor: 'var(--color-border-default)' }">
          <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-text-tertiary)]">Security</p>
          <h2 class="mt-3 text-lg font-bold">Password</h2>
          <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">Local password changes and temporary-password enforcement arrive in Plan 02.</p>
        </article>
        <article class="rounded-2xl border bg-[var(--color-surface-secondary)] p-6" :style="{ borderColor: 'var(--color-border-default)' }">
          <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-text-tertiary)]">Display</p>
          <h2 class="mt-3 text-lg font-bold">Theme</h2>
          <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">Use the header control to switch between light and dark modes.</p>
        </article>
      </div>
    </section>
  </AppLayout>
</template>
