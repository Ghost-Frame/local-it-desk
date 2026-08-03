#!/usr/bin/env bash
# Reject dependency families that are outside the Local IT Desk product boundary.

set -euo pipefail

# Dependency sources inspected before any build or release.
readonly dependency_files=(
  Cargo.toml
  Cargo.lock
  crates/server/Cargo.toml
  frontend/package.json
  frontend/pnpm-lock.yaml
)

# Verifies that the committed Caddy module graph matches downloaded module content.
(
  cd caddy
  go mod verify
)

# Direct Caddy modules are limited to the reviewed edge server and this local wrapper.
mapfile -t caddy_direct_modules < <(
  cd caddy
  go list -mod=readonly -m -f '{{if not .Indirect}}{{.Path}}{{end}}' all | sed '/^$/d' | sort
)
readonly -a expected_caddy_direct_modules=(
  github.com/caddyserver/caddy/v2
  local-it-desk/caddy
)
if [[ "${caddy_direct_modules[*]}" != "${expected_caddy_direct_modules[*]}" ]]; then
  printf 'unexpected direct Caddy module set: %s\n' "${caddy_direct_modules[*]}" >&2
  exit 1
fi

# Package-name fragments reserved for excluded identity, desktop, messaging, and AI surfaces.
readonly forbidden_pattern='@tauri-apps|vite-plugin-pwa|workbox-|openidconnect|jsonwebtoken|tungstenite|web-push|vapid|async-openai|anthropic|modelcontextprotocol|(^|[-_/])mcp([-_/]|$)'

# Returns success for a match, cleanly rejects no-match, and fails on scanner errors.
grep_matches() {
  local grep_status
  if grep --line-number --ignore-case --extended-regexp -- "$forbidden_pattern" "${dependency_files[@]}"; then
    return 0
  else
    grep_status="$?"
  fi
  if [[ "${grep_status}" -eq 1 ]]; then
    return 1
  fi
  printf 'Dependency boundary check failed: grep exited with status %s.\n' "${grep_status}" >&2
  exit 1
}

if grep_matches; then
  echo "excluded dependency found" >&2
  exit 1
fi

echo "dependency boundary clean"
