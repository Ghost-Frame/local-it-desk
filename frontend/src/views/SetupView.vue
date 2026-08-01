<script setup lang="ts">
/** First-run form that creates the local desk's initial administrator. */
import { ref } from "vue";
import { useRouter } from "vue-router";

import { useAuthStore } from "@/stores/auth";

const router = useRouter();
const authStore = useAuthStore();
const displayName = ref("");
const username = ref("");
const password = ref("");
const confirmation = ref("");
const error = ref<string | null>(null);
const isSubmitting = ref(false);

/** Validates confirmation and creates the one-time first administrator. */
async function submit(): Promise<void> {
  if (isSubmitting.value) return;
  error.value = null;
  if (password.value !== confirmation.value) {
    error.value = "The password confirmation does not match.";
    return;
  }
  isSubmitting.value = true;
  try {
    await authStore.setup({
      username: username.value,
      display_name: displayName.value,
      password: password.value,
    });
    await router.replace("/");
  } catch {
    error.value = "Setup could not be completed. Review the details and try again.";
  } finally {
    password.value = "";
    confirmation.value = "";
    isSubmitting.value = false;
  }
}
</script>

<template>
  <main class="min-h-screen bg-[var(--color-surface-primary)] px-5 py-8 sm:px-8 lg:py-14">
    <div class="mx-auto grid w-full max-w-6xl gap-10 lg:grid-cols-[0.82fr_1.18fr] lg:items-start">
      <section class="pt-4 lg:sticky lg:top-14" aria-labelledby="setup-heading">
        <div class="flex items-center gap-3">
          <span class="h-2.5 w-2.5 rounded-full bg-[var(--color-status-warning)]" aria-hidden="true" />
          <p class="font-mono text-xs uppercase tracking-[0.22em] text-[var(--color-accent-primary)]">
            First run / local setup
          </p>
        </div>
        <h1 id="setup-heading" class="mt-6 max-w-lg text-4xl font-bold leading-tight tracking-tight sm:text-5xl">
          Put the help desk under local control.
        </h1>
        <p class="mt-5 max-w-lg text-base leading-7 text-[var(--color-text-secondary)]">
          Create the first administrator account. This person manages staff access, works tickets, and handles password resets.
        </p>
        <div
          class="mt-8 max-w-lg rounded-xl border border-l-4 bg-[var(--color-surface-secondary)] p-5"
          :style="{ borderColor: 'var(--color-border-default)', borderLeftColor: 'var(--color-status-warning)' }"
        >
          <p class="font-mono text-xs font-bold uppercase tracking-[0.16em]">Before you continue</p>
          <p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">
            Record the username in the operator runbook. The password is stored only as a secure one-way hash.
          </p>
        </div>
      </section>

      <section
        class="rounded-2xl border bg-[var(--color-surface-elevated)] p-6 shadow-[var(--shadow-lg)] sm:p-9"
        :style="{ borderColor: 'var(--color-border-default)' }"
      >
        <form aria-labelledby="account-heading" @submit.prevent="submit">
          <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-text-tertiary)]">Step 1 of 1</p>
          <h2 id="account-heading" class="mt-2 text-2xl font-bold">Administrator account</h2>
          <p id="setup-help" class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">
            Use a named account for the technician who owns this installation.
          </p>

          <div class="mt-7 grid gap-5 sm:grid-cols-2">
            <div class="sm:col-span-2">
              <label for="setup-name" class="block text-sm font-semibold">Full name</label>
              <input
                id="setup-name"
                v-model="displayName"
                name="name"
                autocomplete="name"
                required
                autofocus
                class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
                :style="{ borderColor: 'var(--color-border-default)' }"
              />
            </div>
            <div class="sm:col-span-2">
              <label for="setup-username" class="block text-sm font-semibold">Local username</label>
              <input
                id="setup-username"
                v-model="username"
                name="username"
                autocomplete="username"
                autocapitalize="none"
                spellcheck="false"
                required
                aria-describedby="username-help"
                class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 font-mono outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
                :style="{ borderColor: 'var(--color-border-default)' }"
              />
              <p id="username-help" class="mt-2 text-xs leading-5 text-[var(--color-text-tertiary)]">
                3 to 32 characters: letters, numbers, dot, dash, or underscore.
              </p>
            </div>
            <div>
              <label for="setup-password" class="block text-sm font-semibold">Password</label>
              <input
                id="setup-password"
                v-model="password"
                name="new-password"
                type="password"
                autocomplete="new-password"
                minlength="12"
                required
                aria-describedby="password-help"
                class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
                :style="{ borderColor: 'var(--color-border-default)' }"
              />
            </div>
            <div>
              <label for="setup-confirmation" class="block text-sm font-semibold">Confirm password</label>
              <input
                id="setup-confirmation"
                v-model="confirmation"
                name="new-password-confirmation"
                type="password"
                autocomplete="new-password"
                minlength="12"
                required
                aria-describedby="password-help"
                class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
                :style="{ borderColor: 'var(--color-border-default)' }"
              />
            </div>
            <p id="password-help" class="sm:col-span-2 text-xs leading-5 text-[var(--color-text-tertiary)]">
              Use 12 to 256 characters. A long, unique passphrase is easiest to manage.
            </p>
          </div>

          <p
            v-if="error"
            role="alert"
            class="mt-5 rounded-lg border border-[color-mix(in_oklch,var(--color-status-error)_35%,transparent)] bg-[color-mix(in_oklch,var(--color-status-error)_10%,transparent)] p-3 text-sm"
          >
            {{ error }}
          </p>
          <button
            type="submit"
            :disabled="isSubmitting"
            class="mt-7 min-h-12 w-full rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white transition hover:bg-[var(--color-accent-primary-hover)] focus:outline-none focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_35%,transparent)] disabled:cursor-wait disabled:opacity-60"
          >
            {{ isSubmitting ? "Creating the desk..." : "Create administrator and continue" }}
          </button>
        </form>
      </section>
    </div>
  </main>
</template>
