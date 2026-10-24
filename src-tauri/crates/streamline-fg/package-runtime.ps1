# Prepare immutable common/NVIDIA/XeSS Release ZIPs without publishing.
$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot '../../tools/graphics-components/package.py'
python $script --archives
if ($LASTEXITCODE -ne 0) { throw 'Graphics Release packaging failed' }
