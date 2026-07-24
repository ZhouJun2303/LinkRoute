# LinkRoute Architecture

## Components

- `linkroute_core`: shared rule, config, validation, normalization, and routing
  decision logic.
- `linkroute_service`: privileged Windows service. It owns packet capture,
  DNS observation, adapter monitoring, configuration writes, status publishing,
  and update orchestration.
- `linkroute_ui`: native desktop UI and tray entry point. It does not need
  administrator rights during normal operation.
- `linkroute_updater`: small bootstrap executable that verifies and applies
  downloaded release packages.

## Routing Model

Routing decisions are made per flow. Each flow is pinned to the selected outlet
when it is established.

Priority:

1. More specific domain rule.
2. Ordinary domain rule.
3. Application rule.
4. Windows default route.

If a rule explicitly matches but its target adapter is unavailable, the flow is
blocked. Unmatched traffic continues to use the Windows default route.

The swap operation is special: the service atomically swaps configured Ethernet
and Wi-Fi targets, closes managed TCP flows, and drops UDP mappings so affected
applications reconnect through the new outlet.

## Packet Backend

The first backend is planned around WinDivert:

- Flow/socket layers provide process correlation.
- Network layer provides packets.
- The service maintains five-tuple to process/rule mappings.
- Outbound packets are rewritten for the selected interface and inbound packets
  are reverse-translated.

The backend trait in `linkroute_service` is intentionally narrow so a future WFP
implementation can replace WinDivert without changing UI or rule storage.

## Security Notes

- No root certificate is installed.
- HTTPS is not intercepted.
- Update manifests are planned to be verified with an embedded Ed25519 public
  key before installation.
- Service and updater traffic is excluded from managed routing to avoid loops.
## Current Implementation Batch

The service scaffold now includes three tested in-memory models that the packet
backend will use:

- `DnsCache` records queried domains, CNAMEs, A/AAAA answers, and TTL expiry so
  a later packet-only flow can recover its domain from the remote IP address.
- `FlowTable` records five-tuples, process IDs, executable paths, domains, and
  routing decisions. Swap and pause operations clear managed flows so apps are
  forced to reconnect through the current rules.
- `AdapterInventory` filters physical Ethernet/Wi-Fi candidates and detects
  active VPN/tunnel-style adapters before enabling routing.

`linkroute-service --probe-windivert` checks whether `WinDivert.dll` and
`WinDivert64.sys` are beside the executable. `linkroute-service --self-test`
executes the service decision pipeline without opening the real driver.
## Config Storage

`ConfigStore` stores `config.toml` under `%ProgramData%\LinkRoute`. Missing
configuration loads defaults. Invalid TOML or semantically invalid rules are
renamed to a timestamped backup and the service recovers with defaults. Saves
validate the config before writing and keep the previous config as a backup.
## IPC Protocol

`linkroute_core::ipc` defines the versioned JSON request/response protocol shared
by the UI and service. The current diagnostic command
`linkroute-service --status-json` emits an `IpcResponse::Status` payload. The
next service milestone will move the same payloads over an ACL-restricted named
pipe so the UI can read status, update config, enable/pause routing, swap
outlets, check updates, and export diagnostics without administrator rights.

