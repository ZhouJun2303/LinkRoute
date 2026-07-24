# LinkRoute

LinkRoute is a Windows dual-network routing tool. It is designed to route selected desktop applications or domains through Ethernet or Wi-Fi, with a small native UI, tray controls, hotkeys, and GitHub-based updates.

GitHub repository: https://github.com/ZhouJun2303/LinkRoute

## Current Status

The repository currently contains a verified Rust workspace, routing rule engine, packet parser, packet endpoint rewrite helpers, service-side NAT mapping, Windows adapter enumeration, packet process-context resolution, configuration store, service IPC handler, Windows service registration commands, ACL-restricted named-pipe IPC, lightweight UI-to-service fallback bridge, native Win32 tray shell, WinDivert asset pipeline, update manifest verifier, installer packaging, and release automation.

The remaining core runtime work is wiring the long-running WinDivert packet loop into the SCM service lifecycle, live service status streaming, and the full rules/settings editor; see [ROADMAP.md](ROADMAP.md).

## What Works Now

- Rule precedence for domains and desktop EXE paths.
- Safe Rust IPv4/IPv6 TCP/UDP/ICMP parser with IPv6 extension-header handling.
- Raw packet to flow-key routing decision path in `ServiceController`.
- Safe packet source/destination address and port rewrite helpers with checksum-field invalidation.
- Service-side NAT table for outbound source rewrite and inbound destination restoration.
- Packet processing result model for forwarded, dropped, and rewritten packets.
- DNS A/AAAA/CNAME cache with TTL expiry.
- Windows TCP/UDP owner-PID lookup and process image path resolution for application-rule packet context.
- Five-tuple flow table and managed-flow clearing for swap behavior.
- Windows `GetAdaptersAddresses` inventory with Ethernet/Wi-Fi classification, physical adapter filtering, source-address selection, and VPN/tunnel detection.
- `%ProgramData%\LinkRoute\config.toml` persistence with invalid-config backup.
- Versioned JSON IPC request/response protocol shared by service and UI.
- Service command handler for status, config, enable, pause, swap, and diagnostics.
- Windows service registration, start, stop, and uninstall commands for the service scaffold.
- ACL-restricted named-pipe transport for local UI-to-service requests.
- UI fallback bridge plus Win32 tray menu and global hotkeys.
- WinDivert download, SHA-256 verification, DLL/driver probing, dynamic symbol loading, packet send/receive wrappers, and checksum helper binding.
- Ed25519 update manifest parsing and signature verification.
- Inno Setup installer and GitHub Actions CI/release workflows.

## Build

Install Rust stable with the MSVC toolchain, Visual Studio Build Tools, Git, and Inno Setup 6. Then run:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Useful service checks:

```powershell
.\scripts\fetch-windivert.ps1 -OutputDir .\target\debug
.\target\debug\linkroute-service.exe --self-test
.\target\debug\linkroute-service.exe --check-config
.\target\debug\linkroute-service.exe --write-default-config
.\target\debug\linkroute-service.exe --probe-windivert
.\target\debug\linkroute-service.exe --load-windivert
.\target\debug\linkroute-service.exe --status-json
```

`--status-json` includes the detected adapter list and marks only usable physical Ethernet/Wi-Fi adapters as outlet candidates.

Service install commands require an elevated shell:

```powershell
.\target\release\linkroute-service.exe --install-service
.\target\release\linkroute-service.exe --start-service
.\target\release\linkroute-service.exe --stop-service
.\target\release\linkroute-service.exe --uninstall-service
```

Useful UI checks:

```powershell
.\target\debug\linkroute-ui.exe --service-path
.\target\debug\linkroute-ui.exe --status-json
.\target\debug\linkroute-ui.exe --status
.\target\debug\linkroute-ui.exe --tray
```

The tray menu supports status, diagnostics, enable/pause, swap, and exit. Global hotkeys are `Ctrl+Alt+L` for enable/pause and `Ctrl+Alt+S` for swapping Ethernet/Wi-Fi rule targets.

When the installed service is running, the UI talks to `\\.\pipe\LinkRouteControl`. If the pipe is unavailable, development builds fall back to invoking the sibling `linkroute-service.exe --handle-request-file` command.

## Package

```powershell
cargo build --workspace --release
.\scripts\build-installer.ps1
.\scripts\package.ps1
```

The `dist/` folder receives the installer, installer SHA-256 file, update manifest, manifest signature placeholder, licenses, and WinDivert runtime files.

## Documentation

- [Architecture](docs/architecture.md)
- [Data plane](docs/data-plane.md)
- [Development](docs/development.md)
- [Git reference](docs/git.md)
- [Release process](docs/release.md)
- [Testing plan](docs/testing.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Roadmap](ROADMAP.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## License

GPL-3.0-only. Bundled third-party components retain their own notices; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).