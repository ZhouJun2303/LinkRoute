# Security Policy

LinkRoute is pre-release software. Do not rely on it for production isolation yet.

## Supported Versions

Only unreleased `main` builds are currently supported for security testing.

## Reporting Vulnerabilities

Open a private security advisory on GitHub when the repository is published. If private advisories are unavailable, contact the maintainer listed in the published repository.

Please include:

- LinkRoute version or commit SHA.
- Windows version and architecture.
- Whether WinDivert files were bundled or manually copied.
- Reproduction steps and expected impact.

## Security Boundaries

- LinkRoute does not decrypt TLS traffic.
- LinkRoute does not install a root certificate.
- Update packages must be checked against an Ed25519 manifest signature before installation.
- Unsigned local test manifests are accepted only by parser diagnostics and rejected by signature verification.
