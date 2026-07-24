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
```

## Build Installer

```powershell
cargo build --workspace --release
.\scripts\fetch-windivert.ps1 -OutputDir .\target\release
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" .\installer\linkroute.iss
.\scripts\package.ps1
```

## Current Architecture Boundary

`linkroute_core` is safe Rust and must remain free of `unsafe`. Windows API and WinDivert FFI are confined to service-side modules.


