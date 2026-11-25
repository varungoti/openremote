#requires -Version 5.1
<#
Install the visible host for the current Windows user. A standalone, hash-verified
FFmpeg executable with gdigrab and libx264 is required. Production installation
requires valid Authenticode; -TestBuild explicitly permits a local unsigned pilot.
Files are staged in a version directory before the configuration/shortcut switch.
Caught activation failures restore their previous files. Power-loss recovery and
native upgrade execution remain release gates; no service is installed here.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$AgentPath,
    [Parameter(Mandatory=$true)][string]$FfmpegPath,
    [Parameter(Mandatory=$true)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ExpectedFfmpegSha256,
    [Parameter(Mandatory=$true)][string]$ApiUrl,
    [string]$DeviceName = $env:COMPUTERNAME,
    [Nullable[bool]]$AllowClipboard,
    [Nullable[bool]]$AllowFileTransfer,
    [switch]$TestBuild,
    [switch]$StartAfterInstall
)
$ErrorActionPreference = 'Stop'

function Invoke-EncoderProbe([string]$Executable, [string]$Arguments, [string]$Directory) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Executable
    $start.Arguments = $Arguments
    $start.WorkingDirectory = $Directory
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    # A shared build must not accidentally pass by finding DLLs in the source PATH.
    $start.EnvironmentVariables['PATH'] = "$env:SystemRoot\System32;$env:SystemRoot"
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw 'FFmpeg preflight did not start.' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(15000)) {
            $process.Kill()
            throw 'FFmpeg preflight exceeded 15 seconds.'
        }
        $output = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) {
            throw 'FFmpeg preflight failed. Use a compatible standalone Windows build.'
        }
        return $output
    } finally { $process.Dispose() }
}

# Kept as one function so real replacement/rollback behavior can be tested without
# invoking Windows consent, COM shortcuts, code signing or application startup.
function Switch-OpenRemoteActivation(
    [string]$ConfigTemp, [string]$ConfigPath, [string]$ConfigBackup,
    [string]$ShortcutTemp, [string]$ShortcutPath, [string]$ShortcutBackup,
    [bool]$HadConfig, [bool]$HadShortcut
) {
    $configCommitted = $false
    $shortcutCommitted = $false
    $rollbackComplete = $true
    $activationComplete = $false
    try {
        if ((Test-Path -LiteralPath $ConfigPath) -ne $HadConfig -or
            (Test-Path -LiteralPath $ShortcutPath) -ne $HadShortcut) {
            throw 'The installation changed during preparation. Retry from the updated configuration.'
        }
        if ($HadConfig) { [IO.File]::Replace($ConfigTemp, $ConfigPath, $ConfigBackup) }
        else { [IO.File]::Move($ConfigTemp, $ConfigPath) }
        $configCommitted = $true
        if ($HadShortcut) { [IO.File]::Replace($ShortcutTemp, $ShortcutPath, $ShortcutBackup) }
        else { [IO.File]::Move($ShortcutTemp, $ShortcutPath) }
        $shortcutCommitted = $true
        $activationComplete = $true
    } catch {
        $activationError = $_
        if ($shortcutCommitted) {
            try {
                if ($HadShortcut) { [IO.File]::Replace($ShortcutBackup, $ShortcutPath, [NullString]::Value) }
                elseif (Test-Path -LiteralPath $ShortcutPath) { Remove-Item -LiteralPath $ShortcutPath -Force }
            } catch { $rollbackComplete = $false; Write-Warning "Shortcut rollback failed; backup: $ShortcutBackup" }
        }
        if ($configCommitted) {
            try {
                if ($HadConfig) { [IO.File]::Replace($ConfigBackup, $ConfigPath, [NullString]::Value) }
                elseif (Test-Path -LiteralPath $ConfigPath) { Remove-Item -LiteralPath $ConfigPath -Force }
            } catch { $rollbackComplete = $false; Write-Warning "Configuration rollback failed; backup: $ConfigBackup" }
        }
        throw $activationError
    } finally {
        if ($activationComplete -or $rollbackComplete) {
            foreach ($path in @($ConfigBackup,$ShortcutBackup)) {
                if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue }
            }
        }
    }
}

function Set-ConfigProperty($Object, [string]$Name, $Value) {
    $Object | Add-Member -MemberType NoteProperty -Name $Name -Value $Value -Force
}

