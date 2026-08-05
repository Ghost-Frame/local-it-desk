# Release checklist

This checklist is for maintainers publishing Local IT Desk. Administrators who
only install or update a school deployment should follow the
[runbook](RUNBOOK.md).

## One-time public repository setup

- Confirm `gh auth status` identifies the approved GitHub owner.
- Confirm `gh repo view Ghost-Frame/local-it-desk` returns not found. Stop if it
  returns an existing repository.
- Run the credential-injected Docker Hub inspection. Stop if it returns an
  existing repository:

  ```sh
  cred exec docker pat --env DOCKERHUB_PAT -- \
    bash scripts/dockerhub-repository.sh ghostframe local-it-desk inspect
  ```

- Create `Ghost-Frame/local-it-desk` as a public GitHub repository only after
  its complete fresh history passes the publication scan.
- Create `docker.io/ghostframe/local-it-desk` as a public repository, then
  enable immutable regular semantic-version tags:

  ```sh
  cred exec docker pat --env DOCKERHUB_PAT -- \
    bash scripts/dockerhub-repository.sh ghostframe local-it-desk create-public
  cred exec docker pat --env DOCKERHUB_PAT -- \
    bash scripts/dockerhub-repository.sh ghostframe local-it-desk set-semver-immutable
  ```

- Store `ghostframe` as the `DOCKERHUB_USERNAME` repository variable and inject
  `DOCKERHUB_TOKEN` through `cred`. Never print the token.
- Apply `release/github-branch-protection.json` to `main`. Read the protection
  back and verify strict `ci`, linear history, resolved conversations, no force
  pushes, no deletion, and pilot administrator enforcement disabled.

## Before each release

- Start from a clean `main` branch whose local head matches the public remote.
- Review every change since the previous release and confirm no private source,
  school-specific data, credentials, or AI features are present.
- Run all publication gates:

  ```sh
  cargo fmt --all -- --check
  cargo test --workspace --all-targets
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  RUSTDOCFLAGS='-D missing_docs' cargo doc --workspace --no-deps --document-private-items
  pnpm --dir frontend install --frozen-lockfile
  pnpm --dir frontend check
  bash scripts/check-release-rights.sh
  bash scripts/check-history.sh
  bash scripts/check-forbidden-surfaces.sh
  bash scripts/check-private-terms.sh
  bash scripts/compose-contract.sh
  bash scripts/check-runbook.sh
  bash scripts/rehearse-release.sh 0.2.1
  ```

- Replace `0.2.1` with the intended version. Never reuse, move, or delete a
  published version tag.
- Verify every release commit and tag uses the pinned SSH signing key in
  `release/allowed_signers`.

## After the release workflow

- Confirm the GitHub release contains the bundle, adjacent SHA-256 checksum,
  SPDX SBOM, and provenance metadata.
- Confirm the Docker manifest contains Linux AMD64 and Linux ARM64 and record
  the immutable manifest digest.
- Run `scripts/verify-release-bundle.sh` against the downloaded bundle.
- Install the downloaded bundle on a fresh host, then verify sign-in, ticket
  creation, assignment, status changes, backup, restore, rollback, restart, and
  `/healthz`.
- Record the release URL, image digest, workflow run, and verification result.
