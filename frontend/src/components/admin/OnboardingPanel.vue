<script setup lang="ts">
/** Ephemeral one-time credential display with copy and print actions. */
import { ref } from "vue";

import { credentialsText } from "@/lib/account-admin";
import type { OneTimeCredential } from "@/types/api";

const props = defineProps<{
  /** Credentials held only by the current component instance. */
  credentials: OneTimeCredential[];
}>();

const emit = defineEmits<{
  /** Clears credential material from parent memory. */
  dismiss: [];
}>();

/** Brief copy-operation status for assistive feedback. */
const copyStatus = ref("");

/** Copies the visible onboarding sheet without persisting it. */
async function copyCredentials(): Promise<void> {
  await navigator.clipboard.writeText(credentialsText(props.credentials));
  copyStatus.value = "Copied. Clear the clipboard after delivery.";
}

/** Opens the browser print workflow for the visible onboarding sheet. */
function printCredentials(): void {
  window.print();
}
</script>

<template>
  <aside class="onboarding-sheet rounded-2xl border-2 border-[var(--color-status-warning)] bg-[var(--color-surface-elevated)] p-5 shadow-[var(--shadow-md)]" aria-labelledby="onboarding-title">
    <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-status-warning)]">Shown once</p>
        <h2 id="onboarding-title" class="mt-2 text-xl font-bold">Deliver these temporary passwords now</h2>
        <p class="mt-1 max-w-2xl text-sm text-[var(--color-text-secondary)]">They cannot be recovered after this panel is dismissed. Each person must change their password at first sign-in.</p>
      </div>
      <div class="flex flex-wrap gap-2 print:hidden">
        <button class="min-h-11 rounded-lg border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="copyCredentials">Copy all</button>
        <button class="min-h-11 rounded-lg border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="printCredentials">Print</button>
        <button class="min-h-11 rounded-lg bg-[var(--color-text-primary)] px-4 text-sm font-bold text-[var(--color-text-inverse)]" @click="emit('dismiss')">I saved them</button>
      </div>
    </div>
    <p class="mt-3 text-sm" role="status">{{ copyStatus }}</p>
    <div class="mt-4 grid gap-3 md:grid-cols-2">
      <article v-for="entry in credentials" :key="entry.user.id" class="rounded-xl bg-[var(--color-surface-tertiary)] p-4">
        <p class="font-bold">{{ entry.user.display_name }}</p>
        <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-sm">
          <dt class="text-[var(--color-text-secondary)]">Username</dt>
          <dd class="font-mono">{{ entry.user.username }}</dd>
          <dt class="text-[var(--color-text-secondary)]">Temporary password</dt>
          <dd class="break-all font-mono font-bold">{{ entry.temporary_password }}</dd>
        </dl>
      </article>
    </div>
  </aside>
</template>

<style scoped>
/** Keeps only the credential sheet visible in browser print output. */
@media print {
  .onboarding-sheet {
    position: fixed;
    inset: 0;
    z-index: 9999;
    border: 0;
    background: white;
    color: black;
  }
}
</style>
