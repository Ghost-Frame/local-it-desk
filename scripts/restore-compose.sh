#!/usr/bin/env bash
# Verifies and restores one archive inside the persistent state volume.
set -euo pipefail

# Container CLI providing Compose and inspect operations.
readonly container_engine="${CONTAINER_ENGINE:-docker}"
# Optional HTTPS override selection shared with the deployment runbook.
readonly use_https="${USE_HTTPS:-false}"
# Positional archive filename stored beneath the state volume backup directory.
readonly backup_filename="${1:-}"
# Explicit non-mutating or mutating restore mode.
readonly restore_mode="${2:-}"
# Fixed in-container archive location assembled only after filename validation.
archive_path=''
# Exact application container inspected before any stop operation.
app_container=''
# Named persistent volume mounted at the atomic state root.
state_volume=''
# Tracks whether the wrapper owes the application a restart on early exit.
app_stopped='false'
# Compose invocation assembled without shell word splitting.
compose_command=("${container_engine}" compose -f compose.yaml)

# Prints the accepted command forms and exits with a usage failure.
usage() {
  printf 'Usage: %s BACKUP_FILENAME --dry-run|--apply\n' "${0}" >&2
  printf 'Example: %s local-it-desk-20260731T120000Z.tar.gz --dry-run\n' "${0}" >&2
  exit 2
}

# Restarts only the application service when apply exits after stopping it.
# ShellCheck cannot see the indirect EXIT-trap invocation.
# shellcheck disable=SC2317,SC2329
restart_app_if_needed() {
  if [[ "${app_stopped}" == 'true' ]]; then
    printf 'Restore did not finish cleanly; restarting the application service.\n' >&2
    "${compose_command[@]}" up --detach app >/dev/null || true
  fi
}
trap restart_app_if_needed EXIT

if [[ "${use_https}" == 'true' ]]; then
  compose_command+=(-f compose.https.yaml)
elif [[ "${use_https}" != 'false' ]]; then
  printf 'USE_HTTPS must be true or false.\n' >&2
  exit 2
fi

if [[ -z "${backup_filename}" || -z "${restore_mode}" || $# -ne 2 ]]; then
  usage
fi
if [[ ! "${backup_filename}" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*\.tar\.gz$ ]]; then
  printf 'Backup filename must be one plain .tar.gz filename.\n' >&2
  exit 2
fi
if [[ "${backup_filename}" != "$(basename -- "${backup_filename}")" ]]; then
  printf 'Backup filename must not contain a directory path.\n' >&2
  exit 2
fi
if [[ "${restore_mode}" != '--dry-run' && "${restore_mode}" != '--apply' ]]; then
  usage
fi
archive_path="/state/backups/${backup_filename}"

app_container="$("${compose_command[@]}" ps --quiet app)"
if [[ -z "${app_container}" ]]; then
  printf 'The application container must be running before restore inspection.\n' >&2
  exit 1
fi
state_volume="$("${container_engine}" inspect "${app_container}" --format '{{range .Mounts}}{{if eq .Destination "/state"}}{{.Name}}{{end}}{{end}}')"
if [[ -z "${state_volume}" ]]; then
  printf 'The application container has no named volume mounted at /state.\n' >&2
  exit 1
fi

printf 'Restore target container: %s\n' "${app_container}"
printf 'Restore target volume: %s\n' "${state_volume}"
printf 'Restore archive: %s\n' "${archive_path}"
printf 'Active generation: /state/current\n'

"${compose_command[@]}" exec -T app \
  /app/local-it-desk-admin restore \
  --archive "${archive_path}" \
  --target-root /state/current \
  --dry-run

if [[ "${restore_mode}" == '--dry-run' ]]; then
  printf 'Dry-run finished; the application remained online and no state changed.\n'
  exit 0
fi

"${compose_command[@]}" stop app
app_stopped='true'
"${compose_command[@]}" run --rm --no-deps \
  --entrypoint /app/local-it-desk-admin app \
  restore \
  --archive "${archive_path}" \
  --target-root /state/current \
  --apply
"${compose_command[@]}" up --detach app >/dev/null

# Newly created application container ID used for the post-restore health gate.
restored_container="$("${compose_command[@]}" ps --quiet app)"
if [[ -z "${restored_container}" ]]; then
  printf 'Compose did not create the restored application container.\n' >&2
  exit 1
fi

# Last observed container health state for an actionable timeout message.
health_state='starting'
for _attempt in $(seq 1 60); do
  health_state="$("${container_engine}" inspect "${restored_container}" --format '{{.State.Health.Status}}')"
  if [[ "${health_state}" == 'healthy' ]]; then
    app_stopped='false'
    printf 'Restore completed and the application is healthy.\n'
    exit 0
  fi
  if [[ "${health_state}" == 'unhealthy' ]]; then
    printf 'Restored application became unhealthy. Previous data remains quarantined.\n' >&2
    exit 1
  fi
  sleep 1
done

printf 'Restored application health timed out with state: %s\n' "${health_state}" >&2
exit 1
