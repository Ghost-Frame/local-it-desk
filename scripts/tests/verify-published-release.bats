#!/usr/bin/env bats
# Verify the published-release trust chain through isolated provider fixtures.
# shellcheck disable=SC2016

# Build one internally consistent release and replace every network command.
setup() {
  export VERIFIER="$BATS_TEST_DIRNAME/../verify-published-release.sh"
  export REPO_ROOT="$BATS_TEST_DIRNAME/../.."
  export FIXTURE_ROOT="$BATS_TEST_TMPDIR/fixture"
  export FIXTURE_VERSION='0.1.1'
  export FIXTURE_TAG="v${FIXTURE_VERSION}"
  export FIXTURE_SOURCE_SHA='aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
  mkdir -p "$FIXTURE_ROOT/assets" "$FIXTURE_ROOT/bin"
  build_raw_manifest
  build_release_assets
  build_provider_metadata
  build_mock_commands
  export VERIFY_GH_BIN="$FIXTURE_ROOT/bin/gh"
  export VERIFY_GIT_BIN="$FIXTURE_ROOT/bin/git"
  export VERIFY_CURL_BIN="$FIXTURE_ROOT/bin/curl"
}

# Create a two-platform OCI image index and record its content digest.
build_raw_manifest() {
  jq -cn '{
    schemaVersion: 2,
    mediaType: "application/vnd.oci.image.index.v1+json",
    manifests: [
      {
        mediaType: "application/vnd.oci.image.manifest.v1+json",
        digest: "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        size: 1,
        platform: {os: "linux", architecture: "amd64"}
      },
      {
        mediaType: "application/vnd.oci.image.manifest.v1+json",
        digest: "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        size: 1,
        platform: {os: "linux", architecture: "arm64"}
      }
    ]
  }' >"$FIXTURE_ROOT/image-index.json"
  FIXTURE_IMAGE_DIGEST="sha256:$(sha256sum "$FIXTURE_ROOT/image-index.json" | awk '{print $1}')"
  export FIXTURE_IMAGE_DIGEST
}

# Create the exact release bundle and its four public release assets.
build_release_assets() {
  local bundle_name="local-it-desk-${FIXTURE_VERSION}"
  local bundle_root="$FIXTURE_ROOT/$bundle_name"
  mkdir -p \
    "$bundle_root/deploy" \
    "$bundle_root/docs" \
    "$bundle_root/release" \
    "$bundle_root/scripts"
  awk -v immutable_image="docker.io/ghostframe/local-it-desk@${FIXTURE_IMAGE_DIGEST}" '
    /^LOCAL_IT_DESK_IMAGE=/ {
      print "LOCAL_IT_DESK_IMAGE=" immutable_image
      next
    }
    { print }
  ' "$REPO_ROOT/.env.example" >"$bundle_root/.env.example"
  cp "$REPO_ROOT/compose.https.yaml" "$bundle_root/compose.https.yaml"
  cp "$REPO_ROOT/deploy/Caddyfile" "$bundle_root/deploy/Caddyfile"
  cp "$REPO_ROOT/docs/BACKUP-RESTORE.md" "$bundle_root/docs/BACKUP-RESTORE.md"
  cp "$REPO_ROOT/docs/ROSTER-IMPORT.md" "$bundle_root/docs/ROSTER-IMPORT.md"
  cp "$REPO_ROOT/docs/RUNBOOK.md" "$bundle_root/docs/RUNBOOK.md"
  cp "$REPO_ROOT/docs/TLS.md" "$bundle_root/docs/TLS.md"
  cp "$REPO_ROOT/release/allowed_signers" "$bundle_root/release/allowed_signers"
  cp "$REPO_ROOT/scripts/restore-compose.sh" "$bundle_root/scripts/restore-compose.sh"
  sed "s/VERSION/${FIXTURE_VERSION}/g" \
    "$REPO_ROOT/release/README.txt" >"$bundle_root/release/README.txt"
  awk -v immutable_image="docker.io/ghostframe/local-it-desk@${FIXTURE_IMAGE_DIGEST}" '
    /^    image: "\$\{LOCAL_IT_DESK_IMAGE:-/ {
      print "    image: \"${LOCAL_IT_DESK_IMAGE:-" immutable_image "}\""
      next
    }
    { print }
  ' "$REPO_ROOT/compose.yaml" >"$bundle_root/compose.yaml"
  printf '%s\n' '{"spdxVersion":"SPDX-2.3"}' \
    >"$bundle_root/release/sbom.spdx.json"
  printf '%s\n' '{}' >"$bundle_root/release/provenance.json"
  jq -n \
    --arg version "$FIXTURE_VERSION" \
    --arg source_sha "$FIXTURE_SOURCE_SHA" \
    --arg digest "$FIXTURE_IMAGE_DIGEST" \
    '{
      schema_version: 1,
      version: $version,
      source_sha: $source_sha,
      image: {
        repository: "docker.io/ghostframe/local-it-desk",
        digest: $digest,
        immutable_reference: ("docker.io/ghostframe/local-it-desk@" + $digest),
        platforms: ["linux/amd64", "linux/arm64"]
      }
    }' >"$bundle_root/release/release-metadata.json"
  (
    cd "$bundle_root" || exit
    find . -type f ! -name SHA256SUMS -print0 \
      | sort -z \
      | xargs -0 sha256sum
  ) >"$bundle_root/SHA256SUMS"
  tar --directory "$FIXTURE_ROOT" --create --gzip --file \
    "$FIXTURE_ROOT/assets/$bundle_name.tar.gz" "$bundle_name"
  (
    cd "$FIXTURE_ROOT/assets" || exit
    sha256sum "$bundle_name.tar.gz" >"$bundle_name.tar.gz.sha256"
  )
  cp "$bundle_root/release/sbom.spdx.json" \
    "$FIXTURE_ROOT/assets/$bundle_name.spdx.json"
  cp "$bundle_root/release/provenance.json" \
    "$FIXTURE_ROOT/assets/$bundle_name.provenance.json"
}

