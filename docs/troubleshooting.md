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

## Unsigned manifest fails verification

This is expected for local packages. The updater parser accepts the manifest format, but signature verification rejects all-zero test signatures.
