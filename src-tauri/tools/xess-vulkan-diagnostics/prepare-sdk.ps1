param([string]$Destination = (Join-Path $PSScriptRoot '../../target/xess-sdk-files-v3.0.2'))
$ErrorActionPreference = 'Stop'
$files = @(
    @{ Path = 'bin/libxess.dll'; Hash = '251659dd84a3e84de67c886a4186e01f3eca49b00641906fe38bb6b807e5d5b7' },
    @{ Path = 'inc/xess/xess.h'; Hash = '98dc435f5aed252d83a161fc1f86223f20c67eb863214fe3b5cb215a3f6e3319' },
    @{ Path = 'inc/xess/xess_vk.h'; Hash = '859086df3c531ceccab628b287f69bd9a4528045bcbdd8c639589082dcbf8977' },
    @{ Path = 'LICENSE.txt'; Hash = '784be42c39a4a4d03cf85d88e15e59d75828fca773ce45b8dbfffbd7eaf208df' },
    @{ Path = 'third-party-programs.txt'; Hash = 'e8d8899dbfc4de0e8ca25560be9358e03c1405d7c956d20a1838c3c709b9b51c' }
)
foreach ($file in $files) {
    $target = Join-Path $Destination $file.Path
    if (Test-Path -LiteralPath $target) {
        if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $file.Hash) {
            throw "Existing SDK file has an unexpected hash: $target"
        }
        continue
    }
    New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
    $partial = "$target.download"
    Invoke-WebRequest -Uri ('https://raw.githubusercontent.com/intel/xess/v3.0.2/' + $file.Path) -OutFile $partial
    if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $file.Hash) {
        throw "Downloaded SDK file has an unexpected hash: $partial"
    }
    Move-Item -LiteralPath $partial -Destination $target
}
Write-Output ([IO.Path]::GetFullPath($Destination))
