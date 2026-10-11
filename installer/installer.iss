; Build from the repository root with build-installer.ps1 to stage FFmpeg first:
;   .\build-installer.ps1
#define MyAppName "拾文"
#define MyAppVersion "1.6.3"
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
OutputBaseFilename=Shiwen-1.6.3-Setup-x64
Compression=lzma2/ultra64
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

; FFmpeg and FFprobe share the same codec libraries.
Source: "..\tools\ffmpeg-shared\ffmpeg.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\ffprobe.exe"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\avcodec-63.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\avdevice-63.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\avfilter-12.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\avformat-63.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\avutil-61.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\swresample-7.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\tools\ffmpeg-shared\swscale-10.dll"; DestDir: "{app}\tools"; Flags: ignoreversion
Source: "..\whisper\*.dll"; DestDir: "{app}\whisper"; Flags: ignoreversion
Source: "..\runtime\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\runtime\*.dll"; DestDir: "{app}\whisper"; Flags: ignoreversion
Source: "..\diarization\*"; DestDir: "{app}\diarization"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Run]
Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

; User-generated documents are retained on uninstall.
