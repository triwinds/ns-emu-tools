# Build one launcher, two isolated layers, and stage the three graphics packages.
$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot '../../tools/graphics-components/package.py'
python $script --build
if ($LASTEXITCODE -ne 0) { throw 'Graphics component build/staging failed' }
