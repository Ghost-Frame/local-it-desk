<script setup lang="ts">
/** Shared-queue filter controls with explicit multi-state and assignment choices. */
import type { PublicCategory, TicketPriority, TicketStatus, User } from "@/types/api";
import type { TicketFilters } from "@/lib/ticket-admin";

/** Current queue controls and the runtime values used to populate them. */
const props = defineProps<{
  /** Current queue filter state. */
  modelValue: TicketFilters;
  /** Active categories available to the operator. */
  categories: PublicCategory[];
  /** Active support accounts available for assignment filtering. */
  supportAccounts: User[];
}>();

/** Immutable filter replacement event consumed by the owning queue. */
const emit = defineEmits<{
  /** Replaces queue filters after one control changes. */
  "update:modelValue": [filters: TicketFilters];
}>();

/** Human-facing lifecycle filter choices. */
const statusOptions: Array<{ value: TicketStatus; label: string }> = [
  { value: "new", label: "New" },
  { value: "open", label: "In progress" },
  { value: "waiting_on_requester", label: "Waiting on requester" },
  { value: "resolved", label: "Resolved" },
  { value: "closed", label: "Closed" },
];

/** Human-facing urgency filter choices. */
const priorityOptions: Array<{ value: TicketPriority; label: string }> = [
  { value: "urgent", label: "Urgent" },
  { value: "high", label: "High" },
  { value: "normal", label: "Normal" },
  { value: "low", label: "Low" },
];

/** Replaces one scalar field while retaining array filter selections. */
function updateScalar<Key extends "search" | "assignee" | "sort">(
  key: Key,
  value: TicketFilters[Key],
): void {
  emit("update:modelValue", { ...props.modelValue, [key]: value });
}

/** Toggles one selected value in a typed array filter. */
function toggleArray(
  key: "statuses" | "priorities" | "categoryIds",
  value: string,
  checked: boolean,
): void {
  const current = props.modelValue[key] as string[];
  const next = checked ? [...current, value] : current.filter((item) => item !== value);
  emit("update:modelValue", { ...props.modelValue, [key]: next });
}

/** Restores the complete default queue filter set. */
function clear(): void {
  emit("update:modelValue", {
    search: "",
    statuses: ["new", "open", "waiting_on_requester", "resolved", "closed"],
    priorities: ["low", "normal", "high", "urgent"],
    categoryIds: [],
    assignee: "all",
    sort: "priority",
  });
}
</script>

<template>
  <div class="space-y-4 rounded-2xl border bg-[var(--color-surface-secondary)] p-4" :style="{ borderColor: 'var(--color-border-default)' }">
    <div class="grid gap-3 lg:grid-cols-[minmax(14rem,1fr)_auto_auto_auto] lg:items-end">
      <label class="grid gap-2 text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">
        Search queue
        <input
          :value="modelValue.search"
          type="search"
          class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 text-sm font-normal normal-case tracking-normal text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent-primary)]"
          placeholder="Number, title, or details"
          @input="updateScalar('search', ($event.target as HTMLInputElement).value)"
        />
      </label>
      <label class="grid gap-2 text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">
        Assignment
        <select :value="modelValue.assignee" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 text-sm font-normal normal-case tracking-normal text-[var(--color-text-primary)]" @change="updateScalar('assignee', ($event.target as HTMLSelectElement).value)">
          <option value="all">All assignments</option>
          <option value="unassigned">Unassigned</option>
          <option v-for="account in supportAccounts" :key="account.id" :value="account.id">{{ account.display_name }}</option>
        </select>
      </label>
      <label class="grid gap-2 text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">
        Sort
        <select :value="modelValue.sort" class="min-h-11 rounded-xl border bg-[var(--color-surface-primary)] px-3 text-sm font-normal normal-case tracking-normal text-[var(--color-text-primary)]" @change="updateScalar('sort', ($event.target as HTMLSelectElement).value as TicketFilters['sort'])">
          <option value="priority">Priority first</option>
          <option value="updated">Recently updated</option>
          <option value="created">Newest created</option>
        </select>
      </label>
      <button type="button" class="min-h-11 rounded-xl px-3 text-sm font-bold text-[var(--color-accent-primary)]" @click="clear">Reset filters</button>
    </div>

    <details>
      <summary class="min-h-11 cursor-pointer py-3 text-sm font-bold">Status, priority, and category filters</summary>
      <div class="grid gap-5 border-t pt-4 md:grid-cols-3" :style="{ borderColor: 'var(--color-border-default)' }">
        <fieldset>
          <legend class="text-xs font-bold uppercase tracking-wider text-[var(--color-text-tertiary)]">Statuses</legend>
          <label v-for="option in statusOptions" :key="option.value" class="mt-2 flex min-h-8 items-center gap-2 text-sm">
            <input type="checkbox" :checked="modelValue.statuses.includes(option.value)" @change="toggleArray('statuses', option.value, ($event.target as HTMLInputElement).checked)" />
            {{ option.label }}
          </label>
        </fieldset>
        <fieldset>
          <legend class="text-xs font-bold uppercase tracking-wider text-[var(--color-text-tertiary)]">Priorities</legend>
          <label v-for="option in priorityOptions" :key="option.value" class="mt-2 flex min-h-8 items-center gap-2 text-sm">
            <input type="checkbox" :checked="modelValue.priorities.includes(option.value)" @change="toggleArray('priorities', option.value, ($event.target as HTMLInputElement).checked)" />
            {{ option.label }}
          </label>
        </fieldset>
        <fieldset>
          <legend class="text-xs font-bold uppercase tracking-wider text-[var(--color-text-tertiary)]">Categories</legend>
          <p v-if="categories.length === 0" class="mt-2 text-sm text-[var(--color-text-secondary)]">No active categories.</p>
          <label v-for="category in categories" :key="category.id" class="mt-2 flex min-h-8 items-center gap-2 text-sm">
            <input type="checkbox" :checked="modelValue.categoryIds.includes(category.id)" @change="toggleArray('categoryIds', category.id, ($event.target as HTMLInputElement).checked)" />
            {{ category.name }}
          </label>
        </fieldset>
      </div>
    </details>
  </div>
</template>
