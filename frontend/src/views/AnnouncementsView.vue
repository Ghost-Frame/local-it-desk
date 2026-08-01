<script setup lang="ts">
/** Staff announcement feed with administrator-only drafting and lifecycle controls. */
import { nextTick, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";

import AnnouncementEditor from "@/components/announcements/AnnouncementEditor.vue";
import AnnouncementList from "@/components/announcements/AnnouncementList.vue";
import AppLayout from "@/components/layout/AppLayout.vue";
import { ApiError, api } from "@/lib/api";
import { useAuthStore } from "@/stores/auth";
import type { Announcement, CreateAnnouncementRequest } from "@/types/api";

/** Current local identity and cumulative role state. */
const authStore = useAuthStore();
/** Current route including an optional server-generated announcement target. */
const route = useRoute();
/** Server-authorized published or administrator announcement records. */
const announcements = ref<Announcement[]>([]);
/** Record currently open in the administrator editor. */
const editing = ref<Announcement | null>(null);
/** Whether the initial feed is loading. */
const loading = ref(true);
/** Current mutation record or draft sentinel. */
const busyId = ref<string | null>(null);
/** Bounded announcement failure guidance. */
const error = ref("");
/** Successful lifecycle feedback. */
const message = ref("");

/** Focuses the published announcement named by a notification route when present. */
async function focusTargetAnnouncement(): Promise<void> {
  /** Stable route target or null when the ordinary feed was opened. */
  const target = typeof route.params.id === "string" ? route.params.id : null;
  if (!target || typeof document === "undefined") return;
  await nextTick();
  /** Rendered announcement card addressed without selector interpolation. */
  const element = document.getElementById("announcement-" + target);
  if (!element) return;
  element.focus({ preventScroll: true });
  element.scrollIntoView({ block: "center" });
}

/** Refocuses the selected card when Vue reuses this view for another notification target. */
watch(
  () => route.params.id,
  () => void focusTargetAnnouncement(),
);

/** Converts announcement API failures into safe recovery guidance. */
function announcementError(failure: unknown): string {
  if (failure instanceof ApiError) {
    if (failure.status === 409) return "That announcement can no longer be changed in its current state.";
    if (failure.status === 404) return "That announcement is no longer available.";
    if (failure.status === 403) return "Your account cannot manage announcements.";
  }
  return "Announcements could not be updated. Try again.";
}

/** Loads the published feed or full administrator history for the current role. */
async function loadAnnouncements(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    announcements.value = authStore.isAdministrator
      ? await api.listAdminAnnouncements()
      : await api.listAnnouncements();
    await focusTargetAnnouncement();
  } catch (failure) {
    error.value = announcementError(failure);
  } finally {
    loading.value = false;
  }
}

/** Creates a draft or updates the selected editable announcement. */
async function saveAnnouncement(details: CreateAnnouncementRequest): Promise<void> {
  busyId.value = editing.value?.id ?? "create";
  error.value = "";
  message.value = "";
  try {
    if (editing.value) {
      await api.updateAnnouncement(editing.value.id, details);
      message.value = "Announcement changes saved.";
    } else {
      await api.createAnnouncement(details);
      message.value = "Draft created. It is not visible to staff until published.";
    }
    editing.value = null;
    await loadAnnouncements();
  } catch (failure) {
    error.value = announcementError(failure);
  } finally {
    busyId.value = null;
  }
}

/** Publishes one draft and refreshes the administrator list. */
async function publishAnnouncement(announcement: Announcement): Promise<void> {
  busyId.value = announcement.id;
  error.value = "";
  message.value = "";
  try {
    await api.publishAnnouncement(announcement.id);
    editing.value = null;
    await loadAnnouncements();
    message.value = "Announcement published to staff.";
  } catch (failure) {
    error.value = announcementError(failure);
  } finally {
    busyId.value = null;
  }
}

/** Archives one record after explicit confirmation of its read-only state. */
async function archiveAnnouncement(announcement: Announcement): Promise<void> {
  if (!window.confirm("Archive this announcement? Archived announcements become read-only.")) return;
  busyId.value = announcement.id;
  error.value = "";
  message.value = "";
  try {
    await api.archiveAnnouncement(announcement.id);
    editing.value = null;
    await loadAnnouncements();
    message.value = "Announcement archived.";
  } catch (failure) {
    error.value = announcementError(failure);
  } finally {
    busyId.value = null;
  }
}

onMounted(() => void loadAnnouncements());
</script>

<template>
  <AppLayout>
    <section class="space-y-6">
      <header class="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div><p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Staff notices</p><h1 class="mt-3 text-4xl font-bold tracking-tight">Announcements</h1><p class="mt-3 max-w-2xl text-[var(--color-text-secondary)]">Local service notices and school technology updates for named staff accounts.</p></div>
        <button type="button" class="min-h-11 rounded-xl border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" :disabled="loading" @click="loadAnnouncements">Refresh announcements</button>
      </header>

      <p v-if="error" class="rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-800 dark:text-red-200" role="alert">{{ error }}</p>
      <p class="min-h-6 text-sm text-[var(--color-text-secondary)]" aria-live="polite">{{ message }}</p>

      <div v-if="authStore.isAdministrator" class="grid gap-6 xl:grid-cols-[minmax(18rem,0.7fr)_minmax(0,1.3fr)]">
        <AnnouncementEditor :announcement="editing" :busy="busyId !== null" @save="saveAnnouncement" @cancel="editing = null" />
        <div>
          <p v-if="loading" role="status">Loading announcement history…</p>
          <AnnouncementList v-else :announcements="announcements" administrator :busy-id="busyId" @edit="editing = $event" @publish="publishAnnouncement" @archive="archiveAnnouncement" />
        </div>
      </div>

      <template v-else>
        <p v-if="loading" role="status">Loading staff announcements…</p>
        <AnnouncementList v-else :announcements="announcements" :administrator="false" :busy-id="null" @edit="editing = $event" @publish="publishAnnouncement" @archive="archiveAnnouncement" />
      </template>
    </section>
  </AppLayout>
</template>
