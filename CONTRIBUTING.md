# Contributing

LinkRoute is a Windows networking tool. Changes must be conservative and testable because routing mistakes can break network access.

## Development Setup

1. Install Rust stable MSVC, Visual Studio Build Tools, Git, and Inno Setup.
2. Run `scripts\fetch-windivert.ps1 -OutputDir .\target\debug` before WinDivert diagnostics.
3. Run the full local gate before opening a pull request:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
.\target\debug\linkroute-service.exe --self-test
.\target\debug\linkroute-service.exe --probe-windivert
.\target\debug\linkroute-service.exe --load-windivert
```

## Pull Requests

- Keep changes scoped to one subsystem when possible.
- Add tests for rule, config, updater, and backend behavior.
- Do not add packet-routing behavior without a real dual-NIC test note.
- Do not commit downloaded WinDivert binaries; use `scripts\fetch-windivert.ps1`.

## Safety Rules

- Do not intercept HTTPS or install root certificates.
- Do not silently fail open for matched rules when the selected adapter is unavailable.
- Do not route service or updater traffic through managed rules.
