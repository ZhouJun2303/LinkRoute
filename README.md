# LinkRoute

LinkRoute is a Windows dual-network routing tool. It is designed to route selected desktop applications or domains through Ethernet or Wi-Fi, with a small native UI, tray controls, hotkeys, and GitHub-based updates.

GitHub repository: https://github.com/ZhouJun2303/LinkRoute

## Current Status

The repository currently contains a verified Rust scaffold, routing rule engine, configuration store, WinDivert asset pipeline, update manifest verifier, installer packaging, and release automation. Real packet rewrite/NAT, production service IPC, and the final native tray UI are still under active development; see [ROADMAP.md](ROADMAP.md).

## What Works Now

- Rule precedence for domains and desktop EXE paths.
- DNS A/AAAA/CNAME cache with TTL expiry.
- Five-tuple flow table and managed-flow clearing for swap behavior.
- Physical adapter filtering and VPN/tunnel detection model.
- `%ProgramData%\LinkRoute\config.toml` persistence with invalid-config backup.
- WinDivert download, SHA-256 verification, DLL/driver probing, and dynamic symbol loading.
- Ed25519 update manifest parsing and signature verification.
- Inno Setup installer build and GitHub Actions workflows.

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
```

`--write-default-config` writes `%ProgramData%\LinkRoute\config.toml`; it may need an elevated shell on a normal Windows install.

## Package

```powershell
cargo build --workspace --release
.\scripts\fetch-windivert.ps1 -OutputDir .\target\release
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" .\installer\linkroute.iss
.\scripts\package.ps1
```

The `dist/` folder receives the installer, update manifest, licenses, and WinDivert runtime files.

## Documentation

- [Architecture](docs/architecture.md)
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

