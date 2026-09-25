param(
    [Parameter(Mandatory=$true)][string]$Destination,
    [string]$ReferenceRoot='D:\ryubing-dlss5-win-x64',
    [string]$BuildRoot=(Join-Path $PSScriptRoot '../../../target/streamline-sdk-route-repro-v3')
)
$ErrorActionPreference='Stop'
if (![IO.Path]::IsPathFullyQualified($Destination)) { throw 'Destination must be absolute' }
if (Test-Path -LiteralPath $Destination) { throw 'Destination already exists' }
$manifest=Get-Content (Join-Path $PSScriptRoot 'runtime-v3.json') -Raw | ConvertFrom-Json
$sources=@()
foreach($item in $manifest.files) {
    $root=if($item.origin -eq 'sdk-route-v3'){$BuildRoot}else{$ReferenceRoot}
    $source=Join-Path $root $item.source
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ne $item.sha256) {
        throw "Source hash mismatch: $source"
    }
    $sources += @{source=$source;item=$item}
}
New-Item -ItemType Directory -Path $Destination -ErrorAction Stop | Out-Null
foreach($entry in $sources) {
    $dest=Join-Path $Destination $entry.item.name
    Copy-Item -LiteralPath $entry.source -Destination $dest -ErrorAction Stop
    if ((Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.item.sha256) {
        throw "Staged hash mismatch: $dest"
    }
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'runtime-v3.json') -Destination (Join-Path $Destination 'runtime-v3.json')
Write-Output "Verified seven experimental DLLs: $Destination"