# Create deterministic GitHub and Docker Hub API responses.
build_provider_metadata() {
  jq -n --arg tag "$FIXTURE_TAG" --arg version "$FIXTURE_VERSION" '{
    tagName: $tag,
    isDraft: false,
    isPrerelease: false,
    assets: [
      {name: ("local-it-desk-" + $version + ".tar.gz")},
      {name: ("local-it-desk-" + $version + ".tar.gz.sha256")},
      {name: ("local-it-desk-" + $version + ".spdx.json")},
      {name: ("local-it-desk-" + $version + ".provenance.json")}
    ]
  }' >"$FIXTURE_ROOT/release.json"
  jq -n '{
    namespace: "ghostframe",
    name: "local-it-desk",
    is_private: false,
    immutable_tags_settings: {
      enabled: true,
      rules: ["^[0-9]+\\.[0-9]+\\.[0-9]+$"]
    }
  }' >"$FIXTURE_ROOT/repository.json"
  jq -n \
    --arg version "$FIXTURE_VERSION" \
    --arg digest "$FIXTURE_IMAGE_DIGEST" \
    '{
      name: $version,
      tag_status: "active",
      content_type: "image",
      digest: $digest,
      media_type: "application/vnd.oci.image.index.v1+json",
      images: [
        {os: "linux", architecture: "amd64", status: "active"},
        {os: "linux", architecture: "arm64", status: "active"}
      ]
    }' >"$FIXTURE_ROOT/tag.json"
  printf '%s\n' '{"token":"fixture-token"}' >"$FIXTURE_ROOT/auth.json"
}

