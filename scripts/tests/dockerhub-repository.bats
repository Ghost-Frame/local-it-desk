#!/usr/bin/env bats
# Verify Docker Hub repository administration without contacting Docker Hub.

# Prepare isolated credential and fixture paths for each test.
setup() {
  export HELPER="$BATS_TEST_DIRNAME/../dockerhub-repository.sh"
  export MOCK_SERVER="$BATS_TEST_DIRNAME/dockerhub_mock.py"
  export TEST_PAT='dckr_pat_fixture_secret_never_print'
  export TEST_BEARER='fixture_bearer_secret_never_print'
  export MOCK_PORT_FILE="$BATS_TEST_TMPDIR/port"
}

# Stop the fixture server after each test.
teardown() {
  if [[ -n "${MOCK_PID:-}" ]]; then
    kill "$MOCK_PID" 2>/dev/null || true
    wait "$MOCK_PID" 2>/dev/null || true
  fi
}

# Launch one Docker Hub API scenario on an ephemeral local port.
start_mock() {
  local scenario="$1"
  MOCK_SCENARIO="$scenario" \
    MOCK_EXPECTED_PAT="$TEST_PAT" \
    MOCK_BEARER_TOKEN="$TEST_BEARER" \
    python3 "$MOCK_SERVER" &
  MOCK_PID=$!
  for _ in {1..100}; do
    [[ -s "$MOCK_PORT_FILE" ]] && break
    sleep 0.02
  done
  [[ -s "$MOCK_PORT_FILE" ]]
  TEST_API_BASE="http://127.0.0.1:$(<"$MOCK_PORT_FILE")"
  export TEST_API_BASE
}

# Assert that neither fixture credential reached captured output.
assert_no_credentials() {
  [[ "$output" != *"$TEST_PAT"* ]]
  [[ "$output" != *"$TEST_BEARER"* ]]
}

@test "inspect reports a missing repository without exposing credentials" {
  start_mock not-found
  run env DOCKERHUB_PAT="$TEST_PAT" DOCKERHUB_API_BASE="$TEST_API_BASE" \
    bash "$HELPER" ghostframe local-it-desk inspect
  [[ "$status" -eq 0 ]]
  [[ "$output" == 'NOT_FOUND ghostframe/local-it-desk' ]]
  assert_no_credentials
}

@test "create-public creates and verifies a public repository" {
  start_mock create
  run env DOCKERHUB_PAT="$TEST_PAT" DOCKERHUB_API_BASE="$TEST_API_BASE" \
    bash "$HELPER" ghostframe local-it-desk create-public
  [[ "$status" -eq 0 ]]
  [[ "$(jq -r '.namespace + "/" + .name' <<<"$output")" == 'ghostframe/local-it-desk' ]]
  [[ "$(jq -r '.is_private' <<<"$output")" == 'false' ]]
  assert_no_credentials
}

@test "create-public rejects an existing private target" {
  start_mock private
  run env DOCKERHUB_PAT="$TEST_PAT" DOCKERHUB_API_BASE="$TEST_API_BASE" \
    bash "$HELPER" ghostframe local-it-desk create-public
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'owner, name, or visibility does not match'* ]]
  assert_no_credentials
}

@test "inspect rejects a response owned by another namespace" {
  start_mock wrong-owner
  run env DOCKERHUB_PAT="$TEST_PAT" DOCKERHUB_API_BASE="$TEST_API_BASE" \
    bash "$HELPER" ghostframe local-it-desk inspect
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'owner, name, or visibility does not match'* ]]
  assert_no_credentials
}

@test "set-semver-immutable writes and verifies the exact rule" {
  start_mock immutable
  run env DOCKERHUB_PAT="$TEST_PAT" DOCKERHUB_API_BASE="$TEST_API_BASE" \
    bash "$HELPER" ghostframe local-it-desk set-semver-immutable
  [[ "$status" -eq 0 ]]
  [[ "$(jq -r '.immutable_tags_settings.enabled' <<<"$output")" == 'true' ]]
  [[ "$(jq -r '.immutable_tags_settings.rules[0]' <<<"$output")" == '^[0-9]+\.[0-9]+\.[0-9]+$' ]]
  assert_no_credentials
}

@test "authentication failures do not relay provider response bodies" {
  start_mock leaky-auth
  run env DOCKERHUB_PAT="$TEST_PAT" DOCKERHUB_API_BASE="$TEST_API_BASE" \
    bash "$HELPER" ghostframe local-it-desk inspect
  [[ "$status" -ne 0 ]]
  [[ "$output" == *'authentication failed with HTTP 401'* ]]
  assert_no_credentials
}
