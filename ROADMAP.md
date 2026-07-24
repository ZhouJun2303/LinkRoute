# Roadmap

This roadmap tracks the path from the current engineering scaffold to a usable Windows release.

## Milestone 1: Packet Spike

- Open WinDivert Network, Flow, and Socket layers.
- Correlate five-tuples to PID and executable path.
- Parse IPv4/IPv6 TCP and UDP packet headers.
- Build outbound source rewrite and inbound reverse mapping.
- Validate TCP, UDP, and QUIC on a real Ethernet + Wi-Fi machine.

## Milestone 2: Local Service

- Install as a Windows service under `LocalSystem`.
- Add ACL-restricted named-pipe IPC for UI commands and status streaming.
- Implement enable, pause, swap, diagnostics, and config update operations.
- Add adapter enumeration through Windows APIs.

## Milestone 3: Native UI

- Replace the current UI scaffold with a native Win32 window.
- Add application rules, domain rules, settings, real-time rates, tray menu, and hotkeys.
- Support Simplified Chinese and English.

## Milestone 4: Release Hardening

- Generate real Ed25519 signatures in release automation.
- Add optional Authenticode signing.
- Add self-hosted Windows dual-NIC performance gates.
- Document driver/security warnings and troubleshooting.
