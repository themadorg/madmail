; Madmail Windows installer (Inno Setup 6+)
; Build:  .\build-setup.ps1 -Arch amd64
;         .\build-setup.ps1 -Arch amd64 -Version 2.35.0
;         iscc /DArch=arm64 /DMyAppVersion=2.35.0 Madmail.iss
;
; Expects pre-built binaries under repo build/:
;   madmail-windows-{amd64,arm64}.exe
;   madmail-tray-windows-{amd64,arm64}.exe
;     optional for a local build; required when iscc is passed /DRequireTray=1
;
; MyAppVersion is passed by build-setup.ps1. The 0.0.0 fallback is only for a
; direct iscc invocation that does not supply /DMyAppVersion.

#ifndef Arch
  #define Arch "amd64"
#endif

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

#ifndef RequireTray
  #define RequireTray "0"
#endif

; TestingBuild=1 is the unsigned prerelease installer only.
; The stable workflow does not pass /DTestingBuild.
;
; Testing is not a side-by-side install. It has its own Inno AppId and program
; directory so Inno will not treat it as an upgrade of stable. The server and
; tray programs still register the service name Madmail, ProgramData\Madmail,
; and the HKCU Run value MadmailTray. InitializeSetup aborts before any file is
; copied when a stable uninstall entry or any of those shared resources exists.
; The supported environment is a fresh disposable Windows VM. A second run on
; the same VM is refused as well, because the first run creates those resources.
#ifndef TestingBuild
  #define TestingBuild "0"
#endif

#if TestingBuild == "1"
  #define MyAppName "Madmail TESTING"
  #define MyTestingLabel MyAppVersion + "-unstable TESTING ONLY - NOT FOR PRODUCTION USE"
#else
  #define MyAppName "Madmail"
#endif
#define MyAppPublisher "themadorg"
#define MyAppURL "https://github.com/themadorg/madmail"
#define MyAppExeName "madmail.exe"
#define MyTrayExeName "madmail-tray.exe"
#define MyServiceName "Madmail"

#if Arch == "arm64"
  #define SetupArch "arm64"
  #define SourceMadmail "..\..\build\madmail-windows-arm64.exe"
  #define SourceTray "..\..\build\madmail-tray-windows-arm64.exe"
#else
  #define SetupArch "amd64"
  #define SourceMadmail "..\..\build\madmail-windows-amd64.exe"
  #define SourceTray "..\..\build\madmail-tray-windows-amd64.exe"
#endif

