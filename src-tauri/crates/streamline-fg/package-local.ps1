# Build an explicitly local experimental bundle. This does not publish SDK binaries.
$ErrorActionPreference = 'Stop'
$crate = $PSScriptRoot
$tauri = [IO.Path]::GetFullPath((Join-Path $crate '../..'))
$bundle = Join-Path $tauri 'target/streamline-fg-package'
$bridgeCrate = Join-Path $crate '../nr-call-bridge'
$previousNrRustFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = '-C target-feature=-crt-static'
    cargo build --locked --release --manifest-path (Join-Path $crate 'Cargo.toml') --lib --bin streamline-layer-probe --features native-nr
    if ($LASTEXITCODE -ne 0) { throw 'Native graphics build failed' }
    cargo build --locked --release --manifest-path (Join-Path $bridgeCrate 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Native NR bridge build failed' }
} finally {
    if ($null -eq $previousNrRustFlags) { Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue }
    else { $env:RUSTFLAGS = $previousNrRustFlags }
}
$bridge = Join-Path $bridgeCrate 'target/release/nvngx.dll'
$bridgeHash = (Get-FileHash -LiteralPath $bridge -Algorithm SHA256).Hash.ToLowerInvariant()
$bridgeContract = Get-Content -LiteralPath (Join-Path $crate '../streamline-nr-contract.rs') -Raw
$expectedBridge = [regex]::Match($bridgeContract, 'pub const BRIDGE: &str = "([0-9a-f]{64})";').Groups[1].Value
if ($bridgeHash -ne $expectedBridge) { throw 'NR bridge changed; revalidate the caller boundary before packaging' }
New-Item -ItemType Directory -Force -Path $bundle | Out-Null
$files = @()
foreach ($name in @('streamline-layer-probe.exe', 'streamline_probe_layer.dll')) {
    $source = Join-Path $crate "target/release/$name"
    Copy-Item -LiteralPath $source -Destination (Join-Path $bundle $name) -Force
    $files += @{name=$name; sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()}
}
Copy-Item -LiteralPath $bridge -Destination (Join-Path $bundle 'nvngx.dll') -Force
$files += @{name='nvngx.dll'; sha256=$bridgeHash}
$runtime = Get-Content -LiteralPath (Join-Path $crate 'sdk-route/runtime-v3.json') -Raw | ConvertFrom-Json
$sr = Get-Content -LiteralPath (Join-Path $crate 'sdk-route/sr-runtime.json') -Raw | ConvertFrom-Json
foreach ($file in @($runtime.files) + @($sr.files)) {
    $source = Join-Path $tauri "target/streamline-route-runtime-v3/$($file.name)"
    $hash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $file.sha256) { throw "Runtime hash mismatch: $($file.name)" }
    Copy-Item -LiteralPath $source -Destination (Join-Path $bundle $file.name) -Force
    $files += @{name=$file.name; sha256=$hash}
}
$version = 'local-' + $files[0].sha256.Substring(0, 12) + '-' + $files[1].sha256.Substring(0, 12)
$manifest = @{version=$version; files=$files; native_nr=$true} | ConvertTo-Json -Depth 4
$manifestPath = Join-Path $tauri 'src/services/graphics_components/streamline-package.json'
[IO.File]::WriteAllText($manifestPath, $manifest + "`n", [Text.UTF8Encoding]::new($false))
# Release builds resolve the package next to NsEmuTools.exe, not in target/.
$releaseBundle = Join-Path $tauri 'target/release/streamline-fg-package'
New-Item -ItemType Directory -Force -Path $releaseBundle | Out-Null
foreach ($file in $files) {
    $source = Join-Path $bundle $file.name
    $destination = Join-Path $releaseBundle $file.name
    Copy-Item -LiteralPath $source -Destination $destination -Force
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $file.sha256) { throw "Release package hash mismatch: $($file.name)" }
}
Write-Output "Local packages ready: $bundle and $releaseBundle. Rebuild the toolbox to embed these hashes."
