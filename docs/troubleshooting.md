# Troubleshooting

## `cargo` not found

Install Rust with rustup and restart PowerShell.

## MSVC linker errors

Install Visual Studio Build Tools with the C++ workload and Windows SDK.

## Inno Setup is not found

Install Inno Setup 6 and rerun:

```powershell
.\scripts\build-installer.ps1
```

The script checks the usual per-user and Program Files install paths for `ISCC.exe`.

## `--probe-windivert` reports missing files

Run:

```powershell
.\scripts\fetch-windivert.ps1 -OutputDir .\target\debug
```

## `--load-windivert` fails

Check that `WinDivert.dll` is next to `linkroute-service.exe`. The command only loads the DLL and resolves symbols; real packet capture will also require administrator rights.

## Service install fails

Run from an elevated PowerShell. The service command uses `sc.exe` and registers this binary path:

```text
"<install-dir>\linkroute-service.exe" --run-service
```

If a previous development install is present, run:

```powershell
.\target\release\linkroute-service.exe --stop-service
.\target\release\linkroute-service.exe --uninstall-service
```

## `linkroute-ui --status-json` cannot reach the named pipe

The UI waits briefly for `\\.\pipe\LinkRouteControl`. If the pipe is not available, it falls back to the sibling service command. Check which service binary the UI would use:

```powershell
.\target\debug\linkroute-ui.exe --service-path
```

Validate the pipe without installing the service:

```powershell
$server = Start-Process -FilePath .\target\debug\linkroute-service.exe -ArgumentList --serve-pipe-once -WindowStyle Hidden -PassThru
Start-Sleep -Milliseconds 500
.\target\debug\linkroute-ui.exe --status-json
$server.WaitForExit(15000)
```

A pipe-backed response reports `"backend_running":true`.

## Mutating UI commands cannot write config

`--enable`, `--pause`, `--toggle`, and `--swap` write `%ProgramData%\LinkRoute\config.toml`. If the named pipe is not available and the UI falls back to the temporary service command bridge, run from an elevated PowerShell if normal user permissions cannot write that location.

## Unsigned manifest fails verification

This is expected for local packages. The updater parser accepts the manifest format, but signature verification rejects all-zero test signatures.