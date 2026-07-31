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

# Package-name fragments reserved for excluded identity, desktop, messaging, and AI surfaces.
readonly forbidden_pattern='@tauri-apps|vite-plugin-pwa|workbox-|openidconnect|jsonwebtoken|tungstenite|web-push|vapid|async-openai|anthropic|modelcontextprotocol|(^|[-_/])mcp([-_/]|$)'

if rg --line-number --ignore-case --regexp "$forbidden_pattern" "${dependency_files[@]}"; then
  echo "excluded dependency found" >&2
  exit 1
fi

echo "dependency boundary clean"
