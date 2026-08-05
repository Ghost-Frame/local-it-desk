#!/usr/bin/env bash
# Exercises the appliance launcher against a deterministic fake container engine.
set -euo pipefail

# Repository root containing the launcher under test.
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly repo_root
# Private temporary workspace for launcher mutation and recorded calls.
contract_root="$(mktemp -d "${TMPDIR:-/tmp}/local-it-desk-contract.XXXXXX")"
readonly contract_root
# Fake release directory that protects the real checkout from test mutations.
readonly release_root="${contract_root}/release"
# Recorded fake-engine argv, one shell-escaped invocation per line.
readonly fake_log="${contract_root}/engine.log"
# File-backed set of images known to the fake engine.
readonly fake_images="${contract_root}/images"

# Removes only the exact temporary files created by this contract.
cleanup() {
  find "${contract_root}" -type f -delete 2>/dev/null || true
  find "${contract_root}" -depth -type d -empty -delete 2>/dev/null || true
}
trap cleanup EXIT

# Fails with one concise contract message.
fail() {
  printf 'Desk contract failed: %s\n' "$1" >&2
  exit 1
}

install -d "${release_root}/scripts" "${release_root}/deploy"
cp "${repo_root}/scripts/desk" "${release_root}/scripts/desk"
cp "${repo_root}/compose.yaml" "${repo_root}/compose.https.yaml" "${release_root}/"
cp "${repo_root}/deploy/Caddyfile" "${release_root}/deploy/Caddyfile"
cp "${repo_root}/Dockerfile" "${release_root}/Dockerfile"
: >"${fake_log}"
: >"${fake_images}"

# Fake container engine implementing only the reviewed launcher command surface.
cat >"${contract_root}/fake-engine" <<'FAKE_ENGINE'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >>"${FAKE_LOG}"
printf '\n' >>"${FAKE_LOG}"

if [[ "${1:-}" == 'compose' && "${2:-}" == 'version' ]]; then
  exit 0
fi
if [[ "${1:-}" == 'image' && "${2:-}" == 'inspect' ]]; then
  grep -Fxq -- "${3:-}" "${FAKE_IMAGES}"
  exit
fi
if [[ "${1:-}" == 'build' ]]; then
  while (( $# > 0 )); do
    if [[ "$1" == '--tag' ]]; then
      printf '%s\n' "$2" >>"${FAKE_IMAGES}"
      break
    fi
    shift
  done
  exit 0
fi
if [[ "${1:-}" == 'pull' ]]; then
  printf '%s\n' "${2:-}" >>"${FAKE_IMAGES}"
  exit 0
fi
if [[ "${1:-}" == 'inspect' ]]; then
  printf '%s\n' "${FAKE_HEALTH:-healthy}"
  exit 0
fi
if [[ "${1:-}" == 'version' ]]; then
  printf 'Fake Engine 1.0\n'
  exit 0
fi

joined=" $* "
if [[ "${joined}" == *' ps --quiet app '* ]]; then
  printf 'app-container\n'
elif [[ "${joined}" == *' ps --quiet caddy '* ]]; then
  printf 'caddy-container\n'
elif [[ "${joined}" == *' exec -T caddy cat /caddy-data/caddy/pki/authorities/local/root.crt '* ]]; then
  printf '%s\n' 'PUBLIC TEST CERTIFICATE'
elif [[ "${joined}" == *' cp app:/state/backups/'* ]]; then
  destination="${!#}"
  printf '%s\n' 'VERIFIED TEST BACKUP' >"${destination}"
elif [[ "${joined}" == *' logs --no-color --tail 200 app caddy '* ]]; then
  printf '%s\n' 'login fake.user@example.com token=do-not-ship 203.0.113.42'
else
  printf 'ok\n'
fi
FAKE_ENGINE
chmod +x "${contract_root}/fake-engine" "${release_root}/scripts/desk"

export CONTAINER_ENGINE="${contract_root}/fake-engine"
export FAKE_LOG="${fake_log}"
export FAKE_IMAGES="${fake_images}"
export DESK_HEALTH_ATTEMPTS=1

if (cd "${release_root}" && scripts/desk install --host 'https://bad/host') >/dev/null 2>&1; then
  fail 'install accepted a host containing scheme and path'
fi
[[ ! -e "${release_root}/.env" ]] || fail 'invalid install mutated .env'

(cd "${release_root}" && scripts/desk install --host schooldesk.local --name 'School IT Desk' --support 'Main office') >/dev/null
grep -Fxq 'HTTPS_HOST=schooldesk.local' "${release_root}/.env" || fail 'install did not persist the validated host'
grep -Fxq 'APP_NAME=School IT Desk' "${release_root}/.env" || fail 'install did not persist the desk name'
[[ "$(stat -c '%a' "${release_root}/.env")" == '600' ]] || fail '.env is not private'
find "${release_root}" -maxdepth 1 -name '.env.tmp.*' -print -quit | grep -q . \
  && fail 'install left a non-atomic temporary configuration'
grep -Fq -- '--project-name local-it-desk' "${fake_log}" || fail 'launcher did not pin its Compose project'
grep -Fq -- 'build --tag local-it-desk:0.2.0' "${fake_log}" || fail 'missing local image was not built'
[[ -s "${release_root}/exports/local-it-desk-root.crt" ]] || fail 'install did not export the public trust certificate'

(cd "${release_root}" && scripts/desk status) >/dev/null
(cd "${release_root}" && scripts/desk certificate) >/dev/null
(cd "${release_root}" && scripts/desk backup) >/dev/null
backup_path="$(find "${release_root}/backups" -maxdepth 1 -type f -name 'local-it-desk-*.tar.gz' -print -quit)"
[[ -n "${backup_path}" && -s "${backup_path}.sha256" ]] || fail 'backup did not export an archive and checksum'

(cd "${release_root}" && scripts/desk support) >/dev/null
support_archive="$(find "${release_root}/support" -maxdepth 1 -type f -name '*.tar.gz' -print -quit)"
[[ -n "${support_archive}" ]] || fail 'support did not create an archive'
if grep -REn 'fake.user|do-not-ship|203\.0\.113\.42' "${release_root}/support"/*/*.txt; then
  fail 'support output retained a test identity, token, or IP address'
fi

before_update="$(cat "${release_root}/.env")"
export FAKE_HEALTH=unhealthy
if (cd "${release_root}" && scripts/desk update registry.example.test/local-it-desk:1.2.3) >/dev/null 2>&1; then
  fail 'unhealthy update unexpectedly succeeded'
fi
[[ "$(cat "${release_root}/.env")" == "${before_update}" ]] || fail 'failed update did not restore the prior .env'
grep -Fq -- 'pull registry.example.test/local-it-desk:1.2.3' "${fake_log}" || fail 'versioned update was not downloaded'

if (cd "${release_root}" && scripts/desk update registry.example.test/local-it-desk:latest) >/dev/null 2>&1; then
  fail 'update accepted a floating image tag'
fi
if grep -En '(compose down|volume (rm|prune)|system prune|rm[[:space:]]+-r)' "${repo_root}/scripts/desk"; then
  fail 'launcher contains a broad destructive operation'
fi
# The literal PowerShell variables prove arguments remain positional at the WSL boundary.
# shellcheck disable=SC2016
grep -Fq '& wsl.exe --cd $RepositoryWslPath bash ./scripts/desk @DeskArguments' \
  "${repo_root}/scripts/desk.ps1" || fail 'PowerShell bridge does not pass launcher arguments positionally'

printf 'Desk launcher contract passed.\n'
