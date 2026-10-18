param(
    [Parameter(Mandatory)][string]$Evidence,
    [int]$GpuIndex = 0,
    [string]$Sdk = (Join-Path $PSScriptRoot '../../target/xess-sdk-files-v3.0.2'),
    [string]$Validation = (Join-Path $PSScriptRoot '../../target/vulkan-sdk-1.4.363.0/Bin')
)
$ErrorActionPreference = 'Stop'
$Evidence = [IO.Path]::GetFullPath($Evidence)
if (Test-Path -LiteralPath $Evidence) { throw "Evidence directory already exists: $Evidence" }
$dll = [IO.Path]::GetFullPath((Join-Path $Sdk 'bin/libxess.dll'))
$exe = Join-Path $PSScriptRoot 'target/release/xess-vulkan-diagnostics.exe'
New-Item -ItemType Directory -Path $Evidence | Out-Null
$oldLayerPath = $env:VK_LAYER_PATH
$oldDisable = $env:VK_LOADER_LAYERS_DISABLE
try {
    $env:VK_LAYER_PATH = [IO.Path]::GetFullPath($Validation)
    $env:VK_LOADER_LAYERS_DISABLE = '~implicit~'
    $matrix = @(
        @{ Name = 'validation-low'; Width = 1280; Height = 720; Extra = @('--validation') },
        @{ Name = 'validation-high'; Width = 1280; Height = 720; Extra = @('--validation', '--high-motion') },
        @{ Name = 'standard-1080'; Width = 1920; Height = 1080; Extra = @('--high-motion', '--temporal') },
        @{ Name = 'standard-1440'; Width = 2560; Height = 1440; Extra = @('--high-motion') },
        @{ Name = 'no-jitter-1080'; Width = 1920; Height = 1080; Extra = @('--high-motion', '--no-jitter', '--temporal') },
        @{ Name = 'flat-depth-no-jitter-1080'; Width = 1920; Height = 1080; Extra = @('--no-jitter', '--flat-depth') },
        @{ Name = 'benchmark-1080'; Width = 1920; Height = 1080; Frames = 1000; Extra = @('--high-motion', '--benchmark-only') },
        @{ Name = 'benchmark-1440'; Width = 2560; Height = 1440; Frames = 1000; Extra = @('--high-motion', '--benchmark-only') }
    )
    foreach ($case in $matrix) {
        $extra = $case.Extra
        $frames = 144
        if ($case.ContainsKey('Frames')) { $frames = $case.Frames }
        $log = Join-Path $Evidence ($case.Name + '.log')
        & $exe --dll $dll --output (Join-Path $Evidence $case.Name) --width $case.Width --height $case.Height --frames $frames --gpu-index $GpuIndex @extra *> $log
        Get-Content -LiteralPath $log -Tail 6
        if ($LASTEXITCODE -ne 0) { throw "XeSS validation failed; see $log" }
    }
} finally {
    $env:VK_LAYER_PATH = $oldLayerPath
    $env:VK_LOADER_LAYERS_DISABLE = $oldDisable
}
