# Native Vulkan SR experiment

This is an opt-in diagnostic, not an emulator SR setting. It calls Streamline DLSS SR on owned Vulkan images and command buffers. There is no application-side Vulkan/D3D12 texture sharing or NR/ReShade hook in this experiment. The separate opt-in game experiment is documented in `SR-present-experiment.md`.

## Run

Build prerequisites are the pinned SDK and Vulkan headers documented in `sdk-route/README.md`. From the repository root:

```powershell
cargo build --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features diagnostics
& src-tauri/crates/streamline-fg/target/debug/streamline-fg-diagnostics.exe --sdk-sr `
  --layer D:\py\ns-emu-tools\src-tauri\crates\streamline-fg\target\debug\streamline_probe_layer.dll `
  --interposer D:\py\ns-emu-tools\src-tauri\target\streamline-route-runtime-v3\sl.interposer.dll `
  --session D:\py\ns-emu-tools\src-tauri\target\sr-vulkan-003
```

Choose a new session directory each run. The parent stages hash-checked libraries, isolates the child and limits it to 180 seconds. A failed or uncertain GPU operation aborts the child rather than destroying potentially in-flight resources. `sr-failure.json` records failures inside the GPU experiment; earlier setup errors are in the child stderr and SDK logs.

The runtime directory needs the existing seven pinned runtime files plus `sl.dlss.dll` and `nvngx_dlss.dll` matching `sdk-route/sr-runtime.json`. The opt-in game integration now includes these binaries in the local package, staging them only for SR launches. For this trial, `sl.dlss.dll` was built from the existing verified v2.12.0 downstream-route checkout, and `nvngx_dlss.dll` came from the inspected reference distribution. Exact hashes and source paths are in the manifest. No binaries are committed.

Local plugin build command (MSBuild 18, v145):

```powershell
$env:CL='/source-charset:windows-1252 /execution-charset:utf-8'
& 'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\MSBuild\Current\Bin\MSBuild.exe' `
  src-tauri/target/streamline-sdk-route-repro-v3/_project/vs2022/sl.dlss.vcxproj `
  /p:Configuration=Develop /p:Platform=x64 /p:PlatformToolset=v145 /m /v:minimal
```

The external NVAPI headers contain legacy-encoded copyright characters; the source charset avoids Chinese-codepage build failures. A rebuilt DLL can have a different hash: deliberately review and pin any replacement rather than disabling runtime verification.

## What is checked

- Load DLSS feature 0 and query Vulkan requirements before device creation.
- Use the existing downstream Vulkan route, then check support on the selected adapter.
- Query Quality mode's optimal input size for 1280x720, then 960x540 output.
- Allocate color, depth, motion and output images. Upload a checker pattern at the actual lower input resolution; use constant depth 0.5, zero motion, zero jitter.
- Tag four resources for the same frame/viewport, evaluate into the recording Vulkan command buffer, submit and wait for its fence before reading output or reusing inputs.
- Three frames per size: reset, preserved history, inverted input with reset. Check that output replaces its magenta sentinel, has contrast, preserves at least 95% of checker-cell centers, and changes when the input changes.
- Free DLSS resources before recreating textures at the new size. Release resources and shut down the SDK. Require empty layer dispatch maps and no SDK error log entries.

Evidence: per-frame JSON and PPM files, `sr-result.json` for GPU checks, `sdk-shutdown.json`, and `sr-session.json` for completed cleanup and retained SDK warnings. Success requires a successful process exit and completed session report, not merely the presence of an output image.

## Observed result, 2026-09-26

RTX 5070 Ti Laptop GPU, reported driver 610.88, SDK v2.12.0. Session `src-tauri/target/sr-vulkan-002` passed all six frames:

| Input | Output | Cell checks per frame | Evaluate |
| --- | --- | --- | --- |
| 853x480 | 1280x720 | 720/720 | success |
| 640x360 | 960x540 | 405/405 | success |

SDK shutdown returned 0. First session `sr-vulkan-001` stopped on the diagnostic's attempt to overwrite an append-only evidence file; per-frame files fixed that issue. The second session completed; the final third run also records the explicit session completion report.

SDK runtime warnings remain about loading graphics APIs before slInit, unused plugins, disabled debug names and unsupported manual-hook state-tracking callbacks. There were no SDK error entries. The dedicated SR command buffer does not need to preserve an emulator's bound graphics state; this experiment does not prove those warnings harmless for injection into the emulator. Vulkan validation-layer coverage and performance benchmarking were not performed.

## Remaining integration work

This establishes SDK/GPU feasibility only. The current present interception sees the already-scaled swapchain. An emulator integration must identify the pre-window-scaling source image, crop/flip and synchronization, run SR at that location, and handle resize/failure fallback. Reusing this synthetic guide setup does not establish temporal quality: real or estimated motion, depth, jitter and HUD handling still need evaluation. Do not report the diagnostic as game SR being enabled.
