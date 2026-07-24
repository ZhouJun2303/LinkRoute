# Changelog

All notable changes to LinkRoute will be documented in this file.

## 0.1.0 - Unreleased

### Added

- Rust workspace with core, service, UI, and updater crates.
- Rule engine for application and domain routing precedence.
- Safe Rust packet parser for IPv4/IPv6 TCP, UDP, ICMP, IPv6 extension headers, flow-key mapping, and endpoint rewrite helpers.
- ServiceController raw-packet routing decision path that records parsed packet flows.
- Service-side NAT table for outbound source rewrite mappings and inbound destination restoration.
- Packet processing outcome model for forwarded, dropped, and rewritten packets.
- DNS cache, flow table, adapter selection, and VPN/tunnel detection models.
- Windows `GetAdaptersAddresses` adapter enumeration with Ethernet/Wi-Fi classification, source-address selection, and virtual adapter filtering.
- Service config persistence under `%ProgramData%\LinkRoute` with invalid-config backup.
- Versioned IPC request/response JSON helpers shared by service and UI.
- Service IPC command handler for status, config, enable, pause, swap, and diagnostics.
- Windows service dispatcher plus install, start, stop, and uninstall commands.
- ACL-restricted named-pipe IPC transport for local UI-to-service requests.
- Inno Setup hooks that register/start the service during install and stop/delete it during uninstall.
- UI fallback bridge that invokes the service command handler through request files when the pipe is unavailable.
- Native Win32 tray shell with status, diagnostics, enable/pause, swap, exit, and global hotkeys.
- WinDivert asset download, SHA-256 verification, installer packaging, DLL probing, dynamic symbol loading, packet send/receive wrappers, and checksum helper binding.
- Release packaging for installer SHA-256, update manifest, and signature placeholder files.
- Ed25519 update manifest parsing and signature verification.
- Inno Setup installer and GitHub Actions CI/release workflows.

### Not Complete Yet

- Long-running WinDivert Network packet loop wired to `ServiceController::process_packet`.
- PID/executable correlation from WinDivert Flow/Socket layers.
- Live service status streaming.
- Full rules/settings editor window.
- Real release signing key integration.