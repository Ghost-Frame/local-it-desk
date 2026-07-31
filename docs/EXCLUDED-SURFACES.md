# Excluded Surface Contract

Local IT Desk is a focused help desk for named staff accounts on a local
network. The repository verification scripts treat the following capabilities
as outside version 1.

## Excluded Product Capabilities

- AI inference, assistants, agents, embeddings, model-provider clients, and
  automated accounts
- MCP servers, tool integrations, and programmatic API tokens
- OIDC, hosted access gateways, external login callbacks, JWT bearer sessions,
  and provider-specific identity fields
- Channels, team chat, direct messages, presence, read cursors, and WebSocket
  event streams
- Assistant-backed documents and changelog publishing
- Web Push, VAPID keys, service workers, and offline application caching
- Tauri and other desktop or mobile packaging
- Anonymous tickets, public registration, shared logins, and student accounts
- Required email, cloud database, analytics, telemetry, or outbound services

## Excluded Deployment Coupling

The source, configuration examples, tests, images, and release bundles must not
contain organization names from the source application, private hostnames,
private network addresses, secret-store paths, production credentials, or
school data.

## Verification Rule

Explanatory references in this file are allowed because this file defines the
rejection contract. Executable source, manifests, routes, public types,
navigation, environment examples, and ordinary documentation must remain free
of the excluded surfaces. Verification reports the exact file and line for
every finding.
