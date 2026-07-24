#define MyAppName "LinkRoute"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "LinkRoute Contributors"

[Setup]
AppId={{8E8B9322-7DFD-4B1D-A3F0-0F5C2BCE62E1}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\LinkRoute
DefaultGroupName=LinkRoute
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=output
OutputBaseFilename=LinkRouteSetup-x64
Compression=lzma
SolidCompression=yes
PrivilegesRequired=admin

[Files]
Source: "..\target\release\linkroute-service.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\linkroute-ui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\linkroute-updater.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\WinDivert.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\WinDivert64.sys"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\LinkRoute"; Filename: "{app}\linkroute-ui.exe"
Name: "{autoprograms}\LinkRoute"; Filename: "{app}\linkroute-ui.exe"

[Run]
Filename: "{app}\linkroute-service.exe"; Parameters: "--install-service"; StatusMsg: "Installing LinkRoute service..."; Flags: runhidden waituntilterminated
Filename: "{app}\linkroute-service.exe"; Parameters: "--start-service"; StatusMsg: "Starting LinkRoute service..."; Flags: runhidden waituntilterminated
Filename: "{app}\linkroute-ui.exe"; Description: "Launch LinkRoute"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\linkroute-service.exe"; Parameters: "--stop-service"; Flags: runhidden waituntilterminated; RunOnceId: "StopLinkRouteService"
Filename: "{app}\linkroute-service.exe"; Parameters: "--uninstall-service"; Flags: runhidden waituntilterminated; RunOnceId: "DeleteLinkRouteService"
