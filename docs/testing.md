# Testing Plan

## Automated

- Rule precedence and longest domain match.
- IPv4/IPv6 TCP and UDP packet parsing.
- IPv6 extension-header traversal before transport parsing.
- Fragmented IPv4/IPv6 rejection until fragment handling is explicit.
- Packet-direction to flow-key mapping for outbound and inbound traffic.
- Raw packet routing through `ServiceController::decide_packet`.
- Service packet processing through `ServiceController::process_packet`.
- Packet source/destination rewrite and checksum-field invalidation.
- NAT table original/translated indexes, replacement, idle purge, outbound rewrite, inbound restore, and missing source-address drop behavior.
- WinDivert address wrapper layout, direction/IP-version flag decoding, and packet pump forwarding/drop/checksum behavior.
- Windows adapter kind mapping and virtual adapter filtering.
- DNS A/AAAA/CNAME cache TTL handling.
- Flow table reverse lookup, idle purge, and managed-flow clearing.
- Physical adapter filtering and VPN/tunnel detection.
- Exact versus suffix domain behavior.
- Disabled rules and disabled global routing.
- Adapter unavailable fail-closed behavior.
- Swap behavior.
- Duplicate rule detection.
- ConfigStore missing-file defaulting, TOML roundtrip, invalid-file backup, and validation errors.
- IPC request/response JSON roundtrips.
- Service IPC command handler for status, config, enable, pause, swap, and diagnostics.
- Service install command string formatting.
- UI bridge path resolution and status summary formatting.
- Update manifest parsing and signature verification.

## Manual Adapter Check

Run the latest service code without installing it:

```powershell
cargo run -p linkroute_service -- --status-json
```

Expected result:

- `adapters` lists Windows adapters returned by `GetAdaptersAddresses`.
- Active physical Ethernet/Wi-Fi adapters have `outlet` set to `ethernet` or `wifi`.
- VPN/tunnel adapters are visible but not outlet candidates.
- `vEthernet`, WSL, Docker, VMware, VirtualBox, Npcap, Wi-Fi Direct virtual adapters, and loopback adapters are not outlet candidates.

## Manual IPC Checks

Validate process fallback:

```powershell
.\target\debug\linkroute-ui.exe --status-json
```

Validate named-pipe transport:

```powershell
$serverOut = 'D:\tmp\linkroute-pipe-server.out'
$serverErr = 'D:\tmp\linkroute-pipe-server.err'
$server = Start-Process -FilePath .\target\debug\linkroute-service.exe -ArgumentList --serve-pipe-once -WindowStyle Hidden -PassThru -RedirectStandardOutput $serverOut -RedirectStandardError $serverErr
Start-Sleep -Milliseconds 500
.\target\debug\linkroute-ui.exe --status-json
$server.WaitForExit(15000)
Get-Content -LiteralPath $serverOut
```

The named-pipe status response should include `"backend_running":true`; fallback status usually shows `false` because it is handled by a short-lived command process.

## Manual Service Checks

Run from an elevated shell on a disposable test machine:

```powershell
.\target\release\linkroute-service.exe --install-service
.\target\release\linkroute-service.exe --start-service
sc.exe query LinkRoute
.\target\release\linkroute-service.exe --stop-service
.\target\release\linkroute-service.exe --uninstall-service
```

## Manual UI Checks

- `linkroute-ui --status-json` returns the same protocol shape as `linkroute-service --status-json`.
- `linkroute-ui --status` prints a compact status summary.
- `linkroute-ui --tray` adds a notification-area icon.
- Tray right-click menu opens and executes status, diagnostics, enable/pause, swap, and exit.
- `Ctrl+Alt+L` toggles enable/pause.
- `Ctrl+Alt+S` swaps Ethernet/Wi-Fi rule targets.

## Real Hardware

A self-hosted Windows 10/11 x64 machine with active Ethernet and Wi-Fi adapters is required for release gates:

- IPv4 and IPv6 TCP routing.
- UDP and QUIC routing.
- DNS A, AAAA, and CNAME observation.
- Adapter unplug and reconnect.
- Sleep and resume.
- DHCP renewal.
- Windows Firewall and Microsoft Defender compatibility.
- VPN/tunnel detection.
- 24 hour stress run.

## Performance Gates

- Total idle memory: no more than 35 MB.
- Idle CPU: below 0.5%.
- 1 Gbps throughput: at least 90% of baseline.
- Added median latency: below 1 ms.