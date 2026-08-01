<script setup lang="ts">
/** Keyboard-accessible single-file picker with client-side size guidance. */
import { ref, useId } from "vue";

const props = withDefaults(
  defineProps<{
    /** Currently selected file. */
    modelValue: File | null;
    /** Visible field label. */
    label?: string;
    /** Optional server-configured byte ceiling. */
    maxBytes?: number;
    /** Whether selection changes are temporarily blocked. */
    disabled?: boolean;
  }>(),
  { label: "Attach a file", maxBytes: 25 * 1024 * 1024, disabled: false },
);

const emit = defineEmits<{
  /** Replaces the selected file. */
  "update:modelValue": [file: File | null];
}>();

const input = ref<HTMLInputElement | null>(null);
const error = ref<string | null>(null);
const inputId = useId();

/** Validates one browser selection without reading file contents. */
function selectFile(event: Event): void {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0] ?? null;
  error.value = null;
  if (!file) {
    emit("update:modelValue", null);
    return;
  }
  if (file.size > props.maxBytes) {
    error.value = `Choose a file smaller than ${Math.floor(props.maxBytes / (1024 * 1024))} MB.`;
    target.value = "";
    emit("update:modelValue", null);
    return;
  }
  emit("update:modelValue", file);
}

/** Clears the chosen file from both the native control and parent state. */
function clearFile(): void {
  if (input.value) input.value.value = "";
  error.value = null;
  emit("update:modelValue", null);
}
</script>

<template>
  <div>
    <label class="block text-sm font-semibold" :for="inputId">{{ label }}</label>
    <input
      :id="inputId"
      ref="input"
      class="mt-2 block min-h-11 w-full rounded-xl border bg-[var(--color-surface-primary)] px-3 py-2 text-sm file:mr-3 file:rounded-lg file:border-0 file:bg-[var(--color-surface-tertiary)] file:px-3 file:py-2 file:font-semibold"
      :style="{ borderColor: error ? 'var(--color-status-error)' : 'var(--color-border-default)' }"
      type="file"
      :disabled="disabled"
      @change="selectFile"
    />
    <div v-if="modelValue" class="mt-2 flex items-center justify-between gap-3 text-xs text-[var(--color-text-secondary)]">
      <span class="truncate">Selected: {{ modelValue.name }}</span>
      <button type="button" class="font-bold text-[var(--color-accent-primary)]" :disabled="disabled" @click="clearFile">
        Remove
      </button>
    </div>
    <p v-if="error" class="mt-2 text-sm text-[var(--color-status-error)]" role="alert">{{ error }}</p>
  </div>
</template>
