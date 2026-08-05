#!/usr/bin/env bash
# Creates one deterministic, digest-pinned operator release bundle.
set -euo pipefail

if [[ "$#" -ne 6 ]]; then
  printf 'Usage: %s VERSION SOURCE_SHA IMAGE_REF IMAGE_DIGEST SBOM PROVENANCE\n' "$0" >&2
  exit 2
fi

# Semantic version without a leading v.
readonly version="$1"
# Full source commit represented by this bundle.
readonly source_sha="$2"
# Registry repository without a mutable tag.
readonly image_ref="$3"
# Immutable multi-architecture manifest digest.
readonly image_digest="$4"
# SPDX JSON file generated from the published image.
readonly sbom_path="$5"
# Sigstore attestation bundle generated for the published image.
readonly provenance_path="$6"
# Repository root containing the reviewed release inputs.
repo_root="$(git rev-parse --show-toplevel)"
readonly repo_root
# Temporary parent retained only for the duration of this build.
staging_parent="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-release.XXXXXX")"
readonly staging_parent
# Root directory stored inside the release archive.
readonly bundle_name="local-it-desk-${version}"
readonly bundle_root="${staging_parent}/${bundle_name}"
# Final archive and outer checksum paths.
readonly archive_path="${repo_root}/dist/${bundle_name}.tar.gz"
readonly archive_checksum_path="${archive_path}.sha256"

# Removes only the exact temporary directory created for this invocation.
cleanup() {
  rm -rf -- "${staging_parent}"
}
trap cleanup EXIT

