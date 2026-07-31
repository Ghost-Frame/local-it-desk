#!/usr/bin/env bash
# Rejects private source-project, identity, network, and organization terms.
set -euo pipefail

# Private terms are split so the scanner does not match its own source.
readonly private_pattern='(Bay[- ]'"'Audio'"'[- ]'"'Video'"'|it[- ]desk[- ]'"'app'"'|synthe'"'os'"'|/home/'"'zan'"'|10\.50\.[0-9]{1,3}\.[0-9]{1,3}|172\.30\.[0-9]{1,3}\.[0-9]{1,3}|157\.180\.[0-9]{1,3}\.[0-9]{1,3})'

# Generated lockfiles and this pattern definition do not contain publishable prose.
readonly allowed_pattern='^(scripts/check-private-terms\.sh|Cargo\.lock|frontend/pnpm-lock\.yaml)$'

# Tracks whether any candidate file contains private data or binary content.
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
  if LC_ALL=C grep -nEi "$private_pattern" "$file"; then
    violations=1
  fi
}

# Reads tracked and not-yet-committed candidate files without following ignored output.
while IFS= read -r -d '' file; do
  check_file "$file"
done < <(git ls-files --cached --others --exclude-standard -z)

if (( violations != 0 )); then
  printf 'Private terms found.\n' >&2
  exit 1
fi

printf 'Private term scan passed.\n'
