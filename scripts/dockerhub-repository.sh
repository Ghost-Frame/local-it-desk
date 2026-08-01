#!/usr/bin/env bash
# Manage the public Docker Hub repository used by Local IT Desk releases.

set -euo pipefail

readonly API_BASE="${DOCKERHUB_API_BASE:-https://hub.docker.com}"
readonly SEMVER_RULE='^[0-9]+\.[0-9]+\.[0-9]+$'
readonly DESCRIPTION='A self-hosted local-network IT help desk'

# Print command usage without including credential values.
usage() {
  printf 'Usage: %s <namespace> <repository> <inspect|create-public|set-semver-immutable>\n' "$0" >&2
}

# Stop with a stable error that never includes a remote response body.
fail() {
  printf 'ERROR: %s\n' "$1" >&2
  exit 1
}

# Remove credential-bearing temporary files created by this process.
cleanup() {
  rm -f -- "$AUTH_REQUEST_FILE" "$AUTH_RESPONSE_FILE" "$AUTH_CONFIG_FILE" \
    "$REQUEST_FILE" "$RESPONSE_FILE"
}

if [[ $# -ne 3 ]]; then
  usage
  exit 2
fi

readonly NAMESPACE="$1"
readonly REPOSITORY="$2"
readonly ACTION="$3"
readonly IDENTIFIER="${DOCKERHUB_IDENTIFIER:-$NAMESPACE}"

[[ "$NAMESPACE" =~ ^[a-z0-9]+([._-][a-z0-9]+)*$ ]] || fail 'namespace is invalid'
[[ "$REPOSITORY" =~ ^[a-z0-9]+([._-][a-z0-9]+)*$ ]] || fail 'repository is invalid'
case "$ACTION" in
  inspect | create-public | set-semver-immutable) ;;
  *)
    usage
    exit 2
    ;;
esac
[[ -n "${DOCKERHUB_PAT:-}" ]] || fail 'DOCKERHUB_PAT is required'

umask 077
AUTH_REQUEST_FILE="$(mktemp)"
AUTH_RESPONSE_FILE="$(mktemp)"
AUTH_CONFIG_FILE="$(mktemp)"
REQUEST_FILE="$(mktemp)"
RESPONSE_FILE="$(mktemp)"
trap cleanup EXIT HUP INT TERM

jq -n --arg identifier "$IDENTIFIER" \
  '{identifier: $identifier, secret: env.DOCKERHUB_PAT}' >"$AUTH_REQUEST_FILE"

auth_status="$({
  curl --silent --show-error \
    --output "$AUTH_RESPONSE_FILE" \
    --write-out '%{http_code}' \
    --header 'Content-Type: application/json' \
    --request POST \
    --data-binary "@$AUTH_REQUEST_FILE" \
    "$API_BASE/v2/auth/token"
} || true)"
[[ "$auth_status" == '200' ]] || fail "Docker Hub authentication failed with HTTP $auth_status"

access_token="$(jq -er '.access_token | select(type == "string" and length > 0)' "$AUTH_RESPONSE_FILE" 2>/dev/null)" \
  || fail 'Docker Hub authentication returned an invalid response'
printf 'header = "Authorization: Bearer %s"\n' "$access_token" >"$AUTH_CONFIG_FILE"
unset access_token DOCKERHUB_PAT
: >"$AUTH_REQUEST_FILE"
: >"$AUTH_RESPONSE_FILE"

HTTP_STATUS=''

# Call a Docker Hub endpoint and retain only its status and temporary body.
api_request() {
  local method="$1"
  local url="$2"
  local data_file="${3:-}"
  local curl_args=(
    --silent
    --show-error
    --output "$RESPONSE_FILE"
    --write-out '%{http_code}'
    --config "$AUTH_CONFIG_FILE"
    --request "$method"
  )

  : >"$RESPONSE_FILE"
  if [[ -n "$data_file" ]]; then
    curl_args+=(--header 'Content-Type: application/json' --data-binary "@$data_file")
  fi
  HTTP_STATUS="$(curl "${curl_args[@]}" "$url")" \
    || fail 'Docker Hub request could not be completed'
}

# Fetch the target repository into the shared response file.
read_repository() {
  api_request GET "$API_BASE/v2/namespaces/$NAMESPACE/repositories/$REPOSITORY"
}

# Require the response to identify the approved public repository exactly.
verify_public_repository() {
  [[ "$HTTP_STATUS" == '200' ]] || fail "repository verification failed with HTTP $HTTP_STATUS"
  jq -e \
    --arg namespace "$NAMESPACE" \
    --arg repository "$REPOSITORY" \
    '.namespace == $namespace and .name == $repository and .is_private == false' \
    "$RESPONSE_FILE" >/dev/null 2>&1 \
    || fail 'repository owner, name, or visibility does not match the approved target'
}

# Print only reviewed repository fields, never a raw provider response.
print_repository() {
  jq '{namespace, name, is_private, immutable_tags_settings}' "$RESPONSE_FILE"
}

# Inspect the repository without treating absence as a command failure.
inspect_repository() {
  read_repository
  if [[ "$HTTP_STATUS" == '404' ]]; then
    printf 'NOT_FOUND %s/%s\n' "$NAMESPACE" "$REPOSITORY"
    return
  fi
  verify_public_repository
  print_repository
}

# Create the approved public repository and verify the resulting resource.
create_public_repository() {
  read_repository
  if [[ "$HTTP_STATUS" == '200' ]]; then
    verify_public_repository
    print_repository
    return
  fi
  [[ "$HTTP_STATUS" == '404' ]] || fail "repository inspection failed with HTTP $HTTP_STATUS"

  jq -n \
    --arg name "$REPOSITORY" \
    --arg namespace "$NAMESPACE" \
    --arg description "$DESCRIPTION" \
    '{name: $name, namespace: $namespace, description: $description, registry: "docker.io", is_private: false}' \
    >"$REQUEST_FILE"
  api_request POST "$API_BASE/v2/namespaces/$NAMESPACE/repositories" "$REQUEST_FILE"
  [[ "$HTTP_STATUS" == '201' ]] || fail "repository creation failed with HTTP $HTTP_STATUS"

  read_repository
  verify_public_repository
  print_repository
}

# Enable the exact immutable semantic-version rule and verify it by reading back the repository.
set_semver_immutable() {
  read_repository
  verify_public_repository

  jq -n --arg rule "$SEMVER_RULE" \
    '{immutable_tags: true, immutable_tags_rules: [$rule]}' >"$REQUEST_FILE"
  api_request PATCH \
    "$API_BASE/v2/namespaces/$NAMESPACE/repositories/$REPOSITORY/immutabletags" \
    "$REQUEST_FILE"
  [[ "$HTTP_STATUS" == '200' ]] || fail "immutable-tag update failed with HTTP $HTTP_STATUS"

  read_repository
  verify_public_repository
  jq -e --arg rule "$SEMVER_RULE" \
    '.immutable_tags_settings.enabled == true and .immutable_tags_settings.rules == [$rule]' \
    "$RESPONSE_FILE" >/dev/null 2>&1 \
    || fail 'immutable-tag verification did not match the approved semantic-version rule'
  print_repository
}

case "$ACTION" in
  inspect) inspect_repository ;;
  create-public) create_public_repository ;;
  set-semver-immutable) set_semver_immutable ;;
esac
