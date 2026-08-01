# Source and Asset Provenance

Local IT Desk uses a fresh public history created for this standalone product.
The project maintainer confirmed authority to license the project-authored
material under Apache License 2.0 before public release.

## Classification

- Project-authored material: application source, tests, scripts, workflows,
  configuration, and documentation are original work controlled by the project
  maintainer.
- Generated dependency metadata: `Cargo.lock` and
  `frontend/pnpm-lock.yaml` are generated dependency-resolution records. They
  identify upstream packages but do not vendor their source.
- Standard license text: `LICENSE` is the unmodified SPDX copy of the Apache
  License 2.0.
- Third-party dependencies: compiled dependencies and container base layers
  retain their upstream licenses. Their reviewed license families are listed
  in `THIRD-PARTY-NOTICES.md`, and release artifacts include an exact software
  bill of materials.
- Assets: the repository contains no copied photographs, logos, fonts, audio,
  videos, production screenshots, or school data.

No development database, account record, ticket, attachment, credential, or
operator-specific network configuration belongs in the source repository or a
release artifact.

## Verification

The release-rights gate enumerates every tracked path, rejects unknown path
classes and non-text assets, verifies the exact Apache license text, and checks
the complete Rust and frontend dependency license sets. The history, private
term, forbidden-surface, bundle, and image gates provide separate checks for
content that is not a licensing concern.
