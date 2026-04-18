<#
.SYNOPSIS
    Sign Windows executables and MSI files with an EV code signing certificate.

.DESCRIPTION
    Decodes the base64-encoded PFX from the WINDOWS_SIGNING_CERT_BASE64 environment
    variable, writes it to a temp file, signs all .exe and .msi files passed as
    arguments with signtool.exe (SHA-256 digest, RFC 3161 timestamp via DigiCert TSA),
    verifies each signature, then deletes the temp PFX.

    Required environment variables:
      WINDOWS_SIGNING_CERT_BASE64  — base64-encoded PFX certificate
      WINDOWS_SIGNING_PASSWORD     — PFX password

.PARAMETER Files
    One or more paths to .exe or .msi files to sign. Wildcards are expanded by the
    shell before this script sees them.

.EXAMPLE
    .\scripts\sign-windows.ps1 artifacts\ta.exe artifacts\ta-daemon.exe artifacts\ta-v0.15.16-alpha.msi

.NOTES
    Idempotent: safe to re-run on already-signed files (signtool replaces the
    existing signature rather than stacking dual signatures when /as is omitted).
#>
param(
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
    [string[]]$Files
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ── Validate required secrets ─────────────────────────────────────────────────
$certBase64 = $env:WINDOWS_SIGNING_CERT_BASE64
$certPassword = $env:WINDOWS_SIGNING_PASSWORD

if ([string]::IsNullOrEmpty($certBase64)) {
    Write-Error "WINDOWS_SIGNING_CERT_BASE64 is not set. Add the EV certificate as a GitHub Actions secret."
    exit 1
}
if ([string]::IsNullOrEmpty($certPassword)) {
    Write-Error "WINDOWS_SIGNING_PASSWORD is not set. Add the PFX password as a GitHub Actions secret."
    exit 1
}

# ── Locate signtool.exe ───────────────────────────────────────────────────────
# windows-latest ships Windows SDK; search standard install paths.
$signtoolCandidates = @(
    "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe",
    "C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\signtool.exe",
    "C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe"
)

$signtool = $null
foreach ($candidate in $signtoolCandidates) {
    if (Test-Path $candidate) {
        $signtool = $candidate
        break
    }
}

# Fall back to PATH lookup.
if (-not $signtool) {
    $signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue)?.Source
}

if (-not $signtool) {
    # Try vswhere to locate the SDK.
    $sdkRoot = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" `
        -latest -requires Microsoft.Component.MSBuild -property installationPath 2>$null
    if ($sdkRoot) {
        $found = Get-ChildItem -Path $sdkRoot -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($found) { $signtool = $found.FullName }
    }
}

if (-not $signtool) {
    Write-Error "signtool.exe not found. Install the Windows SDK (part of Visual Studio or standalone)."
    exit 1
}
Write-Host "signtool: $signtool"

# ── Write PFX to temp file ────────────────────────────────────────────────────
$tempPfx = [System.IO.Path]::GetTempFileName() + ".pfx"
try {
    $pfxBytes = [Convert]::FromBase64String($certBase64)
    [System.IO.File]::WriteAllBytes($tempPfx, $pfxBytes)
    Write-Host "PFX written to temp file (will be deleted after signing)"

    # ── DigiCert RFC 3161 timestamp server ────────────────────────────────────
    # Using DigiCert's TSA so the signature remains valid after cert expiry.
    $tsaUrl = "http://timestamp.digicert.com"

    $failed = @()

    foreach ($file in $Files) {
        if (-not (Test-Path $file)) {
            Write-Warning "File not found, skipping: $file"
            continue
        }

        $ext = [System.IO.Path]::GetExtension($file).ToLower()
        if ($ext -notin @('.exe', '.msi')) {
            Write-Warning "Skipping non-exe/msi file: $file"
            continue
        }

        Write-Host ""
        Write-Host "──── Signing: $file ────"

        # Sign with SHA-256 digest + RFC 3161 timestamp.
        & $signtool sign `
            /fd sha256 `
            /tr $tsaUrl `
            /td sha256 `
            /f $tempPfx `
            /p $certPassword `
            /v `
            $file

        if ($LASTEXITCODE -ne 0) {
            Write-Error "signtool sign failed for: $file (exit $LASTEXITCODE)"
            $failed += $file
            continue
        }

        # Verify the signature immediately after signing.
        Write-Host "Verifying: $file"
        & $signtool verify /pa /v $file

        if ($LASTEXITCODE -ne 0) {
            Write-Error "signtool verify failed for: $file (exit $LASTEXITCODE)"
            $failed += $file
        } else {
            Write-Host "OK: $file"
        }
    }

    if ($failed.Count -gt 0) {
        Write-Error "Signing failed for $($failed.Count) file(s):`n  $($failed -join "`n  ")"
        exit 1
    }

    Write-Host ""
    Write-Host "All files signed and verified successfully."

} finally {
    # Always delete the temp PFX, even on error.
    if (Test-Path $tempPfx) {
        Remove-Item -Force $tempPfx
        Write-Host "Temp PFX deleted."
    }
}
