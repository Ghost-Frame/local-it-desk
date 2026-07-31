<script setup lang="ts">
/** Application root that initializes the selected visual theme. */
import { RouterView } from "vue-router";

import { useTheme } from "@/composables/useTheme";

useTheme();
</script>

<template>
  <RouterView />
</template>
