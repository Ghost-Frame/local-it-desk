#!/usr/bin/env bash
# Independently validates one Local IT Desk operator release archive.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  printf 'Usage: %s ARCHIVE\n' "$0" >&2
  exit 2
fi

# Release archive supplied by the operator or rehearsal.
archive_path="$(realpath -- "$1")"
readonly archive_path
# Adjacent checksum published with the archive.
readonly archive_checksum_path="${archive_path}.sha256"
# Archive filename used to derive the one permitted root directory.
archive_filename="$(basename -- "${archive_path}")"
readonly archive_filename
# Strict semantic version parsed from the archive name.
version="${archive_filename#local-it-desk-}"
version="${version%.tar.gz}"
readonly version
# One temporary extraction root owned by this verification invocation.
verification_root="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-bundle-verify.XXXXXX")"
readonly verification_root
# Expected archive root matching the immutable release name.
readonly bundle_name="local-it-desk-${version}"
readonly bundle_root="${verification_root}/${bundle_name}"

# Removes only the exact temporary extraction root created above.
cleanup() {
  rm -rf -- "${verification_root}"
}
trap cleanup EXIT

# Prints one actionable verification failure.
fail() {
  printf 'Release bundle verification failed: %s\n' "$1" >&2
  exit 1
}

[[ -f "${archive_path}" && ! -L "${archive_path}" ]] \
  || fail 'archive must be one regular file'
[[ -f "${archive_checksum_path}" && ! -L "${archive_checksum_path}" ]] \
  || fail 'adjacent archive checksum is missing or unsafe'
[[ "${archive_filename}" == "local-it-desk-${version}.tar.gz" ]] \
  || fail 'archive name must be local-it-desk-MAJOR.MINOR.PATCH.tar.gz'
