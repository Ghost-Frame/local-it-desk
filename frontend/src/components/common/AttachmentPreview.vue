<script setup lang="ts">
/** Same-origin attachment link with safe inline thumbnails for detected images. */
import { computed } from "vue";

import { api } from "@/lib/api";
import { classifyAttachmentKind, formatAttachmentSize } from "@/lib/attachments";
import type { Attachment } from "@/types/api";

const props = defineProps<{
  /** Server-validated attachment metadata. */
  attachment: Attachment;
}>();

/** Authenticated same-origin attachment endpoint. */
const source = computed(() => api.attachmentUrl(props.attachment.id));
/** Whether server-detected media is safe for an image thumbnail. */
const isImage = computed(() => classifyAttachmentKind(props.attachment.media_type) === "image");
</script>

<template>
  <a
    :href="source"
    class="group flex min-h-16 items-center gap-3 rounded-xl border bg-[var(--color-surface-primary)] p-3 transition hover:border-[var(--color-border-strong)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--color-accent-primary)]"
    :download="attachment.original_name"
  >
    <img
      v-if="isImage"
      :src="source"
      alt=""
      class="h-11 w-11 rounded-lg border object-cover"
      :style="{ borderColor: 'var(--color-border-default)' }"
    />
    <span v-else class="grid h-11 w-11 place-items-center rounded-lg bg-[var(--color-surface-tertiary)] font-mono text-xs" aria-hidden="true">
      FILE
    </span>
    <span class="min-w-0">
      <span class="block truncate text-sm font-semibold group-hover:text-[var(--color-accent-primary)]">{{ attachment.original_name }}</span>
      <span class="mt-0.5 block text-xs text-[var(--color-text-tertiary)]">{{ formatAttachmentSize(attachment.size_bytes) }} · Download</span>
    </span>
  </a>
</template>
