#!/usr/bin/env bash
# Inspects one built image and blocks private content, build material, and fixed severe CVEs.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  printf 'Usage: %s IMAGE\n' "$0" >&2
  exit 2
fi

readonly image_ref="$1"
readonly container_engine="${CONTAINER_ENGINE:-docker}"
readonly trivy_image="${TRIVY_IMAGE:-docker.io/aquasec/trivy:0.72.0}"
scan_root="$(mktemp -d)"
readonly scan_root
container_id=""

# Removes only temporary scanner state and the exact inspection container.
cleanup() {
  if [[ -n "${container_id}" ]]; then
    "${container_engine}" rm "${container_id}" >/dev/null 2>&1 || true
  fi
  rm -rf -- "${scan_root}"
}
trap cleanup EXIT

"${container_engine}" image inspect "${image_ref}" >/dev/null
image_user="$("${container_engine}" image inspect "${image_ref}" --format '{{.Config.User}}')"
case "${image_user}" in
  ''|root|0|0:0)
    printf 'Image check failed: configured runtime user is root or empty.\n' >&2
    exit 1
    ;;
esac

image_history="$("${container_engine}" history --no-trunc "${image_ref}")"
if rg --ignore-case 'password=|token=|authorization:|PRIVATE KEY|gh[pousr]_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{20,}' <<<"${image_history}"; then
  printf 'Image check failed: credential-shaped layer command found.\n' >&2
  exit 1
fi

container_id="$("${container_engine}" create "${image_ref}")"
"${container_engine}" export --output "${scan_root}/rootfs.tar" "${container_id}"
tar -tf "${scan_root}/rootfs.tar" >"${scan_root}/rootfs-files.txt"
if rg --ignore-case '(^|/)(\.git|Cargo\.lock|Cargo\.toml|package\.json|pnpm-lock\.yaml)$|\.(rs|ts|vue)$|(^|/)workspace/' "${scan_root}/rootfs-files.txt"; then
  printf 'Image check failed: build source or lock metadata found in runtime filesystem.\n' >&2
  exit 1
fi

mkdir "${scan_root}/rootfs"
tar -xf "${scan_root}/rootfs.tar" -C "${scan_root}/rootfs"
# Private content terms are assembled from fragments to avoid publishing the protected values.
readonly extracted_private_pattern='Bay[- ]'"'Audio'"'[- ]'"'Video'"'|it-desk-'"'app'"'|synthe'"'os'"'|/home/'"'zan'"'|10\.50\.[0-9]{1,3}\.[0-9]{1,3}|172\.30\.[0-9]{1,3}\.[0-9]{1,3}|gir'"'box'"'\.org|Invader '"'Zim'"'|agent-'"'forge'"'|kleos-'"'cli'"''
if rg --hidden --no-messages --ignore-case "${extracted_private_pattern}" "${scan_root}/rootfs"; then
  printf 'Image check failed: private content found in runtime filesystem.\n' >&2
  exit 1
fi
if rg --hidden --no-messages --ignore-case 'eg_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|glpat-[A-Za-z0-9_-]{20,}|BEGIN [A-Z ]*PRIVATE KEY' "${scan_root}/rootfs"; then
  printf 'Image check failed: credential-shaped content found in runtime filesystem.\n' >&2
  exit 1
fi

if [[ "${container_engine}" == "podman" ]]; then
  "${container_engine}" save --format docker-archive --output "${scan_root}/image.tar" "${image_ref}"
else
  "${container_engine}" save --output "${scan_root}/image.tar" "${image_ref}"
fi

# The release policy blocks fixed HIGH or CRITICAL vulnerabilities. Unfixed findings remain in the report for operator review.
"${container_engine}" run --rm \
  --volume "${scan_root}:/scan:ro" \
  "${trivy_image}" image \
  --input /scan/image.tar \
  --scanners vuln \
  --severity HIGH,CRITICAL \
  --ignore-unfixed \
  --exit-code 1 \
  --no-progress

printf 'Image check passed for %s as user %s.\n' "${image_ref}" "${image_user}"
