$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$previousTargetDir = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'BiliSubtitleDownloader\build'

Push-Location $projectRoot
try {
    cargo build --release --bins
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }

    $builtExe = Join-Path $env:CARGO_TARGET_DIR 'release\bili-subtitle-downloader.exe'
    $builtCli = Join-Path $env:CARGO_TARGET_DIR 'release\bili-subtitle-cli.exe'
    $rootExe = Join-Path $projectRoot '拾文.exe'
    Copy-Item -LiteralPath $builtExe -Destination $rootExe -Force
    Copy-Item -LiteralPath $builtCli -Destination (Join-Path $projectRoot 'bili-subtitle-cli.exe') -Force

    $packageDir = Join-Path $projectRoot 'release-package'
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Copy-Item -LiteralPath $builtExe -Destination (Join-Path $packageDir '拾文.exe') -Force
    Copy-Item -LiteralPath $builtExe -Destination (Join-Path $packageDir 'bili-subtitle-downloader.exe') -Force
    Copy-Item -LiteralPath $builtCli -Destination (Join-Path $packageDir 'bili-subtitle-cli.exe') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $packageDir -Force
    foreach ($relative in @('tools\yt-dlp.exe', 'tools\ffmpeg.exe', 'tools\ffprobe.exe', 'whisper\whisper-cli.exe', 'whisper\ggml-base-q5_1.bin')) {
        $source = Join-Path $projectRoot $relative
        if (Test-Path -LiteralPath $source) {
            $destination = Join-Path $packageDir $relative
            New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
            Copy-Item -LiteralPath $source -Destination $destination -Force
        }
    }
    Copy-Item -Path (Join-Path $projectRoot 'whisper\*.dll') -Destination (Join-Path $packageDir 'whisper') -Force
    Copy-Item -Path (Join-Path $projectRoot 'runtime\*.dll') -Destination $packageDir -Force
    Copy-Item -Path (Join-Path $projectRoot 'runtime\*.dll') -Destination (Join-Path $packageDir 'whisper') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'licenses') -Destination $packageDir -Recurse -Force
    Write-Host "Release ready: $rootExe"
    Write-Host "Package ready: $packageDir"
} finally {
    Pop-Location
    if ($null -eq $previousTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousTargetDir
    }
}
