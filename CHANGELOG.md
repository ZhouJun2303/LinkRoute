# Changelog

All notable changes to LinkRoute will be documented in this file.

## 0.1.0 - Unreleased

### Added

- Rust workspace with core, service, UI, and updater crates.
- Rule engine for application and domain routing precedence.
- DNS cache, flow table, adapter selection, and VPN/tunnel detection models.
- Service config persistence under `%ProgramData%\LinkRoute` with invalid-config backup.
- WinDivert asset download, SHA-256 verification, installer packaging, DLL probing, and dynamic symbol loading.
- Ed25519 update manifest parsing and signature verification.
- Inno Setup installer and GitHub Actions CI/release workflows.

### Not Complete Yet

- Real packet rewrite/NAT loop.
- Production native UI and tray controls.
- Signed release manifest generation.
- Real Windows service registration and IPC.
