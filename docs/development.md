# Development Guide

## Required Tools

- Rust stable MSVC toolchain.
- Visual Studio Build Tools with C++ workload.
- Git.
- Inno Setup 6.
- PowerShell 5 or later.

## Local Validation

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
.\scripts\fetch-windivert.ps1 -OutputDir .\target\debug
.\target\debug\linkroute-service.exe --self-test
.\target\debug\linkroute-service.exe --probe-windivert
.\target\debug\linkroute-service.exe --load-windivert
.\target\debug\linkroute-service.exe --status-json
$request = @{ type = "get-status" } | ConvertTo-Json -Compress
Set-Content -LiteralPath .\request.json -Value $request -Encoding ASCII
.\target\debug\linkroute-service.exe --handle-request-file .\request.json
Remove-Item -LiteralPath .\request.json -Force
.\target\debug\linkroute-ui.exe --service-path
.\target\debug\linkroute-ui.exe --status-json
.\target\debug\linkroute-ui.exe --status
```

Run service install commands only from an elevated shell and only when you intend to change the local machine:

```powershell
.\target\release\linkroute-service.exe --install-service
.\target\release\linkroute-service.exe --start-service
.\target\release\linkroute-service.exe --stop-service
.\target\release\linkroute-service.exe --uninstall-service
```

Run the tray manually when you need interactive validation:

```powershell
.\target\debug\linkroute-ui.exe --tray
```

The tray process stays alive until the Exit menu item is selected.

## Build Installer

```powershell
cargo build --workspace --release
.\scripts\build-installer.ps1
.\scripts\package.ps1
```

`build-installer.ps1` fetches WinDivert into `target\release`, finds Inno Setup 6, and writes `installer\output\LinkRouteSetup-x64.exe`. `package.ps1` copies the installer and release metadata to `dist\`.

## Current Architecture Boundary

`linkroute_core` is safe Rust and must remain free of `unsafe`. Windows API and WinDivert FFI are confined to service-side and UI-shell modules.