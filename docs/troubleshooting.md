# Troubleshooting

## `cargo` not found

Install Rust with rustup and restart PowerShell.

## MSVC linker errors

Install Visual Studio Build Tools with the C++ workload and Windows SDK.

## `--probe-windivert` reports missing files

Run:

```powershell
.\scripts\fetch-windivert.ps1 -OutputDir .\target\debug
```

## `--load-windivert` fails

Check that `WinDivert.dll` is next to `linkroute-service.exe`. The command only loads the DLL and resolves symbols; real packet capture will also require administrator rights.

## `linkroute-ui --status-json` cannot start the service command

Build the workspace first and make sure `linkroute-service.exe` is beside `linkroute-ui.exe`:

```powershell
cargo build --workspace
.\target\debug\linkroute-ui.exe --service-path
```

When running unit-test binaries from `target\debug\deps`, the UI bridge also checks one directory up for `target\debug\linkroute-service.exe`.

## Mutating UI commands cannot write config

`--enable`, `--pause`, `--toggle`, and `--swap` write `%ProgramData%\LinkRoute\config.toml` through the service command handler. Until the LocalSystem service is wired, run from an elevated PowerShell if normal user permissions cannot write that location.

## Unsigned manifest fails verification

This is expected for local packages. The updater parser accepts the manifest format, but signature verification rejects all-zero test signatures.