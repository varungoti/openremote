#requires -Version 5.1
<#
Default: remove program files and shortcut, preserve all local data.
-RemoveLocalIdentity removes host credentials/configuration, preserving received files.
-RemoveReceivedFiles separately and explicitly deletes the ReceivedFiles directory.
#>
[CmdletBinding()]
param([switch]$RemoveLocalIdentity, [switch]$RemoveReceivedFiles)
$ErrorActionPreference = 'Stop'
if (Get-Process -Name 'openremote-agent' -ErrorAction SilentlyContinue) {
    throw 'Disconnect and quit OpenRemote before uninstalling.'
}
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\OpenRemote'
$stateDir = Join-Path $env:LOCALAPPDATA 'OpenRemote'
$menuDir = Join-Path ([Environment]::GetFolderPath('Programs')) 'OpenRemote'
function Remove-OwnedDirectory([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "The application directory is a reparse point; review it locally before removal: $Path"
        }
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}
$installLock = $null
try {
    if (Test-Path -LiteralPath $stateDir) {
        $installLock = [IO.File]::Open((Join-Path $stateDir 'install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    }
    Remove-OwnedDirectory $installDir
    Remove-OwnedDirectory $menuDir
    if ($RemoveLocalIdentity -and (Test-Path -LiteralPath $stateDir)) {
        foreach ($name in @('config.json','identity.dpapi','device.dpapi','identity.new','device.new','host-instance.lock')) {
            $path = Join-Path $stateDir $name
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
        }
        Get-ChildItem -LiteralPath $stateDir -File -Force |
            Where-Object { $_.Name -match '^config\.[0-9a-f]{32}\.(new|backup)$' } |
            Remove-Item -Force
    }
    if ($RemoveReceivedFiles) { Remove-OwnedDirectory (Join-Path $stateDir 'ReceivedFiles') }
} finally {
    if ($null -ne $installLock) { $installLock.Dispose() }
}
Write-Host 'OpenRemote application removed. Server enrollment is not automatically revoked.'
if (-not $RemoveReceivedFiles) { Write-Host 'Received files were preserved.' }