$agentSource = (Resolve-Path -LiteralPath $AgentPath).Path
$encoderSource = (Resolve-Path -LiteralPath $FfmpegPath).Path
$agentHash = (Get-FileHash -LiteralPath $agentSource -Algorithm SHA256).Hash.ToLowerInvariant()
$encoderHash = (Get-FileHash -LiteralPath $encoderSource -Algorithm SHA256).Hash.ToLowerInvariant()
if ($encoderHash -ne $ExpectedFfmpegSha256.ToLowerInvariant()) {
    throw 'FFmpeg does not match the expected trusted distribution hash.'
}
[Uri]$origin = $ApiUrl
if (-not $origin.IsAbsoluteUri -or $origin.Scheme -ne 'https' -or
    -not $origin.Host -or $origin.AbsolutePath -ne '/' -or
    $origin.Query -or $origin.Fragment -or $origin.UserInfo) {
    throw 'Use an HTTPS origin without credentials, query, fragment, or path.'
}
if ([string]::IsNullOrWhiteSpace($DeviceName) -or
    [Text.Encoding]::UTF8.GetByteCount($DeviceName) -gt 80 -or
    $DeviceName.Trim() -ne $DeviceName -or
    $DeviceName -match '[\x00-\x1f\x7f]') {
    throw 'Set a PC name of 1-80 UTF-8 bytes without control characters.'
}
if (-not $TestBuild -and (Get-AuthenticodeSignature -LiteralPath $agentSource).Status -ne 'Valid') {
    throw 'Use a signed host, or explicitly choose -TestBuild for a local unsigned pilot.'
}
if (Get-Process -Name 'openremote-agent' -ErrorAction SilentlyContinue) {
    throw 'Disconnect and quit the running OpenRemote host before upgrading.'
}

$installDir = Join-Path $env:LOCALAPPDATA 'Programs\OpenRemote'
$stateDir = Join-Path $env:LOCALAPPDATA 'OpenRemote'
$configPath = Join-Path $stateDir 'config.json'
$menuDir = Join-Path ([Environment]::GetFolderPath('Programs')) 'OpenRemote'
$shortcutPath = Join-Path $menuDir 'OpenRemote Host.lnk'
$hadConfig = Test-Path -LiteralPath $configPath
$hadShortcut = Test-Path -LiteralPath $shortcutPath
$originalConfigHash = if ($hadConfig) { (Get-FileHash -LiteralPath $configPath -Algorithm SHA256).Hash } else { $null }
$originalShortcutHash = if ($hadShortcut) { (Get-FileHash -LiteralPath $shortcutPath -Algorithm SHA256).Hash } else { $null }
if ($hadConfig) {
    $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    if ($config.api_base_url.TrimEnd('/') -ne $ApiUrl.TrimEnd('/')) {
        throw 'This PC belongs to a different server. Existing identity was preserved; use the documented re-enrollment procedure.'
    }
} else {
    $config = [PSCustomObject][ordered]@{
        api_base_url = $ApiUrl.TrimEnd('/')
        device_name = $DeviceName
        allow_unattended = $false
        trusted_controller_keys = @()
        allow_input = $true
        allow_clipboard = $false
        allow_file_transfer = $false
        heartbeat_seconds = 15
        capture_backend = 'gdi'
        fps = 30
        max_width = 1280
        bitrate_bps = 4000000
        allow_loopback_http = $false
    }
}
if (-not $config.PSObject.Properties['allow_clipboard']) {
    Set-ConfigProperty $config 'allow_clipboard' $false
}
if (-not $config.PSObject.Properties['allow_file_transfer']) {
    Set-ConfigProperty $config 'allow_file_transfer' $false
}
if ($PSBoundParameters.ContainsKey('AllowClipboard')) {
    Set-ConfigProperty $config 'allow_clipboard' ([bool]$AllowClipboard)
}
if ($PSBoundParameters.ContainsKey('AllowFileTransfer')) {
    Set-ConfigProperty $config 'allow_file_transfer' ([bool]$AllowFileTransfer)
}
if ($PSBoundParameters.ContainsKey('DeviceName')) {
    Set-ConfigProperty $config 'device_name' $DeviceName
}
if ([string]::IsNullOrWhiteSpace($config.device_name) -or
    [Text.Encoding]::UTF8.GetByteCount($config.device_name) -gt 80 -or
    $config.device_name.Trim() -ne $config.device_name -or
    $config.device_name -match '[\x00-\x1f\x7f]') {
    throw 'The effective PC name must be trimmed, printable and 1-80 UTF-8 bytes. Supply -DeviceName to update it.'
}
if ($config.heartbeat_seconds -lt 5 -or $config.heartbeat_seconds -gt 30) {
    throw 'Set heartbeat_seconds between 5 and 30 before upgrading; the server presence lease is 45 seconds.'
}

