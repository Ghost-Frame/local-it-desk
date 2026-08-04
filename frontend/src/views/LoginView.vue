<script setup lang="ts">
/** Named-staff login for the built-in local account system. */
import { ref } from "vue";
import { useRoute, useRouter } from "vue-router";

import { landingPath, safePostLoginPath } from "@/lib/router-guards";
import { useAuthStore } from "@/stores/auth";

const route = useRoute();
const router = useRouter();
const authStore = useAuthStore();
const username = ref("");
const password = ref("");
const error = ref<string | null>(null);
const isSubmitting = ref(false);

/** Submits local credentials and continues to the permitted destination. */
async function submit(): Promise<void> {
  if (isSubmitting.value) return;
  isSubmitting.value = true;
  error.value = null;
  try {
    await authStore.login({ username: username.value, password: password.value });
    const requestedDestination = safePostLoginPath(route.query.redirect);
    const destination = authStore.mustChangePassword
      ? "/change-password"
      : requestedDestination === "/"
        ? landingPath(authStore.user?.role)
        : requestedDestination;
    await router.replace(destination);
  } catch {
    error.value = "We could not sign you in. Check your details and try again.";
  } finally {
    password.value = "";
    isSubmitting.value = false;
  }
}
</script>

<template>
  <main
    class="grid min-h-screen bg-[var(--color-surface-primary)] lg:grid-cols-[minmax(0,1.08fr)_minmax(28rem,0.92fr)]"
  >
    <section
      class="relative hidden overflow-hidden border-r bg-[var(--color-surface-secondary)] p-12 lg:flex lg:flex-col lg:justify-between"
      :style="{ borderColor: 'var(--color-border-default)' }"
      aria-label="Service information"
    >
      <div class="absolute -right-24 top-24 h-80 w-80 rounded-full border border-[var(--color-border-default)] opacity-60" />
      <div class="absolute -right-8 top-40 h-48 w-48 rounded-full border border-[var(--color-border-strong)] opacity-40" />
      <div class="relative flex items-center gap-3">
        <span class="h-2.5 w-2.5 rounded-full bg-[var(--color-status-success)]" aria-hidden="true" />
        <p class="font-mono text-xs uppercase tracking-[0.24em] text-[var(--color-accent-primary)]">
          {{ authStore.publicConfig?.app_name ?? "Local IT Desk" }}
        </p>
      </div>
      <div class="relative max-w-xl">
        <p class="font-mono text-sm text-[var(--color-text-tertiary)]">STAFF SUPPORT / LOCAL NETWORK</p>
        <h1 class="mt-5 text-5xl font-bold leading-[1.03] tracking-tight xl:text-6xl">
          Technical help, without the hallway hunt.
        </h1>
        <p class="mt-6 max-w-lg text-lg leading-8 text-[var(--color-text-secondary)]">
          Submit a request, follow the repair, and keep every update attached to the right staff member.
        </p>
      </div>
      <p class="relative max-w-md text-sm leading-6 text-[var(--color-text-tertiary)]">
        Need an account or password reset?
        {{ authStore.publicConfig?.support_contact ?? "Contact your local IT technician." }}
      </p>
    </section>

    <section class="flex items-center justify-center p-6 sm:p-12">
      <form class="w-full max-w-md" aria-labelledby="login-title" @submit.prevent="submit">
        <div class="mb-9 lg:hidden">
          <p class="font-mono text-xs uppercase tracking-[0.22em] text-[var(--color-accent-primary)]">
            {{ authStore.publicConfig?.app_name ?? "Local IT Desk" }}
          </p>
        </div>
        <p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">
          Named staff access
        </p>
        <h2 id="login-title" class="mt-3 text-3xl font-bold tracking-tight">Sign in to the desk</h2>
        <p id="login-help" class="mt-3 text-sm leading-6 text-[var(--color-text-secondary)]">
          Use the local username issued by your IT technician.
        </p>

        <div class="mt-8 space-y-5">
          <div>
            <label for="login-username" class="block text-sm font-semibold">Username</label>
            <input
              id="login-username"
              v-model="username"
              name="username"
              autocomplete="username"
              autocapitalize="none"
              spellcheck="false"
              required
              autofocus
              aria-describedby="login-help"
              class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-elevated)] px-4 outline-none transition focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
              :style="{ borderColor: 'var(--color-border-default)' }"
            />
          </div>
          <div>
            <label for="login-password" class="block text-sm font-semibold">Password</label>
            <input
              id="login-password"
              v-model="password"
              name="password"
              type="password"
              autocomplete="current-password"
              required
              class="mt-2 min-h-12 w-full rounded-lg border bg-[var(--color-surface-elevated)] px-4 outline-none transition focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
              :style="{ borderColor: 'var(--color-border-default)' }"
            />
          </div>
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
          class="mt-6 min-h-12 w-full rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white transition hover:bg-[var(--color-accent-primary-hover)] focus:outline-none focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_35%,transparent)] disabled:cursor-wait disabled:opacity-60"
        >
          {{ isSubmitting ? "Signing in..." : "Sign in" }}
        </button>
        <p class="mt-6 text-center font-mono text-[0.68rem] uppercase tracking-[0.16em] text-[var(--color-text-tertiary)]">
          Local network only · Named staff accounts
        </p>
      </form>
    </section>
  </main>
</template>
