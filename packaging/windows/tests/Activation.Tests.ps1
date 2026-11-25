#requires -Version 5.1
# Exercises the actual installer function on private temporary files. Does not
# install an app or mock successful Windows execution. Run with pwsh -File ... .
$ErrorActionPreference = 'Stop'
$installer = Join-Path (Split-Path -Parent $PSScriptRoot) 'Install-OpenRemote.ps1'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($installer,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$function = $ast.Find({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Switch-OpenRemoteActivation' }, $true)
if ($null -eq $function) { throw 'The installer activation function is missing.' }
# Load only the owned source function; the installer entry point never runs.
. ([scriptblock]::Create($function.Extent.Text))
function Assert-Equal($Actual,$Expected,[string]$Message) {
    if ($Actual -cne $Expected) { throw "Assertion failed: $Message" }
}
$results = @()
foreach ($existing in @($false,$true)) {
    foreach ($failShortcut in @($false,$true)) {
        $directory = Join-Path ([IO.Path]::GetTempPath()) ('openremote-activation-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $directory | Out-Null
        $config = Join-Path $directory 'config.json'
        $shortcut = Join-Path $directory 'host.lnk'
        $configNew = Join-Path $directory 'config.new'
        $shortcutNew = Join-Path $directory 'host.new.lnk'
        $configBackup = Join-Path $directory 'config.backup'
        $shortcutBackup = Join-Path $directory 'host.backup.lnk'
        try {
            if ($existing) { [IO.File]::WriteAllText($config,'old-config'); [IO.File]::WriteAllText($shortcut,'old-shortcut') }
            [IO.File]::WriteAllText($configNew,'new-config')
            if (-not $failShortcut) { [IO.File]::WriteAllText($shortcutNew,'new-shortcut') }
            $failed = $false
            try { Switch-OpenRemoteActivation $configNew $config $configBackup $shortcutNew $shortcut $shortcutBackup $existing $existing }
            catch { $failed = $true }
            Assert-Equal $failed $failShortcut 'Failure must propagate only for the injected missing shortcut.'
            if (-not $failShortcut) {
                Assert-Equal ([IO.File]::ReadAllText($config)) 'new-config' 'New configuration activated.'
                Assert-Equal ([IO.File]::ReadAllText($shortcut)) 'new-shortcut' 'New shortcut activated.'
            } elseif ($existing) {
                Assert-Equal ([IO.File]::ReadAllText($config)) 'old-config' 'Old configuration restored byte-for-byte.'
                Assert-Equal ([IO.File]::ReadAllText($shortcut)) 'old-shortcut' 'Old shortcut preserved.'
            } else {
                Assert-Equal (Test-Path -LiteralPath $config) $false 'Failed new installation removes its activated configuration.'
                Assert-Equal (Test-Path -LiteralPath $shortcut) $false 'Failed new installation leaves no active shortcut.'
            }
            Assert-Equal (Test-Path -LiteralPath $configBackup) $false 'No recovery backup discarded before successful rollback.'
            Assert-Equal (Test-Path -LiteralPath $shortcutBackup) $false 'No stale shortcut backup after success/rollback.'
            $results += @{ existing_installation=$existing; injected_shortcut_failure=$failShortcut; status='passed' }
        } finally { Remove-Item -LiteralPath $directory -Recurse -Force }
    }
}
@{
    checks=$results
    powershell=$PSVersionTable.PSVersion.ToString()
    platform=[Environment]::OSVersion.Platform.ToString()
    scope='Actual file activation/rollback logic. Does not establish Windows COM, encoder, signing or installation behavior.'
}|ConvertTo-Json -Depth 6
