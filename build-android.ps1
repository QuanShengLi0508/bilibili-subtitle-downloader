param([string]$Flutter = "$env:LOCALAPPDATA\ShiwenBuild\flutter\bin\flutter.bat")
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $Flutter)) { throw '请安装 Flutter，并通过 -Flutter 指定 flutter.bat 路径。' }
$mobile = Join-Path $PSScriptRoot 'mobile'
$stage = Join-Path $env:LOCALAPPDATA 'ShiwenBuild\app'
if (!(Test-Path (Join-Path $mobile 'android\key.properties'))) { throw '请先配置 mobile/android/key.properties 发布签名。' }
New-Item -ItemType Directory -Force $stage | Out-Null
& robocopy $mobile $stage /E /XD .dart_tool build dist .git .cxx .gradle /XF .flutter-plugins-dependencies /NFL /NDL /NJH /NJS /NP
if ($LASTEXITCODE -ge 8) { throw '复制构建源码失败。' }
$gradleProperties = Join-Path $stage 'android\gradle.properties'
Add-Content $gradleProperties "`norg.gradle.jvmargs=-Xmx4G -XX:MaxMetaspaceSize=2G -Dfile.encoding=UTF-8"
$proxy = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
if ($proxy.ProxyEnable -eq 1 -and $proxy.ProxyServer -match '^([^:;=]+):(\d+)$') {
    $proxyHost = $Matches[1]; $proxyPort = $Matches[2]
    Add-Content $gradleProperties "`nsystemProp.https.proxyHost=$proxyHost`nsystemProp.https.proxyPort=$proxyPort`nsystemProp.http.proxyHost=$proxyHost`nsystemProp.http.proxyPort=$proxyPort"
}
Push-Location $stage
try {
    & $Flutter pub get
    if ($LASTEXITCODE) { throw '获取手机依赖失败。' }
    & $Flutter build apk --release --target-platform android-arm64
    if ($LASTEXITCODE) { throw '安卓安装包构建失败。' }
} finally { Pop-Location }
$dist = Join-Path $mobile 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$apk = Join-Path $dist 'Shiwen-1.0.1-android-arm64.apk'
Copy-Item (Join-Path $stage 'build\app\outputs\flutter-apk\app-release.apk') $apk -Force
$hash = (Get-FileHash -LiteralPath $apk -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content "$apk.sha256" "$hash  Shiwen-1.0.1-android-arm64.apk" -Encoding ascii
Write-Host "安卓安装包：$apk"

