param([string]$BinaryDirectory = (Join-Path $PSScriptRoot 'target/release'))
$ErrorActionPreference = 'Stop'
$intel = Get-Content (Join-Path $PSScriptRoot 'sdk.json') -Raw | ConvertFrom-Json
$intelRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../target/xess-fg-sdk-v3.0.2'))
foreach ($file in $intel.files) {
    $destination = Join-Path $intelRoot $file.path
    if (!(Test-Path -LiteralPath $destination)) {
        New-Item -ItemType Directory -Force -Path (Split-Path $destination) | Out-Null
        $partial = "$destination.download"
        Invoke-WebRequest "https://raw.githubusercontent.com/intel/xess/$($intel.commit)/$($file.path)" -OutFile $partial
        if ((Get-FileHash -LiteralPath $partial).Hash.ToLower() -ne $file.sha256) { throw "Intel download hash mismatch: $($file.path)" }
        Move-Item -LiteralPath $partial -Destination $destination
    }
    if ((Get-FileHash -LiteralPath $destination).Hash.ToLower() -ne $file.sha256) { throw "Intel file hash mismatch: $($file.path)" }
}
$agility = Get-Content (Join-Path $PSScriptRoot 'agility.json') -Raw | ConvertFrom-Json
$agilityRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../../target/d3d12-agility-$($agility.version)"))
New-Item -ItemType Directory -Force -Path $agilityRoot | Out-Null
$package = Join-Path $agilityRoot "Microsoft.Direct3D.D3D12.$($agility.version).nupkg"
if (!(Test-Path -LiteralPath $package)) {
    Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/microsoft.direct3d.d3d12/$($agility.version)/microsoft.direct3d.d3d12.$($agility.version).nupkg" -OutFile $package
}
if ((Get-FileHash -LiteralPath $package).Hash.ToLower() -ne $agility.package_sha256) { throw 'Agility package hash mismatch' }
$archive = [IO.Compression.ZipFile]::OpenRead($package)
try {
    foreach ($name in @('LICENSE.txt','LICENSE-CODE.txt')) {
        [IO.Compression.ZipFileExtensions]::ExtractToFile($archive.GetEntry($name),(Join-Path $agilityRoot $name),$true)
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $BinaryDirectory 'D3D12') | Out-Null
    foreach ($file in $agility.files) {
        $source = Join-Path $agilityRoot $file.path
        [IO.Compression.ZipFileExtensions]::ExtractToFile($archive.GetEntry("build/native/bin/x64/$($file.path)"),$source,$true)
        if ((Get-FileHash -LiteralPath $source).Hash.ToLower() -ne $file.sha256) { throw "Agility file hash mismatch: $($file.path)" }
        $destination = Join-Path $BinaryDirectory "D3D12/$($file.path)"
        if ((Test-Path -LiteralPath $destination) -and (Get-FileHash -LiteralPath $destination).Hash.ToLower() -ne $file.sha256) { throw "Refusing unexpected staged file: $destination" }
        Copy-Item -LiteralPath $source -Destination $destination
    }
} finally { $archive.Dispose() }
foreach ($path in @((Join-Path $intelRoot 'bin/libxess_fg.dll'),(Join-Path $intelRoot 'bin/libxell.dll'))) {
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Intel Corporation') { throw "Invalid Intel signature: $path" }
}
Write-Output $intelRoot
