<script setup lang="ts">
/** Accessible ticket priority badge with explicit urgency text. */
import type { TicketPriority } from "@/types/api";

defineProps<{
  /** Current ticket urgency. */
  priority: TicketPriority;
}>();

/** Human-facing priority labels. */
const labels: Record<TicketPriority, string> = {
  low: "Low priority",
  normal: "Normal priority",
  high: "High priority",
  urgent: "Urgent priority",
};

/** Priority-specific treatment that retains an icon and text cue. */
const classes: Record<TicketPriority, string> = {
  low: "text-[var(--color-text-tertiary)]",
  normal: "text-[var(--color-text-secondary)]",
  high: "text-amber-700 dark:text-amber-300",
  urgent: "font-bold text-red-700 dark:text-red-300",
};
</script>

<template>
  <span class="inline-flex items-center gap-1 text-xs" :class="classes[priority]">
    <span aria-hidden="true">{{ priority === "urgent" ? "▲▲" : priority === "high" ? "▲" : "◆" }}</span>
    {{ labels[priority] }}
  </span>
</template>
