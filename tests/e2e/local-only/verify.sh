#!/usr/bin/env bash
# Drives the named-account HTTP journey used by the isolated Compose smoke test.
set -euo pipefail

# Requested journey phase: seed, verify, mutate, or verify-restored.
readonly phase="${1:-}"
# Loopback application origin selected by the smoke orchestrator.
readonly base_url="${2:-}"
# Private host directory carrying test evidence and non-secret identifiers.
readonly state_dir="${3:-}"
# Deliberately test-only administrator username.
readonly admin_username='smoke.admin'
# Deliberately test-only administrator password.
readonly admin_password='Smoke administrator passphrase 2026!'
# Deliberately test-only requester username.
readonly requester_username='smoke.requester'
# Deliberately test-only requester replacement password.
readonly requester_password='Smoke requester passphrase 2026!'
# Stable original ticket title used across update and restore assertions.
readonly original_ticket_title='Library display has no image'
# Stable post-backup sentinel title that restore must remove.
readonly sentinel_ticket_title='Post-backup restore sentinel'
# Response status populated by request helpers.
response_status=''
# Response body populated by JSON request helpers.
response_body=''
# Optional rotated cookie populated by JSON request helpers.
response_cookie=''
# Authenticated cookie populated by the login helper.
auth_cookie=''
# Authenticated CSRF proof populated by the login helper.
auth_csrf=''

# Prints accepted client phases without exposing test credentials.
usage() {
  printf 'Usage: %s seed|verify|mutate|verify-restored BASE_URL STATE_DIR\n' "${0}" >&2
  exit 2
}

# Fails one assertion with a phase-scoped diagnostic.
fail() {
  printf 'Local-only journey failed during %s: %s\n' "${phase}" "$1" >&2
  exit 1
}

# Redacts authentication material from one JSON error response.
sanitized_response() {
  jq -c 'del(.csrf_token, .temporary_password, .password)' <<<"${response_body}" 2>/dev/null \
    || printf '{"error":"non-JSON response omitted"}'
}

# Sends one same-origin JSON request and retains its bounded private response.
json_request() {
  local method="$1"
  local path="$2"
  local payload="$3"
  local cookie="${4:-}"
  local csrf="${5:-}"
  local headers_path
  local body_path
  local curl_args
  headers_path="$(mktemp "${state_dir}/headers.XXXXXX")"
  body_path="$(mktemp "${state_dir}/body.XXXXXX")"
  curl_args=(
    --silent --show-error
    --connect-timeout 5 --max-time 30
    --request "${method}"
    --header "Origin: ${base_url}"
    --header 'Content-Type: application/json'
    --dump-header "${headers_path}"
    --output "${body_path}"
    --write-out '%{http_code}'
  )
  if [[ -n "${cookie}" ]]; then
    curl_args+=(--header "Cookie: ${cookie}")
  fi
  if [[ -n "${csrf}" ]]; then
    curl_args+=(--header "X-CSRF-Token: ${csrf}")
  fi
  if [[ "${method}" != 'GET' ]]; then
    curl_args+=(--data "${payload}")
  fi
  response_status="$(curl "${curl_args[@]}" "${base_url}${path}")"
  response_body="$(<"${body_path}")"
  response_cookie="$(awk '
    BEGIN { IGNORECASE = 1 }
    /^set-cookie:/ {
      sub(/\r$/, "")
      sub(/^[^:]+:[[:space:]]*/, "")
      split($0, parts, ";")
      print parts[1]
      exit
    }
  ' "${headers_path}")"
}

# Requires the most recent request to match one exact HTTP status.
expect_status() {
  local expected="$1"
  local context="$2"
  if [[ "${response_status}" != "${expected}" ]]; then
    printf '%s returned HTTP %s: %s\n' \
      "${context}" "${response_status}" "$(sanitized_response)" >&2
    exit 1
  fi
}

# Authenticates one known smoke account and retains its current session in memory.
authenticate() {
  local username="$1"
  local password="$2"
  local payload
  payload="$(jq -n --arg username "${username}" --arg password "${password}" \
    '{username:$username,password:$password}')"
  json_request POST /api/auth/login "${payload}"
  expect_status 200 "login for ${username}"
  auth_cookie="${response_cookie}"
  auth_csrf="$(jq -er '.csrf_token' <<<"${response_body}")"
  [[ -n "${auth_cookie}" ]] || fail 'login did not issue a session cookie'
}

