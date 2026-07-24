# Roadmap

This roadmap tracks the path from the current engineering scaffold to a usable Windows release.

## Milestone 1: Packet Spike

Completed scaffold:

- Parse IPv4/IPv6 TCP, UDP, ICMP, and common IPv6 extension headers.
- Reject fragmented packets until fragment reassembly/flow correlation is explicit.
- Convert raw packets into outbound/inbound five-tuple flow keys.
- Route parsed packets through `ServiceController` and record managed flows.
- Dynamically bind WinDivert `Recv`, `Send`, and `WinDivertHelperCalcChecksums`.

Remaining:

- Open long-running WinDivert Network, Flow, and Socket handles.
- Correlate five-tuples to PID and executable path.
- Build outbound source rewrite and inbound reverse mapping.
- Validate TCP, UDP, and QUIC on a real Ethernet + Wi-Fi machine.

## Milestone 2: Local Service

Completed scaffold:

- Windows service dispatcher for SCM start/stop.
- Service install, start, stop, and uninstall commands.
- Installer hooks that register and start the service.
- ACL-restricted named-pipe IPC for request/response UI commands.

Remaining:

- Add live status streaming so tray state follows the running service without polling.
- Add adapter enumeration through Windows APIs.
- Apply enable/pause/swap directly to the running backend once the packet loop is active.

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