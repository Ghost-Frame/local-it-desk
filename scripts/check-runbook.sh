#!/usr/bin/env bash
# Validates operator documentation paths, safety language, and marked shell blocks.
set -euo pipefail

# Repository-relative documentation files governed by this contract.
readonly runbook_files=(
  README.md
  docs/RUNBOOK.md
  docs/TLS.md
  docs/BACKUP-RESTORE.md
)

# Shipped paths that operator documentation depends on.
readonly required_paths=(
  .env.example
  compose.yaml
  compose.https.yaml
  deploy/Caddyfile
  docs/ARCHITECTURE.md
  docs/EXCLUDED-SURFACES.md
  docs/ROSTER-IMPORT.md
  scripts/restore-compose.sh
  scripts/smoke-compose.sh
)

# Container CLI providing static Compose rendering.
readonly container_engine="${CONTAINER_ENGINE:-docker}"

# Fails with a concise runbook contract message.
fail() {
  printf 'Runbook contract failed: %s\n' "$1" >&2
  exit 1
}

# Resolves and verifies every local Markdown link in one source file.
check_local_links() {
  local source_file="$1"
  local source_dir
  local raw_link
  local target
  local candidate
  source_dir="$(dirname "$source_file")"
  while IFS= read -r raw_link; do
    target="${raw_link#](}"
    target="${target%%#*}"
    [[ -z "$target" || "$target" == http://* || "$target" == https://* || "$target" == mailto:* ]] && continue
    if [[ "$source_dir" == "." ]]; then
      candidate="$target"
    else
      candidate="$source_dir/$target"
    fi
    [[ -e "$candidate" ]] || fail "$source_file links to missing path $candidate"
  done < <(grep -Eo ']\([^)]+' "$source_file" || true)
}

# Extracts marked Bash blocks and checks each block without executing it.
check_shell_blocks() {
  local source_file="$1"
  local output_dir="$2"
  local in_block=0
  local block_count=0
  local block_file=''
  local line
  # The source and generated block paths are distinct; ShellCheck cannot infer that contract.
  # shellcheck disable=SC2094
  while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line" == '~~~sh runbook-check' ]]; then
      ((block_count += 1))
      block_file="$output_dir/$(basename "$source_file").$block_count.sh"
      printf '#!/usr/bin/env bash\nset -euo pipefail\n' > "$block_file"
      in_block=1
      continue
    fi
    if (( in_block == 1 )) && [[ "$line" == '~~~' ]]; then
      bash -n "$block_file" || fail "$source_file contains invalid shell in block $block_count"
      in_block=0
      block_file=''
      continue
    fi
    if (( in_block == 1 )); then
      printf '%s\n' "$line" >> "$block_file"
    fi
  done < "$source_file"
  (( in_block == 0 )) || fail "$source_file has an unterminated runbook-check block"
  (( block_count > 0 )) || fail "$source_file has no checked command blocks"
}

for path in "${runbook_files[@]}" "${required_paths[@]}"; do
  [[ -e "$path" ]] || fail "required path is missing: $path"
done

# Temporary extracted blocks contain syntax-check input only.
block_dir="$(mktemp -d)"
trap 'find "$block_dir" -type f -delete 2>/dev/null || true; rmdir "$block_dir" 2>/dev/null || true' EXIT

for file in "${runbook_files[@]}"; do
  check_local_links "$file"
  check_shell_blocks "$file" "$block_dir"
done

# Rejects release placeholders and commands with broad destructive scope.
if grep -nEi '(<[^>]+>|YOUR_|CHANGE_ME|REPLACE_ME|example\.com)' "${runbook_files[@]}"; then
  fail 'documentation contains a placeholder'
fi
if grep -nEi '(docker compose down|volume (rm|prune)|system prune|rm[[:space:]]+-rf|chmod[[:space:]]+777|--privileged|docker\.sock)' "${runbook_files[@]}"; then
  fail 'documentation contains a forbidden broad or privileged operation'
fi

# Required warnings and recovery subjects must remain explicit.
grep -Fqi 'Plain HTTP is for evaluation' README.md || fail 'README lacks the HTTP evaluation warning'
grep -Fqi 'Do not enter real staff credentials' README.md || fail 'README lacks the real-credential warning'
grep -Fq -- '--project-name local-it-desk-evaluation' README.md || fail 'README evaluation does not use an isolated project'
grep -Fq -- '--project-name local-it-desk-evaluation' docs/RUNBOOK.md || fail 'runbook evaluation does not use an isolated project'
grep -Fqi 'Only after a managed client completes' docs/TLS.md || fail 'TLS guide lacks the client-trust gate'
grep -Fqi 'not an accepted backup' docs/BACKUP-RESTORE.md || fail 'backup guide lacks the off-host acceptance gate'
grep -Fqi 'pre-restore safety backup' docs/RUNBOOK.md || fail 'runbook lacks the restore safety-backup contract'
grep -Fqi 'Administrator recovery' docs/RUNBOOK.md || fail 'runbook lacks administrator recovery'
grep -Fqi 'Image and data rollback' docs/RUNBOOK.md || fail 'runbook lacks rollback'
grep -Fqi 'immutable SHA-256 digest' docs/RUNBOOK.md || fail 'runbook lacks a digest-pinned update gate'
grep -Fqi 'Host migration and safe stop' docs/RUNBOOK.md || fail 'runbook lacks host migration'
grep -Fqi 'Sanitized support bundle' docs/RUNBOOK.md || fail 'runbook lacks sanitized diagnostics'

bash -n scripts/restore-compose.sh
bash -n scripts/smoke-compose.sh
"${container_engine}" compose --env-file .env.example -f compose.yaml config --quiet
"${container_engine}" compose --env-file .env.example -f compose.yaml -f compose.https.yaml config --quiet

printf 'Runbook contract passed: %s checked command block(s).\n' "$(find "$block_dir" -type f | wc -l)"
