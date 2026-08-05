<script setup lang="ts">
/** Paste-first requester onboarding with server preview and atomic apply. */
import { computed, ref } from "vue";

import { accountErrorMessage } from "@/lib/account-admin";
import { api } from "@/lib/api";
import { buildRequesterRosterCsv, parseStaffNames } from "@/lib/staff-onboarding";
import type { OneTimeCredential, RosterPreview } from "@/types/api";

const props = withDefaults(defineProps<{
  /** Existing local usernames reserved during client-side suggestion. */
  existingUsernames?: string[];
}>(), {
  existingUsernames: () => [],
});

const emit = defineEmits<{
  /** Reports credentials returned by a successful all-or-nothing apply. */
  imported: [credentials: OneTimeCredential[]];
}>();

/** Newline-separated staff names retained only while this component is mounted. */
const pastedNames = ref("");
/** Latest canonical server preview for the exact generated CSV. */
const preview = ref<RosterPreview | null>(null);
/** CSV snapshot associated with the latest server preview. */
const previewedCsv = ref("");
/** Bounded operator-facing request failure. */
const errorMessage = ref("");
/** Whether a preview or apply operation is active. */
const busy = ref(false);
/** Current pure parsing result for immediate name and username feedback. */
const parsed = computed(() => parseStaffNames(pastedNames.value, props.existingUsernames));
/** Current requester-only CSV generated from the pasted names. */
const generatedCsv = computed(() => buildRequesterRosterCsv(parsed.value.rows));

/** Clears stale server state whenever pasted input changes. */
function namesChanged(): void {
  preview.value = null;
  previewedCsv.value = "";
  errorMessage.value = "";
}

/** Requests canonical server validation without creating accounts. */
async function previewStaff(): Promise<void> {
  if (!parsed.value.valid || busy.value) return;
  busy.value = true;
  errorMessage.value = "";
  const requestedCsv = generatedCsv.value;
  try {
    const result = await api.previewRoster(requestedCsv);
    if (generatedCsv.value === requestedCsv) {
      preview.value = result;
      previewedCsv.value = requestedCsv;
    }
  } catch (error) {
    preview.value = null;
    errorMessage.value = accountErrorMessage(error, "The staff list could not be checked.");
  } finally {
    busy.value = false;
  }
}

/** Applies the exact valid preview as one server transaction. */
async function createStaff(): Promise<void> {
  const csv = generatedCsv.value;
  if (!preview.value?.valid || previewedCsv.value !== csv || busy.value) return;
  busy.value = true;
  errorMessage.value = "";
  try {
    const result = await api.applyRoster(csv);
    emit("imported", result.created);
    pastedNames.value = "";
    preview.value = null;
    previewedCsv.value = "";
  } catch (error) {
    errorMessage.value = accountErrorMessage(error, "No accounts were created. Check the list and try again.");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5 sm:p-6" :style="{ borderColor: 'var(--color-border-default)' }" aria-labelledby="quick-add-title">
    <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Fast staff setup</p>
    <h2 id="quick-add-title" class="mt-2 text-xl font-bold">Paste staff names</h2>
    <p id="quick-add-help" class="mt-2 max-w-2xl text-sm leading-6 text-[var(--color-text-secondary)]">
      Put one person on each line. Every account is a requester. You can review all usernames before anything is created.
    </p>
    <label for="quick-staff-names" class="mt-5 block text-sm font-semibold">Staff names</label>
    <textarea
      id="quick-staff-names"
      v-model="pastedNames"
      rows="7"
      aria-describedby="quick-add-help"
      placeholder="Casey Smith&#10;Jordan Lee&#10;Morgan Rivera"
      class="mt-2 min-h-44 w-full resize-y rounded-xl border bg-[var(--color-surface-elevated)] p-4 outline-none focus:border-[var(--color-accent-primary)] focus:ring-3 focus:ring-[color-mix(in_oklch,var(--color-accent-primary)_24%,transparent)]"
      :style="{ borderColor: 'var(--color-border-default)' }"
      @input="namesChanged"
    />

    <ul v-if="pastedNames.trim() && parsed.errors.length" class="mt-4 grid gap-2" role="alert">
      <li v-for="problem in parsed.errors" :key="`${problem.line}-${problem.message}`" class="rounded-lg bg-[color-mix(in_oklch,var(--color-status-error)_10%,transparent)] p-3 text-sm">
        <strong>{{ problem.line ? `Line ${problem.line}:` : "Staff list:" }}</strong> {{ problem.message }}
      </li>
    </ul>

    <div v-if="parsed.rows.length" class="mt-5 overflow-x-auto rounded-xl border" :style="{ borderColor: 'var(--color-border-default)' }" aria-live="polite">
      <table class="w-full min-w-[30rem] text-left text-sm">
        <thead class="bg-[var(--color-surface-tertiary)] text-xs uppercase tracking-wider text-[var(--color-text-secondary)]">
          <tr><th class="p-3">Name</th><th class="p-3">Username</th><th class="p-3">Access</th></tr>
        </thead>
        <tbody>
          <tr v-for="row in parsed.rows" :key="row.username" class="border-t" :style="{ borderColor: 'var(--color-border-default)' }">
            <td class="p-3 font-semibold">{{ row.displayName }}</td><td class="p-3 font-mono">{{ row.username }}</td><td class="p-3">Submit and view own requests</td>
          </tr>
        </tbody>
      </table>
    </div>

    <p v-if="errorMessage" class="mt-4 rounded-lg bg-[color-mix(in_oklch,var(--color-status-error)_10%,transparent)] p-3 text-sm" role="alert">{{ errorMessage }}</p>
    <p v-if="preview" class="mt-4 text-sm font-semibold" aria-live="polite">
      {{ preview.valid ? `${preview.rows.length} staff accounts are ready.` : "The server found a conflict. No accounts were created." }}
    </p>
    <ul v-if="preview?.errors.length" class="mt-2 grid gap-2" role="list">
      <li v-for="(problem, index) in preview.errors" :key="`${problem.row_number}-${problem.field}-${index}`" class="rounded-lg border border-[var(--color-status-error)] p-3 text-sm">
        <strong>{{ problem.row_number ? `Row ${problem.row_number}:` : "Staff list:" }}</strong> {{ problem.message }}
      </li>
    </ul>

    <div class="mt-5 flex flex-col gap-2 sm:flex-row">
      <button type="button" :disabled="!parsed.valid || busy" class="min-h-11 rounded-lg border px-4 text-sm font-bold disabled:cursor-not-allowed disabled:opacity-45" :style="{ borderColor: 'var(--color-border-default)' }" @click="previewStaff">
        {{ busy ? "Checking…" : "Review with server" }}
      </button>
      <button type="button" :disabled="!preview?.valid || previewedCsv !== generatedCsv || busy" class="min-h-11 rounded-lg bg-[var(--color-accent-primary)] px-4 text-sm font-bold text-white disabled:cursor-not-allowed disabled:opacity-45" @click="createStaff">
        Create staff accounts
      </button>
    </div>
  </section>
</template>
