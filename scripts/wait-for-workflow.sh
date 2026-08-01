#!/usr/bin/env bash
# Resolves and watches one exact GitHub Actions run for a workflow and commit.
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  printf 'Usage: %s WORKFLOW COMMIT_SHA\n' "$0" >&2
  exit 2
fi

readonly workflow="$1"
readonly commit_sha="$2"
readonly timeout_seconds="${WORKFLOW_TIMEOUT_SECONDS:-1800}"
readonly poll_seconds="${WORKFLOW_POLL_SECONDS:-10}"
readonly deadline="$((SECONDS + timeout_seconds))"

if [[ ! "${commit_sha}" =~ ^[0-9a-f]{40}$ ]]; then
  printf 'Workflow wait failed: commit must be one full SHA.\n' >&2
  exit 2
fi

# Polls until GitHub indexes one unambiguous run for the exact workflow and SHA.
while (( SECONDS < deadline )); do
  runs_json="$(gh run list --workflow "${workflow}" --commit "${commit_sha}" --limit 20 --json databaseId,headSha,status,conclusion)"
  matching_count="$(jq --arg sha "${commit_sha}" '[.[] | select(.headSha == $sha)] | length' <<<"${runs_json}")"
  if [[ "${matching_count}" -gt 1 ]]; then
    printf 'Workflow wait failed: %s runs match %s and are ambiguous.\n' "${matching_count}" "${commit_sha}" >&2
    exit 1
  fi
  if [[ "${matching_count}" -eq 1 ]]; then
    run_id="$(jq --arg sha "${commit_sha}" -r '.[] | select(.headSha == $sha) | .databaseId' <<<"${runs_json}")"
    gh run watch "${run_id}" --exit-status
    exit 0
  fi
  sleep "${poll_seconds}"
done

printf 'Workflow wait failed: no %s run appeared for %s within %s seconds.\n' "${workflow}" "${commit_sha}" "${timeout_seconds}" >&2
exit 1
