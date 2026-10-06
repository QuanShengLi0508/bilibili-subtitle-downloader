; Build from the repository root with Inno Setup 6:
;   ISCC.exe installer\installer.iss
#define MyAppName "拾文"
#define MyAppVersion "1.5.3"
#define MyAppPublisher "QuanShengLi0508"
#define MyAppExeName "拾文.exe"

[Setup]
AppId={{7C7F0BE5-0F65-4D1C-BD08-35F90A21D073}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\BilibiliSubtitleVideoDownloader
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=Shiwen-1.5.3-Setup-x64
Compression=lzma2/fast
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\subtitle-extractor.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

[Languages]
Name: "chinesesimp"; MessagesFile: "ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\拾文.exe"; DestName: "{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\bili-subtitle-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\tools\node.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\yt-dlp.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\whisper\whisper-cli.exe"; DestDir: "{app}\whisper"; Flags: ignoreversion
Source: "..\whisper\ggml-base-q5_1.bin"; DestDir: "{app}\whisper"; Flags: ignoreversion

Source: "..\tools\ffmpeg.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffprobe.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\whisper\*.dll"; DestDir: "{app}\whisper"; Flags: ignoreversion
Source: "..\runtime\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\runtime\*.dll"; DestDir: "{app}\whisper"; Flags: ignoreversion
Source: "..\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Run]
Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

; User-generated documents are retained on uninstall.
