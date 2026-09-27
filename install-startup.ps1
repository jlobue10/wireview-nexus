# Registers the WireView -> Nexus daemon to start (hidden) at login for the current user.
# Run once from this folder:  powershell -ExecutionPolicy Bypass -File install-startup.ps1 [-Layout combined] [-ExtraArgs "--wire-limit 10.5"]
# Remove with:                 powershell -ExecutionPolicy Bypass -File install-startup.ps1 -Uninstall
# (install.ps1 calls this for you.)
param(
    [switch]$Uninstall,
    [switch]$NoStart,
    [ValidateSet('combined', 'per-wire', 'total-current', 'total-power')][string]$Layout = 'combined',
    [string]$ExtraArgs = ''
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$script = Join-Path $here 'nexus_wireview.py'
$startup = [Environment]::GetFolderPath('Startup')
$lnk = Join-Path $startup 'WireView Nexus.lnk'

function Stop-Daemon {
    Get-CimInstance Win32_Process -Filter "Name LIKE 'python%'" |   # python.exe, pythonw.exe, pythonw3.12.exe (Store)
        Where-Object { $_.CommandLine -like '*nexus_wireview.py*' } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

if ($Uninstall) {
    Stop-Daemon
    if (Test-Path $lnk) { Remove-Item $lnk; Write-Host "Removed $lnk" } else { Write-Host 'Not installed.' }
    exit 0
}

# Prefer a venv next to the script (python -m venv venv; venv\Scripts\pip install -r requirements.txt)
$pythonw = Join-Path $here 'venv\Scripts\pythonw.exe'
if (-not (Test-Path $pythonw)) {
    $pythonw = (Get-Command pythonw.exe -ErrorAction SilentlyContinue).Source
    if (-not $pythonw) { $pythonw = Join-Path (Split-Path (Get-Command python.exe -ErrorAction Stop).Source) 'pythonw.exe' }
}
if (-not (Test-Path $pythonw)) { throw "pythonw.exe not found; install Python 3.10+ and 'pip install -r requirements.txt'" }

$args = '"' + $script + '" --layout ' + $Layout
if ($ExtraArgs) { $args += ' ' + $ExtraArgs }

$ws = New-Object -ComObject WScript.Shell
$s = $ws.CreateShortcut($lnk)
$s.TargetPath = $pythonw
$s.Arguments = $args
$s.WorkingDirectory = $here
$s.WindowStyle = 7
$s.Description = 'Shows WireView Pro II readings on the iCUE Nexus'
$s.Save()
Write-Host "Installed: $lnk  ($pythonw $args)"
if ($NoStart) { exit 0 }
Stop-Daemon   # replace a running copy so new options take effect
Write-Host "Starting it now..."
Start-Process -FilePath $pythonw -ArgumentList $args -WorkingDirectory $here -WindowStyle Hidden