if [[ ! "${version}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  printf 'Bundle build failed: version must be MAJOR.MINOR.PATCH.\n' >&2
  exit 1
fi
if [[ ! "${source_sha}" =~ ^[0-9a-f]{40}$ ]]; then
  printf 'Bundle build failed: source SHA must be 40 lowercase hexadecimal characters.\n' >&2
  exit 1
fi
if ! git -C "${repo_root}" cat-file -e "${source_sha}^{commit}"; then
  printf 'Bundle build failed: source SHA is not a local commit.\n' >&2
  exit 1
fi
if [[ "${image_ref}" != 'docker.io/ghostframe/local-it-desk' ]]; then
  printf 'Bundle build failed: unexpected release image repository.\n' >&2
  exit 1
fi
if [[ ! "${image_digest}" =~ ^sha256:[0-9a-f]{64}$ ]]; then
  printf 'Bundle build failed: image digest must be one SHA-256 digest.\n' >&2
  exit 1
fi
if [[ ! -s "${sbom_path}" ]] || ! jq -e '.spdxVersion | startswith("SPDX-")' "${sbom_path}" >/dev/null; then
  printf 'Bundle build failed: SBOM must be non-empty SPDX JSON.\n' >&2
  exit 1
fi
if [[ ! -s "${provenance_path}" ]] || ! jq -e 'type == "object"' "${provenance_path}" >/dev/null; then
  printf 'Bundle build failed: provenance must be a non-empty JSON object.\n' >&2
  exit 1
fi

# Crate version declared by the tagged source.
crate_version="$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' "${repo_root}/crates/server/Cargo.toml")"
readonly crate_version
if [[ "${crate_version}" != "${version}" ]]; then
  printf 'Bundle build failed: crate version does not match release version.\n' >&2
  exit 1
fi
if ! grep -Fq -- "org.opencontainers.image.version=\"${version}\"" "${repo_root}/Dockerfile"; then
  printf 'Bundle build failed: image label does not match release version.\n' >&2
  exit 1
fi
# Expected mutable source default that is replaced only inside the release bundle.
readonly source_image_default="docker.io/ghostframe/local-it-desk:${version}"
if ! grep -Fq -- "\${LOCAL_IT_DESK_IMAGE:-${source_image_default}}" "${repo_root}/compose.yaml"; then
  printf 'Bundle build failed: source Compose version does not match release version.\n' >&2
  exit 1
fi
# Expected source-build image selected by the operator environment template.
readonly source_environment_image="local-it-desk:${version}"
if [[ "$(awk '/^LOCAL_IT_DESK_IMAGE=/{ count += 1 } END { print count + 0 }' "${repo_root}/.env.example")" -ne 1 \
  || "$(grep -Fxc -- "LOCAL_IT_DESK_IMAGE=${source_environment_image}" "${repo_root}/.env.example")" -ne 1 ]]; then
  printf 'Bundle build failed: source environment image does not match release version.\n' >&2
  exit 1
fi

# Files copied without source-code or operator data.
readonly release_inputs=(
  .env.example
  QUICKSTART.md
  compose.yaml
  compose.https.yaml
  deploy/Caddyfile
  docs/RUNBOOK.md
  docs/TLS.md
  docs/BACKUP-RESTORE.md
  docs/ROSTER-IMPORT.md
  docs/STAFF-GUIDE.md
  release/README.txt
  release/allowed_signers
  scripts/restore-compose.sh
)

for input_path in "${release_inputs[@]}"; do
  if [[ ! -f "${repo_root}/${input_path}" ]]; then
    printf 'Bundle build failed: required input %s is missing.\n' "${input_path}" >&2
    exit 1
  fi
done

install -d -m 0755 \
  "${bundle_root}/deploy" \
  "${bundle_root}/docs" \
  "${bundle_root}/release" \
  "${bundle_root}/scripts" \
  "${repo_root}/dist"

for input_path in "${release_inputs[@]}"; do
  install -m 0644 "${repo_root}/${input_path}" "${bundle_root}/${input_path}"
done
chmod 0755 "${bundle_root}/scripts/restore-compose.sh"

# Replaces the source-build image with the exact published digest in the operator environment.
awk -v immutable_image="${image_ref}@${image_digest}" '
  /^LOCAL_IT_DESK_IMAGE=/ {
    print "LOCAL_IT_DESK_IMAGE=" immutable_image
    replaced = 1
    next
  }
  { print }
  END { if (replaced != 1) exit 1 }
' "${repo_root}/.env.example" >"${bundle_root}/.env.example"

# Renders the operator template with the exact release version.
if ! grep -Fq -- VERSION "${repo_root}/release/README.txt"; then
  printf 'Bundle build failed: release README template has no version placeholder.\n' >&2
  exit 1
fi
awk -v release_version="${version}" '{ gsub(/VERSION/, release_version); print }' \
  "${repo_root}/release/README.txt" >"${bundle_root}/release/README.txt"
if grep -Fq -- VERSION "${bundle_root}/release/README.txt"; then
  printf 'Bundle build failed: release README contains an unresolved placeholder.\n' >&2
  exit 1
fi

# Replaces only the application image default with the immutable release digest.
awk -v immutable_image="${image_ref}@${image_digest}" '
  /^    image: "\$\{LOCAL_IT_DESK_IMAGE:-/ {
    print "    image: \"${LOCAL_IT_DESK_IMAGE:-" immutable_image "}\""
    replaced = 1
    next
  }
  { print }
  END { if (replaced != 1) exit 1 }
' "${repo_root}/compose.yaml" >"${bundle_root}/compose.yaml"

install -m 0644 "${sbom_path}" "${bundle_root}/release/sbom.spdx.json"
install -m 0644 "${provenance_path}" "${bundle_root}/release/provenance.json"

# Release metadata binds the bundle, source, platforms, and image manifest.
jq -n \
  --arg version "${version}" \
  --arg source_sha "${source_sha}" \
  --arg image_ref "${image_ref}" \
  --arg image_digest "${image_digest}" \
  '{
    schema_version: 1,
    version: $version,
    source_sha: $source_sha,
    image: {
      repository: $image_ref,
      digest: $image_digest,
      immutable_reference: ($image_ref + "@" + $image_digest),
      platforms: ["linux/amd64", "linux/arm64"]
    }
  }' >"${bundle_root}/release/release-metadata.json"

# SHA256SUMS covers every regular file inside the extracted bundle except itself.
(
  cd "${bundle_root}"
  while IFS= read -r -d '' release_file; do
    sha256sum "${release_file}"
  done < <(find . -type f ! -name SHA256SUMS -print0 | sort -z)
) >"${bundle_root}/SHA256SUMS"

# Normalized ownership, ordering, and timestamps make identical inputs reproducible.
tar \
  --directory "${staging_parent}" \
  --sort=name \
  --mtime='@0' \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --format=ustar \
  --create \
  "${bundle_name}" \
  | gzip -n >"${archive_path}"

(
  cd "${repo_root}/dist"
  sha256sum "${bundle_name}.tar.gz" >"${bundle_name}.tar.gz.sha256"
)

printf 'Created %s\nCreated %s\n' "${archive_path}" "${archive_checksum_path}"
