#!/usr/bin/env bash
# Rejects inherited product surfaces from candidate repository text.
set -euo pipefail

# Terms that identify excluded collaboration, remote-authentication, and desktop surfaces.
readonly forbidden_pattern='(/api/(channels|dms|documents|changelog|push-subscriptions|api-tokens|unread)|/ws([^[:alnum:]_]|$)|AuthCallback|Channel(Store|View|Message|Member)|DmThread|DirectMessage|DocumentVersion|ChangelogEntry|PushSubscription|ApiToken|OIDC|Authentik|Tauri|WebSocket|VitePWA|Workbox|modelcontextprotocol|async-openai|anthropic)'

# Files that explain or test the exclusion contract and may name excluded surfaces.
readonly allowed_pattern='^(docs/EXCLUDED-SURFACES\.md|crates/server/tests/router_contract\.rs|frontend/tests/surface-contract\.test\.ts|scripts/check-dependencies\.sh|scripts/check-forbidden-surfaces\.sh|Cargo\.lock|caddy/go\.(mod|sum)|frontend/pnpm-lock\.yaml)$'

# Exact deny-only route needed to keep the browser fallback from serving the retired socket path.
readonly allowed_deny_route='.route("/ws", any(not_found));'

# Tracks whether any candidate file violates the text-only or surface contract.
violations=0

# Inspects one candidate file and prints every matching line.
check_file() {
  local file="$1"
  if [[ "$file" =~ $allowed_pattern ]]; then
    return
  fi
  if [[ -s "$file" ]] && ! LC_ALL=C grep -Iq . "$file"; then
    printf '%s: binary files are not permitted in the foundation repository\n' "$file"
    violations=1
    return
  fi
  local matches
  matches="$(LC_ALL=C grep -nEi "$forbidden_pattern" "$file" | grep -vF "$allowed_deny_route" || true)"
  if [[ -n "$matches" ]]; then
    printf '%s\n' "$matches"
    violations=1
  fi
}

# Reads tracked and not-yet-committed candidate files without following ignored output.
while IFS= read -r -d '' file; do
  check_file "$file"
done < <(git ls-files --cached --others --exclude-standard -z)

if (( violations != 0 )); then
  printf 'Forbidden product surfaces found.\n' >&2
  exit 1
fi

printf 'Forbidden surface scan passed.\n'
