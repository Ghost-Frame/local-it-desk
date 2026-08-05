<script setup lang="ts">
/** Four-stage first-run workflow for branding, administrator, staff, and handoff. */
import { computed, ref } from "vue";
import { useRouter } from "vue-router";

import OnboardingPanel from "@/components/admin/OnboardingPanel.vue";
import StaffQuickAdd from "@/components/admin/StaffQuickAdd.vue";
import { api } from "@/lib/api";
import { useAuthStore } from "@/stores/auth";
import type { OneTimeCredential } from "@/types/api";

/** Client-side router used only after the final review stage. */
const router = useRouter();
/** Shared first-run identity and public branding state. */
const authStore = useAuthStore();
/** Current one-based setup stage. */
const step = ref(1);
/** Human-facing desk name saved after administrator creation. */
const appName = ref(authStore.publicConfig?.app_name ?? "Local IT Desk");
/** Optional local support location or phone. */
const supportContact = ref(authStore.publicConfig?.support_contact ?? "");
/** Named administrator display name. */
const displayName = ref("");
/** Named administrator local username. */
const username = ref("");
/** Administrator private password held only during submission. */
const password = ref("");
/** Administrator password confirmation held only during submission. */
const confirmation = ref("");
/** One-time requester credentials retained until explicit dismissal. */
const onboardingCredentials = ref<OneTimeCredential[]>([]);
/** Safe first-run failure guidance. */
const error = ref("");
/** Non-blocking branding warning after atomic administrator creation. */
const warning = ref("");
/** Whether the administrator request is active. */
const isSubmitting = ref(false);
/** Current same-origin address used on staff cards and URL-only QR codes. */
const deskUrl = computed(() => window.location.origin);

/** Validates desk basics before revealing credential creation. */
function continueToAdministrator(): void {
  error.value = "";
  if (!appName.value.trim()) {
    error.value = "Enter a name for the help desk.";
    return;
  }
  step.value = 2;
}

/** Creates the only initial administrator, then applies runtime branding. */
async function createAdministrator(): Promise<void> {
  if (isSubmitting.value) return;
  error.value = "";
  warning.value = "";
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
    step.value = 3;
    try {
      await api.updateAdminSettings({
        app_name: appName.value.trim(),
        support_contact: supportContact.value.trim(),
      });
      await authStore.refreshPublicConfig();
    } catch {
      warning.value = "The administrator was created, but the desk name could not be saved. You can change it later in Settings.";
    }
  } catch {
    error.value = "The administrator could not be created. Review the details and try again.";
  } finally {
    password.value = "";
    confirmation.value = "";
    isSubmitting.value = false;
  }
}

/** Retains newly issued requester credentials for immediate print or copy. */
function staffCreated(credentials: OneTimeCredential[]): void {
  onboardingCredentials.value = credentials;
}

/** Clears every one-time requester credential from application memory. */
function dismissCredentials(): void {
  onboardingCredentials.value = [];
}

/** Advances from optional staff onboarding to final handoff. */
function reviewSetup(): void {
  step.value = 4;
}

/** Opens the administrator's active ticket workspace. */
async function finishSetup(): Promise<void> {
  onboardingCredentials.value = [];
  await router.replace("/administration");
}
</script>

