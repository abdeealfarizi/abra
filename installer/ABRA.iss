; ABRA installer (Inno Setup 6). Per-user install, no administrator rights required.
; Build:  powershell -File installer\build-installer.ps1   (needs Inno Setup 6: https://jrsoftware.org/isinfo.php)

#ifndef AppVersion
  #define AppVersion "0.3.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\target\release"
#endif

#define AppName "ABRA"
#define AppExe "abra.exe"

[Setup]
AppId={{939A19A7-DB24-45FB-836C-6ED85D256BC3}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=ABRA contributors
AppPublisherURL=https://github.com/abdeealfarizi/abra
AppSupportURL=https://github.com/abdeealfarizi/abra/issues
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
LicenseFile=..\LICENSE.md
OutputDir=..\dist
OutputBaseFilename=ABRA-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
SetupIconFile=..\assets\abra.ico
CloseApplications=no

[Tasks]
Name: "autostart"; Description: "Launch ABRA when I sign in to Windows"; Flags: unchecked
Name: "startmenu"; Description: "Create a Start menu shortcut"

[Files]
Source: "{#SourceDir}\abra.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\abra-guard.exe"; DestDir: "{app}"; Flags: ignoreversion
; Inter (SIL OFL) is loaded from the "fonts" folder next to abra.exe when SF Pro is not installed.
Source: "..\assets\fonts\*"; DestDir: "{app}\fonts"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\PANDUAN.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: startmenu

[Registry]
; Same value the app writes itself (Settings > General > Launch at Windows login).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "ABRA"; ValueData: """{app}\{#AppExe}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch ABRA now"; Flags: nowait postinstall skipifsilent

[Code]
// Stop a running ABRA the safe way: kill abra.exe first, give abra-guard.exe a moment to restore the taskbar
// (and any windows Stage Manager had parked), then stop the guard.
procedure StopAbra();
var
  rc: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM abra.exe', '', SW_HIDE, ewWaitUntilTerminated, rc);
  Sleep(2500);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM abra-guard.exe', '', SW_HIDE, ewWaitUntilTerminated, rc);
  Sleep(300);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopAbra();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopAbra();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    if MsgBox('Also delete your ABRA settings (%APPDATA%\ABRA)?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DelTree(ExpandConstant('{userappdata}\ABRA'), True, True, True);
  end;
end;
