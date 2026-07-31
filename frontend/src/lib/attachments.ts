/** Ticket and announcement attachment validation helpers. */

/** Maximum attachment size enforced before upload begins. */
export const MAX_UPLOAD_BYTES = 25 * 1024 * 1024;

/** File-like shape accepted by upload validation. */
export interface UploadCandidate {
  /** Human-facing filename. */
  name: string;
  /** Exact size in bytes. */
  size: number;
  /** Browser-reported media type. */
  type: string;
}

/** Preview modes supported by the attachment renderer. */
export type AttachmentKind = "image" | "file";

/** Classifies safe inline images and download-only files. */
export function classifyAttachmentKind(mediaType: string | null | undefined): AttachmentKind {
  return mediaType?.startsWith("image/") ? "image" : "file";
}

/** Formats an attachment size as a compact label. */
export function formatAttachmentSize(sizeBytes: number): string {
  if (sizeBytes >= 1024 * 1024) return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.ceil(sizeBytes / 1024))} KB`;
}

/** Splits upload candidates into accepted and oversized groups. */
export function validateUploadCandidates<T extends UploadCandidate>(
  candidates: T[],
): { accepted: T[]; rejected: Array<{ file: T; reason: string }> } {
  const accepted: T[] = [];
  const rejected: Array<{ file: T; reason: string }> = [];
  for (const candidate of candidates) {
    if (candidate.size > MAX_UPLOAD_BYTES) {
      rejected.push({ file: candidate, reason: "Files must be 25 MB or smaller." });
    } else {
      accepted.push(candidate);
    }
  }
  return { accepted, rejected };
}
