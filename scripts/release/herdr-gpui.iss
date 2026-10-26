; Per-user Windows installer for Herdr UI, compiled by
; `windows-installer.py build`, which passes the defines below. It installs the
; release zip's tree unchanged: herdr-gpui.exe, a console program because it is
; also the command line terminals and agents run, herdr-gpui-launcher.exe,
; which starts it without a console window, herdr-acp.exe, which the app starts
; chats with, herdr-remote-control.exe, the remote control daemon the app runs as
; a scheduled task, herdr-automate.exe, the automation daemon, and the licenses.
; Closing applications before replacing files also ends running chats, whose
; agents' sessions can be resumed, and Remote control, which the app starts
; again. It never installs, starts, or stops the Herdr daemon, and uninstalling
; keeps settings and Herdr sessions.
; The directory and shortcut match site/install.ps1, so either replaces the other.

#ifndef Version
  #error Pass /DVersion=YYYYMMDD.COUNTER
#endif
#ifndef Target
  #error Pass /DTarget=TARGET_TRIPLE
#endif
#ifndef Architecture
  #error Pass /DArchitecture=x64compatible or arm64
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=the extracted zip root
#endif
#ifndef OutputDir
  #error Pass /DOutputDir=DIRECTORY
#endif
; Optional: the release workflow passes /DSignTool=NAME with a matching
; /SNAME=COMMAND, and ISCC then Authenticode-signs the uninstaller and Setup.exe.
; Local and pull request builds pass neither and stay unsigned.

#define AppName "Herdr UI"
#define AppExe "herdr-gpui.exe"
#define Launcher "herdr-gpui-launcher.exe"
; The automation daemon the app finds beside its executable.
#define Automate "herdr-automate.exe"
; The app's AppUserModelID (herdr_updater::APP_ID), which each window carries with
; relaunch properties naming the launcher (src/taskbar.rs). Shortcuts carry it
; too, so a pinned shortcut and the window share one taskbar button.
#define AppUserModelID "so.pen.herdr-gpui"
#define Icons "..\..\assets\icons"

