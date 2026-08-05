<script setup lang="ts">
/** Role-aware help-desk navigation ordered around each account's active work. */
import { computed } from "vue";
import { storeToRefs } from "pinia";

import { useAuthStore } from "@/stores/auth";

defineProps<{
  /** Whether the small-screen navigation drawer is open. */
  open: boolean;
}>();

const emit = defineEmits<{
  /** Closes the small-screen navigation drawer. */
  close: [];
}>();

const authStore = useAuthStore();
const { canWorkTickets, displayName, logoUrl, publicConfig } = storeToRefs(authStore);

/** Navigation sequence that puts the current role's primary task first. */
const primaryItems = computed(() => {
  const common = [
    { name: "tickets", label: "Tickets", path: "/tickets" },
    { name: "announcements", label: "Announcements", path: "/announcements" },
    { name: "settings", label: "Settings", path: "/settings" },
  ];
  const items = canWorkTickets.value
    ? [{ name: "administration", label: "Manage Desk", path: "/administration" }, ...common]
    : common;
  return items.map((item, index) => ({ ...item, eyebrow: String(index + 1).padStart(2, "0") }));
});
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-30 bg-slate-950/55 backdrop-blur-sm lg:hidden"
    aria-hidden="true"
    @click="emit('close')"
  />
  <aside
    class="fixed inset-y-0 left-0 z-40 flex w-[var(--sidebar-width)] flex-col border-r bg-[var(--color-surface-secondary)] transition-transform lg:translate-x-0"
    :class="open ? 'translate-x-0' : '-translate-x-full'"
    :style="{ borderColor: 'var(--color-border-default)' }"
    aria-label="Primary navigation"
  >
    <div class="border-b px-5 py-6" :style="{ borderColor: 'var(--color-border-default)' }">
      <img v-if="logoUrl" :src="logoUrl" alt="" class="mb-4 h-12 max-w-full object-contain object-left" />
      <p class="font-mono text-[0.65rem] uppercase tracking-[0.24em] text-[var(--color-text-tertiary)]">
        Staff support
      </p>
      <p class="mt-2 truncate text-lg font-bold tracking-tight text-[var(--color-text-primary)]">
        {{ publicConfig?.app_name ?? "Local IT Desk" }}
      </p>
    </div>

    <nav class="flex-1 space-y-1 p-3">
      <router-link
        v-for="item in primaryItems"
        :key="item.name"
        :to="item.path"
        class="group flex min-h-11 items-center gap-3 rounded-lg border border-transparent px-3 text-sm font-semibold text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-tertiary)] hover:text-[var(--color-text-primary)]"
        active-class="!border-[var(--color-border-default)] !bg-[var(--color-surface-elevated)] !text-[var(--color-accent-primary)]"
        @click="emit('close')"
      >
        <span class="font-mono text-[0.65rem] text-[var(--color-text-tertiary)]">{{ item.eyebrow }}</span>
        <span>{{ item.label }}</span>
      </router-link>
    </nav>

    <div class="border-t px-5 py-4" :style="{ borderColor: 'var(--color-border-default)' }">
      <p class="truncate text-sm font-semibold text-[var(--color-text-primary)]">{{ displayName }}</p>
      <p class="mt-1 text-xs capitalize text-[var(--color-text-tertiary)]">
        {{ authStore.user?.role ?? "requester" }}
      </p>
    </div>
  </aside>
</template>
