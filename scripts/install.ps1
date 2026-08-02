[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^v\d+\.\d+\.\d+$')][string]$Version,
    [string]$InstallDir = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "Programs\json-state-tx"),
    [string]$CodexHome,
    [switch]$InstallSkill,
    [switch]$AddToUserPath
)

$ErrorActionPreference = "Stop"
$repo = "starSumi/json-state-tx"
$asset = "json-state-tx-$Version-x86_64-pc-windows-msvc.zip"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "json-state-tx-$([Guid]::NewGuid().ToString('N'))"
[System.IO.Directory]::CreateDirectory($tempRoot) | Out-Null

try {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI (gh) is required to download and verify the release attestation."
    }
    gh release download $Version --repo $repo --pattern $asset --pattern SHA256SUMS --dir $tempRoot
    if ($LASTEXITCODE -ne 0) { throw "Release download failed." }

    $expectedLine = Get-Content -LiteralPath (Join-Path $tempRoot "SHA256SUMS") | Where-Object { $_ -match [regex]::Escape($asset) }
    if (-not $expectedLine) { throw "SHA256SUMS does not contain $asset" }
    $expected = ($expectedLine -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tempRoot $asset)).Hash.ToLowerInvariant()
    if ($actual -cne $expected) { throw "Checksum mismatch for $asset" }

    gh attestation verify (Join-Path $tempRoot $asset) --repo $repo | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Release attestation verification failed." }

    $extract = Join-Path $tempRoot "extract"
    Expand-Archive -LiteralPath (Join-Path $tempRoot $asset) -DestinationPath $extract
    [System.IO.Directory]::CreateDirectory($InstallDir) | Out-Null
    Copy-Item -LiteralPath (Join-Path $extract "json-state-tx.exe") -Destination (Join-Path $InstallDir "json-state-tx.exe") -Force

    if ($AddToUserPath) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $parts = @($userPath -split ';' | Where-Object { $_ })
        if ($InstallDir -notin $parts) {
            [Environment]::SetEnvironmentVariable("Path", (($parts + $InstallDir) -join ';'), "User")
        }
        if ($env:Path -notlike "*$InstallDir*") { $env:Path = "$InstallDir;$env:Path" }
    }

    if ($InstallSkill) {
        if (-not $CodexHome) {
            $CodexHome = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path ([Environment]::GetFolderPath("UserProfile")) ".codex" }
        }
        $source = Join-Path $extract "skills\json-state-tx"
        $codexRoot = [System.IO.Path]::GetFullPath($CodexHome)
        $skillsRoot = [System.IO.Path]::GetFullPath((Join-Path $codexRoot "skills"))
        $target = [System.IO.Path]::GetFullPath((Join-Path $skillsRoot "json-state-tx"))
        if ([System.IO.Path]::GetDirectoryName($target) -cne $skillsRoot) {
            throw "Refusing to replace a skill outside the requested Codex skills directory."
        }
        [System.IO.Directory]::CreateDirectory((Split-Path -Parent $target)) | Out-Null
        if (Test-Path -LiteralPath $target) {
            $existing = Get-Item -LiteralPath $target -Force
            if ($existing.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                Remove-Item -LiteralPath $target -Force
            }
            else {
                Remove-Item -LiteralPath $target -Recurse -Force
            }
        }
        Copy-Item -LiteralPath $source -Destination $target -Recurse
        Write-Host "Installed Codex skill: $target"
        Write-Host "Restart Codex to load the skill."
    }

    & (Join-Path $InstallDir "json-state-tx.exe") --version
}
finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
}
