#!/usr/bin/env bash
# Exercises clean install, update, backup, restore, rollback, and restart through Compose.
set -euo pipefail

# Repository root resolved from this script rather than the caller directory.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly repo_root
# Container CLI providing build, inspect, network, and Compose operations.
readonly container_engine="${CONTAINER_ENGINE:-docker}"
# Unique project name prevents reuse of developer or prior smoke state.
readonly compose_project="${SMOKE_PROJECT_NAME:-local-it-desk-smoke-$(id -u)-$$}"
# Configurable high loopback port used only by this isolated smoke project.
readonly smoke_http_port="${SMOKE_HTTP_PORT:-$((20000 + ($$ % 20000)))}"
# Exact browser origin shared by Compose and the HTTP journey client.
readonly base_url="http://127.0.0.1:${smoke_http_port}"
# Private retained directory for bounded diagnostics and journey identifiers.
evidence_dir="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-smoke.XXXXXX")"
readonly evidence_dir
# Initial image reference used before the update exercise.
readonly old_image="local-it-desk-smoke:0.1.0-${compose_project}"
# Replacement image reference used during the update exercise.
readonly new_image="local-it-desk-smoke:0.1.1-${compose_project}"
# Fixed verified archive name stored inside only this project state volume.
readonly backup_filename='local-it-desk-smoke-backup.tar.gz'
# Compose command constrained to this unique project and evaluation profile.
compose_command=(
  "${container_engine}" compose
  --project-name "${compose_project}"
  --file "${repo_root}/compose.yaml"
)
# Tracks whether Compose resources were created and should be stopped on exit.
compose_started='false'

# Prints one failure message and leaves the exit trap to collect evidence.
fail() {
  printf 'Local-only Compose smoke failed: %s\n' "$1" >&2
  exit 1
}

# Removes credential-shaped values from diagnostics before display or storage.
redact_stream() {
  sed -E \
    -e 's/(local_it_desk_session=)[^;[:space:]]+/\1[REDACTED]/gI' \
    -e 's/("?(csrf_token|temporary_password|password)"?[[:space:]]*[:=][[:space:]]*)[^, }]+/\1[REDACTED]/gI'
}

# Replaces exact regular HTTP evidence files with credential-redacted copies.
sanitize_evidence() {
  local evidence_path
  local sanitized_path
  while IFS= read -r -d '' evidence_path; do
    if [[ ! -f "${evidence_path}" || -L "${evidence_path}" ]]; then
      printf 'Unsafe evidence path refused: %s\n' "${evidence_path}" >&2
      return 1
    fi
    sanitized_path="$(mktemp "${evidence_dir}/sanitized.XXXXXX")"
    redact_stream <"${evidence_path}" >"${sanitized_path}"
    chmod 0600 "${sanitized_path}"
    mv "${sanitized_path}" "${evidence_path}"
  done < <(find "${evidence_dir}" -maxdepth 1 -type f \
    \( -name 'headers.*' -o -name 'body.*' -o -name 'attachment-response.*' \) -print0)
}

# Captures bounded failure evidence, then stops only this exact smoke project.
finish_smoke() {
  local exit_status="$?"
  if [[ "${compose_started}" == 'true' ]]; then
    if [[ "${exit_status}" -ne 0 ]]; then
      {
        printf 'Compose status for %s\n' "${compose_project}"
        "${compose_command[@]}" ps --all 2>&1 || true
        printf 'Last 120 bounded log lines\n'
        "${compose_command[@]}" logs --no-color --tail 120 2>&1 || true
      } | redact_stream | tee "${evidence_dir}/failure.log" >&2
    fi
    "${compose_command[@]}" stop app >/dev/null 2>&1 || true
  fi
  if ! sanitize_evidence; then
    printf 'Smoke evidence redaction failed.\n' >&2
    exit_status=1
  fi
  if [[ "${compose_started}" == 'true' ]]; then
    printf 'Smoke project retained for explicit inspection: %s\n' "${compose_project}"
  else
    printf 'Selected project left unchanged: %s\n' "${compose_project}"
  fi
  printf 'Smoke evidence retained at: %s\n' "${evidence_dir}"
  exit "${exit_status}"
}
trap finish_smoke EXIT

# Waits until the exact application container reports healthy.
wait_for_health() {
  local container_id
  local health_state='starting'
  container_id="$("${compose_command[@]}" ps --quiet app)"
  [[ -n "${container_id}" ]] || fail 'Compose did not create the application container'
  for _attempt in $(seq 1 60); do
    health_state="$("${container_engine}" inspect "${container_id}" --format '{{.State.Health.Status}}')"
    if [[ "${health_state}" == 'healthy' ]]; then
      return 0
    fi
    if [[ "${health_state}" == 'unhealthy' ]]; then
      fail 'application container became unhealthy'
    fi
    sleep 1
  done
  fail "application health timed out in state ${health_state}"
}

# Proves the application has only one internal network and no runtime egress.
assert_runtime_isolation() {
  local container_id
  local network_json
  local network_name
  local internal_state
  container_id="$("${compose_command[@]}" ps --quiet app)"
  network_json="$("${container_engine}" inspect "${container_id}" --format '{{json .NetworkSettings.Networks}}')"
  [[ "$(jq 'keys | length' <<<"${network_json}")" == '1' ]] \
    || fail 'application must attach to exactly one runtime network'
  network_name="$(jq -er 'keys[0]' <<<"${network_json}")"
  internal_state="$("${container_engine}" network inspect "${network_name}" --format '{{.Internal}}')"
  [[ "${internal_state}" == 'true' ]] \
    || fail "runtime network ${network_name} is not internal"
}

