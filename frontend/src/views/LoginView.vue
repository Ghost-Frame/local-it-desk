<script setup lang="ts">
/** Built-in local account login for named school staff. */
import { ref } from "vue";
import { useRouter } from "vue-router";

import { useAuthStore } from "@/stores/auth";

const router = useRouter();
const authStore = useAuthStore();
const username = ref("");
const password = ref("");
const error = ref<string | null>(null);
const isSubmitting = ref(false);

/** Submits credentials to the server-managed session endpoint. */
async function submit(): Promise<void> {
  if (isSubmitting.value) return;
  isSubmitting.value = true;
  error.value = null;
  try {
    await authStore.login({ username: username.value, password: password.value });
    await router.replace("/");
  } catch {
    error.value = "Sign-in failed. Check your username and password.";
  } finally {
    password.value = "";
    isSubmitting.value = false;
  }
}
</script>

<template>
  <main class="grid min-h-screen bg-[var(--color-surface-primary)] lg:grid-cols-[minmax(0,1.1fr)_minmax(28rem,0.9fr)]">
    <section class="hidden border-r p-12 lg:flex lg:flex-col lg:justify-between" :style="{ borderColor: 'var(--color-border-default)' }">
      <p class="font-mono text-xs uppercase tracking-[0.24em] text-[var(--color-accent-primary)]">Local IT Desk</p>
      <div class="max-w-xl">
        <p class="font-mono text-sm text-[var(--color-text-tertiary)]">STAFF SUPPORT / LOCAL NETWORK</p>
        <h1 class="mt-5 text-6xl font-bold leading-[1.02] tracking-tight">A quieter way to ask for technical help.</h1>
        <p class="mt-6 text-lg leading-8 text-[var(--color-text-secondary)]">Every request belongs to a named staff account. No anonymous submissions and no outside account provider required.</p>
      </div>
      <p class="text-sm text-[var(--color-text-tertiary)]">Contact your local technician if you need an account or password reset.</p>
    </section>

    <section class="flex items-center justify-center p-6 sm:p-12">
      <form class="w-full max-w-md space-y-6" @submit.prevent="submit">
        <div>
          <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Named staff access</p>
          <h2 class="mt-3 text-3xl font-bold">Sign in</h2>
          <p class="mt-2 text-sm text-[var(--color-text-secondary)]">Use the local username issued by your IT technician.</p>
        </div>
        <label class="block text-sm font-semibold">
          Username
          <input v-model="username" autocomplete="username" required class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-secondary)] px-4 outline-none focus:ring-2 focus:ring-[var(--color-accent-primary)]" :style="{ borderColor: 'var(--color-border-default)' }" />
        </label>
        <label class="block text-sm font-semibold">
          Password
          <input v-model="password" type="password" autocomplete="current-password" required class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-secondary)] px-4 outline-none focus:ring-2 focus:ring-[var(--color-accent-primary)]" :style="{ borderColor: 'var(--color-border-default)' }" />
        </label>
        <p v-if="error" role="alert" class="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-700 dark:text-red-300">{{ error }}</p>
        <button :disabled="isSubmitting" class="min-h-12 w-full rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white disabled:opacity-60">
          {{ isSubmitting ? "Signing in..." : "Sign in" }}
        </button>
      </form>
    </section>
  </main>
</template>
