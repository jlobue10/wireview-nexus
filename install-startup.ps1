# Registers the WireView -> Nexus daemon to start (hidden) at login for the current user.
# Run once from this folder:  powershell -ExecutionPolicy Bypass -File install-startup.ps1 [-Layout combined] [-ExtraArgs "--wire-limit 10.5"]
# Remove with:                 powershell -ExecutionPolicy Bypass -File install-startup.ps1 -Uninstall
param(
    [switch]$Uninstall,
    [ValidateSet('combined', 'per-wire', 'total-current', 'total-power')][string]$Layout = 'combined',
    [string]$ExtraArgs = ''
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$script = Join-Path $here 'nexus_wireview.py'
$startup = [Environment]::GetFolderPath('Startup')
$lnk = Join-Path $startup 'WireView Nexus.lnk'

if ($Uninstall) {
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
Write-Host "Starting it now..."
Start-Process -FilePath $pythonw -ArgumentList $args -WorkingDirectory $here -WindowStyle Hidden
