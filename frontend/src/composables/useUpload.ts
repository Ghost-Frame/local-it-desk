/** Reserved upload-state boundary for the attachment implementation plan. */

import { ref } from "vue";

/** Creates inert upload state until attachment persistence is implemented. */
export function useUpload() {
  const isUploading = ref(false);
  const uploadError = ref<string | null>(null);
  return { isUploading, uploadError };
}
