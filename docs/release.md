# Release Process

Releases are published from signed `vX.Y.Z` tags.

Expected artifacts:

- `LinkRouteSetup-x64.exe`
- `LinkRouteSetup-x64.exe.sha256`
- `update-manifest.txt`
- `update-manifest.txt.sig`
- License notices and third-party source links

The installer is generated with Inno Setup. Authenticode signing is optional for
early releases, but unsigned builds will show Windows SmartScreen warnings.

The stable updater checks GitHub Releases every 24 hours or when the user asks.
It downloads to `%ProgramData%\LinkRoute\updates`, verifies the manifest,
prompts the user, then invokes the bootstrap updater to stop the service, replace
files, run a health check, and roll back on failure.

Update manifests are verified with Ed25519. Local unsigned packaging writes an all-zero placeholder signature that the updater parser accepts but signature verification rejects; production release automation must replace it with a real signature from the release signing key.

