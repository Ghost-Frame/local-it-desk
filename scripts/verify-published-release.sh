#!/usr/bin/env bash
# Independently verifies one published Local IT Desk release and image.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  printf 'Usage: %s vMAJOR.MINOR.PATCH\n' "$0" >&2
  exit 2
fi

# Immutable release tag supplied by the maintainer.
readonly release_tag="$1"
# Semantic version derived from the required leading-v tag.
readonly version="${release_tag#v}"
# Public source repository that owns the release and attestations.
readonly github_repository='Ghost-Frame/local-it-desk'
# Public container repository embedded in every release bundle.
readonly image_repository='docker.io/ghostframe/local-it-desk'
# Docker Hub API endpoint used to verify repository policy.
readonly dockerhub_repository_url='https://hub.docker.com/v2/namespaces/ghostframe/repositories/local-it-desk'
# Docker Hub API endpoint used to inspect the immutable version tag.
readonly dockerhub_tag_url="${dockerhub_repository_url}/tags/${version}"
# Docker registry authorization endpoint for anonymous pulls.
readonly registry_auth_url='https://auth.docker.io/token?service=registry.docker.io&scope=repository:ghostframe/local-it-desk:pull'
# Docker registry endpoint returning the raw version manifest.
readonly registry_manifest_url="https://registry-1.docker.io/v2/ghostframe/local-it-desk/manifests/${version}"
# Repository root containing the trusted signer allowlist and bundle verifier.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly repo_root
# Command paths may be replaced only by the isolated test harness.
readonly gh_bin="${VERIFY_GH_BIN:-gh}"
readonly git_bin="${VERIFY_GIT_BIN:-git}"
readonly curl_bin="${VERIFY_CURL_BIN:-curl}"
readonly bundle_verifier="${VERIFY_BUNDLE_VERIFIER:-${repo_root}/scripts/verify-release-bundle.sh}"
# Private temporary root owned by this verification invocation.
verification_root="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-published-verify.XXXXXX")"
readonly verification_root
# Download root for the exact public GitHub release assets.
readonly assets_root="${verification_root}/assets"
# Fresh clone root used only to validate the public tag and source commit.
readonly source_root="${verification_root}/source"

# Removes only the exact temporary root created by this invocation.
cleanup() {
  rm -rf -- "${verification_root}"
}
trap cleanup EXIT

# Prints one actionable published-release verification failure.
fail() {
  printf 'Published release verification failed: %s\n' "$1" >&2
  exit 1
}

# Requires a command name or executable path before network work begins.
require_command() {
  local candidate="$1"
  command -v "${candidate}" >/dev/null \
    || fail "required command is unavailable: ${candidate}"
}

