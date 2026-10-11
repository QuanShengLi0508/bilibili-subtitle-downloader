$ErrorActionPreference = 'Stop'

# FFmpeg 9.0.2 full shared build, from the same Gyan release as the previous
# bundled static executables. The fixed hash prevents a changed download from
# silently entering release installers.
$projectRoot = Split-Path $PSScriptRoot -Parent
$downloadUrl = 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-full_build-shared.7z'
$archiveHash = '4D2060A8B34A940AA47D785142055BB92A63053781E55F2ACE4546EDD519A8F5'
$archivePath = Join-Path $projectRoot 'tmp\ffmpeg-9.0.2-full_build-shared.7z'
$stagePath = Join-Path $projectRoot 'tools\ffmpeg-shared'
$expectedFiles = [ordered]@{
    'ffmpeg.exe'       = 'B1A495A71491BF208F95043CA06654AD76D59738533EC8FF237A759E29ABD2A3'
    'ffprobe.exe'      = '26283152241767BB54A09009DE2EEA3408549D1840AB3A0725320253557C0532'
    'avcodec-63.dll'   = '4D458A276FDA363709ED2AC4AD14AEA122C536E61919FEEBD919D5F8490BE7A9'
    'avdevice-63.dll'  = '3E74AA37E9E21DEBCCFAF4AEF24AA2A946854109AC066F78D8C1F984BC82B209'
    'avfilter-12.dll'  = '4EC6EB2DB944740C364D93B29D91C9137BEB38D09E923F6ED14F31AA039851DE'
    'avformat-63.dll'  = '360E66109B2CE3F02D56CD940E625A6909304EEA1F4D0FCBBBB2D40D74308EE0'
    'avutil-61.dll'    = '9230E42B6B8E1FA20EC83AF1EB4C4B2AE90B0C8D7C29093369250EED562372DF'
    'swresample-7.dll' = '1F2A0BDCA315F1A05FE398E7A0D97DB8D0E34BA2A86EC59303BF32BBD9430123'
    'swscale-10.dll'   = 'AF91DB7F45644157469A110438A8C648314927EC83E334D4CACAC3870D99FABE'
}

function Test-ExpectedFiles($directory) {
    foreach ($name in $expectedFiles.Keys) {
        $path = Join-Path $directory $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $expectedFiles[$name]) {
            return $false
        }
    }
    return $true
}

if (Test-ExpectedFiles $stagePath) {
    Write-Host 'Verified FFmpeg shared runtime is ready.'
    return
}

New-Item -ItemType Directory -Path (Split-Path $archivePath -Parent) -Force | Out-Null
if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf) -or
    (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash -ne $archiveHash) {
    $downloadPath = "$archivePath.download"
    & curl.exe -L --fail --retry 3 --silent --show-error --output $downloadPath $downloadUrl
    if ($LASTEXITCODE -ne 0) { throw 'Could not download the pinned FFmpeg shared build.' }
    if ((Get-FileHash -LiteralPath $downloadPath -Algorithm SHA256).Hash -ne $archiveHash) {
        throw 'FFmpeg archive SHA256 does not match the pinned release.'
    }
    Move-Item -LiteralPath $downloadPath -Destination $archivePath -Force
}

$extractRoot = Join-Path $projectRoot 'tmp\ffmpeg-shared-staging'
New-Item -ItemType Directory -Path $extractRoot -Force | Out-Null
& tar.exe -xf $archivePath -C $extractRoot 'ffmpeg-9.0.2-full_build-shared/bin'
if ($LASTEXITCODE -ne 0) { throw 'Could not extract the FFmpeg shared build.' }
$binPath = Join-Path $extractRoot 'ffmpeg-9.0.2-full_build-shared\bin'
if (-not (Test-ExpectedFiles $binPath)) {
    throw 'Extracted FFmpeg files do not match the pinned release.'
}

New-Item -ItemType Directory -Path $stagePath -Force | Out-Null
foreach ($name in $expectedFiles.Keys) {
    Copy-Item -LiteralPath (Join-Path $binPath $name) -Destination (Join-Path $stagePath $name) -Force
}
if (-not (Test-ExpectedFiles $stagePath)) { throw 'Staged FFmpeg verification failed.' }
Write-Host 'Verified FFmpeg shared runtime is ready.'
