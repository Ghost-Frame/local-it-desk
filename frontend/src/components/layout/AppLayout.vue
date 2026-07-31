<script setup lang="ts">
/** Responsive authenticated application shell. */
import { ref } from "vue";

import AppHeader from "./AppHeader.vue";
import AppSidebar from "./AppSidebar.vue";

/** Whether compact navigation is visible on small screens. */
const navigationOpen = ref(false);
</script>

<template>
  <div class="min-h-screen bg-[var(--color-surface-primary)] text-[var(--color-text-primary)]">
    <a
      href="#main-content"
      class="sr-only z-50 rounded-md bg-[var(--color-accent-primary)] px-4 py-2 text-white focus:not-sr-only focus:fixed focus:left-4 focus:top-4"
    >
      Skip to content
    </a>
    <AppSidebar :open="navigationOpen" @close="navigationOpen = false" />
    <div class="min-h-screen lg:pl-[var(--sidebar-width)]">
      <AppHeader @toggle-navigation="navigationOpen = !navigationOpen" />
      <main id="main-content" class="mx-auto w-full max-w-[96rem] p-4 sm:p-6 lg:p-8">
        <slot />
      </main>
    </div>
  </div>
</template>