# Install command shims that emulate GitHub, Git, and public registry reads.
build_mock_commands() {
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'if [[ "$1 $2" == "release view" ]]; then' \
    '  cat "$FIXTURE_ROOT/release.json"' \
    'elif [[ "$1 $2" == "release download" ]]; then' \
    '  while [[ "$#" -gt 0 ]]; do' \
    '    if [[ "$1" == "--dir" ]]; then shift; destination="$1"; fi' \
    '    shift' \
    '  done' \
    '  cp "$FIXTURE_ROOT/assets/"* "$destination/"' \
    'elif [[ "$1 $2" == "attestation verify" ]]; then' \
    '  [[ "${FIXTURE_FAIL_ATTESTATION:-0}" == 0 ]]' \
    '  printf "%s\n" "[{}]"' \
    'else' \
    '  exit 1' \
    'fi' >"$FIXTURE_ROOT/bin/gh"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'if [[ "$1" == "clone" ]]; then' \
    '  mkdir -p "${@: -1}"' \
    'elif [[ " $* " == *" cat-file -t "* ]]; then' \
    '  printf "%s\n" tag' \
    'elif [[ " $* " == *" verify-tag "* ]]; then' \
    '  [[ "${FIXTURE_FAIL_SIGNATURE:-0}" == 0 ]]' \
    'elif [[ " $* " == *" rev-parse "* ]]; then' \
    '  printf "%s\n" "$FIXTURE_SOURCE_SHA"' \
    'else' \
    '  exit 1' \
    'fi' >"$FIXTURE_ROOT/bin/git"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'destination=' \
    'url="${@: -1}"' \
    'while [[ "$#" -gt 0 ]]; do' \
    '  if [[ "$1" == "--output" ]]; then shift; destination="$1"; fi' \
    '  shift' \
    'done' \
    'case "$url" in' \
    '  */repositories/local-it-desk) source="$FIXTURE_ROOT/repository.json" ;;' \
    '  */tags/*) source="$FIXTURE_ROOT/tag.json" ;;' \
    '  *auth.docker.io*) source="$FIXTURE_ROOT/auth.json" ;;' \
    '  *registry-1.docker.io*) source="$FIXTURE_ROOT/image-index.json" ;;' \
    '  *) exit 1 ;;' \
    'esac' \
    'cp "$source" "$destination"' >"$FIXTURE_ROOT/bin/curl"
  chmod 0755 "$FIXTURE_ROOT/bin/gh" "$FIXTURE_ROOT/bin/git" "$FIXTURE_ROOT/bin/curl"
}

# Accept one internally consistent release across every trust boundary.
@test "accepts a signed, digest-bound, two-platform published release" {
  run bash "$VERIFIER" "$FIXTURE_TAG"
  [[ "$status" -eq 0 ]]
  [[ "$output" == *"Published release verified: $FIXTURE_TAG"* ]]
  [[ "$output" == *"Image: docker.io/ghostframe/local-it-desk@$FIXTURE_IMAGE_DIGEST"* ]]
}

# Reject a tag before any provider command runs when syntax is mutable or vague.
@test "rejects a tag that is not strict semantic versioning" {
  run bash "$VERIFIER" '0.1.0'
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'tag must be vMAJOR.MINOR.PATCH'* ]]
}

# Reject public release contents beyond the reviewed four-file contract.
@test "rejects an unexpected GitHub release asset" {
  jq '.assets += [{name: "unexpected.txt"}]' \
    "$FIXTURE_ROOT/release.json" >"$FIXTURE_ROOT/release.changed.json"
  mv "$FIXTURE_ROOT/release.changed.json" "$FIXTURE_ROOT/release.json"
  run bash "$VERIFIER" "$FIXTURE_TAG"
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'asset list differs from the four-file contract'* ]]
}

# Reject a release whose annotated tag does not match the trusted signer.
@test "rejects an untrusted release tag signature" {
  export FIXTURE_FAIL_SIGNATURE=1
  run bash "$VERIFIER" "$FIXTURE_TAG"
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'release tag signature is not trusted'* ]]
}

# Reject a nominal multi-platform release missing one supported architecture.
@test "rejects Docker Hub metadata without the ARM64 image" {
  jq '.images = [.images[] | select(.architecture != "arm64")]' \
    "$FIXTURE_ROOT/tag.json" >"$FIXTURE_ROOT/tag.changed.json"
  mv "$FIXTURE_ROOT/tag.changed.json" "$FIXTURE_ROOT/tag.json"
  run bash "$VERIFIER" "$FIXTURE_TAG"
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'target platforms do not match the bundle'* ]]
}

# Reject a provenance bundle that GitHub cannot bind to the release workflow.
@test "rejects invalid published image provenance" {
  export FIXTURE_FAIL_ATTESTATION=1
  run bash "$VERIFIER" "$FIXTURE_TAG"
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'provenance signature or identity is invalid'* ]]
}
