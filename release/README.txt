Local IT Desk release bundle

This archive installs Local IT Desk from an immutable container image digest.
It contains operator configuration templates and documentation, not ticket data,
accounts, passwords, attachments, or application source code.

Before extraction, place the archive and its .sha256 file in an empty directory:

  sha256sum --check local-it-desk-VERSION.tar.gz.sha256
  tar --extract --gzip --file local-it-desk-VERSION.tar.gz
  cd local-it-desk-VERSION
  sha256sum --check SHA256SUMS

Replace VERSION with the downloaded release version. Then read docs/RUNBOOK.md
and docs/TLS.md completely. Use plain HTTP only for a throwaway evaluation.
Configure trusted HTTPS before creating real staff accounts or entering school
support information.

The release metadata under release/release-metadata.json records the source
commit, image digest, and supported Linux architectures. The SPDX bill of
materials and signed provenance bundle are stored beside it.