[[ "${version}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || fail 'archive version is not strict semantic versioning'

for required_command in realpath sha256sum tar find sort jq grep; do
  command -v "${required_command}" >/dev/null \
    || fail "required command is unavailable: ${required_command}"
done

# The published outer checksum must name only this adjacent archive.
checksum_record="$(cat -- "${archive_checksum_path}")"
readonly checksum_record
[[ "${checksum_record}" =~ ^[0-9a-f]{64}[[:space:]][[:space:]]${archive_filename}$ ]] \
  || fail 'outer checksum record has an unexpected name or format'
(
  cd "$(dirname -- "${archive_path}")"
  sha256sum --check "$(basename -- "${archive_checksum_path}")" >/dev/null
) || fail 'outer archive checksum does not match'

# Rejects traversal, extra roots, and non-regular payload types before extraction.
while IFS= read -r member_path; do
  [[ "${member_path}" == "${bundle_name}" || "${member_path}" == "${bundle_name}/"* ]] \
    || fail "archive member escapes the expected root: ${member_path}"
  [[ "${member_path}" != /* && "${member_path}" != *'/../'* && "${member_path}" != '../'* ]] \
    || fail "archive member contains an unsafe path: ${member_path}"
done < <(tar --list --gzip --file "${archive_path}")
if tar --list --verbose --gzip --file "${archive_path}" \
  | awk 'substr($1, 1, 1) != "d" && substr($1, 1, 1) != "-" { found=1 } END { exit !found }'; then
  fail 'archive contains a link, device, or other non-regular payload'
fi

tar --extract --gzip --file "${archive_path}" \
  --directory "${verification_root}" --no-same-owner --no-same-permissions
[[ -d "${bundle_root}" && ! -L "${bundle_root}" ]] \
  || fail 'expected bundle root was not extracted'

# Exact regular-file allowlist for the operator-only release payload.
readonly required_files=(
  .env.example
  QUICKSTART.md
  SHA256SUMS
  compose.https.yaml
  compose.yaml
  deploy/Caddyfile
  docs/BACKUP-RESTORE.md
  docs/ROSTER-IMPORT.md
  docs/RUNBOOK.md
  docs/STAFF-GUIDE.md
  docs/TLS.md
  release/README.txt
  release/allowed_signers
  release/provenance.json
  release/release-metadata.json
  release/sbom.spdx.json
  scripts/desk
  scripts/desk.ps1
  scripts/restore-compose.sh
)
# Sorted actual regular-file paths beneath the archive root.
actual_files="$(cd "${bundle_root}" && find . -type f -printf '%P\n' | sort)"
readonly actual_files
# Sorted required-file paths used for an exact payload comparison.
expected_files="$(printf '%s\n' "${required_files[@]}" | sort)"
readonly expected_files
[[ "${actual_files}" == "${expected_files}" ]] \
  || fail 'archive payload differs from the operator-file allowlist'

(
  cd "${bundle_root}"
  sha256sum --check SHA256SUMS >/dev/null
) || fail 'internal file checksum does not match'

# Verified release metadata bound to the archive name and immutable image reference.
readonly metadata_path="${bundle_root}/release/release-metadata.json"
image_digest="$(jq -er '.image.digest' "${metadata_path}")" \
  || fail 'release metadata has no image digest'
readonly image_digest
[[ "${image_digest}" =~ ^sha256:[0-9a-f]{64}$ ]] \
  || fail 'release metadata image digest is malformed'
jq -e \
  --arg version "${version}" \
  --arg digest "${image_digest}" \
  '.schema_version == 1
   and .version == $version
   and (.source_sha | test("^[0-9a-f]{40}$"))
   and .image.repository == "docker.io/ghostframe/local-it-desk"
   and .image.digest == $digest
   and .image.immutable_reference == ("docker.io/ghostframe/local-it-desk@" + $digest)
   and .image.platforms == ["linux/amd64", "linux/arm64"]' \
  "${metadata_path}" >/dev/null \
  || fail 'release metadata does not match the archive contract'

readonly immutable_image="docker.io/ghostframe/local-it-desk@${image_digest}"
grep -Fq -- "\${LOCAL_IT_DESK_IMAGE:-${immutable_image}}" "${bundle_root}/compose.yaml" \
  || fail 'Compose does not contain the metadata image digest'
if grep -En 'docker\.io/ghostframe/local-it-desk:(latest|[0-9]+\.[0-9]+\.[0-9]+)' "${bundle_root}/compose.yaml"; then
  fail 'Compose contains a mutable production image reference'
fi
[[ "$(grep -Fxc -- "LOCAL_IT_DESK_IMAGE=${immutable_image}" "${bundle_root}/.env.example")" -eq 1 ]] \
  || fail 'environment template does not contain the metadata image digest exactly once'
if grep -En '^LOCAL_IT_DESK_IMAGE=.*:(latest|[0-9]+\.[0-9]+\.[0-9]+)$' \
  "${bundle_root}/.env.example"; then
  fail 'environment template contains a mutable production image reference'
fi
[[ "$(grep -Fxc -- "readonly default_install_image='${immutable_image}'" \
  "${bundle_root}/scripts/desk")" -eq 1 ]] \
  || fail 'launcher does not contain the metadata image digest exactly once'
if grep -En "^readonly default_install_image='.*:(latest|[0-9]+\.[0-9]+\.[0-9]+)'$" \
  "${bundle_root}/scripts/desk"; then
  fail 'launcher contains a mutable production image reference'
fi

# Every relative Markdown link in the operator documentation must resolve inside the bundle.
while IFS= read -r -d '' documentation_path; do
  while IFS= read -r markdown_link; do
    link_target="${markdown_link#](}"
    link_target="${link_target%)}"
    link_target="${link_target%%#*}"
    if [[ "${link_target}" == /* ]]; then
      fail "operator documentation has an absolute local link: ${link_target}"
    fi
    if [[ -z "${link_target}" \
      || "${link_target}" =~ ^[a-zA-Z][a-zA-Z0-9+.-]*: ]]; then
      continue
    fi
    resolved_link="$(realpath -m -- "$(dirname -- "${documentation_path}")/${link_target}")"
    [[ "${resolved_link}" == "${bundle_root}/"* && -f "${resolved_link}" ]] \
      || fail "operator documentation has an unresolved local link: ${link_target}"
  done < <(grep -Eo '\]\([^)]*\)' "${documentation_path}" || true)
done < <(find "${bundle_root}/docs" -type f -name '*.md' -print0 | sort -z)

jq -e '.spdxVersion | startswith("SPDX-")' "${bundle_root}/release/sbom.spdx.json" >/dev/null \
  || fail 'software bill of materials is not SPDX JSON'
jq -e 'type == "object"' "${bundle_root}/release/provenance.json" >/dev/null \
  || fail 'provenance is not a JSON object'
if grep -Ern 'VERSION|CHANGE_ME|REPLACE_ME|YOUR_' \
  "${bundle_root}/.env.example" \
  "${bundle_root}/compose.yaml" \
  "${bundle_root}/compose.https.yaml" \
  "${bundle_root}/deploy" \
  "${bundle_root}/docs" \
  "${bundle_root}/release/README.txt" \
  "${bundle_root}/scripts/desk" \
  "${bundle_root}/scripts/desk.ps1"; then
  fail 'archive contains an unresolved release placeholder'
fi
if find "${bundle_root}" -type f \
  \( -name '*.db' -o -name '*.db-shm' -o -name '*.db-wal' -o -name '*.pem' \
     -o -name '*.key' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \
     -o -name 'package.json' -o -name 'pnpm-lock.yaml' -o -name '*.rs' \
     -o -name '*.ts' -o -name '*.vue' \) -print -quit | grep -q .; then
  fail 'archive contains data, credentials, or unpublished source material'
fi

printf 'Release bundle verified: %s\n' "${archive_path}"
