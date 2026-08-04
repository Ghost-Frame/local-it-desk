#!/usr/bin/env bash
# Verifies tracked-file provenance, project license text, and dependency licenses.
set -euo pipefail

# Resolves the repository root independently of the caller's working directory.
repo_root="$(git rev-parse --show-toplevel)"
readonly repo_root
cd "${repo_root}"

readonly apache_license_sha256="074e6e32c86a4c0ef8b3ed25b721ca23aca83df277cd88106ef7177c354615ff"

# Fails when one required public provenance file is absent or empty.
require_nonempty_file() {
  local path="$1"
  if [[ ! -s "${path}" ]]; then
    printf 'Release rights check failed: missing or empty %s.\n' "${path}" >&2
    exit 1
  fi
}

for required_file in LICENSE NOTICE THIRD-PARTY-NOTICES.md docs/PROVENANCE.md; do
  require_nonempty_file "${required_file}"
done

actual_license_sha256="$(sha256sum LICENSE | awk '{print $1}')"
if [[ "${actual_license_sha256}" != "${apache_license_sha256}" ]]; then
  printf 'Release rights check failed: LICENSE is not the reviewed Apache-2.0 text.\n' >&2
  exit 1
fi

if git ls-files --stage | awk '$1 == "160000" { found = 1 } END { exit !found }'; then
  printf 'Release rights check failed: Git submodules are not classified for redistribution.\n' >&2
  exit 1
fi

# Includes tracked files plus public-bound untracked files during local preparation.
mapfile -t release_paths < <(git ls-files --cached --others --exclude-standard)

# Classifies every tracked path into one reviewed provenance category.
for tracked_path in "${release_paths[@]}"; do
  case "${tracked_path}" in
    LICENSE)
      classification="standard-license-text"
      ;;
    Cargo.lock|caddy/go.sum|frontend/pnpm-lock.yaml)
      classification="generated-dependency-metadata"
      ;;
    .dockerignore|.editorconfig|.env.example|.gitignore|CONTRIBUTING.md|Cargo.toml|Dockerfile|README.md|QUICKSTART.md|SECURITY.md|NOTICE|THIRD-PARTY-NOTICES.md|CHANGELOG.md|compose.yaml|compose.https.yaml|rust-toolchain.toml)
      classification="project-authored"
      ;;
    .github/dependabot.yml|.github/workflows/*.yml|caddy/go.mod|caddy/main.go|crates/server/Cargo.toml|crates/server/src/*.rs|crates/server/tests/*.rs|deploy/Caddyfile|docs/*.md|frontend/index.html|frontend/package.json|frontend/pnpm-workspace.yaml|frontend/tsconfig.json|frontend/tsconfig.test.json|frontend/vite.config.ts|frontend/src/*.css|frontend/src/*.ts|frontend/src/*.vue|frontend/tests/*.ts|release/*|scripts/desk|scripts/*.ps1|scripts/*.sh|scripts/tests/*.bats|scripts/tests/*.py|tests/e2e/local-only/*.md|tests/e2e/local-only/*.sh)
      classification="project-authored"
      ;;
    *)
      printf 'Release rights check failed: unclassified tracked path %s.\n' "${tracked_path}" >&2
      exit 1
      ;;
  esac

  mime_type="$(file --brief --mime-type "${tracked_path}")"
  case "${mime_type}" in
    text/*|application/javascript|application/x-javascript|application/json|application/toml|application/x-empty)
      ;;
    *)
      printf 'Release rights check failed: unreviewed non-text asset %s (%s, %s).\n' "${tracked_path}" "${mime_type}" "${classification}" >&2
      exit 1
      ;;
  esac
done

readonly -a allowed_cargo_licenses=(
  '(Apache-2.0 OR MIT) AND BSD-3-Clause'
  '(MIT OR Apache-2.0) AND Unicode-3.0'
  '0BSD OR MIT OR Apache-2.0'
  'Apache-2.0'
  'Apache-2.0 / MIT'
  'Apache-2.0 AND ISC'
  'Apache-2.0 OR BSL-1.0'
  'Apache-2.0 OR ISC OR MIT'
  'Apache-2.0 OR MIT'
  'Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT'
  'BSD-2-Clause OR Apache-2.0 OR MIT'
  'BSD-3-Clause'
  'CDLA-Permissive-2.0'
  'ISC'
  'MIT'
  'MIT AND BSD-3-Clause'
  'MIT OR Apache-2.0'
  'MIT OR Apache-2.0 OR LGPL-2.1-or-later'
  'MIT OR Apache-2.0 OR Zlib'
  'MIT OR Zlib OR Apache-2.0'
  'MIT/Apache-2.0'
  'Unicode-3.0'
  'Unlicense OR MIT'
  'Unlicense/MIT'
  'Zlib'
  'Zlib OR Apache-2.0 OR MIT'
)

# Returns success when an exact Cargo license expression was reviewed.
cargo_license_allowed() {
  local candidate="$1"
  local allowed
  for allowed in "${allowed_cargo_licenses[@]}"; do
    if [[ "${candidate}" == "${allowed}" ]]; then
      return 0
    fi
  done
  return 1
}

while IFS= read -r cargo_license; do
  if [[ "${cargo_license}" == "NONE" ]] || ! cargo_license_allowed "${cargo_license}"; then
    printf 'Release rights check failed: unreviewed Cargo license expression %s.\n' "${cargo_license}" >&2
    exit 1
  fi
done < <(cargo metadata --format-version 1 --locked | jq -r '[.packages[] | select(.source != null) | (.license // "NONE")] | unique[]')

while IFS= read -r frontend_license; do
  case "${frontend_license}" in
    0BSD|Apache-2.0|BSD-2-Clause|BSD-3-Clause|ISC|MIT|MPL-2.0)
      ;;
    *)
      printf 'Release rights check failed: unreviewed frontend license %s.\n' "${frontend_license}" >&2
      exit 1
      ;;
  esac
done < <(pnpm --dir frontend licenses list --json | jq -r 'keys[]')

readonly go_licenses_version="v2.0.1"
readonly caddy_build_tags="nobadger,nomysql,nopgx"
go_license_stderr="$(mktemp)"
readonly go_license_stderr
trap 'rm -f "${go_license_stderr}"' EXIT

if ! caddy_license_report="$({
  cd caddy
  GOFLAGS="-tags=${caddy_build_tags}" go run "github.com/google/go-licenses/v2@${go_licenses_version}" \
    report --ignore local-it-desk/caddy . 2>"${go_license_stderr}"
})"; then
  cat "${go_license_stderr}" >&2
  printf 'Release rights check failed: Caddy license discovery did not complete.\n' >&2
  exit 1
fi

# Rejects Caddy dependencies whose detected license was not explicitly reviewed.
while IFS=, read -r go_package _ go_license; do
  if [[ -z "${go_package}" ]]; then
    continue
  fi
  case "${go_license}" in
    Apache-2.0|BSD-2-Clause|BSD-3-Clause|CC0-1.0|MIT|OFL-1.1)
      ;;
    *)
      printf 'Release rights check failed: unreviewed Caddy license %s for %s.\n' "${go_license}" "${go_package}" >&2
      exit 1
      ;;
  esac
done <<< "${caddy_license_report}"

printf 'Release rights check passed for %s public-bound files.\n' "${#release_paths[@]}"
