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
- Replace the temporary process bridge with ACL-restricted named-pipe IPC.
- Add live status streaming so tray state follows the running service.
- Add adapter enumeration through Windows APIs.

## Milestone 3: Native UI

- Expand the basic Win32 tray shell into a compact native window.
- Add application rules, domain rules, settings, and real-time rates.
- Keep tray menu and global hotkeys small, fast, and always available.
- Support Simplified Chinese and English.

## Milestone 4: Release Hardening

- Generate real Ed25519 signatures in release automation.
- Add optional Authenticode signing.
- Add self-hosted Windows dual-NIC performance gates.
- Document driver/security warnings and troubleshooting.