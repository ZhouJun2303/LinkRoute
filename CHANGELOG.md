# Changelog

All notable changes to LinkRoute will be documented in this file.

## 0.1.0 - Unreleased

### Added

- Rust workspace with core, service, UI, and updater crates.
- Rule engine for application and domain routing precedence.
- DNS cache, flow table, adapter selection, and VPN/tunnel detection models.
- Service config persistence under `%ProgramData%\LinkRoute` with invalid-config backup.
- Versioned IPC request/response JSON helpers shared by service and UI.
- Service IPC command handler for status, config, enable, pause, swap, and diagnostics.
- Windows service dispatcher plus install, start, stop, and uninstall commands.
- Inno Setup hooks that register/start the service during install and stop/delete it during uninstall.
- Lightweight UI bridge that invokes the service command handler through request files.
- Native Win32 tray shell with status, diagnostics, enable/pause, swap, exit, and global hotkeys.
- WinDivert asset download, SHA-256 verification, installer packaging, DLL probing, and dynamic symbol loading.
- Release packaging for installer SHA-256, update manifest, and signature placeholder files.
- Ed25519 update manifest parsing and signature verification.
- Inno Setup installer and GitHub Actions CI/release workflows.

### Not Complete Yet

- Real packet rewrite/NAT loop.
- ACL-restricted named-pipe IPC and live service status streaming.
- Full rules/settings editor window.
- Real release signing key integration.