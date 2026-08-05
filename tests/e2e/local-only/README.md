# Local-only Compose smoke journey

This harness proves that a new Local IT Desk evaluation installation can
complete its core staff workflow without calling an outside service. It uses a
unique Compose project, a unique named state volume, a loopback-only HTTP port,
and a private temporary evidence directory. It never reuses the normal
development stack or its data.

Run from the repository root:

```sh
CONTAINER_ENGINE=docker bash scripts/smoke-compose.sh
```

For Podman with its Compose provider:

```sh
CONTAINER_ENGINE=podman bash scripts/smoke-compose.sh
```

The harness builds the release image before the observation window. It then
checks that the evaluation app has exactly one ordinary Compose network so
Docker can publish the loopback HTTP port. The separate HTTPS smoke verifies
the school topology: the application joins only the internal network, while
Caddy spans the internal and ingress networks and is the only published edge.

The journey covers first administrator setup, named requester creation, forced
password replacement, a ticket, attachment, public and internal comments,
resolution, an announcement, notification read state, backup verification,
image recreation, restore, image rollback, and stop-start recovery. Success
ends with this marker:

```text
LOCAL_ONLY_SMOKE_OK
```

`SMOKE_HTTP_PORT` selects a different loopback port when the default derived
port is unavailable. `SMOKE_PROJECT_NAME` may select an explicit unique project
name using lowercase letters, digits, underscores, and hyphens.

The script stops its application and edge containers but intentionally retains
the exact smoke project, state volume, image tags, and evidence directory. This
preserves the demonstrated backup, restored state, and quarantine generation
for direct inspection. Removal is a separate explicit operator decision after
those artifacts are no longer needed.

`verify.sh` is the focused HTTP client. Its `seed`, `verify`, `mutate`, and
`verify-restored` phases are orchestrated by the top-level script. Its embedded
credentials are deliberately test-only and must never be reused for a real
installation.
