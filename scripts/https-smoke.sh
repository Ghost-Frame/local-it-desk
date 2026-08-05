#!/usr/bin/env bash
# Verifies the hardened HTTPS Compose profile against one isolated local project.
set -euo pipefail

# Container CLI providing Compose and image inspection commands.
readonly container_engine="${CONTAINER_ENGINE:-docker}"
# Unique project name prevents reuse of operator or prior smoke state.
readonly compose_project="${HTTPS_SMOKE_PROJECT_NAME:-local-it-desk-https-smoke-$(id -u)-$$}"
# High loopback port avoids binding the production HTTPS port during verification.
readonly https_port="${HTTPS_SMOKE_PORT:-$((24000 + ($$ % 10000)))}"
# Exact image already built by the container contract.
readonly image_ref="${HTTPS_SMOKE_IMAGE:-local-it-desk:verify}"
# Loopback HTTPS origin used for every smoke request.
readonly base_url="https://localhost:${https_port}"
# Private retained evidence directory for the exported public trust root.
evidence_dir="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-https-smoke.XXXXXX")"
readonly evidence_dir
# Exact public root path copied from Caddy for trust verification.
readonly root_certificate="${evidence_dir}/local-it-desk-root.crt"
# Compose arguments shared by every lifecycle command.
readonly -a compose_args=(
  --project-name "${compose_project}"
  --env-file .env.example
  -f compose.yaml
  -f compose.https.yaml
)

# Stops only this exact smoke project and preserves its evidence for inspection.
finish_smoke() {
  local exit_code=$?
  if (( exit_code != 0 )); then
    "${container_engine}" compose "${compose_args[@]}" ps --all >&2 || true
    "${container_engine}" compose "${compose_args[@]}" logs --tail 100 >&2 || true
  fi
  "${container_engine}" compose "${compose_args[@]}" stop >/dev/null 2>&1 || true
  exit "${exit_code}"
}
trap finish_smoke EXIT

# Fails with one actionable HTTPS smoke message.
fail() {
  printf 'HTTPS Compose smoke failed: %s\n' "$1" >&2
  exit 1
}

command -v curl >/dev/null
command -v sha256sum >/dev/null
"${container_engine}" compose version >/dev/null
"${container_engine}" image inspect "${image_ref}" >/dev/null
[[ "${https_port}" =~ ^[0-9]+$ ]] || fail 'HTTPS_SMOKE_PORT must be numeric'
(( https_port >= 1024 && https_port <= 65535 )) || fail 'HTTPS_SMOKE_PORT must be between 1024 and 65535'

export LOCAL_IT_DESK_IMAGE="${image_ref}"
export HTTPS_BIND_ADDRESS='127.0.0.1'
export HTTPS_PORT="${https_port}"
export HTTPS_ORIGIN="${base_url}"
export HTTPS_HOST='localhost'

[[ -z "$("${container_engine}" compose "${compose_args[@]}" ps --all --quiet)" ]] \
  || fail "project ${compose_project} already contains containers"

"${container_engine}" compose "${compose_args[@]}" config --quiet
"${container_engine}" compose "${compose_args[@]}" up --detach --no-build

# Exact service container identifiers created by this smoke project.
app_container="$("${container_engine}" compose "${compose_args[@]}" ps --quiet app)"
readonly app_container
caddy_container="$("${container_engine}" compose "${compose_args[@]}" ps --quiet caddy)"
readonly caddy_container
[[ -n "${app_container}" && -n "${caddy_container}" ]] || fail 'Compose did not create both services'

# Last observed application and edge health states.
app_health='starting'
caddy_health='starting'
for _attempt in $(seq 1 60); do
  app_health="$("${container_engine}" inspect "${app_container}" --format '{{.State.Health.Status}}')"
  caddy_health="$("${container_engine}" inspect "${caddy_container}" --format '{{.State.Health.Status}}')"
  [[ "${app_health}" != 'unhealthy' ]] || fail 'application service became unhealthy'
  [[ "${caddy_health}" != 'unhealthy' ]] || fail 'Caddy service became unhealthy'
  if [[ "${app_health}" == 'healthy' && "${caddy_health}" == 'healthy' ]]; then
    break
  fi
  sleep 1
done
[[ "${app_health}" == 'healthy' && "${caddy_health}" == 'healthy' ]] \
  || fail "services did not become healthy: app=${app_health}, caddy=${caddy_health}"

# Public local CA material used by managed clients to trust this exact deployment.
"${container_engine}" compose "${compose_args[@]}" exec -T caddy \
  cat /caddy-data/caddy/pki/authorities/local/root.crt >"${root_certificate}"
[[ -s "${root_certificate}" ]] || fail 'Caddy did not create an exportable public root certificate'
root_fingerprint_before="$(sha256sum "${root_certificate}" | awk '{print $1}')"
readonly root_fingerprint_before

# Exact readiness payload returned through the TLS edge.
ready_body="$(curl --cacert "${root_certificate}" --fail --silent --show-error --http2 "${base_url}/health/ready")"
readonly ready_body
[[ "${ready_body}" == '{"status":"ready"}' ]] || fail 'TLS readiness payload was not exact'

# Response headers proving browser hardening survived the reverse proxy.
response_headers="$(curl --cacert "${root_certificate}" --fail --silent --show-error --http2 --head "${base_url}/")"
readonly response_headers
grep -Eqi '^HTTP/2 200' <<<"${response_headers}" || fail 'browser shell did not use HTTP/2 with status 200'
grep -Eqi '^content-security-policy:' <<<"${response_headers}" || fail 'Content-Security-Policy is absent'
grep -Eqi '^permissions-policy:' <<<"${response_headers}" || fail 'Permissions-Policy is absent'
grep -Eqi '^referrer-policy: no-referrer' <<<"${response_headers}" || fail 'Referrer-Policy is incorrect'
grep -Eqi '^x-content-type-options: nosniff' <<<"${response_headers}" || fail 'nosniff protection is absent'
grep -Eqi '^x-frame-options: DENY' <<<"${response_headers}" || fail 'frame denial is absent'

# Recreating only the TLS edge must retain the exact private CA in its named volume.
"${container_engine}" compose "${compose_args[@]}" up --detach --no-build --force-recreate caddy
recreated_caddy_container="$("${container_engine}" compose "${compose_args[@]}" ps --quiet caddy)"
readonly recreated_caddy_container
for _attempt in $(seq 1 30); do
  caddy_health="$("${container_engine}" inspect "${recreated_caddy_container}" --format '{{.State.Health.Status}}')"
  [[ "${caddy_health}" != 'unhealthy' ]] || fail 'recreated Caddy service became unhealthy'
  [[ "${caddy_health}" == 'healthy' ]] && break
  sleep 1
done
[[ "${caddy_health}" == 'healthy' ]] || fail 'recreated Caddy service did not become healthy'
root_fingerprint_after="$("${container_engine}" compose "${compose_args[@]}" exec -T caddy \
  sha256sum /caddy-data/caddy/pki/authorities/local/root.crt | awk '{print $1}')"
readonly root_fingerprint_after
[[ "${root_fingerprint_after}" == "${root_fingerprint_before}" ]] \
  || fail 'Caddy rotated its local CA during service recreation'

printf 'HTTPS Compose smoke passed for %s with image %s; public trust root retained at %s.\n' \
  "${compose_project}" "${image_ref}" "${root_certificate}"
