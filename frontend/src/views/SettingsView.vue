<script setup lang="ts">
/** Personal controls and administrator runtime branding and category settings. */
import { onMounted, ref } from "vue";

import FileUpload from "@/components/common/FileUpload.vue";
import AppLayout from "@/components/layout/AppLayout.vue";
import { ApiError, api } from "@/lib/api";
import { useAuthStore } from "@/stores/auth";
import type { AdminSettings, Category, TicketPriority } from "@/types/api";

/** Current identity, role, and public runtime configuration. */
const authStore = useAuthStore();
/** Administrator-visible non-secret settings. */
const settings = ref<AdminSettings | null>(null);
/** Complete active and inactive category list. */
const categories = ref<Category[]>([]);
/** Editable application name. */
const appName = ref("");
/** Editable optional support contact. */
const supportContact = ref("");
/** Editable requester default priority. */
const defaultPriority = ref<TicketPriority>("normal");
/** Raster logo selected for upload. */
const logoFile = ref<File | null>(null);
/** New category name draft. */
const newCategoryName = ref("");
/** New category description draft. */
const newCategoryDescription = ref("");
/** New category display order draft. */
const newCategoryOrder = ref(0);
/** Whether administrator settings are loading. */
const loading = ref(false);
/** Whether one settings operation is in flight. */
const saving = ref(false);
/** Bounded settings failure guidance. */
const error = ref("");
/** Successful settings feedback. */
const message = ref("");

/** Converts settings failures into bounded administrator recovery guidance. */
function settingsError(failure: unknown): string {
  if (failure instanceof ApiError) {
    if (failure.status === 409) return "That change conflicts with the current category configuration.";
    if (failure.status === 413) return "The selected logo exceeds the configured upload limit.";
    if (failure.status === 415) return "Choose a PNG, JPEG, or WebP logo whose extension matches its contents.";
    if (failure.status === 403) return "Administrator access is required for that change.";
  }
  return "Settings could not be updated. Try again.";
}

/** Copies the canonical administrator response into editable controls. */
function acceptSettings(updated: AdminSettings): void {
  settings.value = updated;
  appName.value = updated.app_name;
  supportContact.value = updated.support_contact ?? "";
  defaultPriority.value = updated.default_priority;
}

/** Loads current non-secret settings and all category states. */
async function loadAdministratorSettings(): Promise<void> {
  if (!authStore.isAdministrator) return;
  loading.value = true;
  error.value = "";
  try {
    const [currentSettings, currentCategories] = await Promise.all([
      api.getAdminSettings(),
      api.listCategories(),
    ]);
    acceptSettings(currentSettings);
    categories.value = currentCategories;
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    loading.value = false;
  }
}

/** Refreshes public branding and requester category choices after a successful mutation. */
async function refreshRuntimeConfig(): Promise<void> {
  await authStore.refreshPublicConfig();
}

/** Saves visible application branding and default urgency. */
async function saveSettings(): Promise<void> {
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    acceptSettings(await api.updateAdminSettings({
      app_name: appName.value.trim(),
      support_contact: supportContact.value.trim(),
      default_priority: defaultPriority.value,
    }));
    await refreshRuntimeConfig();
    message.value = "Runtime settings saved.";
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    saving.value = false;
  }
}

/** Uploads and activates one detected safe raster logo. */
async function uploadLogo(): Promise<void> {
  if (!logoFile.value) return;
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    acceptSettings(await api.uploadLogo(logoFile.value));
    logoFile.value = null;
    await refreshRuntimeConfig();
    message.value = "Logo updated across the local help desk.";
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    saving.value = false;
  }
}

/** Creates one active requester-selectable category. */
async function createCategory(): Promise<void> {
  if (!newCategoryName.value.trim()) return;
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    await api.createCategory({
      name: newCategoryName.value.trim(),
      description: newCategoryDescription.value.trim() || null,
      sort_order: newCategoryOrder.value,
    });
    newCategoryName.value = "";
    newCategoryDescription.value = "";
    newCategoryOrder.value = 0;
    await loadAdministratorSettings();
    await refreshRuntimeConfig();
    message.value = "Category created.";
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    saving.value = false;
  }
}

