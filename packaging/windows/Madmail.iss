; Madmail Windows installer (Inno Setup 6+)
; Build:  .\build-setup.ps1 -Arch amd64
;         iscc /DArch=arm64 Madmail.iss
;
; Expects pre-built binaries under repo build/:
;   madmail-windows-{amd64,arm64}.exe
;   madmail-tray-windows-{amd64,arm64}.exe (optional)

#ifndef Arch
  #define Arch "amd64"
#endif

#define MyAppName "Madmail"
#define MyAppVersion "2.17.3"
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
AppId={{8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={autopf}\{#MyAppName}
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
; Tray helper (skip if not built yet)
Source: "{#SourceTray}"; DestDir: "{app}"; DestName: "{#MyTrayExeName}"; Flags: ignoreversion skipifsourcedoesntexist

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
; Best-effort: stop service + firewall; keep data by default (see UninstallKeepData)
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
  if FeaturePage.Values[1] then
    Args := Args + ' --enable-iroh';

  Args := Args + ' --install-service';
  if WizardIsTaskSelected('startservice') then
    Args := Args + ' --start-service';
  if WizardIsTaskSelected('firewall') then
    Args := Args + ' --firewall';

  Result := Args;
  Log('Install CLI: madmail ' + Args);
end;

function UninstallCliArgs(Param: String): String;
begin
  { Keep mail data by default; operators can wipe ProgramData manually }
  Result := 'uninstall --force --keep-data --keep-binary';
  Result := Result + ' --config ' + Quoted(ConfigDir + '\madmail.conf');
  Result := Result + ' --state-dir ' + Quoted(StateDir);
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
  FeaturePage.Add('Iroh relay discovery');
  FeaturePage.Values[0] := True;
  FeaturePage.Values[1] := False;

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

procedure CurStepChanged(CurStep: TSetupStep);
var
  LogPath: String;
  ResultCode: Integer;
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

    { Verify SCM registration; offer repair if configure step failed silently. }
    if Exec(ExpandConstant('{cmd}'),
      '/C sc query {#MyServiceName} >nul 2>&1',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    begin
      if ResultCode <> 0 then
      begin
        MsgBox(
          'Madmail files were installed, but the Windows service "{#MyServiceName}" is not registered.'#13#10#13#10 +
          'This often happens if antivirus blocked the configure step, or the installer was not elevated.'#13#10#13#10 +
          'See log: ' + LogPath + #13#10#13#10 +
          'Repair (elevated Command Prompt):'#13#10 +
          '"' + ExpandConstant('{app}\{#MyAppExeName}') + '" --config "' + ConfigDir + '\madmail.conf" --state-dir "' + StateDir + '" service install --start',
          mbError, MB_OK);
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
