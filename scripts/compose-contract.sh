#!/usr/bin/env bash
# Renders and rejects unsafe evaluation or school HTTPS Compose configurations.
set -euo pipefail

# Container CLI providing the Compose subcommand.
readonly container_engine="${CONTAINER_ENGINE:-docker}"

# Fails with one actionable message when a required deployment file is absent.
require_file() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    printf 'Missing required Compose file: %s\n' "${path}" >&2
    exit 1
  fi
}

# Evaluates one jq invariant against rendered Compose JSON.
assert_json() {
  local document="$1"
  local expression="$2"
  local failure_message="$3"
  if ! jq --exit-status "${expression}" >/dev/null <<<"${document}"; then
    printf 'Compose contract failed: %s\n' "${failure_message}" >&2
    exit 1
  fi
}

require_file compose.yaml
require_file compose.https.yaml
require_file deploy/Caddyfile
require_file .env.example
command -v jq >/dev/null
"${container_engine}" compose version >/dev/null

# Canonical evaluation and school configurations after interpolation and merge.
evaluation_json="$("${container_engine}" compose --env-file .env.example -f compose.yaml config --format json)"
school_json="$("${container_engine}" compose --env-file .env.example -f compose.yaml -f compose.https.yaml config --format json)"

assert_json "${evaluation_json}" '.services | keys == ["app", "caddy"]' 'evaluation profile must contain only the app and Caddy edge services'
assert_json "${evaluation_json}" '.services.app.image | test(":[0-9]+\\.[0-9]+\\.[0-9]+$")' 'application image must use a version tag'
assert_json "${evaluation_json}" '(.services.app.ports // []) | length == 0' 'evaluation profile must not publish the application port directly'
assert_json "${evaluation_json}" '.services.caddy.ports | length == 1 and .[0].target == 8080 and .[0].published == "8080"' 'evaluation profile must publish only HTTP port 8080 through Caddy'
assert_json "${evaluation_json}" '.networks["desk-internal"].internal == true and (.networks["desk-ingress"].internal // false) == false' 'deployment must separate the internal application network from the ingress network'
assert_json "${evaluation_json}" '.services.app.networks | keys == ["desk-internal"]' 'application must attach only to the internal network'
assert_json "${evaluation_json}" '(.services.caddy.networks | keys) == ["desk-ingress", "desk-internal"] and .services.caddy.networks["desk-ingress"].gw_priority == 1' 'Caddy must bridge internal traffic to the selected ingress gateway'
assert_json "${evaluation_json}" '.services.app.environment.COOKIE_SECURE == "false" and (.services.app.environment.APP_ORIGIN | startswith("http://"))' 'evaluation cookies and origin must use the explicit HTTP contract'
assert_json "${evaluation_json}" '.services.app.read_only == true and (.services.app.cap_drop | index("ALL")) and (.services.app.security_opt | index("no-new-privileges:true"))' 'application service must be read-only with all capabilities dropped and no-new-privileges'
assert_json "${evaluation_json}" '[.services.app.volumes[] | {source, target}] == [{"source":"desk-state","target":"/state"}]' 'one state volume must contain active, backup, staging, and quarantine generations'
assert_json "${evaluation_json}" '.volumes | keys == ["desk-state"]' 'only the atomic state volume may persist application data'
assert_json "${evaluation_json}" '.services.app.environment.DATABASE_PATH == "/state/current/data/local-it-desk.db" and .services.app.environment.UPLOAD_DIR == "/state/current/attachments" and .services.app.environment.BRANDING_DIR == "/state/current/branding"' 'application paths must resolve beneath the active current generation'
assert_json "${evaluation_json}" '.services.app.healthcheck.test | index("/app/local-it-desk-healthcheck")' 'application healthcheck is required'
assert_json "${evaluation_json}" '.services.app.logging.driver == "json-file" and .services.app.logging.options["max-size"] == "10m" and .services.app.logging.options["max-file"] == "3"' 'application logs must use cross-engine rotation'

assert_json "${school_json}" '.services | keys == ["app", "caddy"]' 'school profile must contain only app and Caddy'
assert_json "${school_json}" '(.services.app.ports // []) | length == 0' 'school profile must remove every application host port'
assert_json "${school_json}" '.services.app.environment.COOKIE_SECURE == "true" and (.services.app.environment.APP_ORIGIN | startswith("https://"))' 'school profile must use an HTTPS origin and secure cookies'
assert_json "${school_json}" '.services.caddy.image | test(":[0-9]+\\.[0-9]+\\.[0-9]+-alpine$")' 'Caddy image must use a versioned Alpine tag'
assert_json "${school_json}" '.services.caddy.user == "10001:10001" and .services.caddy.read_only == true and .services.caddy.cap_drop == ["ALL"] and .services.caddy.cap_add == ["NET_BIND_SERVICE"] and (.services.caddy.security_opt | index("no-new-privileges:true"))' 'Caddy must run non-root with a read-only hardened filesystem and only its executable file capability'
assert_json "${school_json}" '.services.caddy.environment.XDG_CONFIG_HOME == "/tmp/config" and .services.caddy.environment.XDG_DATA_HOME == "/tmp/data" and (.services.caddy.tmpfs | length == 1) and (.services.caddy.tmpfs[0] | startswith("/tmp:"))' 'Caddy writable state must stay beneath one cross-engine ephemeral tmpfs'
assert_json "${school_json}" '.services.caddy.ports | length == 1 and .[0].target == 8443 and .[0].published == "443"' 'school profile must publish only the configured HTTPS edge'
assert_json "${school_json}" '[.services.caddy.volumes[] | select(.type == "bind") | {target, read_only}] | sort_by(.target) == [{"target":"/certs","read_only":true},{"target":"/etc/caddy/Caddyfile","read_only":true}]' 'Caddy configuration and certificates must be read-only bind mounts'
assert_json "${school_json}" '.services.caddy.healthcheck.test | join(" ") | contains("http://127.0.0.1:8080/health/ready")' 'Caddy loopback proxy healthcheck is required'
assert_json "${school_json}" '[.services[] | (.privileged // false)] | all(. == false)' 'privileged mode is forbidden'
assert_json "${school_json}" '[.services[] | (.network_mode // "")] | all(. != "host")' 'host networking is forbidden'
assert_json "${school_json}" '[.services[].volumes[]?.source // ""] | all(contains("docker.sock") | not)' 'Docker socket mounts are forbidden'
assert_json "${school_json}" '[.services[] | has("healthcheck")] | all' 'every school service must have a healthcheck'

require_file scripts/restore-compose.sh
[[ -x scripts/restore-compose.sh ]]
bash -n scripts/restore-compose.sh
grep -Fq -- 'stop app' scripts/restore-compose.sh
grep -Fq -- '--target-root /state/current' scripts/restore-compose.sh
grep -Fq -- 'Destination "/state"' scripts/restore-compose.sh
if grep -Eq '(compose down|volume (rm|prune)|docker\.sock)' scripts/restore-compose.sh; then
  printf 'Compose contract failed: restore wrapper contains a forbidden broad or privileged operation.\n' >&2
  exit 1
fi

printf 'Compose contracts passed.\n'
