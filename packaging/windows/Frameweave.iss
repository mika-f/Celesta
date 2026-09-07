#ifndef PackageDir
  #error PackageDir is required
#endif
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{60C64FD1-8827-468C-A148-6EF371267EF4}
AppName=Frameweave
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Frameweave
DefaultGroupName=Frameweave
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=Frameweave-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\Frameweave.exe
CloseApplications=yes

[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Frameweave"; Filename: "{app}\Frameweave.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\Frameweave.exe"; Description: "Launch Frameweave"; Flags: nowait postinstall skipifsilent
