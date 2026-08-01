#!/usr/bin/env bash
# Rehearses the complete signed, multi-architecture release without publishing.
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  printf 'Usage: %s VERSION\n' "$0" >&2
  exit 2
fi

# Strict release version without a leading v.
readonly version="$1"
[[ "${version}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || { printf 'Release rehearsal failed: version must be strict MAJOR.MINOR.PATCH\n' >&2; exit 1; }
# Repository root containing the clean release candidate.
repo_root="$(git rev-parse --show-toplevel)"
readonly repo_root
# Container engine used for rootless local image operations.
readonly container_engine="${CONTAINER_ENGINE:-podman}"
# Full commit represented by the rehearsal artifacts.
source_sha="$(git -C "${repo_root}" rev-parse HEAD)"
readonly source_sha
[[ -z "$(git -C "${repo_root}" status --porcelain --untracked-files=all)" ]] \
  || { printf 'Release rehearsal failed: working tree must be clean before rehearsal\n' >&2; exit 1; }
install -d -m 0755 "${repo_root}/dist"
# Unique retained directory containing non-source rehearsal evidence.
rehearsal_root="$(mktemp -d "${repo_root}/dist/rehearsal-${version}.XXXXXX")"
readonly rehearsal_root
# Exact temporary clone used only for signature rejection tests.
tag_test_root="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-tag-test.XXXXXX")"
readonly tag_test_root
# Exact temporary allowlisted context used by the unprivileged ARM cross-build.
arm_context="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-arm-context.XXXXXX")"
readonly arm_context
# Native and ARM image names retained for direct post-run inspection.
readonly native_image="local-it-desk-rehearsal:${version}-amd64-$$"
readonly arm_image="local-it-desk-rehearsal:${version}-arm64-$$"
# Architecture-specific OCI archives retained as release evidence.
readonly amd64_archive="${rehearsal_root}/local-it-desk-${version}-linux-amd64.oci.tar"
readonly arm64_archive="${rehearsal_root}/local-it-desk-${version}-linux-arm64.oci.tar"

# Removes only temporary clones and generated source copies; evidence remains under dist.
cleanup() {
  rm -rf -- "${tag_test_root}" "${arm_context}"
}
trap cleanup EXIT

# Prints one actionable rehearsal failure.
fail() {
  printf 'Release rehearsal failed: %s\n' "$1" >&2
  exit 1
}

# Asserts that an OCI archive configuration reports one exact platform.
assert_oci_platform() {
  local archive="$1"
  local expected_architecture="$2"
  local inspection_root
  local manifest_digest
  local config_digest
  inspection_root="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-oci-inspect.XXXXXX")"
  tar --extract --file "${archive}" --directory "${inspection_root}"
  manifest_digest="$(jq -er '.manifests[0].digest' "${inspection_root}/index.json")"
  manifest_digest="${manifest_digest#sha256:}"
  config_digest="$(jq -er '.config.digest' "${inspection_root}/blobs/sha256/${manifest_digest}")"
  config_digest="${config_digest#sha256:}"
  jq -e \
    --arg architecture "${expected_architecture}" \
    '.os == "linux" and .architecture == $architecture' \
    "${inspection_root}/blobs/sha256/${config_digest}" >/dev/null \
    || fail "OCI archive does not report linux/${expected_architecture}: ${archive}"
  rm -rf -- "${inspection_root}"
}

