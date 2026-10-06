param([string]$Proxy)
$ErrorActionPreference = 'Stop'
$taskProject = Split-Path $PSScriptRoot -Parent
$taskCache = Join-Path $env:LOCALAPPDATA 'ShiwenBuild\speaker-resources'
$taskDestination = Join-Path $taskProject 'diarization'
New-Item -ItemType Directory -Path $taskCache,$taskDestination -Force | Out-Null
$taskAssets = Get-Content -LiteralPath (Join-Path $taskProject 'assets\speaker-resources.json') -Raw | ConvertFrom-Json
foreach ($taskAsset in $taskAssets) {
    $taskFile = Join-Path $taskCache $taskAsset.name
    if (!(Test-Path -LiteralPath $taskFile)) {
        $taskDownload = @{Uri=$taskAsset.url;OutFile="$taskFile.part";TimeoutSec=600}
        if ($Proxy) { $taskDownload.Proxy=$Proxy }
        Invoke-WebRequest @taskDownload
        if ((Get-FileHash -LiteralPath "$taskFile.part").Hash -ne $taskAsset.sha256) { throw "Speaker asset checksum mismatch: $($taskAsset.name)" }
        Move-Item -LiteralPath "$taskFile.part" -Destination $taskFile
    }
    if ((Get-FileHash -LiteralPath $taskFile).Hash -ne $taskAsset.sha256) { throw "Speaker asset checksum mismatch: $($taskAsset.name)" }
    if ($taskAsset.name.EndsWith('.tar.bz2')) {
        & tar -xf $taskFile -C $taskCache
        if ($LASTEXITCODE) { throw 'Speaker asset extraction failed' }
    }
}
$taskBin = Join-Path $taskCache 'sherpa-onnx-v1.13.8-win-x64-shared-MD-Release-no-tts\bin'
Copy-Item -LiteralPath (Join-Path $taskBin 'sherpa-onnx-offline-speaker-diarization.exe') -Destination $taskDestination -Force
Copy-Item -Path (Join-Path $taskBin '*.dll') -Destination $taskDestination -Force
Copy-Item -Path (Join-Path $taskProject 'runtime\*.dll') -Destination $taskDestination -Force
Copy-Item -LiteralPath (Join-Path $taskCache 'sherpa-onnx-pyannote-segmentation-3-0\model.onnx') -Destination (Join-Path $taskDestination 'segmentation.onnx') -Force
Copy-Item -LiteralPath (Join-Path $taskCache $taskAssets[2].name) -Destination (Join-Path $taskDestination 'embedding.onnx') -Force
Write-Output 'Local speaker recognition resources ready.'
