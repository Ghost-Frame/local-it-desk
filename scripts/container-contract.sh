#!/usr/bin/env bash
# Builds and inspects the release image against the minimal non-root runtime contract.
set -euo pipefail

# Container CLI used for build, inspection, and the isolated runtime probe.
readonly container_engine="${CONTAINER_ENGINE:-docker}"
# Local verification tag replaced on each contract run.
readonly image_ref="${LOCAL_IT_DESK_VERIFY_IMAGE:-local-it-desk:verify}"
# Unique exact-name container owned by this contract invocation.
readonly container_name="local-it-desk-contract-$$"

# Stops only the exact temporary container created by this script.
cleanup() {
  "${container_engine}" stop --time 5 "${container_name}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Fails with one actionable message when a required release file is absent.
require_file() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    printf 'Missing required container file: %s\n' "${path}" >&2
    exit 1
  fi
}

require_file Dockerfile
require_file .dockerignore
require_file crates/server/src/bin/local-it-desk-healthcheck.rs

"${container_engine}" version >/dev/null
# Podman must emit Docker schema metadata so the declared healthcheck is retained.
build_options=()
if [[ "$(basename "${container_engine}")" == 'podman' ]]; then
  build_options+=(--format docker)
fi
"${container_engine}" build "${build_options[@]}" --tag "${image_ref}" .

# Runtime image metadata must identify a non-root process and explicit lifecycle contract.
runtime_user="$("${container_engine}" image inspect "${image_ref}" --format '{{.Config.User}}')"
if [[ -z "${runtime_user}" || "${runtime_user}" == "0" || "${runtime_user}" == "root" || "${runtime_user}" == 0:* ]]; then
  printf 'Runtime image must configure a non-root user; found %q.\n' "${runtime_user}" >&2
  exit 1
fi
[[ "$("${container_engine}" image inspect "${image_ref}" --format '{{json .Config.Entrypoint}}')" == '["/app/local-it-desk"]' ]]
[[ "$("${container_engine}" image inspect "${image_ref}" --format '{{.Config.StopSignal}}')" == 'SIGTERM' ]]
[[ "$("${container_engine}" image inspect "${image_ref}" --format '{{json .Config.ExposedPorts}}')" == *'3000/tcp'* ]]
[[ "$("${container_engine}" image inspect "${image_ref}" --format '{{json .Config.Healthcheck.Test}}')" == *'/app/local-it-desk-healthcheck'* ]]

# Runtime filesystem contains only release artifacts and grants writes to explicit data paths.
"${container_engine}" run --rm --entrypoint /bin/sh "${image_ref}" -eu -c '
  test "$(id -u)" -ne 0
  test -x /app/local-it-desk
  test -x /app/local-it-desk-admin
  test -x /app/local-it-desk-healthcheck
  test -s /app/frontend/index.html
  test ! -e /workspace
  test ! -e /app/Cargo.toml
  test ! -e /app/package.json
  test ! -e /root/.cargo
  test ! -e /root/.cache
  test -w /data
  test -w /attachments
  test -w /branding
  test -w /backups
  test ! -w /app
'

# The probe must fail closed when no readiness listener is reachable.
if "${container_engine}" run --rm \
  --entrypoint /app/local-it-desk-healthcheck \
  --env HEALTHCHECK_ADDR=127.0.0.1:1 \
  "${image_ref}" >/dev/null 2>&1; then
  printf 'Healthcheck unexpectedly accepted an unavailable endpoint.\n' >&2
  exit 1
fi

# Image configuration and history must not embed registry credentials or source remotes.
if "${container_engine}" image inspect "${image_ref}" | grep -Eqi '(github\.com|git@|https://[^" ]+@|password=|token=)'; then
  printf 'Image configuration contains a credential-like value or Git remote.\n' >&2
  exit 1
fi
if "${container_engine}" history --no-trunc "${image_ref}" | grep -Eqi '(password=|token=|authorization:)'; then
  printf 'Image history contains a credential-like build command.\n' >&2
  exit 1
fi

"${container_engine}" run --detach --rm \
  --name "${container_name}" \
  --publish 127.0.0.1::3000 \
  "${image_ref}" >/dev/null

# Waits for the in-image readiness probe to establish database and HTTP readiness.
health_state='starting'
for _attempt in $(seq 1 30); do
  health_state="$("${container_engine}" inspect "${container_name}" --format '{{.State.Health.Status}}')"
  if [[ "${health_state}" == 'healthy' ]]; then
    break
  fi
  if [[ "${health_state}" == 'unhealthy' ]]; then
    "${container_engine}" logs --tail 50 "${container_name}" >&2 || true
    exit 1
  fi
  sleep 1
done
if [[ "${health_state}" != 'healthy' ]]; then
  printf 'Container did not become healthy; last state: %s\n' "${health_state}" >&2
  "${container_engine}" logs --tail 50 "${container_name}" >&2 || true
  exit 1
fi

# Published evaluation port must serve both readiness JSON and the compiled browser shell.
host_port="$("${container_engine}" port "${container_name}" 3000/tcp | awk -F: 'NR == 1 { print $NF }')"
[[ -n "${host_port}" ]]
[[ "$(curl --fail --silent --show-error "http://127.0.0.1:${host_port}/health/ready")" == '{"status":"ready"}' ]]
curl --fail --silent --show-error "http://127.0.0.1:${host_port}/" | grep -Fq '<div id="app"></div>'

printf 'Container contract passed for %s.\n' "${image_ref}"