# Builds an ARM64 image with native build stages and a no-RUN foreign runtime stage.
build_arm64_image() {
  install -d -m 0755 \
    "${arm_context}/state/current/data" \
    "${arm_context}/state/current/attachments" \
    "${arm_context}/state/current/branding" \
    "${arm_context}/state/backups"
  git -C "${repo_root}" archive --format=tar HEAD \
    Cargo.toml Cargo.lock rust-toolchain.toml crates frontend \
    | tar --extract --file - --directory "${arm_context}"
  find "${arm_context}/state" -type d -exec sh -c 'printf "rehearsal\n" >"$1/.keep"' _ {} \;

  cat >"${arm_context}/Containerfile" <<'CONTAINERFILE'
# syntax=docker/dockerfile:1

# Frontend output is architecture-independent and builds on the host platform.
FROM --platform=linux/amd64 node:24.4.1-bookworm-slim AS frontend-builder
ARG PNPM_VERSION=11.3.0
WORKDIR /workspace/frontend
RUN npm install --global "pnpm@${PNPM_VERSION}"
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm build

# Rust binaries cross-compile with Debian's AArch64 GNU toolchain on amd64.
FROM --platform=linux/amd64 rust:1.88.0-bookworm AS rust-builder
RUN apt-get update \
    && apt-get install --yes --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
    && apt-get clean \
    && find /var/lib/apt/lists -type f -delete \
    && rustup target add aarch64-unknown-linux-gnu
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
WORKDIR /workspace
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ ./crates/
RUN cargo build --release --locked --bins --target aarch64-unknown-linux-gnu \
    && aarch64-linux-gnu-readelf -h target/aarch64-unknown-linux-gnu/release/local-it-desk \
      | grep -Fq 'Machine:                           AArch64' \
    && install -d -m 0755 /out \
    && install -m 0755 target/aarch64-unknown-linux-gnu/release/local-it-desk /out/ \
    && install -m 0755 target/aarch64-unknown-linux-gnu/release/local-it-desk-admin /out/ \
    && install -m 0755 target/aarch64-unknown-linux-gnu/release/local-it-desk-healthcheck /out/

# The foreign runtime stage executes no commands, so host binfmt is unnecessary.
FROM --platform=linux/arm64 debian:12.15-slim AS runtime
ARG RELEASE_VERSION
LABEL org.opencontainers.image.title="Local IT Desk" \
      org.opencontainers.image.description="Local-network help desk for named staff accounts" \
      org.opencontainers.image.version="${RELEASE_VERSION}"
COPY --from=rust-builder --chown=0:0 /out/local-it-desk /app/local-it-desk
COPY --from=rust-builder --chown=0:0 /out/local-it-desk-admin /app/local-it-desk-admin
COPY --from=rust-builder --chown=0:0 /out/local-it-desk-healthcheck /app/local-it-desk-healthcheck
COPY --from=frontend-builder --chown=0:0 /workspace/frontend/dist/ /app/frontend/
COPY --from=rust-builder --chown=0:0 /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --chown=10001:10001 state/ /state/
ENV LISTEN_ADDR="0.0.0.0:3000" \
    APP_ORIGIN="http://localhost:8080" \
    DATABASE_PATH="/state/current/data/local-it-desk.db" \
    UPLOAD_DIR="/state/current/attachments" \
    BRANDING_DIR="/state/current/branding" \
    SERVE_FRONTEND="true" \
    FRONTEND_DIR="/app/frontend" \
    HEALTHCHECK_ADDR="127.0.0.1:3000"
USER 10001:10001
EXPOSE 3000
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 CMD ["/app/local-it-desk-healthcheck"]
ENTRYPOINT ["/app/local-it-desk"]
CONTAINERFILE

  "${container_engine}" build \
    --platform linux/arm64 \
    --format docker \
    --build-arg "RELEASE_VERSION=${version}" \
    --file "${arm_context}/Containerfile" \
    --tag "${arm_image}" \
    "${arm_context}"
}

[[ -s "${repo_root}/release/allowed_signers" ]] \
  || fail 'release signer allowlist is missing'
[[ "$(git -C "${repo_root}" config --get gpg.format)" == 'ssh' ]] \
  || fail 'Git signing format must be ssh'
signing_key="$(git -C "${repo_root}" config --get user.signingkey)"
readonly signing_key
[[ -f "${signing_key}" ]] || fail 'configured release signing key is unavailable'
public_signing_key="$(ssh-keygen -y -f "${signing_key}" | awk '{print $1 " " $2}')"
readonly public_signing_key
if ! awk '{print $2 " " $3}' "${repo_root}/release/allowed_signers" \
  | grep -Fxq -- "${public_signing_key}"; then
  fail 'configured release signing key is not pinned in allowed_signers'
fi

for required_command in git ssh-keygen jq sha256sum tar gzip cargo pnpm "${container_engine}"; do
  command -v "${required_command}" >/dev/null \
    || fail "required command is unavailable: ${required_command}"
done
[[ "$(basename "${container_engine}")" == 'podman' ]] \
  || fail 'local multi-architecture rehearsal currently requires rootless Podman'
"${container_engine}" compose version >/dev/null

# The candidate commit and a new fixture tag must verify against the pinned signer.
git -C "${repo_root}" -c gpg.ssh.allowedSignersFile="${repo_root}/release/allowed_signers" \
  verify-commit HEAD
git clone --quiet "${repo_root}" "${tag_test_root}/repo"
git -C "${tag_test_root}/repo" config user.name GhostFrame
git -C "${tag_test_root}/repo" config user.email 271867738+Ghost-Frame@users.noreply.github.com
git -C "${tag_test_root}/repo" config gpg.format ssh
git -C "${tag_test_root}/repo" config user.signingkey "${signing_key}"
git -C "${tag_test_root}/repo" tag --sign --message signed-rehearsal v9.9.8
git -C "${tag_test_root}/repo" \
  -c gpg.ssh.allowedSignersFile="${repo_root}/release/allowed_signers" \
  verify-tag v9.9.8
git -C "${tag_test_root}/repo" -c tag.gpgSign=false \
  tag --annotate --message unsigned-rehearsal v9.9.9
if git -C "${tag_test_root}/repo" \
  -c gpg.ssh.allowedSignersFile="${repo_root}/release/allowed_signers" \
  verify-tag v9.9.9 >/dev/null 2>&1; then
  fail 'unsigned tag unexpectedly passed signer verification'