# Downloads one authenticated attachment into a private evidence path.
download_attachment() {
  local attachment_id="$1"
  local cookie="$2"
  local output_path="$3"
  response_status="$(curl \
    --silent --show-error \
    --connect-timeout 5 --max-time 30 \
    --header "Origin: ${base_url}" \
    --header "Cookie: ${cookie}" \
    --output "${output_path}" \
    --write-out '%{http_code}' \
    "${base_url}/api/attachments/${attachment_id}")"
  [[ "${response_status}" == '200' ]] || fail "attachment download returned HTTP ${response_status}"
}

# Creates the complete first-run state through public HTTP contracts.
seed_journey() {
  local admin_cookie
  local admin_csrf
  local category_id
  local temporary_password
  local requester_cookie
  local requester_csrf
  local ticket_id
  local attachment_id
  local announcement_id
  local attachment_file
  local attachment_response
  local attachment_sha256

  json_request GET /api/setup/status '{}'
  expect_status 200 'setup status'
  jq -e '.setup_required == true' <<<"${response_body}" >/dev/null \
    || fail 'clean install did not require setup'

  json_request POST /api/setup "$(jq -n \
    --arg username "${admin_username}" \
    --arg display_name 'Vocational IT Teacher' \
    --arg password "${admin_password}" \
    '{username:$username,display_name:$display_name,password:$password}')"
  expect_status 200 'first administrator setup'
  admin_cookie="${response_cookie}"
  admin_csrf="$(jq -er '.csrf_token' <<<"${response_body}")"
  jq -e '.user.role == "administrator"' <<<"${response_body}" >/dev/null \
    || fail 'first account was not an administrator'

  json_request GET /api/config '{}' "${admin_cookie}"
  expect_status 200 'public configuration'
  category_id="$(jq -er '.default_category_id' <<<"${response_body}")"

  json_request POST /api/admin/users "$(jq -n \
    --arg username "${requester_username}" \
    --arg display_name 'Library Staff' \
    '{username:$username,display_name:$display_name,role:"requester",email:null}')" \
    "${admin_cookie}" "${admin_csrf}"
  expect_status 201 'named requester creation'
  temporary_password="$(jq -er '.temporary_password' <<<"${response_body}")"

  authenticate "${requester_username}" "${temporary_password}"
  json_request POST /api/auth/password "$(jq -n \
    --arg current_password "${temporary_password}" \
    --arg new_password "${requester_password}" \
    '{current_password:$current_password,new_password:$new_password}')" \
    "${auth_cookie}" "${auth_csrf}"
  expect_status 200 'forced requester password replacement'
  requester_cookie="${response_cookie}"
  requester_csrf="$(jq -er '.csrf_token' <<<"${response_body}")"

  json_request POST /api/tickets "$(jq -n \
    --arg title "${original_ticket_title}" \
    --arg category_id "${category_id}" \
    '{title:$title,description:"The circulation computer is on but the wall display reports no signal.",category_id:$category_id,priority:"high"}')" \
    "${requester_cookie}" "${requester_csrf}"
  expect_status 201 'requester ticket creation'
  ticket_id="$(jq -er '.id' <<<"${response_body}")"

  attachment_file="${state_dir}/smoke-attachment.png"
  printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=' \
    | base64 --decode >"${attachment_file}"
  attachment_sha256="$(sha256sum "${attachment_file}" | awk '{print $1}')"
  attachment_response="$(mktemp "${state_dir}/attachment-response.XXXXXX")"
  response_status="$(curl \
    --silent --show-error \
    --connect-timeout 5 --max-time 30 \
    --header "Origin: ${base_url}" \
    --header "Cookie: ${requester_cookie}" \
    --header "X-CSRF-Token: ${requester_csrf}" \
    --form 'parent_kind=ticket' \
    --form "parent_id=${ticket_id}" \
    --form "file=@${attachment_file};type=image/png;filename=display.png" \
    --output "${attachment_response}" \
    --write-out '%{http_code}' \
    "${base_url}/api/attachments")"
  [[ "${response_status}" == '201' ]] || fail "attachment upload returned HTTP ${response_status}"
  attachment_id="$(jq -er '.id' "${attachment_response}")"

  json_request POST "/api/tickets/${ticket_id}/comments" \
    '{"body":"I reseated the HDMI cable and the issue remains.","visibility":"public"}' \
    "${requester_cookie}" "${requester_csrf}"
  expect_status 201 'requester public comment'
  json_request POST "/api/tickets/${ticket_id}/comments" \
    '{"body":"Bring the spare display adapter before visiting the library.","visibility":"internal"}' \
    "${admin_cookie}" "${admin_csrf}"
  expect_status 201 'administrator internal comment'
  json_request PATCH "/api/tickets/${ticket_id}" \
    '{"status":"open","priority":"urgent"}' "${admin_cookie}" "${admin_csrf}"
  expect_status 200 'ticket open transition'
  json_request PATCH "/api/tickets/${ticket_id}" \
    '{"status":"resolved","priority":"urgent"}' "${admin_cookie}" "${admin_csrf}"
  expect_status 200 'ticket resolution'

  json_request POST /api/admin/announcements \
    '{"title":"Display service restored","body":"Library display service is available again.","is_pinned":true}' \
    "${admin_cookie}" "${admin_csrf}"
  expect_status 201 'announcement creation'
  announcement_id="$(jq -er '.id' <<<"${response_body}")"
  json_request POST "/api/admin/announcements/${announcement_id}/publish" '{}' \
    "${admin_cookie}" "${admin_csrf}"
  expect_status 200 'announcement publication'

  json_request GET /api/notifications '{}' "${requester_cookie}"
  expect_status 200 'requester notifications'
  for kind in ticket_created ticket_status_changed ticket_resolved announcement_published; do
    jq -e --arg kind "${kind}" 'any(.[]; .kind == $kind)' <<<"${response_body}" >/dev/null \
      || fail "missing requester notification ${kind}"
  done
  json_request POST /api/notifications/read-all '{}' "${requester_cookie}" "${requester_csrf}"
  expect_status 204 'notification read-all'

  jq -n \
    --arg category_id "${category_id}" \
    --arg ticket_id "${ticket_id}" \
    --arg attachment_id "${attachment_id}" \
    --arg attachment_sha256 "${attachment_sha256}" \
    --arg announcement_id "${announcement_id}" \
    '{category_id:$category_id,ticket_id:$ticket_id,attachment_id:$attachment_id,attachment_sha256:$attachment_sha256,announcement_id:$announcement_id,sentinel_id:null}' \
    >"${state_dir}/journey.json"
}

