$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    & (Join-Path $PSScriptRoot 'build-release.ps1')
    $compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($compiler) { $compilerPath = $compiler.Source } else {
        $compilerPath = @(
            (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
            (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
        ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
    if (-not $compilerPath) { throw 'Please install Inno Setup 6 before building the installer.' }
    & $compilerPath (Join-Path $PSScriptRoot 'installer\installer.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
} finally { Pop-Location }
