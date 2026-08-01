<script setup lang="ts">
/** Accessible ticket lifecycle badge with text and shape cues. */
import { computed } from "vue";

import type { TicketStatus } from "@/types/api";

const props = defineProps<{
  /** Current server lifecycle state. */
  status: TicketStatus;
}>();

/** Human-facing state labels kept independent of color. */
const labels: Record<TicketStatus, string> = {
  new: "New",
  open: "In progress",
  waiting_on_requester: "Waiting on you",
  resolved: "Resolved",
  closed: "Closed",
};

/** State-specific border and text treatment. */
const classes: Record<TicketStatus, string> = {
  new: "border-blue-500/35 bg-blue-500/10 text-blue-700 dark:text-blue-300",
  open: "border-violet-500/35 bg-violet-500/10 text-violet-700 dark:text-violet-300",
  waiting_on_requester: "border-amber-500/45 bg-amber-500/10 text-amber-800 dark:text-amber-200",
  resolved: "border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
  closed: "border-slate-500/35 bg-slate-500/10 text-slate-700 dark:text-slate-300",
};

/** Current visible lifecycle label. */
const label = computed(() => labels[props.status]);
</script>

<template>
  <span
    class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[0.68rem] font-bold uppercase tracking-[0.12em]"
    :class="classes[status]"
  >
    <span aria-hidden="true">{{ status === "closed" ? "■" : status === "resolved" ? "✓" : "●" }}</span>
    {{ label }}
  </span>
</template>
