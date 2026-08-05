<script setup lang="ts">
/** Compatibility route that forwards authenticated users to active work. */
import { onMounted } from "vue";
import { useRouter } from "vue-router";

import { landingPath } from "@/lib/router-guards";
import { useAuthStore } from "@/stores/auth";

/** Router used for the compatibility redirect. */
const router = useRouter();
/** Current identity used to select the active workspace. */
const authStore = useAuthStore();

onMounted(async () => {
  await router.replace(landingPath(authStore.user?.role));
});
</script>

<template>
  <main class="flex min-h-screen items-center justify-center bg-[var(--color-surface-primary)] p-6">
    <p class="text-sm text-[var(--color-text-secondary)]" role="status">Opening your workspace…</p>
  </main>
</template>
