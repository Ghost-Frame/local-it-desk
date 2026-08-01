#!/usr/bin/env bash
# Rejects private metadata, prohibited paths, large blobs, and secrets across Git history.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
readonly repo_root
cd "${repo_root}"

if [[ "$(git rev-parse --is-shallow-repository)" != "false" ]]; then
  printf 'History check failed: a complete non-shallow history is required.\n' >&2
  exit 1
fi

# Rejects files that may contain operator instructions, credentials, or private state.
while IFS= read -r historical_path; do
  [[ -z "${historical_path}" ]] && continue
  case "${historical_path}" in
    .env.example)
      ;;
    .env|.env.*|*/.env|*/.env.*|CLAUDE.md|*/CLAUDE.md|GEMINI.md|*/GEMINI.md|AGENTS.md|*/AGENTS.md|GROWTH.md|*/GROWTH.md|.claude/*|*/.claude/*|migration-data/*|*/migration-data/*|*.pem|*.key|*/id_rsa*|*/id_ed25519*|credentials.json|*/credentials.json|secrets.*|*/secrets.*)
      printf 'History check failed: prohibited path %s.\n' "${historical_path}" >&2
      exit 1
      ;;
  esac
done < <(git log --all --name-only --pretty=format: | sort -u)

# Private metadata terms are assembled from fragments to avoid publishing the protected values.
readonly private_metadata_pattern='gir'"'box'"'\.org|/home/'"'zan'"'|10\.50\.[0-9]{1,3}\.[0-9]{1,3}|172\.30\.[0-9]{1,3}\.[0-9]{1,3}|157\.180\.[0-9]{1,3}\.[0-9]{1,3}|hetzner-'"'dedi'"'|hetzner-'"'zan'"'|cachyOS-'"'msi'"'|Invader '"'Zim'"'|agent-'"'forge'"'|kleos-'"'cli'"''

# Returns success for a match, cleanly rejects no-match, and fails on scanner errors.
grep_private_metadata() {
  local grep_status
  if grep --ignore-case --extended-regexp -- "${private_metadata_pattern}"; then
    return 0
  else
    grep_status="$?"
  fi
  if [[ "${grep_status}" -eq 1 ]]; then
    return 1
  fi
  printf 'History check failed: grep exited with status %s.\n' "${grep_status}" >&2
  exit 1
}

commit_metadata="$(git log --all --format='%H %an <%ae> %cn <%ce> %B')"
readonly commit_metadata
if grep_private_metadata <<<"${commit_metadata}"; then
  printf 'History check failed: private commit metadata or message found.\n' >&2
  exit 1
fi

readonly secret_pattern='eg_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|glpat-[A-Za-z0-9_-]{20,}|BEGIN [A-Z ]*PRIVATE KEY|authorization:[[:space:]]*(bearer|basic)[[:space:]]+[A-Za-z0-9._~+/=-]+'
# Private content terms are assembled from fragments to avoid publishing the protected values.
readonly content_private_pattern='Bay[- ]'"'Audio'"'[- ]'"'Video'"'|it-desk-'"'app'"'|synthe'"'os'"'|/home/'"'zan'"'|10\.50\.[0-9]{1,3}\.[0-9]{1,3}|172\.30\.[0-9]{1,3}\.[0-9]{1,3}|157\.180\.[0-9]{1,3}\.[0-9]{1,3}|gir'"'box'"'\.org|hetzner-'"'dedi'"'|hetzner-'"'zan'"'|cachyOS-'"'msi'"'|Invader '"'Zim'"'|agent-'"'forge'"'|kleos-'"'cli'"''

# Scans each historical tree while excluding the scanners that define the patterns.
while IFS= read -r commit_sha; do
  if git grep --line-number --ignore-case --extended-regexp "${secret_pattern}" "${commit_sha}" -- . \
    ':(exclude)scripts/check-history.sh'; then
    printf 'History check failed: credential-shaped content found in %s.\n' "${commit_sha}" >&2
    exit 1
  fi
  if git grep --line-number --ignore-case --extended-regexp "${content_private_pattern}" "${commit_sha}" -- . \
    ':(exclude)scripts/check-history.sh' \
    ':(exclude)scripts/check-private-terms.sh'; then
    printf 'History check failed: private content found in %s.\n' "${commit_sha}" >&2
    exit 1
  fi
done < <(git rev-list --all)

# Blocks large historical blobs because this source-only project has no reviewed binary assets.
while read -r object_sha object_type object_size object_path; do
  if [[ "${object_type}" == "blob" && "${object_size}" -gt 1048576 ]]; then
    printf 'History check failed: blob %s is %s bytes at %s.\n' "${object_sha}" "${object_size}" "${object_path}" >&2
    exit 1
  fi
done < <(git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(objectsize) %(rest)')

printf 'History check passed across %s commits.\n' "$(git rev-list --all --count)"
