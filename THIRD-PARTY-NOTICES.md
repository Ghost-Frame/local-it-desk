# Third-Party Notices

Local IT Desk is distributed under the Apache License 2.0. The source tree does
not vendor third-party source code, fonts, images, or browser assets.

The compiled server and browser application include dependencies resolved by
`Cargo.lock` and `frontend/pnpm-lock.yaml`. Those dependencies remain under
their respective licenses. The reviewed dependency set uses these SPDX license
families and expressions:

- 0BSD
- Apache-2.0, including LLVM exception alternatives
- BSD-2-Clause and BSD-3-Clause
- BSL-1.0
- CDLA-Permissive-2.0
- ISC
- MIT
- MPL-2.0
- Unicode-3.0
- Unlicense
- Zlib

Some packages are offered under more than one of these licenses. Local IT Desk
uses the compatible permissive choice where the package offers a choice. Exact
package versions and license expressions are recorded by the lockfiles and the
software bill of materials attached to each release.

The container build also incorporates the official Debian, Rust, and Node.js
base images named in `Dockerfile`. Their operating-system packages and runtime
components retain their upstream licenses.

Run `bash scripts/check-release-rights.sh` after dependency changes. A new or
unknown license expression blocks release until it is reviewed and this notice
is updated.
