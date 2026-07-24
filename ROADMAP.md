# Roadmap

This roadmap tracks the path from the current engineering scaffold to a usable Windows release.

## Milestone 1: Packet Spike

Completed scaffold:

- Parse IPv4/IPv6 TCP, UDP, ICMP, and common IPv6 extension headers.
- Reject fragmented packets until fragment reassembly/flow correlation is explicit.
- Convert raw packets into outbound/inbound five-tuple flow keys.
- Route parsed packets through `ServiceController` and record managed flows.
- Rewrite packet source/destination addresses and ports before WinDivert checksum recalculation.
- Maintain outbound NAT mappings and inbound reverse mappings for rewritten packets.
- Dynamically bind WinDivert `Recv`, `Send`, and `WinDivertHelperCalcChecksums`.
- Run packets through a tested packet pump boundary that forwards, drops, or recalculates checksums before reinjection.
- Resolve packet process context from a cache and from Windows IP Helper TCP/UDP owner-PID tables for application-rule matching.

Remaining:

- Wire the WinDivert Network packet pump into the SCM-hosted service lifecycle.
- Start the packet worker from the SCM-hosted service and add real dual-NIC validation.
- Validate TCP, UDP, and QUIC on a real Ethernet + Wi-Fi machine.

## Milestone 2: Local Service

Completed scaffold:

- Windows service dispatcher for SCM start/stop.
- Service install, start, stop, and uninstall commands.
- Installer hooks that register and start the service.
- ACL-restricted named-pipe IPC for request/response UI commands.
- SCM service controller ownership through a shared handle so IPC and packet processing can run concurrently.
- Windows adapter enumeration through `GetAdaptersAddresses`, including source-address selection and virtual adapter filtering.

Remaining:

- Add live status streaming so tray state follows the running service without polling.
- Apply enable/pause/swap directly to the running packet loop once the loop is active.

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