/** Saves editable name, description, and display order on one category. */
async function saveCategory(category: Category): Promise<void> {
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    await api.updateCategory(category.id, {
      name: category.name.trim(),
      description: category.description?.trim() ?? "",
      sort_order: category.sort_order,
    });
    await loadAdministratorSettings();
    await refreshRuntimeConfig();
    message.value = "Category updated.";
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    saving.value = false;
  }
}

/** Toggles requester availability while preserving the protected default category. */
async function toggleCategory(category: Category): Promise<void> {
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    await api.updateCategory(category.id, { is_active: !category.is_active });
    await loadAdministratorSettings();
    await refreshRuntimeConfig();
    message.value = category.is_active ? "Category disabled." : "Category enabled.";
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    saving.value = false;
  }
}

/** Selects one active category as the new-request default. */
async function selectDefaultCategory(category: Category): Promise<void> {
  saving.value = true;
  error.value = "";
  message.value = "";
  try {
    acceptSettings(await api.selectDefaultCategory(category.id));
    await refreshRuntimeConfig();
    message.value = "Default category updated.";
  } catch (failure) {
    error.value = settingsError(failure);
  } finally {
    saving.value = false;
  }
}

onMounted(() => void loadAdministratorSettings());
</script>

<template>
  <AppLayout>
    <section class="space-y-7">
      <header><p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Local preferences</p><h1 class="mt-3 text-4xl font-bold tracking-tight">Settings</h1><p class="mt-3 max-w-2xl text-[var(--color-text-secondary)]">Manage your local account and the help desk’s live browser configuration.</p></header>

      <p v-if="error" class="rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-800 dark:text-red-200" role="alert">{{ error }}</p>
      <p class="min-h-6 text-sm text-[var(--color-text-secondary)]" aria-live="polite">{{ message }}</p>

      <div class="grid gap-4 md:grid-cols-2">
        <article class="rounded-2xl border bg-[var(--color-surface-secondary)] p-6" :style="{ borderColor: 'var(--color-border-default)' }"><p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-text-tertiary)]">Security</p><h2 class="mt-3 text-lg font-bold">Password</h2><p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">Change your local password using your current password for confirmation.</p><router-link to="/change-password" class="mt-4 inline-flex min-h-11 items-center font-bold text-[var(--color-accent-primary)]">Change password</router-link></article>
        <article class="rounded-2xl border bg-[var(--color-surface-secondary)] p-6" :style="{ borderColor: 'var(--color-border-default)' }"><p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-text-tertiary)]">Display</p><h2 class="mt-3 text-lg font-bold">Theme</h2><p class="mt-2 text-sm leading-6 text-[var(--color-text-secondary)]">Use the header control to switch this browser between light and dark modes.</p></article>
      </div>

      <template v-if="authStore.isAdministrator">
        <p v-if="loading" role="status">Loading runtime settings…</p>
        <div v-else class="space-y-7">
          <form class="rounded-2xl border bg-[var(--color-surface-primary)] p-5 sm:p-6" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="saveSettings">
            <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Live branding</p><h2 class="mt-2 text-2xl font-bold">Help desk identity</h2>
            <div class="mt-5 grid gap-4 md:grid-cols-2">
              <label class="grid gap-2 text-sm font-bold">Application name<input v-model="appName" class="min-h-11 rounded-xl border bg-[var(--color-surface-secondary)] px-3 font-normal" maxlength="80" required :disabled="saving" /></label>
              <label class="grid gap-2 text-sm font-bold">Support contact<input v-model="supportContact" class="min-h-11 rounded-xl border bg-[var(--color-surface-secondary)] px-3 font-normal" maxlength="200" placeholder="Room, extension, or local contact" :disabled="saving" /></label>
              <label class="grid gap-2 text-sm font-bold">Default priority<select v-model="defaultPriority" class="min-h-11 rounded-xl border bg-[var(--color-surface-secondary)] px-3 font-normal" :disabled="saving"><option value="low">Low</option><option value="normal">Normal</option><option value="high">High</option><option value="urgent">Urgent</option></select></label>
            </div>
            <button type="submit" class="mt-5 min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-5 text-sm font-bold text-white disabled:opacity-50" :disabled="saving || !appName.trim()">Save runtime settings</button>
          </form>

          <form class="rounded-2xl border bg-[var(--color-surface-secondary)] p-5 sm:p-6" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="uploadLogo">
            <h2 class="text-xl font-bold">Upload logo</h2><p class="mt-2 text-sm text-[var(--color-text-secondary)]">PNG, JPEG, or WebP. New branding appears without rebuilding the image.</p>
            <div class="mt-4 max-w-xl"><FileUpload v-model="logoFile" label="Raster logo file" :max-bytes="authStore.publicConfig?.max_upload_bytes" :disabled="saving" /></div>
            <button type="submit" class="mt-4 min-h-11 rounded-xl border px-4 text-sm font-bold disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="saving || !logoFile">Upload logo</button>
          </form>

          <section aria-labelledby="categories-heading">
            <div><p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-accent-primary)]">Request routing</p><h2 id="categories-heading" class="mt-2 text-2xl font-bold">Categories</h2><p class="mt-2 text-sm text-[var(--color-text-secondary)]">Disabled categories remain on existing tickets but disappear from new requests.</p></div>
            <form class="mt-5 grid gap-3 rounded-2xl border bg-[var(--color-surface-secondary)] p-4 md:grid-cols-[1fr_1.4fr_7rem_auto] md:items-end" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="createCategory">
              <label class="grid gap-2 text-sm font-bold">Name<input v-model="newCategoryName" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 font-normal" maxlength="80" required :disabled="saving" /></label>
              <label class="grid gap-2 text-sm font-bold">Description<input v-model="newCategoryDescription" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 font-normal" maxlength="500" :disabled="saving" /></label>
              <label class="grid gap-2 text-sm font-bold">Order<input v-model.number="newCategoryOrder" type="number" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 font-normal" :disabled="saving" /></label>
              <button type="submit" class="min-h-11 rounded-xl bg-[var(--color-accent-primary)] px-4 text-sm font-bold text-white" :disabled="saving || !newCategoryName.trim()">Add category</button>
            </form>

            <div class="mt-4 space-y-3">
              <form v-for="category in categories" :key="category.id" class="grid gap-3 rounded-2xl border bg-[var(--color-surface-primary)] p-4 lg:grid-cols-[1fr_1.4fr_7rem_auto] lg:items-end" :style="{ borderColor: 'var(--color-border-default)' }" @submit.prevent="saveCategory(category)">
                <label class="grid gap-2 text-sm font-bold">Category name<input v-model="category.name" class="min-h-11 rounded-xl border bg-[var(--color-surface-secondary)] px-3 font-normal" maxlength="80" :disabled="saving" /></label>
                <label class="grid gap-2 text-sm font-bold">Description<input v-model="category.description" class="min-h-11 rounded-xl border bg-[var(--color-surface-secondary)] px-3 font-normal" maxlength="500" :disabled="saving" /></label>
                <label class="grid gap-2 text-sm font-bold">Order<input v-model.number="category.sort_order" type="number" class="min-h-11 rounded-xl border bg-[var(--color-surface-secondary)] px-3 font-normal" :disabled="saving" /></label>
                <div class="flex flex-wrap gap-2">
                  <button type="submit" class="min-h-11 rounded-xl border px-3 text-sm font-bold" :disabled="saving">Save</button>
                  <button v-if="category.is_active && settings?.default_category_id !== category.id" type="button" class="min-h-11 px-3 text-sm font-bold text-red-700 dark:text-red-300" :disabled="saving" @click="toggleCategory(category)">Disable category</button>
                  <button v-else-if="!category.is_active" type="button" class="min-h-11 px-3 text-sm font-bold text-emerald-700 dark:text-emerald-300" :disabled="saving" @click="toggleCategory(category)">Enable category</button>
                  <span v-if="settings?.default_category_id === category.id" class="inline-flex min-h-11 items-center px-3 text-xs font-bold uppercase tracking-wider text-[var(--color-accent-primary)]">Default category</span>
                  <button v-else-if="category.is_active" type="button" class="min-h-11 px-3 text-sm font-bold text-[var(--color-accent-primary)]" :disabled="saving" @click="selectDefaultCategory(category)">Make default</button>
                </div>
              </form>
            </div>
          </section>
        </div>
      </template>
    </section>
  </AppLayout>
</template>
