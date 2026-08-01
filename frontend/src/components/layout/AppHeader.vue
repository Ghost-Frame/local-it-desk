<script setup lang="ts">
/** Compact header with navigation, theme, and session controls. */
import { storeToRefs } from "pinia";
import { useRouter } from "vue-router";

import { useTheme } from "@/composables/useTheme";
import { useAuthStore } from "@/stores/auth";
import NotificationMenu from "@/components/layout/NotificationMenu.vue";

const emit = defineEmits<{
  /** Toggles the small-screen navigation drawer. */
  "toggle-navigation": [];
}>();

const router = useRouter();
const authStore = useAuthStore();
const { displayName } = storeToRefs(authStore);
const { isDark, setTheme } = useTheme();

/** Ends the current session and returns to the local login screen. */
async function signOut(): Promise<void> {
  await authStore.logout();
  await router.replace("/login");
}
</script>

<template>
  <header
    class="sticky top-0 z-20 flex min-h-16 items-center justify-between border-b bg-[color-mix(in_oklch,var(--color-surface-elevated)_92%,transparent)] px-4 backdrop-blur sm:px-6"
    :style="{ borderColor: 'var(--color-border-default)' }"
  >
    <button
      class="min-h-11 min-w-11 rounded-lg border text-sm font-bold lg:hidden"
      :style="{ borderColor: 'var(--color-border-default)' }"
      aria-label="Toggle navigation"
      @click="emit('toggle-navigation')"
    >
      Menu
    </button>
    <p class="hidden font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-text-tertiary)] sm:block">
      Local network service
    </p>
    <div class="flex items-center gap-2">
      <span class="hidden max-w-40 truncate text-sm text-[var(--color-text-secondary)] md:block">
        {{ displayName }}
      </span>
      <NotificationMenu />
      <button
        class="min-h-11 rounded-lg border px-3 text-xs font-semibold uppercase tracking-wider"
        :style="{ borderColor: 'var(--color-border-default)' }"
        @click="setTheme(isDark ? 'light' : 'dark')"
      >
        {{ isDark ? "Light" : "Dark" }}
      </button>
      <button
        class="min-h-11 rounded-lg bg-[var(--color-accent-primary)] px-3 text-xs font-semibold uppercase tracking-wider text-white"
        @click="signOut"
      >
        Sign out
      </button>
    </div>
  </header>
</template>
