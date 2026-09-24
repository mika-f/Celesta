#ifndef PackageDir
  #error PackageDir is required
#endif
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{60C64FD1-8827-468C-A148-6EF371267EF4}
AppName=Celesta
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Celesta
DefaultGroupName=Celesta
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=Celesta-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\Celesta.exe
CloseApplications=yes

[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Celesta"; Filename: "{app}\Celesta.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\Celesta.exe"; Description: "Launch Celesta"; Flags: nowait postinstall skipifsilent
