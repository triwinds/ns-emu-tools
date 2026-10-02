# Create independent immutable layer/runtime Release assets.
# Run package-local.ps1 first after rebuilding the layer. Publishing is a separate gh operation.
param(
    [string]$NrDirectory = (Join-Path $PSScriptRoot '../../target/release/graphics-components/cache/aio-v1/runtimes'),
    [ValidatePattern('^v[1-9][0-9]*$')][string]$BundleRevision = 'v1',
    [ValidatePattern('^v[1-9][0-9]*$')][string]$RuntimeRevision = 'v1',
    [switch]$EmbedManifest
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$tauri = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$repository = [IO.Path]::GetFullPath((Join-Path $tauri '..'))
$manifestPath = Join-Path $tauri 'src/services/graphics_components/streamline-package.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$bundle = Join-Path $tauri 'target/streamline-fg-package'
$nrRoot = (Resolve-Path -LiteralPath $NrDirectory).Path
$nr = Join-Path $nrRoot 'nvngx_dlssnr.dll'
$contract = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../streamline-nr-contract.rs') -Raw
$nrHash = (Get-FileHash -LiteralPath $nr -Algorithm SHA256).Hash.ToLowerInvariant()
if (-not $contract.Contains('"' + $nrHash + '"')) { throw 'NR runtime is not in the tested contract' }
$original = @($manifest.files | Where-Object { $_.name -match '\.(dll|exe)$' -and $_.name -ne 'nvngx_dlssnr.dll' })
foreach ($file in $original) {
    if ([IO.Path]::GetFileName($file.name) -ne $file.name) { throw 'Invalid artifact name' }
    if ((Get-FileHash -LiteralPath (Join-Path $bundle $file.name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) { throw "Changed artifact: $($file.name)" }
}
$output = Join-Path $tauri 'target/runtime-release'
New-Item -ItemType Directory -Force -Path $output | Out-Null
$stage = Join-Path $output ('stage-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
foreach ($file in $original) { Copy-Item -LiteralPath (Join-Path $bundle $file.name) -Destination (Join-Path $stage $file.name) }
Copy-Item -LiteralPath $nr -Destination (Join-Path $stage 'nvngx_dlssnr.dll')
foreach ($notice in @(
    @{source=(Join-Path $tauri 'target/streamline-sdk-v2.12.0/license.txt'); name='LICENSE-Streamline.txt'},
    @{source=(Join-Path $tauri 'target/streamline-sdk-v2.12.0/3rd-party-licenses.md'); name='LICENSE-Streamline-third-party.md'},
    @{source=(Join-Path $nrRoot 'licenses/ngx-sdk-license.txt'); name='LICENSE-NGX.txt'},
    @{source=(Join-Path $nrRoot 'licenses/nvngx_dlss.license.txt'); name='LICENSE-DLSS.txt'},
    @{source=(Join-Path $repository 'LICENSE'); name='LICENSE-NsEmuTools.txt'},
    @{source=(Join-Path $nrRoot 'manifest.json'); name='SOURCE-NR.json'},
    @{source=(Join-Path $PSScriptRoot 'sdk-route/runtime-v3.json'); name='SOURCE-Streamline.json'}
)) { Copy-Item -LiteralPath $notice.source -Destination (Join-Path $stage $notice.name) }
$readme = @'
Experimental NS Emu Tools Windows x64 stable graphics runtime bundle.
Includes Streamline downstream routing runtime, DLSS SR/FG, Reflex and NR 310.8.0.
The companion layer ZIP contains the launcher, Vulkan layer and native NR caller.
Install both through NS Emu Tools using the combined manifest and file digests.
NR uses synthetic constant depth and hardware optical flow. Compatibility and
strict Vulkan validation remain experimental. This is not an NVIDIA publication.
Upstream licenses and NR provenance are preserved alongside the binaries.
Corresponding layer/bridge source and SDK patches accompany the layer Release.
SDK base: NVIDIA-RTX/Streamline e8aaa6eaac968711fb62473d4ae8256dde20919b.
'@
[IO.File]::WriteAllText((Join-Path $stage 'README-RUNTIMES.txt'), $readme, [Text.UTF8Encoding]::new($false))
$layerReadme = "NS Emu Tools Windows x64 launcher, Vulkan layer and native NR caller bridge.`nRequires the companion stable runtime ZIP specified in the combined manifest.`nCorresponding source and SDK patches accompany this layer Release.`n"
[IO.File]::WriteAllText((Join-Path $stage 'README-LAYER.txt'), $layerReadme, [Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $repository 'LICENSE') -Destination (Join-Path $stage 'LICENSE-Layer.txt')
$files = @(Get-ChildItem -LiteralPath $stage -File | Sort-Object Name | ForEach-Object {
    @{name=$_.Name; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
})
function Write-Archive($archivePath, $entries) {
    if (Test-Path -LiteralPath $archivePath) {
        $existing = [IO.Compression.ZipFile]::OpenRead($archivePath)
        try {
            if ($existing.Entries.Count -ne $entries.Count) { throw "Existing archive member count differs: $archivePath" }
            foreach ($item in $entries) {
                $matches = @($existing.Entries | Where-Object FullName -eq $item.name)
                if ($matches.Count -ne 1) { throw "Existing archive member missing/duplicate: $($item.name)" }
                $stream = $matches[0].Open()
                $checkSha = [Security.Cryptography.SHA256]::Create()
                try { $actual = ([BitConverter]::ToString($checkSha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
                finally { $checkSha.Dispose(); $stream.Dispose() }
                if ($actual -ne (Get-FileHash -LiteralPath $item.source -Algorithm SHA256).Hash.ToLowerInvariant()) {
                    throw "Existing archive differs; use a new revision: $archivePath ($($item.name))"
                }
            }
        } finally { $existing.Dispose() }
        return
    }
    $zip = [IO.Compression.ZipFile]::Open($archivePath, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($item in $entries) {
            $entry = $zip.CreateEntry($item.name, [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $inputFile = [IO.File]::OpenRead($item.source)
            $stream = $entry.Open()
            try { $inputFile.CopyTo($stream) } finally { $stream.Dispose(); $inputFile.Dispose() }
        }
    } finally { $zip.Dispose() }
}
$layerNames = @('streamline-layer-probe.exe', 'streamline_probe_layer.dll', 'nvngx.dll', 'LICENSE-Layer.txt', 'README-LAYER.txt')
$layerFiles = @($files | Where-Object { $_.name -in $layerNames })
$stableFiles = @($files | Where-Object { $_.name -notin $layerNames })
$stableIdentity = ($stableFiles | ForEach-Object { $_.name + ':' + $_.sha256 }) -join "`n"
$sha = [Security.Cryptography.SHA256]::Create()
try { $stableDigest = ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($stableIdentity)))).Replace('-', '').ToLowerInvariant() }
finally { $sha.Dispose() }
$runtimeVersion = 'streamline-runtime-' + $stableDigest.Substring(0, 12) + '-' + $RuntimeRevision
$version = 'streamline-layer-' + ($original | Where-Object name -eq 'streamline-layer-probe.exe').sha256.Substring(0, 12) + '-' + ($original | Where-Object name -eq 'streamline_probe_layer.dll').sha256.Substring(0, 12) + '-' + ($original | Where-Object name -eq 'nvngx.dll').sha256.Substring(0, 12) + '-' + $stableDigest.Substring(0, 12) + '-' + $BundleRevision
$parts = @()
$checksums = @()
foreach ($part in @(@{version=$version; files=$layerFiles}, @{version=$runtimeVersion; files=$stableFiles})) {
    $archiveName = $part.version + '.zip'
    $archivePath = Join-Path $output $archiveName
    if (Test-Path -LiteralPath $archivePath) {
        # Stable archives can be reused across layer updates, after full member verification.
        $existing = [IO.Compression.ZipFile]::OpenRead($archivePath)
        try {
            if ($existing.Entries.Count -ne $part.files.Count) { throw 'Existing archive member count differs' }
            foreach ($file in $part.files) {
                $matches = @($existing.Entries | Where-Object FullName -eq $file.name)
                if ($matches.Count -ne 1) { throw "Existing archive member missing/duplicate: $($file.name)" }
                $stream = $matches[0].Open()
                $checkSha = [Security.Cryptography.SHA256]::Create()
                try { $actual = ([BitConverter]::ToString($checkSha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
                finally { $checkSha.Dispose(); $stream.Dispose() }
                if ($actual -ne $file.sha256) { throw "Existing archive differs: $($file.name)" }
            }
        } finally { $existing.Dispose() }
    } else {
        Write-Archive $archivePath @($part.files | ForEach-Object { @{name=$_.name; source=(Join-Path $stage $_.name)} })
    }
    $hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $parts += @{files=@($part.files | ForEach-Object { $_.name }); download=@{name=$archiveName; sha256=$hash; url="https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/$($part.version)/$archiveName"}}
    $checksums += "$hash  $archiveName"
    Write-Output "Release asset ready: $archivePath ($hash), tag $($part.version)"
}
$release = @{version=$version; native_nr=$true; schema_version=1; launcher_protocol=1; files=$files; parts=$parts}
$sourceFiles = @(rg --files --hidden -g '!**/target/**' -g '!**/evidence/**' (Join-Path $tauri 'crates/streamline-fg') (Join-Path $tauri 'crates/nr-call-bridge'))
$sourceFiles += @(Join-Path $tauri 'crates/streamline-nr-contract.rs'; Join-Path $tauri 'crates/streamline-target-policy.rs'; Join-Path $repository 'LICENSE')
$sourceArchive = Join-Path $output ($version + '-sources.zip')
# Reuse corresponding source only when all members still match exactly.
Write-Archive $sourceArchive @($sourceFiles | Sort-Object | ForEach-Object {
    @{source=$_; name=$_.Substring($repository.Length + 1).Replace('\', '/')}
})
$serialized = $release | ConvertTo-Json -Depth 6
if ($EmbedManifest) { [IO.File]::WriteAllText($manifestPath, $serialized + "`n", [Text.UTF8Encoding]::new($false)) }
[IO.File]::WriteAllText((Join-Path $output ($version + '-manifest.json')), $serialized + "`n", [Text.UTF8Encoding]::new($false))
$checksums += ((Get-FileHash -LiteralPath $sourceArchive -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + [IO.Path]::GetFileName($sourceArchive))
[IO.File]::WriteAllText((Join-Path $output ($version + '-SHA256SUMS.txt')), ($checksums -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
Write-Output "Split Release ready. Publish the layer and runtime tags above, then use -EmbedManifest and rebuild the toolbox. Local launch configuration is preserved by default."