# Verifies durable accounts, records, visibility boundaries, and attachment bytes.
verify_journey() {
  local expect_restored="$1"
  local admin_cookie
  local requester_cookie
  local ticket_id
  local attachment_id
  local expected_sha256
  local announcement_id
  local sentinel_id
  local downloaded_file
  local actual_sha256

  [[ -s "${state_dir}/journey.json" ]] || fail 'journey metadata is missing'
  ticket_id="$(jq -er '.ticket_id' "${state_dir}/journey.json")"
  attachment_id="$(jq -er '.attachment_id' "${state_dir}/journey.json")"
  expected_sha256="$(jq -er '.attachment_sha256' "${state_dir}/journey.json")"
  announcement_id="$(jq -er '.announcement_id' "${state_dir}/journey.json")"

  authenticate "${admin_username}" "${admin_password}"
  admin_cookie="${auth_cookie}"
  json_request GET '/api/admin/users?page=1&page_size=100' '{}' "${admin_cookie}"
  expect_status 200 'administrator account list'
  jq -e --arg admin "${admin_username}" --arg requester "${requester_username}" \
    '.total == 2 and any(.items[]; .username == $admin and .role == "administrator") and any(.items[]; .username == $requester and .role == "requester")' \
    <<<"${response_body}" >/dev/null || fail 'named account roster did not persist'

  authenticate "${requester_username}" "${requester_password}"
  requester_cookie="${auth_cookie}"
  json_request GET "/api/tickets/${ticket_id}" '{}' "${requester_cookie}"
  expect_status 200 'ticket detail'
  jq -e --arg title "${original_ticket_title}" \
    '.title == $title and .status == "resolved" and .priority == "urgent"' \
    <<<"${response_body}" >/dev/null || fail 'ticket content or lifecycle did not persist'
  json_request GET "/api/tickets/${ticket_id}/comments" '{}' "${requester_cookie}"
  expect_status 200 'requester comment visibility'
  jq -e 'length == 1 and .[0].visibility == "public"' <<<"${response_body}" >/dev/null \
    || fail 'requester could see an internal comment or lost the public comment'

  authenticate "${admin_username}" "${admin_password}"
  admin_cookie="${auth_cookie}"
  json_request GET "/api/tickets/${ticket_id}/comments" '{}' "${admin_cookie}"
  expect_status 200 'administrator comment visibility'
  jq -e 'length == 2 and any(.[]; .visibility == "internal")' <<<"${response_body}" >/dev/null \
    || fail 'administrator comment history did not persist'

  authenticate "${requester_username}" "${requester_password}"
  requester_cookie="${auth_cookie}"
  json_request GET /api/announcements '{}' "${requester_cookie}"
  expect_status 200 'published announcement feed'
  jq -e --arg id "${announcement_id}" 'any(.[]; .id == $id and .state == "published")' \
    <<<"${response_body}" >/dev/null || fail 'published announcement did not persist'
  json_request GET /api/notifications/unread-count '{}' "${requester_cookie}"
  expect_status 200 'notification read state'
  jq -e '.count == 0' <<<"${response_body}" >/dev/null \
    || fail 'notification read state did not persist'

  downloaded_file="$(mktemp "${state_dir}/download.XXXXXX")"
  download_attachment "${attachment_id}" "${requester_cookie}" "${downloaded_file}"
  actual_sha256="$(sha256sum "${downloaded_file}" | awk '{print $1}')"
  [[ "${actual_sha256}" == "${expected_sha256}" ]] \
    || fail 'attachment checksum changed across the lifecycle'

  if [[ "${expect_restored}" == 'true' ]]; then
    sentinel_id="$(jq -er '.sentinel_id' "${state_dir}/journey.json")"
    json_request GET '/api/tickets?page_size=100' '{}' "${requester_cookie}"
    expect_status 200 'restored ticket list'
    jq -e --arg sentinel "${sentinel_id}" 'all(.items[]; .id != $sentinel)' \
      <<<"${response_body}" >/dev/null || fail 'post-backup sentinel survived restore'
  fi
}

