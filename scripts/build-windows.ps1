#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Target='x86_64-pc-windows-msvc',
    [string]$SigningThumbprint,
    [string]$TimestampUrl='https://timestamp.digicert.com'
)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$revision = (git -C $root rev-parse HEAD).Trim()
if ($LASTEXITCODE) { throw 'A Git source checkout is required to record build provenance.' }
$sourceDirty = [bool](git -C $root status --porcelain --untracked-files=normal)
if ($LASTEXITCODE) { throw 'Could not inspect source status.' }
if ($SigningThumbprint -and $sourceDirty) { throw 'Commit or clean the source changes before producing a signed build.' }
# Hash exact non-ignored source inputs. HEAD alone does not identify a dirty checkout.
$sourcePaths = @(git -C $root ls-files --cached --others --exclude-standard | Sort-Object -Unique)
if ($LASTEXITCODE) { throw 'Could not enumerate source inputs.' }
$sourceLines = foreach ($relative in $sourcePaths) {
    $path = Join-Path $root $relative
    if (Test-Path -LiteralPath $path -PathType Leaf) {
        ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $relative)
    } else { 'deleted  ' + $relative }
}
$sourceText = ($sourceLines -join "`n") + "`n"
$sourceHasher = [Security.Cryptography.SHA256]::Create()
try { $sourceDigest = ([BitConverter]::ToString($sourceHasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($sourceText)))).Replace('-','').ToLowerInvariant() }
finally { $sourceHasher.Dispose() }
$artifactRoot = Join-Path $root 'artifacts'
$transaction = [Guid]::NewGuid().ToString('N')
$out = Join-Path $artifactRoot 'windows'
$stageOut = Join-Path $artifactRoot ('windows-build-' + $transaction)
$previousOut = Join-Path $artifactRoot ('windows-previous-' + $transaction)
Push-Location (Join-Path $root 'agent-windows')
try{
    cargo fmt --check
    if($LASTEXITCODE){throw 'rustfmt failed'}
    cargo test --locked
    if($LASTEXITCODE){throw 'Host tests failed'}
    cargo clippy --locked --all-targets -- -D warnings
    if($LASTEXITCODE){throw 'Clippy failed'}
    cargo build --locked --release --target $Target
    if($LASTEXITCODE){throw 'Windows build failed'}
    New-Item -ItemType Directory -Force -Path $stageOut|Out-Null
    $binary=Join-Path $stageOut 'openremote-agent.exe'
    Copy-Item -LiteralPath "target\$Target\release\openremote-agent.exe" -Destination $binary -Force
    if($SigningThumbprint){
        signtool sign /sha1 $SigningThumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $binary
        if($LASTEXITCODE){throw 'Authenticode signing failed'}
        signtool verify /pa $binary
        if($LASTEXITCODE){throw 'Signature verification failed'}
    }
    $manifest=[ordered]@{
        source_revision=$revision
        source_dirty=$sourceDirty
        source_manifest_sha256=$sourceDigest
        target=$Target
        sha256=(Get-FileHash $binary -Algorithm SHA256).Hash.ToLowerInvariant()
        authenticode=(Get-AuthenticodeSignature $binary).Status.ToString()
        release_ready=$false
        note='A binary build alone is not the release gate. Complete docs/release-gates.md.'
    }
    [IO.File]::WriteAllText((Join-Path $stageOut 'source-inputs.sha256'), $sourceText, (New-Object Text.UTF8Encoding($false)))
    $manifest|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $stageOut 'build-manifest.json')
    # Publish a complete, mutually matching binary/manifest/source set only after success.
    $movedPrevious = $false
    if (Test-Path -LiteralPath $out) {
        Move-Item -LiteralPath $out -Destination $previousOut
        $movedPrevious = $true
    }
    try { Move-Item -LiteralPath $stageOut -Destination $out }
    catch {
        if ($movedPrevious) { Move-Item -LiteralPath $previousOut -Destination $out }
        throw
    }
    if ($movedPrevious) { Remove-Item -LiteralPath $previousOut -Recurse -Force -ErrorAction SilentlyContinue }
    Write-Host "Built $(Join-Path $out 'openremote-agent.exe'). Signing status: $($manifest.authenticode)"
} finally {
    if (Test-Path -LiteralPath $stageOut) { Remove-Item -LiteralPath $stageOut -Recurse -Force -ErrorAction SilentlyContinue }
    Pop-Location
}
