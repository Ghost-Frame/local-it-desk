<script setup lang="ts">
/** Accessible private-notification menu with mobile full-viewport fallback. */
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";

import { api } from "@/lib/api";
import { formatTicketTimestamp } from "@/lib/ticket-requester";
import type { Notification } from "@/types/api";

/** Browser router used only for validated same-origin application paths. */
const router = useRouter();
/** Trigger button used to restore focus when the dialog closes. */
const trigger = ref<HTMLButtonElement | null>(null);
/** Dialog container used for initial focus placement. */
const panel = ref<HTMLElement | null>(null);
/** Whether the notification dialog is open. */
const open = ref(false);
/** Current account's newest private notifications. */
const notifications = ref<Notification[]>([]);
/** Bounded unread count shown as text and a badge. */
const unreadCount = ref(0);
/** Whether notification records are loading. */
const loading = ref(false);
/** Bounded notification error guidance. */
const error = ref("");
/** Periodic unread-count refresh handle. */
let refreshTimer: number | undefined;

/** Accepts only root-relative non-protocol-relative application targets. */
function safeTarget(path: string | null): string | null {
  return path && path.startsWith("/") && !path.startsWith("//") ? path : null;
}

/** Loads the current unread count without opening the menu. */
async function refreshCount(): Promise<void> {
  try {
    unreadCount.value = await api.getUnreadNotificationCount();
  } catch {
    // A transient count failure must not interrupt the application shell.
  }
}

/** Loads the current account's bounded private notification history. */
async function loadNotifications(): Promise<void> {
  loading.value = true;
  error.value = "";
  try {
    /** Fetches the bounded history and authoritative unread total together. */
    const [records, count] = await Promise.all([
      api.listNotifications(),
      api.getUnreadNotificationCount(),
    ]);
    notifications.value = records;
    unreadCount.value = count;
  } catch {
    error.value = "Notifications could not be loaded. Try again.";
  } finally {
    loading.value = false;
  }
}

/** Opens the dialog, loads records, and moves focus into its first action. */
async function openMenu(): Promise<void> {
  open.value = true;
  await loadNotifications();
  await nextTick();
  panel.value?.querySelector<HTMLElement>("[data-notification-action]")?.focus();
}

/** Closes the dialog and optionally returns keyboard focus to its trigger. */
function closeMenu(restoreFocus = true): void {
  open.value = false;
  if (restoreFocus) void nextTick(() => trigger.value?.focus());
}

/** Marks one owned notice read before following a validated local target. */
async function activate(notification: Notification): Promise<void> {
  error.value = "";
  try {
    if (!notification.read_at) {
      await api.markNotificationRead(notification.id);
      notification.read_at = new Date().toISOString();
      unreadCount.value = Math.max(0, unreadCount.value - 1);
    }
    const target = safeTarget(notification.target_path);
    closeMenu(false);
    if (target) await router.push(target);
  } catch {
    error.value = "That notification could not be opened. Try again.";
  }
}

/** Marks every current notification read and updates local display state. */
async function markAllRead(): Promise<void> {
  error.value = "";
  try {
    await api.markAllNotificationsRead();
    const readAt = new Date().toISOString();
    notifications.value = notifications.value.map((notification) => ({
      ...notification,
      read_at: notification.read_at ?? readAt,
    }));
    unreadCount.value = 0;
  } catch {
    error.value = "Notifications could not be marked read. Try again.";
  }
}

/** Closes the dialog when the keyboard escape key is pressed. */
function handleEscape(event: KeyboardEvent): void {
  if (open.value && event.key === "Escape") closeMenu();
}

/** Cycles Tab focus inside the declared modal notification dialog. */
function trapDialogFocus(event: KeyboardEvent): void {
  if (event.key !== "Tab" || !panel.value) return;
  /** Enabled interactive elements participating in the dialog tab order. */
  const actions = Array.from(panel.value.querySelectorAll<HTMLElement>(
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  ));
  /** First focus boundary inside the dialog. */
  const first = actions.at(0);
  /** Last focus boundary inside the dialog. */
  const last = actions.at(-1);
  if (!first || !last) {
    event.preventDefault();
    return;
  }
  if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.value)) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

