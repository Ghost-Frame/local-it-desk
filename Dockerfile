# syntax=docker/dockerfile:1

FROM node:24.4.1-bookworm-slim AS frontend-builder

ARG PNPM_VERSION=11.3.0
WORKDIR /workspace/frontend
RUN npm install --global "pnpm@${PNPM_VERSION}"
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm build

FROM rust:1.88.0-bookworm AS rust-builder

WORKDIR /workspace
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ ./crates/
RUN cargo build --release --locked --bins

FROM debian:12.11-slim AS runtime

LABEL org.opencontainers.image.title="Local IT Desk" \
      org.opencontainers.image.description="Local-network help desk for named staff accounts" \
      org.opencontainers.image.version="0.1.0"

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && apt-get clean \
    && find /var/lib/apt/lists -type f -delete \
    && groupadd --gid 10001 localdesk \
    && useradd --uid 10001 --gid 10001 --system --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin localdesk \
    && install -d -o root -g root -m 0755 /app \
    && install -d -o 10001 -g 10001 -m 0750 /data /attachments /branding /backups

COPY --from=rust-builder --chown=root:root /workspace/target/release/local-it-desk /app/local-it-desk
COPY --from=rust-builder --chown=root:root /workspace/target/release/local-it-desk-admin /app/local-it-desk-admin
COPY --from=rust-builder --chown=root:root /workspace/target/release/local-it-desk-healthcheck /app/local-it-desk-healthcheck
COPY --from=frontend-builder --chown=root:root /workspace/frontend/dist/ /app/frontend/

ENV LISTEN_ADDR="0.0.0.0:3000" \
    APP_ORIGIN="http://localhost:8080" \
    DATABASE_PATH="/data/local-it-desk.db" \
    UPLOAD_DIR="/attachments" \
    BRANDING_DIR="/branding" \
    SERVE_FRONTEND="true" \
    FRONTEND_DIR="/app/frontend" \
    HEALTHCHECK_ADDR="127.0.0.1:3000"

USER 10001:10001
EXPOSE 3000
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 CMD ["/app/local-it-desk-healthcheck"]
ENTRYPOINT ["/app/local-it-desk"]