# Existing enrollment, origin and source checks finish before staging changes.
$transaction = [Guid]::NewGuid().ToString('N')
$releaseDir = Join-Path $installDir ('releases\' + $agentHash.Substring(0,16) + '-' + $encoderHash.Substring(0,16))
$stageDir = Join-Path $installDir ('staging-' + $transaction)
$configTemp = Join-Path $stateDir ('config.' + $transaction + '.new')
$configBackup = Join-Path $stateDir ('config.' + $transaction + '.backup')
$shortcutTemp = Join-Path $menuDir ('OpenRemote Host.' + $transaction + '.lnk')
$shortcutBackup = Join-Path $menuDir ('OpenRemote Host.' + $transaction + '.backup.lnk')
$installLock = $null
try {
    New-Item -ItemType Directory -Force -Path $stageDir, $stateDir, $menuDir | Out-Null
    $installLock = [IO.File]::Open((Join-Path $stateDir 'install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $stagedAgent = Join-Path $stageDir 'openremote-agent.exe'
    $stagedEncoder = Join-Path $stageDir 'ffmpeg.exe'
    Copy-Item -LiteralPath $agentSource -Destination $stagedAgent
    Copy-Item -LiteralPath $encoderSource -Destination $stagedEncoder
    if ((Get-FileHash -LiteralPath $stagedAgent -Algorithm SHA256).Hash.ToLowerInvariant() -ne $agentHash -or
        (Get-FileHash -LiteralPath $stagedEncoder -Algorithm SHA256).Hash.ToLowerInvariant() -ne $encoderHash) {
        throw 'A staged executable hash changed; the active installation was preserved.'
    }
    if (-not $TestBuild -and (Get-AuthenticodeSignature -LiteralPath $stagedAgent).Status -ne 'Valid') {
        throw 'The staged host signature is invalid.'
    }
    $devices = Invoke-EncoderProbe $stagedEncoder '-hide_banner -nostdin -devices' $stageDir
    $encoders = Invoke-EncoderProbe $stagedEncoder '-hide_banner -nostdin -encoders' $stageDir
    if ($devices -notmatch '(?m)^\s*D\s+gdigrab\s' -or $encoders -notmatch '(?m)^\s*V\S*\s+libx264\s') {
        throw 'The standalone FFmpeg build must include gdigrab capture and libx264 encoding.'
    }
    if ($config.capture_backend -eq 'dxgi') {
        $filters = Invoke-EncoderProbe $stagedEncoder '-hide_banner -nostdin -filters' $stageDir
        if ($filters -notmatch '(?m)^\s*\S+\s+ddagrab\s') {
            throw 'The configured DXGI backend requires an FFmpeg build with ddagrab.'
        }
    }
    $null = Invoke-EncoderProbe $stagedEncoder '-hide_banner -nostdin -loglevel error -f lavfi -i testsrc2=size=64x64:rate=1 -frames:v 1 -pix_fmt yuv420p -c:v libx264 -f null NUL' $stageDir
    if (Test-Path -LiteralPath $releaseDir) {
        foreach ($entry in @(@('openremote-agent.exe',$agentHash), @('ffmpeg.exe',$encoderHash))) {
            $existing = Join-Path $releaseDir $entry[0]
            if (-not (Test-Path -LiteralPath $existing) -or
                (Get-FileHash -LiteralPath $existing -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry[1]) {
                throw 'The existing version directory does not match its expected executable hashes.'
            }
        }
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $releaseDir) | Out-Null
        Move-Item -LiteralPath $stageDir -Destination $releaseDir
    }
    $agentTarget = Join-Path $releaseDir 'openremote-agent.exe'
    $encoderTarget = Join-Path $releaseDir 'ffmpeg.exe'
    Set-ConfigProperty $config 'ffmpeg_path' $encoderTarget
    Set-ConfigProperty $config 'ffmpeg_sha256' $encoderHash
    [IO.File]::WriteAllText($configTemp, ($config | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutTemp)
    $shortcut.TargetPath = $agentTarget
    $shortcut.WorkingDirectory = $releaseDir
    $shortcut.Description = 'OpenRemote visible desktop host'
    $shortcut.Save()
    # Protect local edits made while the staged encoder was being checked.
    if (($hadConfig -and (Get-FileHash -LiteralPath $configPath -Algorithm SHA256).Hash -ne $originalConfigHash) -or
        ($hadShortcut -and (Get-FileHash -LiteralPath $shortcutPath -Algorithm SHA256).Hash -ne $originalShortcutHash)) {
        throw 'Configuration or shortcut changed during preparation. Existing files were preserved.'
    }
    Switch-OpenRemoteActivation $configTemp $configPath $configBackup $shortcutTemp $shortcutPath $shortcutBackup $hadConfig $hadShortcut
} finally {
    foreach ($path in @($stageDir,$configTemp,$shortcutTemp)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue }
    }
    if ($null -ne $installLock) { $installLock.Dispose() }
}
Write-Host "Installed OpenRemote host: $agentTarget"
Write-Host "Configuration: $configPath"
Write-Host "Clipboard allowed locally: $($config.allow_clipboard); file uploads allowed locally: $($config.allow_file_transfer)"
if ($config.allow_unattended) {
    Write-Host 'Existing unattended setting and trusted phone keys were preserved.'
} else {
    Write-Host 'Unattended access is off. Trust a phone locally before enabling it.'
}
if ($StartAfterInstall) {
    $previousEnrollmentToken = $env:OPENREMOTE_ENROLLMENT_TOKEN
    $hadEnrollmentToken = Test-Path Env:\OPENREMOTE_ENROLLMENT_TOKEN
    $credential = Read-Host 'Enrollment token (needed only for first enrollment)' -AsSecureString
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($credential)
    try {
        $env:OPENREMOTE_ENROLLMENT_TOKEN = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
        Start-Process -FilePath $agentTarget -WorkingDirectory $releaseDir
    } finally {
        if ($hadEnrollmentToken) { $env:OPENREMOTE_ENROLLMENT_TOKEN = $previousEnrollmentToken }
        else { Remove-Item Env:\OPENREMOTE_ENROLLMENT_TOKEN -ErrorAction SilentlyContinue }
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
    }
}