# Requires one named state volume owned by this unique Compose project.
assert_isolated_state_volume() {
  local container_id
  local state_volume
  container_id="$("${compose_command[@]}" ps --quiet app)"
  state_volume="$("${container_engine}" inspect "${container_id}" \
    --format '{{range .Mounts}}{{if eq .Destination "/state"}}{{.Name}}{{end}}{{end}}')"
  [[ -n "${state_volume}" ]] || fail 'application has no named /state volume'
  [[ "${state_volume}" == *"${compose_project//-/_}"* || "${state_volume}" == *"${compose_project}"* ]] \
    || fail "state volume ${state_volume} is not owned by smoke project ${compose_project}"
}

# Refuses any selected project that already owns Compose state or service objects.
assert_unused_project() {
  local existing_containers
  existing_containers="$("${compose_command[@]}" ps --all --quiet 2>/dev/null || true)"
  [[ -z "${existing_containers}" ]] \
    || fail "smoke project already has containers: ${compose_project}"
  if "${container_engine}" volume inspect "${compose_project}_desk-state" >/dev/null 2>&1; then
    fail "smoke project already has a state volume: ${compose_project}"
  fi
  if "${container_engine}" network inspect "${compose_project}_desk-internal" >/dev/null 2>&1; then
    fail "smoke project already has a network: ${compose_project}"
  fi
}

# Recreates only the application with one explicit image reference and verifies it.
activate_image() {
  local image_ref="$1"
  local container_id
  local actual_image
  export LOCAL_IT_DESK_IMAGE="${image_ref}"
  "${compose_command[@]}" up --detach --no-deps app >/dev/null
  wait_for_health
  container_id="$("${compose_command[@]}" ps --quiet app)"
  actual_image="$("${container_engine}" inspect "${container_id}" --format '{{.Config.Image}}')"
  [[ "${actual_image}" == "${image_ref}" || "${actual_image}" == "localhost/${image_ref}" ]] \
    || fail "application activated unexpected image ${actual_image}"
}

cd "${repo_root}"
for required_command in "${container_engine}" jq curl base64 sha256sum; do
  command -v "${required_command}" >/dev/null \
    || fail "required command is unavailable: ${required_command}"
done
"${container_engine}" compose version >/dev/null
[[ "${compose_project}" =~ ^[a-z0-9][a-z0-9_-]+$ ]] \
  || fail 'SMOKE_PROJECT_NAME must contain only lowercase letters, digits, underscores, and hyphens'
[[ "${smoke_http_port}" =~ ^[0-9]+$ ]] \
  || fail 'SMOKE_HTTP_PORT must be numeric'
[[ -x tests/e2e/local-only/verify.sh ]] \
  || fail 'local-only HTTP verifier is missing or not executable'
chmod 0700 "${evidence_dir}"
assert_unused_project

# Podman must emit Docker image metadata so the healthcheck survives the build.
build_options=()
if [[ "$(basename "${container_engine}")" == 'podman' ]]; then
  build_options+=(--format docker)
fi
"${container_engine}" build "${build_options[@]}" --tag "${old_image}" .

export COMPOSE_PROJECT_NAME="${compose_project}"
export HTTP_BIND_ADDRESS='127.0.0.1'
export HTTP_PORT="${smoke_http_port}"
export APP_ORIGIN="${base_url}"
export LOCAL_IT_DESK_IMAGE="${old_image}"
"${compose_command[@]}" up --detach app >/dev/null
compose_started='true'
wait_for_health
assert_runtime_isolation
assert_isolated_state_volume

tests/e2e/local-only/verify.sh seed "${base_url}" "${evidence_dir}"
"${compose_command[@]}" exec -T app \
  /app/local-it-desk-admin backup \
  --database /state/current/data/local-it-desk.db \
  --attachments /state/current/attachments \
  --branding /state/current/branding \
  --output "/state/backups/${backup_filename}"
"${compose_command[@]}" exec -T app \
  /app/local-it-desk-admin verify-backup \
  --archive "/state/backups/${backup_filename}"

"${container_engine}" tag "${old_image}" "${new_image}"
activate_image "${new_image}"
tests/e2e/local-only/verify.sh verify "${base_url}" "${evidence_dir}"
tests/e2e/local-only/verify.sh mutate "${base_url}" "${evidence_dir}"

CONTAINER_ENGINE="${container_engine}" \
COMPOSE_PROJECT_NAME="${compose_project}" \
LOCAL_IT_DESK_IMAGE="${new_image}" \
HTTP_BIND_ADDRESS='127.0.0.1' \
HTTP_PORT="${smoke_http_port}" \
APP_ORIGIN="${base_url}" \
  scripts/restore-compose.sh "${backup_filename}" --apply
wait_for_health
tests/e2e/local-only/verify.sh verify-restored "${base_url}" "${evidence_dir}"

activate_image "${old_image}"
tests/e2e/local-only/verify.sh verify-restored "${base_url}" "${evidence_dir}"
"${compose_command[@]}" stop app >/dev/null
"${compose_command[@]}" up --detach app >/dev/null
wait_for_health
tests/e2e/local-only/verify.sh verify-restored "${base_url}" "${evidence_dir}"

printf 'LOCAL_ONLY_SMOKE_OK\n'
