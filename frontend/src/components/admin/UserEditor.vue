<script setup lang="ts">
/** Form for creating one named local staff account. */
import { ref } from "vue";

import type { CreateUserRequest, UserRole } from "@/types/api";

const emit = defineEmits<{
  /** Requests creation of one validated account draft. */
  create: [details: CreateUserRequest];
}>();

defineProps<{
  /** Whether an account creation request is currently active. */
  busy: boolean;
}>();

/** Normalized username draft. */
const username = ref("");
/** Human-facing display-name draft. */
const displayName = ref("");
/** Optional contact email draft. */
const email = ref("");
/** Initial cumulative role draft. */
const role = ref<UserRole>("requester");

/** Emits one account draft and clears it only after parent submission begins. */
function submit(): void {
  emit("create", {
    username: username.value.trim().toLowerCase(),
    display_name: displayName.value.trim(),
    email: email.value.trim() || null,
    role: role.value,
  });
}
</script>

<template>
  <form class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="submit">
    <div class="mb-5">
      <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">New account</p>
      <h2 class="mt-2 text-xl font-bold">Add a staff member</h2>
      <p class="mt-1 text-sm text-[var(--color-text-secondary)]">They will receive a temporary password and must replace it at first sign-in.</p>
    </div>
    <div class="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      <label class="grid gap-2 text-sm font-semibold">
        Username
        <input v-model="username" required autocomplete="off" pattern="[a-z0-9._-]+" class="min-h-11 rounded-lg border bg-[var(--color-surface-elevated)] px-3 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" />
      </label>
      <label class="grid gap-2 text-sm font-semibold">
        Display name
        <input v-model="displayName" required autocomplete="off" class="min-h-11 rounded-lg border bg-[var(--color-surface-elevated)] px-3 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" />
      </label>
      <label class="grid gap-2 text-sm font-semibold">
        Email <span class="font-normal text-[var(--color-text-tertiary)]">optional</span>
        <input v-model="email" type="email" autocomplete="off" class="min-h-11 rounded-lg border bg-[var(--color-surface-elevated)] px-3 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" />
      </label>
      <label class="grid gap-2 text-sm font-semibold">
        Access level
        <select v-model="role" class="min-h-11 rounded-lg border bg-[var(--color-surface-elevated)] px-3 font-normal" :style="{ borderColor: 'var(--color-border-default)' }">
          <option value="requester">Requester</option>
          <option value="technician">Technician</option>
          <option value="administrator">Administrator</option>
        </select>
      </label>
    </div>
    <button :disabled="busy" class="mt-5 min-h-11 rounded-lg bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white disabled:cursor-not-allowed disabled:opacity-50">
      {{ busy ? "Creating…" : "Create account" }}
    </button>
  </form>
</template>