onMounted(() => {
  void refreshCount();
  refreshTimer = window.setInterval(() => void refreshCount(), 60_000);
  document.addEventListener("keydown", handleEscape);
});

onUnmounted(() => {
  if (refreshTimer !== undefined) window.clearInterval(refreshTimer);
  document.removeEventListener("keydown", handleEscape);
});
</script>

<template>
  <div class="relative">
    <button
      ref="trigger"
      type="button"
      class="relative min-h-11 rounded-lg border px-3 text-xs font-semibold uppercase tracking-wider"
      :style="{ borderColor: 'var(--color-border-default)' }"
      aria-haspopup="dialog"
      :aria-expanded="open"
      aria-controls="notification-dialog"
      @click="open ? closeMenu() : openMenu()"
    >
      Notices
      <span v-if="unreadCount" class="ml-1 rounded-full bg-red-700 px-2 py-0.5 text-[0.65rem] text-white">{{ unreadCount > 99 ? "99+" : unreadCount }}</span>
      <span class="sr-only" aria-live="polite">Unread notifications: {{ unreadCount }}</span>
    </button>

    <div v-if="open" class="fixed inset-0 z-40 bg-slate-950/55 sm:bg-slate-950/30" aria-hidden="true" @click="closeMenu()" />
    <section
      v-if="open"
      id="notification-dialog"
      ref="panel"
      class="fixed inset-0 z-50 flex flex-col bg-[var(--color-surface-primary)] sm:absolute sm:inset-auto sm:right-0 sm:top-14 sm:max-h-[38rem] sm:w-[26rem] sm:rounded-2xl sm:border sm:shadow-2xl"
      :style="{ borderColor: 'var(--color-border-default)' }"
      role="dialog"
      aria-modal="true"
      aria-labelledby="notification-heading"
      @keydown="trapDialogFocus"
    >
      <header class="flex items-center justify-between gap-3 border-b p-4" :style="{ borderColor: 'var(--color-border-default)' }">
        <div><h2 id="notification-heading" class="text-lg font-bold">Notifications</h2><p class="mt-1 text-xs text-[var(--color-text-secondary)]">Unread notifications: {{ unreadCount }}</p></div>
        <button type="button" class="min-h-11 px-3 text-sm font-bold" data-notification-action @click="closeMenu()">Close</button>
      </header>
      <div class="flex-1 overflow-y-auto p-3">
        <p v-if="error" class="rounded-xl bg-red-500/10 p-3 text-sm text-red-800 dark:text-red-200" role="alert">{{ error }}</p>
        <p v-if="loading" class="p-4 text-sm" role="status">Loading notifications…</p>
        <div v-else-if="notifications.length" class="space-y-2">
          <button v-for="notification in notifications" :key="notification.id" type="button" class="block min-h-20 w-full rounded-xl border p-3 text-left" :class="notification.read_at ? 'bg-[var(--color-surface-primary)]' : 'bg-[var(--color-surface-secondary)]'" :style="{ borderColor: 'var(--color-border-default)' }" data-notification-action @click="activate(notification)">
            <span class="flex items-start justify-between gap-3"><strong class="text-sm">{{ notification.title }}</strong><span v-if="!notification.read_at" class="mt-1 h-2 w-2 shrink-0 rounded-full bg-[var(--color-accent-primary)]"><span class="sr-only">Unread</span></span></span>
            <span class="mt-1 block text-xs leading-5 text-[var(--color-text-secondary)]">{{ notification.body }}</span>
            <time class="mt-2 block text-[0.68rem] text-[var(--color-text-tertiary)]" :datetime="notification.created_at">{{ formatTicketTimestamp(notification.created_at) }}</time>
          </button>
        </div>
        <p v-else class="p-6 text-center text-sm text-[var(--color-text-secondary)]">No notifications yet.</p>
      </div>
      <footer class="border-t p-3 text-right" :style="{ borderColor: 'var(--color-border-default)' }"><button type="button" class="min-h-11 px-3 text-sm font-bold text-[var(--color-accent-primary)]" :disabled="unreadCount === 0" @click="markAllRead">Mark all read</button></footer>
    </section>
  </div>
</template>