[Setup]
; Never change: Windows finds an existing installation by it.
AppId={{7FA70894-4FC0-4923-BDCB-B05F7475610A}
AppName={#AppName}
AppVersion={#Version}
AppVerName={#AppName} {#Version}
AppPublisher=Herdr UI
AppPublisherURL=https://herdrui.dev
AppSupportURL=https://github.com/penso/herdr-gpui/issues
DefaultDirName={localappdata}\Programs\{#AppName}
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed={#Architecture}
ArchitecturesInstallIn64BitMode={#Architecture}
; Broadcasts the PATH change so new terminals see it without signing out.
ChangesEnvironment=yes
; A running app is closed before its files are replaced; Herdr sessions live
; in the daemon and keep running.
CloseApplications=yes
RestartApplications=no
OutputDir={#OutputDir}
OutputBaseFilename=Herdr-{#Version}-{#Target}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Herdr's icon in place of Inno's stock box: the setup file's own icon, the
; corner image on every page, and, centered rather than stretched, the welcome
; and finish pages' panel. Each list runs from 100% to 250% DPI.
SetupIconFile={#Icons}\Herdr.ico
WizardSmallImageFile={#Icons}\windows\wizard-small-55.png,{#Icons}\windows\wizard-small-69.png,{#Icons}\windows\wizard-small-83.png,{#Icons}\windows\wizard-small-110.png,{#Icons}\windows\wizard-small-138.png
WizardImageFile={#Icons}\windows\wizard-128.png,{#Icons}\windows\wizard-160.png,{#Icons}\windows\wizard-192.png,{#Icons}\windows\wizard-256.png
WizardImageStretch=no
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
SetupLogging=yes
#ifdef SignTool
SignTool={#SignTool}
SignedUninstaller=yes
#endif

[Tasks]
Name: "addtopath"; Description: "Add herdr-gpui to PATH, for terminals and agents"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\{#Launcher}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\herdr-acp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\herdr-remote-control.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\{#Automate}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion

; Uninstall removes only directories Setup created. Over an install.ps1 copy
; these already existed, and would otherwise be left behind empty.
[UninstallDelete]
Type: dirifempty; Name: "{app}\licenses"
Type: dirifempty; Name: "{app}"

; herdr-gpui:// links, for this user only: jump list entries, notification
; clicks, and links from other apps start the launcher with the link as its
; only argument; it passes the link to herdr-gpui.exe, which hands it to the
; running app. Removed on uninstall. The app also registers its
; AppUserModelID for notifications on first use; that key goes too.
[Registry]
Root: HKCU; Subkey: "Software\Classes\herdr-gpui"; ValueType: string; ValueName: ""; ValueData: "URL:Herdr UI link"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\herdr-gpui"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\herdr-gpui\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"
Root: HKCU; Subkey: "Software\Classes\herdr-gpui\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#Launcher}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\{#AppUserModelID}"; Flags: uninsdeletekey dontcreatekey
; Settings > General > Launch at login writes a Run value naming the launcher,
; and Task Manager's Startup apps may mark it off. Setup creates neither; both
; go on uninstall, so Windows is not left starting a missing program.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#AppName}"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"; ValueName: "{#AppName}"; Flags: uninsdeletevalue dontcreatekey

; "Open in Herdr UI" on a folder, and on the background inside one, for this
; user only. The launcher runs `herdr-gpui open`, which opens the folder as
; Open Project does and starts the app first when it is not running; running
; herdr-gpui.exe directly would flash a console window on every click.
; Windows 11 lists these under "Show more options": its compact menu only shows
; commands of packaged apps. Removed on uninstall.
Root: HKCU; Subkey: "Software\Classes\Directory\shell\HerdrUI"; ValueType: string; ValueName: ""; ValueData: "Open in {#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\HerdrUI"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExe}"",0"
Root: HKCU; Subkey: "Software\Classes\Directory\shell\HerdrUI\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#Launcher}"" open ""%V"""
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\HerdrUI"; ValueType: string; ValueName: ""; ValueData: "Open in {#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\HerdrUI"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExe}"",0"
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\HerdrUI\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#Launcher}"" open ""%V"""

; Shortcuts open the launcher, so the app starts without a console window.
[Icons]
Name: "{userprograms}\{#AppName}"; Filename: "{app}\{#Launcher}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExe}"; AppUserModelID: "{#AppUserModelID}"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#Launcher}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExe}"; AppUserModelID: "{#AppUserModelID}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#Launcher}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; The app's in-app update runs Setup silently with /RELAUNCH=1 and quits; this
; starts the updated app, through the launcher, once its files are in place.
Filename: "{app}\{#Launcher}"; WorkingDir: "{app}"; Flags: nowait; Check: RelaunchRequested
; The automation daemon Setup stopped to replace it, now the new copy.
Filename: "{app}\{#Automate}"; Parameters: "start"; WorkingDir: "{app}"; Flags: runhidden nowait; Check: AutomateStopped

[Code]
const
  EnvironmentKey = 'Environment';

var
  StoppedAutomate: Boolean;

{ Set by the app's own updater, which has quit for the install. }
function RelaunchRequested(): Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

function UserPath(): String;
begin
  if not RegQueryStringValue(HKCU, EnvironmentKey, 'Path', Result) then
    Result := '';
end;

{ Path without Entry. Entries compare without case or a trailing backslash. }
function WithoutEntry(Path, Entry: String): String;
var
  Rest, Part: String;
  Separator: Integer;
begin
  Result := '';
  Rest := Path;
  while Rest <> '' do
  begin
    Separator := Pos(';', Rest);
    if Separator = 0 then
    begin
      Part := Rest;
      Rest := '';
    end
    else
    begin
      Part := Copy(Rest, 1, Separator - 1);
      Delete(Rest, 1, Separator);
    end;
    if (Part <> '') and (CompareText(RemoveBackslashUnlessRoot(Part), RemoveBackslashUnlessRoot(Entry)) <> 0) then
    begin
      if Result <> '' then
        Result := Result + ';';
      Result := Result + Part;
    end;
  end;
end;

{ The user PATH without this installation's entries. Pre-release builds of this
  installer put the bin subdirectory there instead, so that goes too. }
function PathWithoutApp(): String;
begin
  Result := WithoutEntry(WithoutEntry(UserPath(), ExpandConstant('{app}')), ExpandConstant('{app}\bin'));
end;

procedure SetUserPath(Path: String);
begin
  if Path <> UserPath() then
    RegWriteExpandStringValue(HKCU, EnvironmentKey, 'Path', Path);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Path: String;
begin
  if CurStep <> ssPostInstall then
    exit;
  Path := PathWithoutApp();
  if WizardIsTaskSelected('addtopath') then
  begin
    if Path <> '' then
      Path := Path + ';';
    Path := Path + ExpandConstant('{app}');
  end;
  SetUserPath(Path);
end;

{ Whether herdr-automate.exe runs from Path. WQL strings escape backslashes
  and quotes, and compare without case. }
function AutomateRunsFrom(Path: String): Boolean;
var
  Locator, Service, Found: Variant;
begin
  StringChangeEx(Path, '\', '\\', True);
  StringChangeEx(Path, '''', '\''', True);
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\CIMV2');
    Found := Service.ExecQuery('SELECT ProcessId FROM Win32_Process WHERE Name = ''{#Automate}'' AND ExecutablePath = ''' + Path + '''');
    Result := Found.Count > 0;
  except
    Log('Could not list running automation daemons: ' + GetExceptionMessage);
    Result := False;
  end;
end;

{ The automation daemon the app starts keeps running after it quits, and
  Windows will not replace or delete its executable while it runs. Restart
  Manager would end it mid-run, or fail to, so ask the installed copy to stop
  once its runs finish and wait for it to exit. A daemon started from anywhere
  else is left alone. Returns an error message, or '' once it is not running. }
function StopAutomate(): String;
var
  Exe: String;
  Code, Waited: Integer;
begin
  Result := '';
  Exe := ExpandConstant('{app}\{#Automate}');
  if not FileExists(Exe) or not AutomateRunsFrom(Exe) then
    exit;
  if not Exec(Exe, 'shutdown', ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
  begin
    Result := 'The automation daemon at ' + Exe + ' did not stop. Stop its runs with herdr-automate stop, then run Setup again.';
    exit;
  end;
  Waited := 0;
  while AutomateRunsFrom(Exe) do
  begin
    if Waited >= 30000 then
    begin
      Result := 'The automation daemon at ' + Exe + ' is still running. Quit it, then run Setup again.';
      exit;
    end;
    Sleep(200);
    Waited := Waited + 200;
  end;
  StoppedAutomate := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := StopAutomate();
end;

function AutomateStopped(): Boolean;
begin
  Result := StoppedAutomate;
end;

function InitializeUninstall(): Boolean;
var
  Problem: String;
begin
  Problem := StopAutomate();
  Result := Problem = '';
  if not Result then
    SuppressibleMsgBox(Problem, mbError, MB_OK, IDOK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    SetUserPath(PathWithoutApp());
end;
