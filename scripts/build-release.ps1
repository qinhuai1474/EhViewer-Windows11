param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$userRoot = [System.IO.Path]::GetFullPath($env:USERPROFILE).TrimEnd("\")
$repoPath = [System.IO.Path]::GetFullPath($repoRoot).TrimEnd("\")
$flagSeparator = [char]31

$rustFlags = @()
if ($env:CARGO_ENCODED_RUSTFLAGS) {
    $rustFlags = @($env:CARGO_ENCODED_RUSTFLAGS -split $flagSeparator)
}

# Longest project path first, then the full user profile.
$rustFlags += "--remap-path-prefix=$repoPath=C:\build\EhViewer-Windows11"
$rustFlags += "--remap-path-prefix=$userRoot=C:\build\user"
$env:CARGO_ENCODED_RUSTFLAGS = $rustFlags -join $flagSeparator

Push-Location $repoRoot
try {
    & pnpm tauri build
    if ($LASTEXITCODE -ne 0) {
        throw "pnpm tauri build failed with exit code $LASTEXITCODE"
    }

    $exe = Join-Path $repoRoot "src-tauri\target\release\ehviewer-windows11.exe"
    $bytes = [System.IO.File]::ReadAllBytes($exe)
    $text = [System.Text.Encoding]::UTF8.GetString($bytes)
    $leaks = @($repoPath, $userRoot) | Where-Object {
        $text.IndexOf($_, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
    }
    if ($leaks.Count -gt 0) {
        throw "Release executable still contains a local path; refusing a shareable build."
    }

    $setup = Get-ChildItem "src-tauri\target\release\bundle\nsis\*_x64-setup.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

    $signatureCommand = $null
    try {
        Import-Module Microsoft.PowerShell.Security -ErrorAction Stop
        $signatureCommand = Get-Command Get-AuthenticodeSignature -ErrorAction Stop
    }
    catch {
        Write-Warning "Authenticode status could not be checked in this PowerShell session."
    }
    if ($signatureCommand) {
        $artifacts = @($exe, $setup.FullName) | Where-Object { $_ -and (Test-Path $_) }
        foreach ($artifact in $artifacts) {
            $signature = Get-AuthenticodeSignature -LiteralPath $artifact
            if ($signature.Status -ne "Valid") {
                Write-Warning "Not validly signed: $artifact. Do not publish until code signing is configured."
            }
        }
    }

    Write-Host "Sanitized release executable: $exe"
    if ($setup) {
        Write-Host "NSIS installer: $($setup.FullName)"
    }
}
finally {
    Pop-Location
}
