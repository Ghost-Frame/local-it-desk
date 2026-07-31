/** Attachment helper tests covering size validation and preview classification. */
import test from "node:test";
import assert from "node:assert/strict";
import {
  MAX_UPLOAD_BYTES,
  classifyAttachmentKind,
  validateUploadCandidates,
} from "../src/lib/attachments.js";

/** Minimal upload candidate fixture used by the Node harness. */
type UploadCandidate = {
  name: string;
  size: number;
  type: string;
};

/** Builds a lightweight upload candidate with sensible defaults. */
function makeCandidate(overrides: Partial<UploadCandidate>): UploadCandidate {
  return {
    name: overrides.name ?? "diagram.png",
    size: overrides.size ?? 1024,
    type: overrides.type ?? "image/png",
  };
}

test("classifyAttachmentKind recognizes inline images and generic files", () => {
  assert.equal(classifyAttachmentKind("image/webp"), "image");
  assert.equal(classifyAttachmentKind("application/pdf"), "file");
  assert.equal(classifyAttachmentKind(""), "file");
});

test("validateUploadCandidates rejects files larger than the 25MB client limit", () => {
  const candidates = [
    makeCandidate({ name: "ok.txt", size: 512, type: "text/plain" }),
    makeCandidate({
      name: "too-big.zip",
      size: MAX_UPLOAD_BYTES + 1,
      type: "application/zip",
    }),
  ];

  const result = validateUploadCandidates(candidates);

  assert.deepEqual(
    result.accepted.map((candidate) => candidate.name),
    ["ok.txt"],
  );
  assert.deepEqual(result.rejected, [
    {
      file: candidates[1],
      reason: "Files must be 25 MB or smaller.",
    },
  ]);
});
