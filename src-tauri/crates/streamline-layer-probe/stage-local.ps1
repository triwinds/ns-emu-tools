# Stage a pinned local FG package next to an already-built toolbox executable.
param([string]$OutputDirectory = (Join-Path $PSScriptRoot '../../target/release'))
$ErrorActionPreference = 'Stop'
$tauri = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$outputPath = (Resolve-Path -LiteralPath $OutputDirectory).Path
$exe = Join-Path $outputPath 'NsEmuTools.exe'
$manifest = Get-Content -LiteralPath (Join-Path $tauri 'src/services/graphics_components/streamline-package.json') -Raw | ConvertFrom-Json
$source = Join-Path $tauri 'target/streamline-fg-package'
$destination = Join-Path $outputPath 'streamline-fg-package'
$binary = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($exe))
if (-not $binary.Contains($manifest.version)) {
    throw 'Toolbox executable does not contain the current package version. Rebuild the toolbox first.'
}
foreach ($file in $manifest.files) {
    if ([IO.Path]::GetFileName($file.name) -ne $file.name) { throw 'Invalid artifact name' }
    $hash = (Get-FileHash -LiteralPath (Join-Path $source $file.name) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $file.sha256 -or -not $binary.Contains($file.sha256)) { throw "Package / toolbox hash mismatch: $($file.name)" }
}
if (Test-Path -LiteralPath $destination) {
    foreach ($file in $manifest.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $destination $file.name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
            throw 'Existing destination differs. Use a fresh output directory; no existing files were overwritten.'
        }
    }
    Write-Output "Pinned FG package already ready: $destination"
    return
}
# Keep partial copies out of the runtime lookup path. A failed staging directory is retained for inspection.
$stage = Join-Path $outputPath ('streamline-fg-stage-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
foreach ($file in $manifest.files) {
    $copy = Join-Path $stage $file.name
    Copy-Item -LiteralPath (Join-Path $source $file.name) -Destination $copy
    if ((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) { throw "Staged hash mismatch: $($file.name)" }
}
# Both paths are explicitly confined to the selected output directory.
if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($stage)) -ne $outputPath -or [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($destination)) -ne $outputPath) { throw 'Invalid staging destination' }
Move-Item -LiteralPath $stage -Destination $destination
Write-Output "Pinned FG package ready: $destination ($($manifest.files.Count) verified files)"