[Setup]
#if TestingBuild == "1"
AppId={{E7A41C90-2B6D-4F58-8C13-5D9A0E6B7F24}
UsePreviousAppDir=no
DefaultDirName={autopf}\MadmailTesting
#else
AppId={{8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F}
DefaultDirName={autopf}\{#MyAppName}
#endif
AppName={#MyAppName}
AppVersion={#MyAppVersion}
#if TestingBuild == "1"
AppVerName={#MyTestingLabel}
VersionInfoProductTextVersion={#MyTestingLabel}
#endif
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}/releases
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=license.txt
OutputDir=..\..\build
OutputBaseFilename=madmail-windows-{#SetupArch}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
MinVersion=10.0
UninstallDisplayIcon={app}\{#MyAppExeName}
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Dirs]
Name: "{commonappdata}\Madmail"
Name: "{commonappdata}\Madmail\config"
Name: "{commonappdata}\Madmail\data"
Name: "{commonappdata}\Madmail\config\certs"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut for Madmail tray"; GroupDescription: "Additional icons:"; Flags: unchecked
Name: "autostart"; Description: "Start &tray when I log in"; GroupDescription: "Startup:"; Flags: checkedonce
Name: "firewall"; Description: "Open Windows &Firewall ports for mail/HTTP"; GroupDescription: "Network:"; Flags: checkedonce
Name: "startservice"; Description: "Start Madmail &service after install"; GroupDescription: "Service:"; Flags: checkedonce

[Files]
; Server binary (required)
Source: "{#SourceMadmail}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion
; Tray helper. Stable release builds pass /DRequireTray=1 so a missing file
; fails the compile. A local build still skips the tray when it was not built.
#if RequireTray == "1"
Source: "{#SourceTray}"; DestDir: "{app}"; DestName: "{#MyTrayExeName}"; Flags: ignoreversion
#else
Source: "{#SourceTray}"; DestDir: "{app}"; DestName: "{#MyTrayExeName}"; Flags: ignoreversion skipifsourcedoesntexist
#endif
#if TestingBuild == "1"
Source: "..\..\build\UNSTABLE.txt"; DestDir: "{app}"; DestName: "UNSTABLE.txt"; Flags: ignoreversion nocompression
#endif

[Icons]
Name: "{group}\Madmail Tray"; Filename: "{app}\{#MyTrayExeName}"; WorkingDir: "{app}"; Check: TrayPresent
Name: "{group}\Madmail Service Status"; Filename: "{app}\{#MyAppExeName}"; Parameters: "service status"; WorkingDir: "{app}"
Name: "{group}\Uninstall Madmail"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Madmail Tray"; Filename: "{app}\{#MyTrayExeName}"; Tasks: desktopicon; Check: TrayPresent

[Run]
; Log configure step to ProgramData\Madmail\install.log (failures were invisible with bare runhidden).
Filename: "{cmd}"; \
  Parameters: "/C """"{app}\{#MyAppExeName}"" {code:InstallCliArgs} > ""{commonappdata}\Madmail\install.log"" 2>&1"""; \
  StatusMsg: "Configuring Madmail (TLS, service)…"; \
  Flags: runhidden waituntilterminated
; Tray autostart
Filename: "{app}\{#MyTrayExeName}"; \
  Parameters: "install-autostart"; \
  StatusMsg: "Registering tray autostart…"; \
  Flags: runhidden waituntilterminated; \
  Tasks: autostart; Check: TrayPresent
; Launch tray now
Filename: "{app}\{#MyTrayExeName}"; \
  Description: "Launch Madmail tray now"; \
  Flags: nowait postinstall skipifsilent; Check: TrayPresent

[UninstallRun]
; Stop service + firewall; ProgramData wipe depends on UninstallWipeData (prompt).
; --keep-binary: Inno removes Program Files after this step.
Filename: "{app}\{#MyAppExeName}"; \
  Parameters: "{code:UninstallCliArgs}"; \
  RunOnceId: "MadmailUninstall"; \
  Flags: runhidden waituntilterminated
Filename: "{app}\{#MyTrayExeName}"; \
  Parameters: "uninstall-autostart"; \
  RunOnceId: "MadmailTrayUninstallAutostart"; \
  Flags: runhidden waituntilterminated; Check: TrayPresent

[Code]
var
  ModePage: TInputOptionWizardPage;
  IdentityPage: TInputQueryWizardPage;
  TlsPage: TInputOptionWizardPage;
  AcmePage: TInputQueryWizardPage;
  DnsPage: TOutputMsgWizardPage;
  LangPage: TInputOptionWizardPage;
  FeaturePage: TInputOptionWizardPage;
  FinishedMsg: String;
  { True = remove %ProgramData%\Madmail (config + mail); False = keep. }
  UninstallWipeData: Boolean;

#if TestingBuild == "1"
{ Inno calls InitializeSetup before the wizard and before [Files] are copied. }
function TestingStableResourcesPresent: Boolean;
var
  UninstallKey: String;
begin
  UninstallKey := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\';
  UninstallKey := UninstallKey + Chr(123);
  UninstallKey := UninstallKey + '8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F';
  UninstallKey := UninstallKey + Chr(125);
  UninstallKey := UninstallKey + '_is1';
  Result :=
    RegKeyExists(HKLM, UninstallKey) or
    RegKeyExists(HKLM32, UninstallKey) or
    RegKeyExists(HKLM64, UninstallKey) or
    RegKeyExists(HKCU, UninstallKey) or
    RegKeyExists(HKCU32, UninstallKey) or
    RegKeyExists(HKCU64, UninstallKey) or
    RegKeyExists(HKLM, 'SYSTEM\CurrentControlSet\Services\Madmail') or
    RegKeyExists(HKLM64, 'SYSTEM\CurrentControlSet\Services\Madmail') or
    RegValueExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'MadmailTray') or
    RegValueExists(HKCU32, 'Software\Microsoft\Windows\CurrentVersion\Run', 'MadmailTray') or
    RegValueExists(HKCU64, 'Software\Microsoft\Windows\CurrentVersion\Run', 'MadmailTray') or
    DirExists(ExpandConstant('{commonappdata}\Madmail')) or
    DirExists(ExpandConstant('{autopf}\Madmail'));
end;

function InitializeSetup: Boolean;
begin
  Result := not TestingStableResourcesPresent;
  if not Result then
    { MsgBox ignores /SUPPRESSMSGBOXES and would block a hidden silent run. }
    SuppressibleMsgBox('Madmail TESTING cannot be installed on a machine that already has stable Madmail.'#13#10#13#10 +
      'This testing setup uses its own installer id and program directory, but the server and tray still use the production service name Madmail, ProgramData\Madmail, and the MadmailTray autostart value. Setup stops before copying files when any of those, or the stable uninstall entry, is present.'#13#10#13#10 +
      'Install this testing build only on a fresh disposable Windows VM.',
      mbError, MB_OK, IDOK);
end;
#endif

function TrayPresent: Boolean;
begin
  Result := FileExists(ExpandConstant('{app}\{#MyTrayExeName}'));
end;

function IsDomainMode: Boolean;
begin
  Result := ModePage.SelectedValueIndex = 2;
end;

function IsLocalMode: Boolean;
begin
  Result := ModePage.SelectedValueIndex = 0;
end;

function IsPublicIpMode: Boolean;
begin
  Result := ModePage.SelectedValueIndex = 1;
end;

function GetIdentity: String;
begin
  Result := Trim(IdentityPage.Values[0]);
end;

function GetAcmeEmail: String;
begin
  Result := Trim(AcmePage.Values[0]);
end;

function GetLangCode: String;
begin
  case LangPage.SelectedValueIndex of
    1: Result := 'fa';
    2: Result := 'ru';
    3: Result := 'es';
  else
    Result := 'en';
  end;
end;

function Quoted(const S: String): String;
begin
  Result := '"' + S + '"';
end;

function ConfigDir: String;
begin
  Result := ExpandConstant('{commonappdata}\Madmail\config');
end;

function StateDir: String;
begin
  Result := ExpandConstant('{commonappdata}\Madmail\data');
end;

{ Build madmail install CLI args from wizard + tasks }
function InstallCliArgs(Param: String): String;
var
  Args: String;
  Ident: String;
  TlsMode: Integer;
begin
  Ident := GetIdentity;
  if Ident = '' then
    Ident := '127.0.0.1';

  Args := 'install --simple --lang ' + GetLangCode;
  Args := Args + ' --config-dir ' + Quoted(ConfigDir);
  Args := Args + ' --state-dir ' + Quoted(StateDir);
  Args := Args + ' --binary-path ' + Quoted(ExpandConstant('{app}\{#MyAppExeName}'));

  if IsDomainMode then
  begin
    Args := Args + ' --domain ' + Ident;
    if GetAcmeEmail <> '' then
      Args := Args + ' --acme-email ' + GetAcmeEmail;
  end
  else
  begin
    Args := Args + ' --ip ' + Ident;
  end;

  TlsMode := TlsPage.SelectedValueIndex;
  { 0 = self-signed, 1 = Let's Encrypt (domain or auto-IP) }
  if TlsMode = 0 then
    Args := Args + ' --tls-mode self_signed --no-obtain-certificate'
  else if IsDomainMode then
    Args := Args + ' --tls-mode autocert'
  else
    Args := Args + ' --auto-ip-cert --acme-email ' + GetAcmeEmail;

  if FeaturePage.Values[0] then
    Args := Args + ' --enable-ss';
  { Iroh omitted: Windows builds do not ship iroh-relay.exe yet; enabling it makes
    the service fail at boot with "iroh-relay: program not found". Re-add when packaged. }

  Args := Args + ' --install-service';
  if WizardIsTaskSelected('startservice') then
    Args := Args + ' --start-service';
  if WizardIsTaskSelected('firewall') then
    Args := Args + ' --firewall';

  Result := Args;
  Log('Install CLI: madmail ' + Args);
end;

function InitializeUninstall(): Boolean;
var
  Answer: Integer;
begin
  { Interactive: ask whether to remove ProgramData\Madmail.
    Silent (/VERYSILENT): keep data (safer for scripted upgrades). }
  Result := True;
  UninstallWipeData := False;
  if UninstallSilent then
  begin
    Log('Silent uninstall: keeping ProgramData\Madmail (use interactive uninstall to wipe)');
    Exit;
  end;
  Answer := MsgBox(
    'Remove all Madmail data under:'#13#10 +
    '  ' + ExpandConstant('{commonappdata}\Madmail') + #13#10#13#10 +
    'This includes configuration, certificates, mailboxes, and install.log.'#13#10#13#10 +
    'Yes = delete all data (clean reinstall)'#13#10 +
    'No = keep data (you can reinstall and reuse mail/config)'#13#10 +
    'Cancel = abort uninstall',
    mbConfirmation, MB_YESNOCANCEL);
  case Answer of
    IDYES:
      begin
        UninstallWipeData := True;
        Log('Uninstall: wipe ProgramData\Madmail');
      end;
    IDNO:
      begin
        UninstallWipeData := False;
        Log('Uninstall: keep ProgramData\Madmail');
      end;
  else
    begin
      Result := False;
      Log('Uninstall cancelled by user');
    end;
  end;
end;

function UninstallCliArgs(Param: String): String;
begin
  Result := 'uninstall --force --keep-binary';
  if not UninstallWipeData then
  begin
    Result := Result + ' --keep-data --keep-config';
  end;
  Result := Result + ' --config ' + Quoted(ConfigDir + '\madmail.conf');
  Result := Result + ' --state-dir ' + Quoted(StateDir);
  Log('Uninstall CLI: madmail ' + Result);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataRoot: String;
begin
  { After madmail uninstall: if user chose wipe, force-remove any leftovers (locked files). }
  if (CurUninstallStep = usPostUninstall) and UninstallWipeData then
  begin
    DataRoot := ExpandConstant('{commonappdata}\Madmail');
    if DirExists(DataRoot) then
    begin
      if not DelTree(DataRoot, True, True, True) then
        Log('Warning: could not fully remove ' + DataRoot)
      else
        Log('Removed ' + DataRoot);
    end;
  end;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := False;
  if PageID = AcmePage.ID then
  begin
    { ACME email only when LE selected }
    Result := (TlsPage.SelectedValueIndex = 0);
  end
  else if PageID = DnsPage.ID then
  begin
    Result := not IsDomainMode;
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Ident: String;
begin
  Result := True;
  if CurPageID = IdentityPage.ID then
  begin
    Ident := GetIdentity;
    if Ident = '' then
    begin
      MsgBox('Please enter an IP address or domain name.', mbError, MB_OK);
      Result := False;
      Exit;
    end;
  end
  else if CurPageID = AcmePage.ID then
  begin
    if (TlsPage.SelectedValueIndex <> 0) and (GetAcmeEmail = '') then
    begin
      MsgBox('Let''s Encrypt requires an email address (user@domain).', mbError, MB_OK);
      Result := False;
      Exit;
    end;
  end;
end;

procedure InitializeWizard;
begin
  ModePage := CreateInputOptionPage(wpLicense,
    'Deployment mode', 'How will this Madmail server be used?',
    'Choose the primary identity for this install.',
    True, False);
  ModePage.Add('Local / lab (127.0.0.1, self-signed recommended)');
  ModePage.Add('Public IP (self-signed or Let''s Encrypt IP certificate)');
  ModePage.Add('Domain name (Let''s Encrypt DNS certificate)');
  ModePage.SelectedValueIndex := 0;

  IdentityPage := CreateInputQueryPage(ModePage.ID,
    'Server identity', 'IP address or domain',
    'Local mode typically uses 127.0.0.1. Public IP mode needs a routable address. Domain mode needs a DNS hostname.');
  IdentityPage.Add('IP or domain:', False);
  IdentityPage.Values[0] := '127.0.0.1';

  TlsPage := CreateInputOptionPage(IdentityPage.ID,
    'TLS certificate', 'How should Madmail obtain TLS certificates?',
    'Self-signed is fine for local testing and Delta Chat with trust-on-first-use. Let''s Encrypt needs port 80 free and a public identity.',
    True, False);
  TlsPage.Add('Self-signed (recommended for local / lab)');
  TlsPage.Add('Let''s Encrypt (domain or public IP certificate)');
  TlsPage.SelectedValueIndex := 0;

  AcmePage := CreateInputQueryPage(TlsPage.ID,
    'Let''s Encrypt contact', 'ACME account email',
    'Used for expiry notices. Must be user@domain (not user@IP).');
  AcmePage.Add('Email:', False);

  LangPage := CreateInputOptionPage(AcmePage.ID,
    'Language', 'Website / UI language',
    'Seeds the Madmail language setting.',
    True, False);
  LangPage.Add('English (en)');
  LangPage.Add('Persian (fa)');
  LangPage.Add('Russian (ru)');
  LangPage.Add('Spanish (es)');
  LangPage.SelectedValueIndex := 0;

  FeaturePage := CreateInputOptionPage(LangPage.ID,
    'Optional features', 'Enable additional services in the generated config',
    'You can change these later via config / CLI.',
    False, False);
  FeaturePage.Add('Shadowsocks proxy');
  FeaturePage.Values[0] := True;

  DnsPage := CreateOutputMsgPage(wpSelectTasks,
    'DNS checklist', 'Domain deployment reminders',
    'Before clients and federation work reliably:'#13#10#13#10 +
    '  1. A / AAAA for your hostname pointing at this server'#13#10 +
    '  2. MX record for the mail domain'#13#10 +
    '  3. Optional: SPF, DKIM, DMARC'#13#10#13#10 +
    'Docs: https://github.com/themadorg/madmail (user guide: DNS and mail authentication)'#13#10#13#10 +
    'Port 80 must be free during install if using Let''s Encrypt.');
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = IdentityPage.ID then
  begin
    if IsLocalMode and (Trim(IdentityPage.Values[0]) = '') then
      IdentityPage.Values[0] := '127.0.0.1';
  end;
  if CurPageID = TlsPage.ID then
  begin
    if IsLocalMode then
      TlsPage.SelectedValueIndex := 0;
  end;
end;

{ Summarize install.log when the configure step failed (service never registered). }
function InstallLogFailureHint(const LogPath: String): String;
var
  Content: AnsiString;
  S: String;
begin
  Result := '';
  if not LoadStringFromFile(LogPath, Content) then
    Exit;
  S := String(Content);
  if (Pos('Let''s Encrypt', S) > 0) or (Pos('HTTP-01', S) > 0) or (Pos('auto-ip-cert', S) > 0) then
  begin
    Result :=
      'Configure failed during Let''s Encrypt (HTTP-01 / TLS certificate).'#13#10 +
      'Install stopped before registering the Windows service — that is expected when TLS fails.'#13#10#13#10 +
      'Typical fixes: open inbound TCP 80 (Windows Firewall + cloud panel), ensure this host owns the public IP, '#13#10 +
      're-test reachability, or re-run the wizard with Self-signed TLS for a lab install.';
    Exit;
  end;
  if (Pos('Install incomplete', S) > 0) or (Pos('configuration error', S) > 0) or (Pos('Error:', S) > 0) then
  begin
    Result :=
      'Configure (madmail install) failed — see install.log for the exact Error.'#13#10 +
      'The Windows service is not registered because install did not finish.';
    Exit;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  LogPath: String;
  ResultCode: Integer;
  Hint: String;
  Msg: String;
begin
  if CurStep = ssPostInstall then
  begin
    LogPath := ExpandConstant('{commonappdata}\Madmail\install.log');
    FinishedMsg :=
      'Madmail is installed under:'#13#10 +
      '  App:   ' + ExpandConstant('{app}') + #13#10 +
      '  Config: ' + ConfigDir + #13#10 +
      '  State:  ' + StateDir + #13#10#13#10 +
      'Service name: {#MyServiceName}'#13#10 +
      'Admin token file: ' + StateDir + '\admin_token'#13#10 +
      'Install log: ' + LogPath + #13#10#13#10 +
      'If the service is missing, open an elevated cmd and run:'#13#10 +
      '  "' + ExpandConstant('{app}\{#MyAppExeName}') + '" --config "' + ConfigDir + '\madmail.conf" --state-dir "' + StateDir + '" service install --start'#13#10#13#10 +
      'Useful commands:'#13#10 +
      '  madmail service status'#13#10 +
      '  madmail admin-token'#13#10 +
      '  madmail-tray --smoke-exit';

    { Verify SCM registration; explain configure failures (e.g. LE) vs silent AV blocks. }
    if Exec(ExpandConstant('{cmd}'),
      '/C sc query {#MyServiceName} >nul 2>&1',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    begin
      if ResultCode <> 0 then
      begin
        Hint := InstallLogFailureHint(LogPath);
        if Hint <> '' then
          Msg := Hint + #13#10#13#10 + 'Full log: ' + LogPath
        else
          Msg :=
            'Madmail files were installed, but the Windows service "{#MyServiceName}" is not registered.'#13#10#13#10 +
            'This often happens if configure failed (see install.log), antivirus blocked the configure step, '#13#10 +
            'or the installer was not elevated.'#13#10#13#10 +
            'See log: ' + LogPath;
        Msg := Msg + #13#10#13#10 +
          'After fixing TLS/config, register the service (elevated Command Prompt):'#13#10 +
          '"' + ExpandConstant('{app}\{#MyAppExeName}') + '" --config "' + ConfigDir + '\madmail.conf" --state-dir "' + StateDir + '" service install --start';
        MsgBox(Msg, mbError, MB_OK);
      end;
    end;
  end;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  S: String;
begin
  S := 'Mode: ';
  case ModePage.SelectedValueIndex of
    0: S := S + 'Local / lab';
    1: S := S + 'Public IP';
    2: S := S + 'Domain';
  end;
  S := S + NewLine + 'Identity: ' + GetIdentity + NewLine;
  if TlsPage.SelectedValueIndex = 0 then
    S := S + 'TLS: self-signed' + NewLine
  else
    S := S + 'TLS: Let''s Encrypt (' + GetAcmeEmail + ')' + NewLine;
  S := S + 'Language: ' + GetLangCode + NewLine + NewLine;
  S := S + MemoDirInfo + NewLine + NewLine + MemoTasksInfo;
  Result := S;
end;
