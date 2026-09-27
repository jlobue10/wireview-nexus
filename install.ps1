# One-command installer for wireview-nexus (WireView Pro II -> iCUE Nexus).
#
# From anywhere (downloads the latest main branch into %LOCALAPPDATA%\wireview-nexus):
#   powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/jlobue10/wireview-nexus/main/install.ps1 | iex"
# With options:
#   powershell -ExecutionPolicy Bypass -c "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jlobue10/wireview-nexus/main/install.ps1))) -Layout per-wire"
# From a clone (installs in place):
#   powershell -ExecutionPolicy Bypass -File install.ps1 [-Layout combined] [-ExtraArgs '--fps 4']
# Remove:
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Uninstall
#
# What it does: finds Python 3.10+ (installs it with winget if missing), creates a venv,
# installs hidapi/Pillow/pyserial, registers the daemon to start hidden at login, starts it.
param(
    [ValidateSet('combined', 'per-wire', 'total-current', 'total-power')][string]$Layout = 'combined',
    [string]$ExtraArgs = '',
    [string]$Dir = '',
    [switch]$Uninstall,
    [switch]$NoStart
)

$ErrorActionPreference = 'Stop'
$Repo = 'jlobue10/wireview-nexus'
$Name = 'wireview-nexus'
$Main = 'nexus_wireview.py'
$Shortcut = 'WireView Nexus.lnk'

function Say($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }

# --- where to install -------------------------------------------------------
$scriptDir = if ($PSCommandPath) { Split-Path -Parent $PSCommandPath } else { '' }
$inPlace = $scriptDir -and (Test-Path (Join-Path $scriptDir $Main))
if (-not $Dir) { $Dir = if ($inPlace) { $scriptDir } else { Join-Path $env:LOCALAPPDATA $Name } }

$startup = [Environment]::GetFolderPath('Startup')
$lnk = Join-Path $startup $Shortcut

if ($Uninstall) {
    Say "Stopping $Name"
    Get-CimInstance Win32_Process -Filter "Name LIKE 'python%'" |   # python.exe, pythonw.exe, pythonw3.12.exe (Store)
        Where-Object { $_.CommandLine -like "*$Main*" } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    if (Test-Path $lnk) { Remove-Item $lnk; Say "Removed startup entry $lnk" }
    if (-not $inPlace -and (Test-Path $Dir)) { Remove-Item -Recurse -Force $Dir; Say "Removed $Dir" }
    Say 'Uninstalled.'
    exit 0
}

# --- get the files ----------------------------------------------------------
if (-not $inPlace) {
    Say "Downloading $Repo to $Dir"
    $zip = Join-Path $env:TEMP "$Name.zip"
    Invoke-WebRequest -UseBasicParsing "https://github.com/$Repo/archive/refs/heads/main.zip" -OutFile $zip
    $tmp = Join-Path $env:TEMP "$Name-extract"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Archive $zip -DestinationPath $tmp
    $src = Get-ChildItem $tmp | Select-Object -First 1
    New-Item -ItemType Directory -Force $Dir | Out-Null
    # Keep an existing venv; refresh everything else.
    Get-ChildItem $src.FullName | ForEach-Object { Copy-Item -Recurse -Force $_.FullName $Dir }
    Remove-Item -Recurse -Force $tmp, $zip
}
Set-Location $Dir

# --- python -----------------------------------------------------------------
function Find-Python {
    foreach ($cand in @('py -3', 'python', 'python3')) {
        try {
            $v = & cmd /c "$cand -c `"import sys;print(sys.version_info[0]*100+sys.version_info[1])`"" 2>$null
            if ($LASTEXITCODE -eq 0 -and [int]$v -ge 310) { return $cand }
        } catch {}
    }
    return $null
}
$py = Find-Python
if (-not $py) {
    Say 'Python 3.10+ not found; installing with winget'
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw 'winget is not available. Install Python 3.10+ from https://www.python.org/downloads/ (tick "Add to PATH") and run this again.'
    }
    winget install -e --id Python.Python.3.12 --accept-package-agreements --accept-source-agreements --silent
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
    $py = Find-Python
    if (-not $py) { throw 'Python was installed but is not on PATH yet. Open a new terminal and run this again.' }
}
Say "Using Python: $py"

# --- venv + packages ---------------------------------------------------------
$venvPy = Join-Path $Dir 'venv\Scripts\python.exe'
if (-not (Test-Path $venvPy)) {
    Say 'Creating virtual environment'
    & cmd /c "$py -m venv `"$Dir\venv`""
    if ($LASTEXITCODE -ne 0) { throw 'venv creation failed' }
}
Say 'Installing packages (hidapi, Pillow, pyserial)'
$pipExtra = if ($env:WIREVIEW_PIP_ARGS) { $env:WIREVIEW_PIP_ARGS -split ' ' } else { @() }
& $venvPy -m pip install --disable-pip-version-check -q @pipExtra -r (Join-Path $Dir 'requirements.txt')
if ($LASTEXITCODE -ne 0) { throw 'pip install failed' }

# --- start at login + now ----------------------------------------------------
$argsList = @('-ExecutionPolicy', 'Bypass', '-File', (Join-Path $Dir 'install-startup.ps1'), '-Layout', $Layout)
if ($ExtraArgs) { $argsList += @('-ExtraArgs', $ExtraArgs) }
if ($NoStart) { $argsList += '-NoStart' }
& powershell @argsList
if ($LASTEXITCODE -ne 0) { throw 'install-startup.ps1 failed' }

Write-Host ''
Say 'Done.'
Write-Host "  Installed in : $Dir"
Write-Host "  Layout       : $Layout   (change: install.ps1 -Layout per-wire)"
Write-Host '  Readings     : straight from the WireView over USB. Close the Thermal Grizzly WireView app'
Write-Host '                 (and disable its auto-start) so the COM port is free. No HWiNFO needed.'
Write-Host '  iCUE         : give the Nexus an empty screen (no widgets, black background) so iCUE stops'
Write-Host '                 redrawing over the daemon.'
Write-Host '  Uninstall    : install.ps1 -Uninstall'
