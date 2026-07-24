# LinkRoute Architecture

## Components

- `linkroute_core`: shared rule, config, validation, normalization, IPC, and routing decision logic.
- `linkroute_service`: privileged Windows service. It owns packet capture, DNS observation, adapter monitoring, configuration writes, status publishing, and update orchestration.
- `linkroute_ui`: native desktop UI and tray entry point. It does not need administrator rights during normal operation once the LocalSystem service is installed and the named pipe is reachable.
- `linkroute_updater`: small bootstrap executable that verifies and applies downloaded release packages.

## Routing Model

Routing decisions are made per flow. Each flow is pinned to the selected outlet when it is established.

Priority:

1. More specific domain rule.
2. Ordinary domain rule.
3. Application rule.
4. Windows default route.

If a rule explicitly matches but its target adapter is unavailable, the flow is blocked. Unmatched traffic continues to use the Windows default route.

The swap operation is special: the service atomically swaps configured Ethernet and Wi-Fi targets, closes managed TCP flows, and drops UDP mappings so affected applications reconnect through the new outlet.

## Packet Backend

The first backend is planned around WinDivert:

- Flow/socket layers provide process correlation.
- Network layer provides packets.
- The service maintains five-tuple to process/rule mappings.
- Outbound packets are rewritten for the selected interface and inbound packets are reverse-translated.

The backend trait in `linkroute_service` is intentionally narrow so a future WFP implementation can replace WinDivert without changing UI or rule storage.

## Security Notes

- No root certificate is installed.
- HTTPS is not intercepted.
- The control pipe DACL allows LocalSystem, Administrators, and the active interactive user class.
- Update manifests are planned to be verified with an embedded Ed25519 public key before installation.
- Service and updater traffic is excluded from managed routing to avoid loops.

## Current Implementation Batch

The service scaffold now includes tested in-memory models that the packet backend will use:

- `DnsCache` records queried domains, CNAMEs, A/AAAA answers, and TTL expiry so a later packet-only flow can recover its domain from the remote IP address.
- `FlowTable` records five-tuples, process IDs, executable paths, domains, and routing decisions. Swap and pause operations clear managed flows so apps are forced to reconnect through the current rules.
- `AdapterInventory` filters physical Ethernet/Wi-Fi candidates and detects active VPN/tunnel-style adapters before enabling routing.

`linkroute-service --probe-windivert` checks whether `WinDivert.dll` and `WinDivert64.sys` are beside the executable. `linkroute-service --self-test` executes the service decision pipeline without opening the real driver.

## Windows Service Lifecycle

`linkroute-service --run-service` runs under the Windows Service Control Manager. The service dispatcher registers a control handler, reports start-pending, running, stop-pending, and stopped states, and stops the backend when SCM sends stop or shutdown.

The service scaffold can be installed, started, stopped, and removed with:

```powershell
linkroute-service.exe --install-service
linkroute-service.exe --start-service
linkroute-service.exe --stop-service
linkroute-service.exe --uninstall-service
```

The Inno Setup installer calls those commands during install and uninstall. The installed service is named `LinkRoute` and uses this binary path shape:

```text
"<install-dir>\linkroute-service.exe" --run-service
```

## Config Storage

`ConfigStore` stores `config.toml` under `%ProgramData%\LinkRoute`. Missing configuration loads defaults. Invalid TOML or semantically invalid rules are renamed to a timestamped backup and the service recovers with defaults. Saves validate the config before writing and keep the previous config as a backup.

## IPC Protocol

`linkroute_core::ipc` defines the versioned JSON request/response protocol shared by the UI and service. The primary local transport is `\\.\pipe\LinkRouteControl`, a message-mode named pipe created by the service. The UI waits briefly for the pipe and falls back to `linkroute-service --handle-request-file <path>` when the service is not running, which keeps local development commands usable.

The service pipe currently handles one request per pipe instance. `GetStatus` is answered from the running service controller so `backend_running` reflects the SCM-hosted service. Config-changing commands still persist through `ConfigStore`; applying those mutations directly to the active packet backend will land with the real packet loop.

`linkroute-service --serve-pipe-once` is a development command that starts the backend scaffold, serves one pipe request, and exits. It exists for local IPC validation without registering a Windows service.

The next service milestone is live status streaming so the tray can update without polling.

## Native UI Shell

The current UI shell is a lightweight Win32 tray process, not a WebView app. It creates a hidden message window, adds a notification-area icon, and handles a small menu for status, diagnostics, enable/pause, swap, and exit. Global hotkeys are registered for the two fast actions:

- `Ctrl+Alt+L`: enable or pause routing.
- `Ctrl+Alt+S`: swap Ethernet/Wi-Fi rule targets.

The full native window for application rules, domain rules, settings, and live rates remains a later milestone.