[[ "${release_tag}" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || fail 'tag must be vMAJOR.MINOR.PATCH with strict semantic versioning'
[[ -s "${repo_root}/release/allowed_signers" && ! -L "${repo_root}/release/allowed_signers" ]] \
  || fail 'trusted release signer allowlist is missing or unsafe'
[[ -x "${bundle_verifier}" ]] \
  || fail 'release bundle verifier is missing or not executable'

for required_command in \
  "${gh_bin}" "${git_bin}" "${curl_bin}" jq sha256sum tar find sort cmp mktemp \
  install awk; do
  require_command "${required_command}"
done

install -d -m 0700 "${assets_root}"

# GitHub metadata must describe one final release with exactly four assets.
readonly release_metadata="${verification_root}/github-release.json"
"${gh_bin}" release view "${release_tag}" \
  --repo "${github_repository}" \
  --json assets,isDraft,isPrerelease,tagName >"${release_metadata}" \
  || fail 'GitHub release metadata could not be downloaded'
jq -e \
  --arg tag "${release_tag}" \
  '.tagName == $tag and .isDraft == false and .isPrerelease == false' \
  "${release_metadata}" >/dev/null \
  || fail 'GitHub release is missing, draft, prerelease, or bound to another tag'

# Expected public asset names for this exact semantic version.
expected_assets="$(printf '%s\n' \
  "local-it-desk-${version}.provenance.json" \
  "local-it-desk-${version}.spdx.json" \
  "local-it-desk-${version}.tar.gz" \
  "local-it-desk-${version}.tar.gz.sha256" | sort)"
readonly expected_assets
# Asset names declared by GitHub release metadata.
declared_assets="$(jq -r '.assets[]?.name' "${release_metadata}" | sort)"
readonly declared_assets
[[ "${declared_assets}" == "${expected_assets}" ]] \
  || fail 'GitHub release asset list differs from the four-file contract'

"${gh_bin}" release download "${release_tag}" \
  --repo "${github_repository}" \
  --dir "${assets_root}" \
  || fail 'GitHub release assets could not be downloaded'
# Regular files actually downloaded from the release.
downloaded_assets="$(find "${assets_root}" -mindepth 1 -maxdepth 1 -type f -printf '%f\n' | sort)"
readonly downloaded_assets
[[ "${downloaded_assets}" == "${expected_assets}" ]] \
  || fail 'downloaded release assets are missing, extra, or unsafe'

# Downloaded release artifact paths used by all later checks.
readonly archive_path="${assets_root}/local-it-desk-${version}.tar.gz"
readonly sbom_path="${assets_root}/local-it-desk-${version}.spdx.json"
readonly provenance_path="${assets_root}/local-it-desk-${version}.provenance.json"

"${git_bin}" clone --quiet --no-checkout \
  "https://github.com/${github_repository}.git" "${source_root}" \
  || fail 'public source repository could not be cloned'
[[ "$("${git_bin}" -C "${source_root}" cat-file -t "${release_tag}")" == 'tag' ]] \
  || fail 'release ref is not an annotated tag'
"${git_bin}" -C "${source_root}" \
  -c "gpg.ssh.allowedSignersFile=${repo_root}/release/allowed_signers" \
  verify-tag "${release_tag}" >/dev/null \
  || fail 'release tag signature is not trusted'
# Full source commit represented by the verified annotated tag.
source_sha="$("${git_bin}" -C "${source_root}" rev-parse "${release_tag}^{commit}")"
readonly source_sha
[[ "${source_sha}" =~ ^[0-9a-f]{40}$ ]] \
  || fail 'verified release tag did not resolve to a full source commit'

"${bundle_verifier}" "${archive_path}" >/dev/null \
  || fail 'operator release bundle did not pass its independent verifier'
# Extraction root used only after the bundle verifier rejected unsafe members.
readonly extracted_root="${verification_root}/extracted"
install -d -m 0700 "${extracted_root}"
tar --extract --gzip --file "${archive_path}" \
  --directory "${extracted_root}" --no-same-owner --no-same-permissions
# Verified directory carried inside the operator archive.
readonly bundle_root="${extracted_root}/local-it-desk-${version}"
# Release metadata that binds source and image identities.
readonly bundle_metadata="${bundle_root}/release/release-metadata.json"
jq -e --arg source_sha "${source_sha}" '.source_sha == $source_sha' \
  "${bundle_metadata}" >/dev/null \
  || fail 'bundle source SHA does not equal the signed tag commit'
cmp --silent "${sbom_path}" "${bundle_root}/release/sbom.spdx.json" \
  || fail 'external SBOM differs from the checksummed bundle copy'
cmp --silent "${provenance_path}" "${bundle_root}/release/provenance.json" \
  || fail 'external provenance differs from the checksummed bundle copy'
# Multi-architecture image index digest recorded by the verified bundle.
image_digest="$(jq -er '.image.digest' "${bundle_metadata}")" \
  || fail 'bundle metadata has no image digest'
readonly image_digest

# Docker Hub repository policy must remain public and immutable for semver tags.
readonly dockerhub_repository_metadata="${verification_root}/dockerhub-repository.json"
"${curl_bin}" --fail --silent --show-error --location \
  --proto '=https' --proto-redir '=https' \
  --output "${dockerhub_repository_metadata}" "${dockerhub_repository_url}" \
  || fail 'Docker Hub repository metadata could not be downloaded'
jq -e '
  .namespace == "ghostframe"
  and .name == "local-it-desk"
  and .is_private == false
  and .immutable_tags_settings.enabled == true
  and .immutable_tags_settings.rules == ["^[0-9]+\\.[0-9]+\\.[0-9]+$"]
' "${dockerhub_repository_metadata}" >/dev/null \
  || fail 'Docker Hub repository is not public with the exact semver immutability rule'

# Docker Hub tag metadata provides an independent digest and platform view.
readonly dockerhub_tag_metadata="${verification_root}/dockerhub-tag.json"
"${curl_bin}" --fail --silent --show-error --location \
  --proto '=https' --proto-redir '=https' \
  --output "${dockerhub_tag_metadata}" "${dockerhub_tag_url}" \
  || fail 'Docker Hub version metadata could not be downloaded'
jq -e \
  --arg version "${version}" \
  --arg digest "${image_digest}" \
  '.name == $version
   and .tag_status == "active"
   and .content_type == "image"
   and .digest == $digest
   and (.media_type == "application/vnd.oci.image.index.v1+json"
        or .media_type == "application/vnd.docker.distribution.manifest.list.v2+json")
   and any(.images[]?; .os == "linux" and .architecture == "amd64" and .status == "active")
   and any(.images[]?; .os == "linux" and .architecture == "arm64" and .status == "active")' \
  "${dockerhub_tag_metadata}" >/dev/null \
  || fail 'Docker Hub version digest or target platforms do not match the bundle'

# Short-lived anonymous registry token used only to obtain the raw manifest bytes.
readonly registry_auth_metadata="${verification_root}/registry-auth.json"
"${curl_bin}" --fail --silent --show-error --location \
  --proto '=https' --proto-redir '=https' \
  --output "${registry_auth_metadata}" "${registry_auth_url}" \
  || fail 'anonymous Docker registry token could not be requested'
registry_token="$(jq -er '.token | select(type == "string" and length > 0)' \
  "${registry_auth_metadata}")" \
  || fail 'Docker registry returned no anonymous pull token'
# Raw registry response whose SHA-256 is the published image index digest.
readonly raw_manifest_path="${verification_root}/image-index.json"
"${curl_bin}" --fail --silent --show-error --location \
  --proto '=https' --proto-redir '=https' \
  --header "Authorization: Bearer ${registry_token}" \
  --header 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json' \
  --output "${raw_manifest_path}" "${registry_manifest_url}" \
  || fail 'raw Docker registry manifest could not be downloaded'
unset registry_token
# SHA-256 identity of the exact raw manifest bytes returned by the registry.
raw_manifest_digest="sha256:$(sha256sum "${raw_manifest_path}" | awk '{print $1}')"
readonly raw_manifest_digest
[[ "${raw_manifest_digest}" == "${image_digest}" ]] \
  || fail 'raw registry manifest digest does not equal the verified bundle digest'
jq -e '
  (.mediaType == "application/vnd.oci.image.index.v1+json"
   or .mediaType == "application/vnd.docker.distribution.manifest.list.v2+json")
  and any(.manifests[]?; .platform.os == "linux" and .platform.architecture == "amd64")
  and any(.manifests[]?; .platform.os == "linux" and .platform.architecture == "arm64")
' "${raw_manifest_path}" >/dev/null \
  || fail 'raw registry manifest lacks the required Linux AMD64 and ARM64 images'

# GitHub verification binds the raw image index to its signed release workflow.
readonly attestation_result="${verification_root}/attestation-result.json"
"${gh_bin}" attestation verify "${raw_manifest_path}" \
  --digest-alg sha256 \
  --bundle "${provenance_path}" \
  --repo "${github_repository}" \
  --source-digest "${source_sha}" \
  --source-ref "refs/tags/${release_tag}" \
  --signer-workflow "${github_repository}/.github/workflows/release.yml" \
  --format json >"${attestation_result}" \
  || fail 'published image provenance signature or identity is invalid'
jq -e 'type == "array" and length > 0' "${attestation_result}" >/dev/null \
  || fail 'GitHub returned no verified image attestation'

printf 'Published release verified: %s\nSource: %s\nImage: %s@%s\n' \
  "${release_tag}" "${source_sha}" "${image_repository}" "${image_digest}"
