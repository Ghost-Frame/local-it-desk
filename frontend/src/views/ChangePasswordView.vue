<script setup lang="ts">
/** Required password replacement flow for temporary or recovered credentials. */
import { ref } from "vue";
import { useRouter } from "vue-router";

import { useAuthStore } from "@/stores/auth";

const router = useRouter();
const authStore = useAuthStore();
const currentPassword = ref("");
const newPassword = ref("");
const confirmation = ref("");
const error = ref<string | null>(null);
const isSubmitting = ref(false);

/** Confirms the current credential and replaces it with a private password. */
async function submit(): Promise<void> {
  if (isSubmitting.value) return;
  error.value = null;
  if (newPassword.value !== confirmation.value) {
    error.value = "The new password confirmation does not match.";
    return;
  }
  isSubmitting.value = true;
  try {
    await authStore.changePassword({
      current_password: currentPassword.value,
      new_password: newPassword.value,
    });
    await router.replace("/");
  } catch {
    error.value = "The password could not be changed. Check the current password and try again.";
  } finally {
    currentPassword.value = "";
    newPassword.value = "";
    confirmation.value = "";
    isSubmitting.value = false;
  }
}

/** Ends the temporary session when the account holder cannot continue. */
async function signOut(): Promise<void> {
  await authStore.logout();
  await router.replace("/login");
}
</script>

<template>
  <main class="flex min-h-screen items-center justify-center bg-[var(--color-surface-primary)] p-5 sm:p-8">
    <section class="w-full max-w-2xl" aria-labelledby="change-heading">
      <div class="mb-6 flex items-center justify-between gap-4">
        <div>
          <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">
            Account security
          </p>
          <p class="mt-2 text-sm text-[var(--color-text-secondary)]">
            Signed in as {{ authStore.displayName }}
          </p>
        </div>
        <button
          type="button"
          class="min-h-11 rounded-lg border px-4 text-sm font-semibold hover:bg-[var(--color-surface-secondary)]"
          :style="{ borderColor: 'var(--color-border-default)' }"
          @click="signOut"
        >
          Sign out
        </button>
      </div>

      <form
        class="rounded-2xl border bg-[var(--color-surface-elevated)] p-6 shadow-[var(--shadow-lg)] sm:p-9"
        :style="{ borderColor: 'var(--color-border-default)' }"
        aria-labelledby="change-heading"
        @submit.prevent="submit"
      >
        <div class="flex gap-4">
          <span
            class="mt-1 flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-[color-mix(in_oklch,var(--color-status-warning)_18%,transparent)] font-mono text-sm font-bold"
            aria-hidden="true"
          >
            !
          </span>
          <div>
            <h1 id="change-heading" class="text-3xl font-bold tracking-tight">Choose your own password</h1>
            <p id="change-help" class="mt-3 text-sm leading-6 text-[var(--color-text-secondary)]">
              Your current password is temporary or was reset by the technician. Replace it before opening the help desk.
            </p>
          </div>
        </div>

        <div class="mt-8 space-y-5">
          <div>
            <label for="current-password" class="block text-sm font-semibold">Current password</label>
            <input
              id="current-password"
              v-model="currentPassword"
              name="current-password"
              type="password"
              autocomplete="current-password"
              required
              autofocus
              class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
              :style="{ borderColor: 'var(--color-border-default)' }"
            />
          </div>
          <div class="grid gap-5 sm:grid-cols-2">
            <div>
              <label for="new-password" class="block text-sm font-semibold">New password</label>
              <input
                id="new-password"
                v-model="newPassword"
                name="new-password"
                type="password"
                autocomplete="new-password"
                minlength="12"
                required
                aria-describedby="new-password-help"
                class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
                :style="{ borderColor: 'var(--color-border-default)' }"
              />
            </div>
            <div>
              <label for="new-password-confirmation" class="block text-sm font-semibold">
                Confirm new password
              </label>
              <input
                id="new-password-confirmation"
                v-model="confirmation"
                name="new-password-confirmation"
                type="password"
                autocomplete="new-password"
                minlength="12"
                required
                aria-describedby="new-password-help"
                class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-primary)] px-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
                :style="{ borderColor: 'var(--color-border-default)' }"
              />
            </div>
          </div>
          <p id="new-password-help" class="text-xs leading-5 text-[var(--color-text-tertiary)]">
            Use 12 to 256 characters and keep this password unique to the help desk.
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
          {{ isSubmitting ? "Changing password..." : "Change password and open the desk" }}
        </button>
      </form>
    </section>
  </main>
</template>
