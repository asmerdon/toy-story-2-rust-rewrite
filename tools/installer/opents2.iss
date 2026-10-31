; OpenTS2 Windows installer (Inno Setup 6). Build with tools\installer\build.ps1, which
; builds the release executables first and passes the version in.
;
; Per-user by default (no admin rights): installs to %LOCALAPPDATA%\Programs\OpenTS2. The
; installer offers an all-users install too, which asks for admin.
;
; It ships only OpenTS2's own executables and text, plus the icon (a render of the game's
; Pizza Planet token, by the owner's decision). NO GAME DATA: the player's own install is
; found by the launcher at run time.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define Root "..\.."

[Setup]
; Never change AppId: upgrades and the uninstaller find earlier installs by it.
AppId={{FFEB3EB6-CB98-4CC0-9A65-060B9900FF3F}
AppName=OpenTS2
AppVersion={#AppVersion}
AppVerName=OpenTS2 {#AppVersion}
AppPublisher=lewiloon2k
AppPublisherURL=https://github.com/asmerdon/toy-story-2-rust-rewrite
AppComments=An open engine for Toy Story 2. Requires the original PC game.
DefaultDirName={autopf}\OpenTS2
DefaultGroupName=OpenTS2
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=disclaimer.txt
SetupIconFile={#Root}\assets\icon\opents2.ico
; The token on the wizard's pages (made by tools\icon\make_icon.py), at 100% and 200% DPI.
WizardImageFile={#Root}\assets\icon\wizard-large.bmp,{#Root}\assets\icon\wizard-large-200.bmp
WizardSmallImageFile={#Root}\assets\icon\wizard-small.bmp,{#Root}\assets\icon\wizard-small-200.bmp
UninstallDisplayIcon={app}\opents2.exe
UninstallDisplayName=OpenTS2
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir={#Root}\target\installer
OutputBaseFilename=OpenTS2-{#AppVersion}-setup
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Root}\target\release\opents2.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\target\release\ts2-viewer.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "disclaimer.txt"; DestDir: "{app}"; DestName: "README.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OpenTS2"; Filename: "{app}\opents2.exe"; Comment: "Play Toy Story 2 with OpenTS2"
Name: "{autodesktop}\OpenTS2"; Filename: "{app}\opents2.exe"; Tasks: desktopicon; Comment: "Play Toy Story 2 with OpenTS2"

[Run]
Filename: "{app}\opents2.exe"; Description: "{cm:LaunchProgram,OpenTS2}"; Flags: nowait postinstall skipifsilent

; Settings, mods (%APPDATA%\OpenTS2) and saves (%APPDATA%\ts2-rust) belong to the player
; and are deliberately left behind on uninstall. The launcher's log is ours, though.
[UninstallDelete]
Type: files; Name: "{userappdata}\OpenTS2\last-run.log"
