$ErrorActionPreference = "Stop"

$Repository = "eric-stone-plus/QUINTE"
$Version = if ($env:QUINTE_VERSION) { $env:QUINTE_VERSION } else { "latest" }
$InstallDir = if ($env:QUINTE_INSTALL_DIR) {
    $env:QUINTE_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Programs\quinte\bin"
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "quinte: 32-bit Windows is not supported"
}
$Asset = "quinte-x86_64-pc-windows-msvc.zip"
$BaseUrl = if ($Version -eq "latest") {
    "https://github.com/$Repository/releases/latest/download"
} else {
    "https://github.com/$Repository/releases/download/$Version"
}

$TempDir = Join-Path ([IO.Path]::GetTempPath()) ("quinte-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
    $Archive = Join-Path $TempDir $Asset
    $Checksums = Join-Path $TempDir "SHA256SUMS"
    Write-Host "quinte: downloading $Asset"
    Invoke-WebRequest -UseBasicParsing "$BaseUrl/$Asset" -OutFile $Archive
    Invoke-WebRequest -UseBasicParsing "$BaseUrl/SHA256SUMS" -OutFile $Checksums

    $Line = Get-Content $Checksums | Where-Object { $_ -match ("\s" + [Regex]::Escape($Asset) + "$") } | Select-Object -First 1
    if (-not $Line) { throw "quinte: $Asset is missing from SHA256SUMS" }
    $Expected = ($Line -split "\s+")[0].ToLowerInvariant()
    $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) { throw "quinte: checksum verification failed" }

    Expand-Archive -Path $Archive -DestinationPath $TempDir -Force
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item (Join-Path $TempDir "quinte.exe") (Join-Path $InstallDir "quinte.exe") -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $Parts = @($UserPath -split ";" | Where-Object { $_ })
    if ($Parts -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable("Path", (($Parts + $InstallDir) -join ";"), "User")
        Write-Host "quinte: added $InstallDir to your user PATH; open a new terminal"
    }
    $env:Path = "$InstallDir;$env:Path"

    Write-Host "quinte: installed $(Join-Path $InstallDir 'quinte.exe')"
    $HomeRoot = if ($env:QUINTE_HOME) { $env:QUINTE_HOME } else { Join-Path $HOME ".quinte" }
    if (-not (Test-Path (Join-Path $HomeRoot "policy.json"))) {
        & (Join-Path $InstallDir "quinte.exe") init
    }
    Write-Host "quinte: run 'quinte doctor' to verify the fixed agent environment"
    Write-Host "quinte: provision target xiaomi-mimo-token-plan-api-key.quinte in Windows Credential Manager, then run 'quinte credential status'"
    Write-Host "quinte: ANTHROPIC_API_KEY remains a non-isolated fallback only"
} finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