# Creates one post-backup record that a successful restore must remove.
mutate_after_backup() {
  local category_id
  local sentinel_id
  local metadata_temp
  category_id="$(jq -er '.category_id' "${state_dir}/journey.json")"
  authenticate "${requester_username}" "${requester_password}"
  json_request POST /api/tickets "$(jq -n \
    --arg title "${sentinel_ticket_title}" \
    --arg category_id "${category_id}" \
    '{title:$title,description:"This record must disappear when the verified backup is restored.",category_id:$category_id,priority:"normal"}')" \
    "${auth_cookie}" "${auth_csrf}"
  expect_status 201 'post-backup sentinel creation'
  sentinel_id="$(jq -er '.id' <<<"${response_body}")"
  metadata_temp="$(mktemp "${state_dir}/journey.XXXXXX")"
  jq --arg sentinel_id "${sentinel_id}" '.sentinel_id = $sentinel_id' \
    "${state_dir}/journey.json" >"${metadata_temp}"
  mv "${metadata_temp}" "${state_dir}/journey.json"
}

if [[ $# -ne 3 || ! "${phase}" =~ ^(seed|verify|mutate|verify-restored)$ ]]; then
  usage
fi
if [[ ! "${base_url}" =~ ^http://127\.0\.0\.1:[0-9]+$ ]]; then
  printf 'BASE_URL must be an explicit loopback HTTP origin.\n' >&2
  exit 2
fi
mkdir -p "${state_dir}"
chmod 0700 "${state_dir}"
command -v curl >/dev/null
command -v jq >/dev/null
command -v base64 >/dev/null
command -v sha256sum >/dev/null

case "${phase}" in
  seed) seed_journey ;;
  verify) verify_journey false ;;
  mutate) mutate_after_backup ;;
  verify-restored) verify_journey true ;;
esac

printf 'Local-only HTTP phase passed: %s\n' "${phase}"
