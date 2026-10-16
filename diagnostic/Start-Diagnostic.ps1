param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [ValidateSet('auto', 'default', 'i16', 'legacy')]
    [string]$Mode = 'auto'
)
$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Executable).Path
if ([IO.Path]::GetFileName($resolved) -ine 'handy.exe') {
    throw 'Select the diagnostic handy.exe inside the installed or portable folder.'
}
$running = Get-Process -Name handy -ErrorAction SilentlyContinue
if ($running) {
    throw 'Fully quit all Handy instances from their tray menu, then retry.'
}
$previousMode = $env:HANDY_MIC_CONFIG
$previousUpdater = $env:HANDY_DISABLE_UPDATER
try {
    $env:HANDY_MIC_CONFIG = $Mode
    $env:HANDY_DISABLE_UPDATER = '1'
    Write-Host "Launching Realtek diagnostic mode: $Mode"
    Start-Process -FilePath $resolved -ArgumentList '--debug' -WorkingDirectory (Split-Path $resolved) -WindowStyle Hidden
} finally {
    $env:HANDY_MIC_CONFIG = $previousMode
    $env:HANDY_DISABLE_UPDATER = $previousUpdater
}
