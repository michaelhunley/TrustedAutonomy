# ta-shell.ps1 -- Start the TA daemon (if needed) and launch the interactive shell.
#
# Usage:
#   .\scripts\ta-shell.ps1 [-Port 7700] [-ProjectRoot .] [shell args...]
#
# The script checks whether the daemon is already listening. If not, it starts
# one in the background and waits for it to become healthy before opening the
# shell. On exit, the daemon keeps running.

param(
    [int]$Port = $(if ($env:TA_DAEMON_PORT) { [int]$env:TA_DAEMON_PORT } else { 7700 }),
    [string]$Bind = $(if ($env:TA_DAEMON_BIND) { $env:TA_DAEMON_BIND } else { "127.0.0.1" }),
    [string]$ProjectRoot = ".",
    [Parameter(ValueFromRemainingArguments)]
    [string[]]$ShellArgs
)

$ErrorActionPreference = "Stop"
$DaemonUrl = "http://${Bind}:${Port}"

# Locate binaries. Prefer siblings of this script, then PATH.
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
function Find-Binary($Name) {
    $release = Join-Path $ScriptDir "..\target\release\$Name.exe"
    $debug = Join-Path $ScriptDir "..\target\debug\$Name.exe"
    if (Test-Path $release) { return $release }
    if (Test-Path $debug) { return $debug }
    $found = Get-Command $Name -ErrorAction SilentlyContinue
    if ($found) { return $found.Source }
    return $Name
}

$TaBin = Find-Binary "ta"
$DaemonBin = Find-Binary "ta-daemon"

# Check if the daemon is already running.
function Test-DaemonHealthy {
    try {
        $null = Invoke-RestMethod -Uri "$DaemonUrl/api/status" -TimeoutSec 2
        return $true
    } catch {
        return $false
    }
}

if (Test-DaemonHealthy) {
    Write-Host "Daemon already running at $DaemonUrl"
} else {
    Write-Host "Starting daemon at $DaemonUrl ..."
    $daemonProcess = Start-Process -FilePath $DaemonBin `
        -ArgumentList "--api", "--project-root", $ProjectRoot `
        -PassThru -NoNewWindow

    # Wait up to 10 seconds for the daemon to become healthy.
    $healthy = $false
    for ($i = 0; $i -lt 20; $i++) {
        if ($daemonProcess.HasExited) {
            Write-Error "Daemon exited unexpectedly (exit code $($daemonProcess.ExitCode))"
            exit 1
        }
        if (Test-DaemonHealthy) {
            Write-Host "Daemon ready (pid $($daemonProcess.Id))"
            $healthy = $true
            break
        }
        Start-Sleep -Milliseconds 500
    }

    if (-not $healthy) {
        Write-Error "Daemon did not become healthy within 10 seconds"
        Stop-Process -Id $daemonProcess.Id -Force -ErrorAction SilentlyContinue
        exit 1
    }
}

# Launch the shell.
& $TaBin shell --url $DaemonUrl @ShellArgs
