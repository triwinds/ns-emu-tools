[CmdletBinding()]
param([switch]$Resume)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskTarget = Join-Path $taskRoot 'target/streamline-sdk-route-repro-v3'
$taskSource = Join-Path $taskRoot 'target/streamline-sdk-v2.12.0'
$taskCache = Join-Path $taskRoot 'target/streamline-packman-cache'
$taskCommit = 'e8aaa6eaac968711fb62473d4ae8256dde20919b'
$taskPatch = Join-Path $PSScriptRoot 'downstream-v3.patch'
function Check-Exit([string]$step) { if ($LASTEXITCODE -ne 0) { throw "$step failed: $LASTEXITCODE" } }
function Get-LfHash([string]$file) {
    $bytes = [Text.Encoding]::UTF8.GetBytes([IO.File]::ReadAllText($file).Replace(([string][char]13), ''))
    $hash = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($hash.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $hash.Dispose() }
}
if ((git -C $taskSource rev-parse HEAD) -ne $taskCommit) { throw 'Unexpected source commit' }
Check-Exit 'source revision'
if (!(Test-Path -LiteralPath $taskTarget)) {
    if ($Resume) { throw 'Cannot resume a missing checkout' }
    git clone --no-hardlinks --no-checkout $taskSource $taskTarget
    Check-Exit 'isolated clone'
    git -C $taskTarget config core.autocrlf false
    Check-Exit 'line ending configuration'
    git -C $taskTarget checkout --detach $taskCommit
    Check-Exit 'pinned checkout'
    git -c core.whitespace=cr-at-eol -C $taskTarget apply --check $taskPatch
    Check-Exit 'patch precheck'
    git -c core.whitespace=cr-at-eol -C $taskTarget apply $taskPatch
    Check-Exit 'patch application'
} elseif (!$Resume) {
    throw 'Experiment directory already exists. Use -Resume to verify it before rebuilding.'
}
if ((git -C $taskTarget rev-parse HEAD) -ne $taskCommit) { throw 'Unexpected experiment commit' }
$taskRules = Get-Content -Raw (Join-Path $PSScriptRoot 'patched-files-v3.json') | ConvertFrom-Json
foreach ($rule in $taskRules) {
    if ((Get-LfHash (Join-Path $taskTarget $rule.path)) -ne $rule.sha256_lf) {
        throw "Patched source differs: $($rule.path)"
    }
}
git -C $taskTarget diff --exit-code --quiet -- . ':!source/platforms/sl.chi/vulkan.cpp' ':!source/platforms/sl.chi/vulkan.h' ':!source/core/sl.api/sl.cpp' ':!source/core/sl.interposer/exports.def' ':!source/core/sl.interposer/vulkan/wrapper.cpp' ':!source/plugins/sl.common/commonEntry.cpp' ':!tools/packman/bootstrap/fetch_file_from_packman_bootstrap.cmd' ':!source/core/sl.interposer/vulkan/layerRoute.cpp' ':!source/core/sl.interposer/vulkan/layerRoute.h'
Check-Exit 'unrelated tracked SDK changes'
$taskExtraSources = @(git -C $taskTarget ls-files --others --exclude-standard -- source include shaders)
Check-Exit 'untracked source inventory'
foreach ($file in $taskExtraSources) {
    if ($file -notin @('source/core/sl.interposer/vulkan/layerRoute.cpp', 'source/core/sl.interposer/vulkan/layerRoute.h')) {
        throw "Unexpected untracked source: $file"
    }
}
$taskVswhere = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio/Installer/vswhere.exe'
$taskVs = & $taskVswhere -latest -products '*' -version '[17,18)' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
Check-Exit 'VS 2022 discovery'
if (!$taskVs) { throw 'VS 2022 C++ tools are required' }
$taskMsbuild = Join-Path $taskVs 'MSBuild/Current/Bin/MSBuild.exe'
$taskOldCache = $env:PM_PACKAGES_ROOT
$taskOldCl = $env:CL
Push-Location $taskTarget
try {
    # Child-process environment only: prevents Packman's bootstrap from using setx.
    $env:PM_PACKAGES_ROOT = $taskCache
    & ./tools/packman/packman.cmd pull -p windows-x86_64 project.xml
    Check-Exit 'Packman dependencies'
    & ./tools/premake5/premake5.exe vs2022 --file=./premake.lua
    Check-Exit 'Premake'
    # Pinned NVAPI headers contain CP1252 copyright bytes. Keep /WX enabled.
    $env:CL = '/source-charset:.1252 /execution-charset:utf-8'
    foreach ($project in @('sl.interposer', 'sl.compute', 'sl.common')) {
        & $taskMsbuild "./_project/vs2022/$project.vcxproj" /m /t:Build /p:Configuration=Develop /p:Platform=x64 /nologo /verbosity:minimal /fileLogger "/fileLoggerParameters:LogFile=$taskTarget/$project.build.log;Verbosity=normal"
        Check-Exit "build $project"
    }
    $taskTestDir = Join-Path $taskTarget '_route_tests'
    New-Item -ItemType Directory -Force $taskTestDir | Out-Null
    $taskSdkXml = [Security.SecurityElement]::Escape($taskTarget)
    $taskTestXml = [Security.SecurityElement]::Escape((Join-Path $PSScriptRoot 'route-contract.cpp'))
    $taskTestProject = @"
<Project DefaultTargets="Build" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ItemGroup Label="ProjectConfigurations"><ProjectConfiguration Include="Debug|x64"><Configuration>Debug</Configuration><Platform>x64</Platform></ProjectConfiguration></ItemGroup>
  <PropertyGroup Label="Globals"><WindowsTargetPlatformVersion>10.0</WindowsTargetPlatformVersion></PropertyGroup>
  <Import Project="&#36;(VCTargetsPath)\Microsoft.Cpp.Default.props" />
  <PropertyGroup Label="Configuration"><ConfigurationType>Application</ConfigurationType><PlatformToolset>v143</PlatformToolset></PropertyGroup>
  <Import Project="&#36;(VCTargetsPath)\Microsoft.Cpp.props" />
  <PropertyGroup><OutDir>$taskSdkXml\_route_tests\bin\</OutDir><IntDir>$taskSdkXml\_route_tests\obj\</IntDir></PropertyGroup>
  <ItemDefinitionGroup><ClCompile><WarningLevel>Level4</WarningLevel><TreatWarningAsError>true</TreatWarningAsError><LanguageStandard>stdcpp17</LanguageStandard><AdditionalIncludeDirectories>$taskSdkXml</AdditionalIncludeDirectories><RuntimeLibrary>MultiThreadedDLL</RuntimeLibrary><ExceptionHandling>Sync</ExceptionHandling></ClCompile></ItemDefinitionGroup>
  <ItemGroup><ClCompile Include="$taskTestXml" /><ClCompile Include="$taskSdkXml\source\core\sl.interposer\vulkan\layerRoute.cpp" /></ItemGroup>
  <Import Project="&#36;(VCTargetsPath)\Microsoft.Cpp.targets" />
</Project>
"@
    $taskProjectPath = Join-Path $taskTestDir 'route-contract.vcxproj'
    [IO.File]::WriteAllText($taskProjectPath, $taskTestProject)
    & $taskMsbuild $taskProjectPath /t:Rebuild /p:Configuration=Debug /p:Platform=x64 /nologo /verbosity:minimal /fileLogger "/fileLoggerParameters:LogFile=$taskTarget/route-contract.build.log;Verbosity=normal"
    Check-Exit 'route contract build'
    & (Join-Path $taskTestDir 'bin/route-contract.exe')
    Check-Exit 'route contract tests'
    $taskArtifacts = foreach ($file in @('_artifacts/sl.interposer/Develop_x64/sl.interposer.dll', '_artifacts/sl.compute/Develop_x64/sl.compute.lib', '_artifacts/sl.common/Develop_x64/sl.common.dll')) {
        $path = Join-Path $taskTarget $file
        [ordered]@{ path = $file; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $path).Length }
    }
    [ordered]@{
        generated_utc = [DateTime]::UtcNow.ToString('o')
        sdk_commit = $taskCommit
        build_script_sha256 = (Get-FileHash -Algorithm SHA256 $PSCommandPath).Hash.ToLowerInvariant()
        contract_source_sha256 = (Get-FileHash -Algorithm SHA256 (Join-Path $PSScriptRoot 'route-contract.cpp')).Hash.ToLowerInvariant()
        patch_sha256 = (Get-FileHash -Algorithm SHA256 $taskPatch).Hash.ToLowerInvariant()
        configuration = 'Develop|x64'
        msbuild = $taskMsbuild
        source_charset = '1252'
        artifacts = @($taskArtifacts)
        route_contract_passed = $true
        sdk_rebuilt = $true
        runtime_routing_verified = $false
        sdk_owned_worker_tested = $false
        fg_enabled = $false
        p0_passed = $false
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 (Join-Path $taskTarget 'build-evidence.json')
}
finally {
    Pop-Location
    $env:PM_PACKAGES_ROOT = $taskOldCache
    $env:CL = $taskOldCl
}