fi

cd "${repo_root}"
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --workspace --no-deps --document-private-items
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend check
bash scripts/check-dependencies.sh
bash scripts/check-release-rights.sh
bash scripts/check-history.sh
bash scripts/check-forbidden-surfaces.sh
bash scripts/check-private-terms.sh
CONTAINER_ENGINE="${container_engine}" bash scripts/compose-contract.sh
CONTAINER_ENGINE="${container_engine}" bash scripts/check-runbook.sh

CONTAINER_ENGINE="${container_engine}" LOCAL_IT_DESK_VERIFY_IMAGE="${native_image}" \
  bash scripts/container-contract.sh
CONTAINER_ENGINE="${container_engine}" bash scripts/check-image.sh "${native_image}"
"${container_engine}" save --format oci-archive --output "${amd64_archive}" "${native_image}"
assert_oci_platform "${amd64_archive}" amd64

build_arm64_image
CONTAINER_ENGINE="${container_engine}" bash scripts/check-image.sh "${arm_image}"
"${container_engine}" save --format oci-archive --output "${arm64_archive}" "${arm_image}"
assert_oci_platform "${arm64_archive}" arm64

# Local digest binds both architecture archive hashes without claiming a registry manifest.
amd64_sha="$(sha256sum "${amd64_archive}" | awk '{print $1}')"
readonly amd64_sha
arm64_sha="$(sha256sum "${arm64_archive}" | awk '{print $1}')"
readonly arm64_sha
manifest_json="${rehearsal_root}/local-manifest.json"
readonly manifest_json
jq -n -S \
  --arg source_sha "${source_sha}" \
  --arg amd64_sha "${amd64_sha}" \
  --arg arm64_sha "${arm64_sha}" \
  '{source_sha: $source_sha, platforms: {"linux/amd64": $amd64_sha, "linux/arm64": $arm64_sha}}' \
  >"${manifest_json}"
image_digest="sha256:$(sha256sum "${manifest_json}" | awk '{print $1}')"
readonly image_digest

# Trivy consumes a temporary Docker archive while OCI archives remain release evidence.
sbom_path="${rehearsal_root}/local-it-desk-${version}.spdx.json"
readonly sbom_path
sbom_input_archive="${rehearsal_root}/local-it-desk-${version}-sbom-input.docker.tar"
readonly sbom_input_archive
"${container_engine}" save --format docker-archive --output "${sbom_input_archive}" "${native_image}"
"${container_engine}" run --rm \
  --volume "${rehearsal_root}:/scan" \
  docker.io/aquasec/trivy:0.72.0 image \
  --input "/scan/$(basename "${sbom_input_archive}")" \
  --format spdx-json \
  --output "/scan/$(basename "${sbom_path}")" \
  --no-progress
rm -f -- "${sbom_input_archive}"
provenance_path="${rehearsal_root}/local-it-desk-${version}.provenance.json"
readonly provenance_path
jq -n -S \
  --arg mode local-rehearsal \
  --arg source_sha "${source_sha}" \
  --arg image_digest "${image_digest}" \
  --arg amd64_sha "${amd64_sha}" \
  --arg arm64_sha "${arm64_sha}" \
  '{mode: $mode, source_sha: $source_sha, image_digest: $image_digest, archives: {"linux/amd64": $amd64_sha, "linux/arm64": $arm64_sha}}' \
  >"${provenance_path}"

bash scripts/build-release-bundle.sh \
  "${version}" \
  "${source_sha}" \
  docker.io/ghostframe/local-it-desk \
  "${image_digest}" \
  "${sbom_path}" \
  "${provenance_path}"
bash scripts/verify-release-bundle.sh "dist/local-it-desk-${version}.tar.gz"

# The lifecycle journey runs from the extracted bundle while reusing the native local image.
bundle_extract_root="${rehearsal_root}/bundle"
readonly bundle_extract_root
install -d -m 0755 "${bundle_extract_root}"
tar --extract --gzip --file "dist/local-it-desk-${version}.tar.gz" \
  --directory "${bundle_extract_root}"
CONTAINER_ENGINE="${container_engine}" \
SMOKE_DEPLOYMENT_ROOT="${bundle_extract_root}/local-it-desk-${version}" \
SMOKE_PREBUILT_IMAGE="${native_image}" \
  bash scripts/smoke-compose.sh

if find "${repo_root}/dist" -type f \
  \( -name '*.db' -o -name '*.db-shm' -o -name '*.db-wal' -o -name '*.pem' \
     -o -name '*.key' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \
     -o -name 'package.json' -o -name 'pnpm-lock.yaml' -o -name '*.rs' \
     -o -name '*.ts' -o -name '*.vue' \) -print -quit | grep -q .; then
  fail 'dist contains data, credentials, or unpublished source material'
fi

printf 'RELEASE_REHEARSAL_OK\nEvidence retained at: %s\n' "${rehearsal_root}"
