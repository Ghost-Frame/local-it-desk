<script setup lang="ts">
/** Preview-first, atomic CSV roster import workflow. */
import { ref } from "vue";

import { accountErrorMessage } from "@/lib/account-admin";
import { api } from "@/lib/api";
import type { OneTimeCredential, RosterPreview } from "@/types/api";

const emit = defineEmits<{
  /** Reports credentials returned by a successful atomic import. */
  imported: [credentials: OneTimeCredential[]];
}>();

/** Raw CSV retained only for the current component lifetime. */
const csvText = ref("");
/** Latest server validation result for the exact retained CSV. */
const preview = ref<RosterPreview | null>(null);
/** Exact CSV snapshot associated with the latest successful preview request. */
const previewedCsv = ref("");
/** Safe operator-facing import error. */
const errorMessage = ref("");
/** Whether a preview or apply request is active. */
const busy = ref(false);

/** Reads the selected local CSV into transient component memory. */
async function selectFile(event: Event): Promise<void> {
  const input = event.currentTarget as HTMLInputElement;
  const file = input.files?.[0];
  csvText.value = file ? await file.text() : "";
  preview.value = null;
  previewedCsv.value = "";
  errorMessage.value = "";
}

/** Requests read-only server validation for the exact retained CSV. */
async function previewRoster(): Promise<void> {
  if (!csvText.value) return;
  busy.value = true;
  errorMessage.value = "";
  const requestedCsv = csvText.value;
  try {
    const result = await api.previewRoster(requestedCsv);
    if (csvText.value === requestedCsv) {
      preview.value = result;
      previewedCsv.value = requestedCsv;
    }
  } catch (error) {
    preview.value = null;
    errorMessage.value = accountErrorMessage(error, "The roster could not be previewed.");
  } finally {
    busy.value = false;
  }
}

/** Applies the validated CSV as one all-or-nothing server transaction. */
async function applyRoster(): Promise<void> {
  if (!preview.value?.valid || csvText.value !== previewedCsv.value) return;
  busy.value = true;
  errorMessage.value = "";
  try {
    const result = await api.applyRoster(csvText.value);
    emit("imported", result.created);
    csvText.value = "";
    preview.value = null;
    previewedCsv.value = "";
  } catch (error) {
    errorMessage.value = accountErrorMessage(error, "The roster was not applied. No accounts were created.");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5" :style="{ borderColor: 'var(--color-border-default)' }">
    <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Advanced onboarding</p>
    <h2 class="mt-2 text-xl font-bold">Import a CSV roster</h2>
    <p class="mt-2 max-w-3xl text-sm text-[var(--color-text-secondary)]">Choose a CSV with the columns <code>username,display_name,role,email</code>. Preview checks every row. Apply creates every account or none of them.</p>
    <label class="mt-5 grid max-w-xl gap-2 text-sm font-semibold">
      Roster CSV
      <input type="file" accept=".csv,text/csv" class="min-h-11 rounded-lg border bg-[var(--color-surface-elevated)] p-2 font-normal" :style="{ borderColor: 'var(--color-border-default)' }" @change="selectFile" />
    </label>
    <div class="mt-4 flex flex-wrap gap-2">
      <button :disabled="!csvText || busy" class="min-h-11 rounded-lg border px-4 text-sm font-bold disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }" @click="previewRoster">{{ busy ? "Checking…" : "Preview roster" }}</button>
      <button :disabled="!preview?.valid || csvText !== previewedCsv || busy" class="min-h-11 rounded-lg bg-[var(--color-accent-primary)] px-4 text-sm font-bold text-white disabled:cursor-not-allowed disabled:opacity-40" @click="applyRoster">Apply all accounts</button>
    </div>
    <p v-if="errorMessage" class="mt-4 rounded-lg bg-[color-mix(in_oklch,var(--color-status-error)_12%,transparent)] p-3 text-sm" role="alert">{{ errorMessage }}</p>

    <div v-if="preview" class="mt-6" aria-live="polite">
      <p class="font-bold">{{ preview.valid ? `${preview.rows.length} rows ready to apply` : "Fix the roster and preview it again" }}</p>
      <ul v-if="preview.errors.length" class="mt-3 grid gap-2" role="list">
        <li v-for="(error, index) in preview.errors" :key="`${error.row_number}-${error.field}-${index}`" class="rounded-lg border border-[var(--color-status-error)] p-3 text-sm">
          <strong>{{ error.row_number ? `Row ${error.row_number}` : "File" }}{{ error.field ? `, ${error.field}` : "" }}:</strong>
          {{ error.message }}
        </li>
      </ul>
      <div v-else class="mt-4 overflow-x-auto rounded-xl border" :style="{ borderColor: 'var(--color-border-default)' }">
        <table class="w-full min-w-[42rem] text-left text-sm">
          <thead class="bg-[var(--color-surface-tertiary)] text-xs uppercase tracking-wider text-[var(--color-text-secondary)]"><tr><th class="p-3">Username</th><th class="p-3">Name</th><th class="p-3">Role</th><th class="p-3">Email</th></tr></thead>
          <tbody><tr v-for="row in preview.rows" :key="row.row_number" class="border-t" :style="{ borderColor: 'var(--color-border-default)' }"><td class="p-3 font-mono">{{ row.username }}</td><td class="p-3">{{ row.display_name }}</td><td class="p-3 capitalize">{{ row.role }}</td><td class="p-3">{{ row.email || "Not provided" }}</td></tr></tbody>
        </table>
      </div>
    </div>
  </section>
</template>