<template>
  <main class="min-h-screen bg-[var(--color-surface-primary)] px-4 py-6 sm:px-8 lg:py-12">
    <div class="mx-auto w-full max-w-5xl">
      <header class="grid gap-5 border-b pb-7 lg:grid-cols-[1fr_auto] lg:items-end" :style="{ borderColor: 'var(--color-border-default)' }">
        <div>
          <p class="font-mono text-xs uppercase tracking-[0.22em] text-[var(--color-accent-primary)]">First-run setup</p>
          <h1 class="mt-3 text-4xl font-bold tracking-tight sm:text-5xl">Make the desk ready for staff.</h1>
          <p class="mt-3 max-w-2xl leading-7 text-[var(--color-text-secondary)]">Four short stages. Accounts stay local, every request has a name, and no school email access is required.</p>
        </div>
        <p class="rounded-full bg-[var(--color-surface-tertiary)] px-4 py-2 font-mono text-xs font-bold">Step {{ step }} of 4</p>
      </header>

      <ol class="my-6 grid grid-cols-2 gap-2 text-sm sm:grid-cols-4" aria-label="Setup progress">
        <li v-for="(label, index) in ['Desk', 'Administrator', 'Staff', 'Finish']" :key="label" class="rounded-lg border px-3 py-2" :class="step === index + 1 ? 'border-[var(--color-accent-primary)] bg-[color-mix(in_oklch,var(--color-accent-primary)_10%,transparent)] font-bold' : 'border-[var(--color-border-default)] text-[var(--color-text-secondary)]'" :aria-current="step === index + 1 ? 'step' : undefined">{{ index + 1 }}. {{ label }}</li>
      </ol>

      <p v-if="error" class="mb-5 rounded-xl border border-[var(--color-status-error)] bg-[color-mix(in_oklch,var(--color-status-error)_10%,transparent)] p-4 text-sm" role="alert">{{ error }}</p>
      <p v-if="warning" class="mb-5 rounded-xl border border-[var(--color-status-warning)] bg-[color-mix(in_oklch,var(--color-status-warning)_10%,transparent)] p-4 text-sm" role="status">{{ warning }}</p>

      <section v-if="step === 1" class="rounded-2xl border bg-[var(--color-surface-elevated)] p-6 shadow-[var(--shadow-lg)] sm:p-9" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="desk-stage-title">
        <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Stage 1</p>
        <h2 id="desk-stage-title" class="mt-2 text-2xl font-bold">Name the desk</h2>
        <p class="mt-2 text-sm text-[var(--color-text-secondary)]">Staff will see this name and support contact on the sign-in screen.</p>
        <form class="mt-7 grid gap-5" @submit.prevent="continueToAdministrator">
          <label class="grid gap-2 text-sm font-semibold">Help desk name<input v-model="appName" required maxlength="80" class="min-h-12 rounded-lg border bg-[var(--color-surface-primary)] px-4 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
          <label class="grid gap-2 text-sm font-semibold">Support contact <span class="font-normal text-[var(--color-text-tertiary)]">Optional, such as Room 104 or extension 225</span><input v-model="supportContact" maxlength="200" class="min-h-12 rounded-lg border bg-[var(--color-surface-primary)] px-4 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
          <button type="submit" class="min-h-12 rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white sm:justify-self-start">Continue to administrator</button>
        </form>
      </section>

      <section v-else-if="step === 2" class="rounded-2xl border bg-[var(--color-surface-elevated)] p-6 shadow-[var(--shadow-lg)] sm:p-9" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="administrator-stage-title">
        <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Stage 2</p>
        <h2 id="administrator-stage-title" class="mt-2 text-2xl font-bold">Create the technician account</h2>
        <p id="administrator-help" class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">This named administrator works tickets, creates staff accounts, and resets forgotten passwords.</p>
        <form class="mt-7 grid gap-5" aria-describedby="administrator-help" @submit.prevent="createAdministrator">
          <label class="grid gap-2 text-sm font-semibold">Full name<input v-model="displayName" name="name" autocomplete="name" required autofocus class="min-h-12 rounded-lg border bg-[var(--color-surface-primary)] px-4 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
          <label class="grid gap-2 text-sm font-semibold">Local username<input v-model="username" name="username" autocomplete="username" autocapitalize="none" spellcheck="false" pattern="[A-Za-z0-9._-]{3,32}" required class="min-h-12 rounded-lg border bg-[var(--color-surface-primary)] px-4 font-mono font-normal" :style="{ borderColor: 'var(--color-border-default)' }" /><span class="font-normal text-[var(--color-text-tertiary)]">3 to 32 letters, numbers, dots, dashes, or underscores.</span></label>
          <div class="grid gap-5 sm:grid-cols-2">
            <label class="grid gap-2 text-sm font-semibold">Password<input v-model="password" name="new-password" type="password" autocomplete="new-password" minlength="12" required class="min-h-12 rounded-lg border bg-[var(--color-surface-primary)] px-4 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
            <label class="grid gap-2 text-sm font-semibold">Confirm password<input v-model="confirmation" name="new-password-confirmation" type="password" autocomplete="new-password" minlength="12" required class="min-h-12 rounded-lg border bg-[var(--color-surface-primary)] px-4 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
          </div>
          <p class="text-xs text-[var(--color-text-tertiary)]">Use a unique passphrase of at least 12 characters. The server stores only a secure one-way hash.</p>
          <div class="flex flex-col-reverse gap-2 sm:flex-row sm:justify-between"><button type="button" class="min-h-11 rounded-lg border px-4 font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="step = 1">Back</button><button type="submit" :disabled="isSubmitting" class="min-h-12 rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white disabled:opacity-55">{{ isSubmitting ? "Creating account…" : "Create administrator" }}</button></div>
        </form>
      </section>

      <section v-else-if="step === 3" class="space-y-5" aria-labelledby="staff-stage-title">
        <div><p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Stage 3</p><h2 id="staff-stage-title" class="mt-2 text-2xl font-bold">Add staff who can submit requests</h2><p class="mt-2 text-sm text-[var(--color-text-secondary)]">This is optional now. You can add or disable people later from Manage Desk.</p></div>
        <OnboardingPanel v-if="onboardingCredentials.length" :credentials="onboardingCredentials" :desk-url="deskUrl" @dismiss="dismissCredentials" />
        <StaffQuickAdd :existing-usernames="[username]" @imported="staffCreated" />
        <button type="button" class="min-h-12 w-full rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white sm:w-auto" @click="reviewSetup">{{ onboardingCredentials.length ? "I saved the cards, review setup" : "Skip for now and review" }}</button>
      </section>

      <section v-else class="rounded-2xl border bg-[var(--color-surface-elevated)] p-6 shadow-[var(--shadow-lg)] sm:p-9" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="finish-stage-title">
        <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-status-success)]">Stage 4</p>
        <h2 id="finish-stage-title" class="mt-2 text-3xl font-bold">The desk is ready.</h2>
        <div class="mt-6 grid gap-3 sm:grid-cols-3">
          <div class="rounded-xl bg-[var(--color-surface-secondary)] p-4"><p class="text-xs uppercase tracking-wider text-[var(--color-text-tertiary)]">Address</p><p class="mt-2 break-all font-mono text-sm">{{ deskUrl }}</p></div>
          <div class="rounded-xl bg-[var(--color-surface-secondary)] p-4"><p class="text-xs uppercase tracking-wider text-[var(--color-text-tertiary)]">Administrator</p><p class="mt-2 font-semibold">{{ displayName }}</p></div>
          <div class="rounded-xl bg-[var(--color-surface-secondary)] p-4"><p class="text-xs uppercase tracking-wider text-[var(--color-text-tertiary)]">Staff access</p><p class="mt-2 font-semibold">Named accounts only</p></div>
        </div>
        <p class="mt-6 text-sm leading-6 text-[var(--color-text-secondary)]">Next, open Manage Desk to work the ticket queue or add more people. Staff will replace temporary passwords the first time they sign in.</p>
        <button type="button" class="mt-7 min-h-12 w-full rounded-lg bg-[var(--color-accent-primary)] px-5 font-bold text-white sm:w-auto" @click="finishSetup">Open Manage Desk</button>
      </section>
    </div>
  </main>
</template>
