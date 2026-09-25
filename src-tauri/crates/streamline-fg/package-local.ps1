# Build an explicitly local experimental bundle. This does not publish SDK binaries.
$ErrorActionPreference = 'Stop'
$crate = $PSScriptRoot
$tauri = [IO.Path]::GetFullPath((Join-Path $crate '../..'))
$bundle = Join-Path $tauri 'target/streamline-fg-package'
cargo build --locked --manifest-path (Join-Path $crate 'Cargo.toml') --lib --bin streamline-layer-probe --features sdk-bridge
if ($LASTEXITCODE -ne 0) { throw 'FG build failed' }
New-Item -ItemType Directory -Force -Path $bundle | Out-Null
$files = @()
foreach ($name in @('streamline-layer-probe.exe', 'streamline_probe_layer.dll')) {
    $source = Join-Path $crate "target/debug/$name"
    Copy-Item -LiteralPath $source -Destination (Join-Path $bundle $name) -Force
    $files += @{name=$name; sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$runtime = Get-Content -LiteralPath (Join-Path $crate 'sdk-route/runtime-v3.json') -Raw | ConvertFrom-Json
foreach ($file in $runtime.files) {
    $source = Join-Path $tauri "target/streamline-route-runtime-v3/$($file.name)"
    $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $file.sha256) { throw "Runtime hash mismatch: $($file.name)" }
    Copy-Item -LiteralPath $source -Destination (Join-Path $bundle $file.name) -Force
    $files += @{name=$file.name; sha256=$hash}
}
$version = 'local-' + $files[0].sha256.Substring(0, 12) + '-' + $files[1].sha256.Substring(0, 12)
$manifest = @{version=$version; files=$files} | ConvertTo-Json -Depth 4
$manifestPath = Join-Path $tauri 'src/services/graphics_components/streamline-package.json'
[IO.File]::WriteAllText($manifestPath, $manifest + "`n", [Text.UTF8Encoding]::new($false))
Write-Output "Local package ready: $bundle. Rebuild the toolbox to embed these hashes."
