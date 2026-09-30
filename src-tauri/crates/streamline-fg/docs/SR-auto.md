# Automatic SR input and ratio control

## Enhanced-launch backend override

Every `--fg` launch now passes the emulator's native `--graphics-backend Vulkan` argument, including launches that first open the game library. The pinned build applies this override after configuration loading, including per-game reloads. The actual child arguments are recorded in `target-command.json`. Transparent diagnostic launches do not force a backend.

The reported crashes in `game-HcZTwU` and `game-9uJOtZ` occurred in `RendererHost` before any layer trace existed: the stored `graphics_backend=2` selected neither valid backend in this build. `sr-vulkan-override-001` retained that invalid value at test start, used the enhanced-launch override with DLAA mode / 2.00× / NVOF / reference parameters, and successfully executed native-source SR at 1920×1080 → 3840×2160 → 2560×1335 window. The emulator exited with code 0 and saved the backend as `Vulkan`; only the temporary close-confirmation setting was restored by the test. FG remained background-gated, so the strict FG activation verifier returned launcher exit 1, as in the preceding tests.

## Behavior

The pinned Ryubing build now attempts to use the original RGBA texture at the existing presentation boundary. The layer preserves the source's vertical orientation and the helper's black bars, resamples that texture directly into the private SR input, runs Vulkan DLSS, and writes the result to the presented image. The application's original scaling draw still executes. This change does not lower emulator rendering resolution or establish a performance gain.

Selection is made again for every presentation. Unknown builds use the previous full-window input path. Missing, ambiguous, non-linear, cropped, destroyed/recreated, or inadequately synchronized sources also use that path. Neither selection path consumes presentation semaphores before SR submits. A changed source geometry or switching paths resets SR history. The normal NVOF input completion and subsequent FG routing remain in place.

The known profile requires the two observed shader fingerprints, a submitted four-vertex quad, static coordinate UBO, one RGBA8 source, a normalized linear sampler, explicit GENERAL source dependency, an unclipped viewport, full-window opaque-black clear, and the matching swapchain image/semaphore. The source must remain alive, and its submission must be the latest observed submission. SDK swapchain queries continue through the SDK dispatcher. Detection does not dump shader binaries or accumulate a whole-session command trace. Unknown descriptor copies/templates, secondary commands, synchronization2/dynamic rendering, or exhausted state budgets disable source selection for the session and leave window-input SR available. This is a limited emulator profile, not a general Vulkan resource tracker.

GPU submission/completion failure remains a fatal diagnostic condition: it is unsafe to retry already-consumed binary semaphores. The automatic fallback is a pre-submission input choice, not a claim that device loss can be recovered.

## Slider

The toolbox uses a 0.50–2.00 slider in 0.05 steps. Its corrected meaning is **SR intermediate output divided by original input**, independently of window size. At 2.00, a 1920×1080 source remains 1920×1080 at the DLSS input, is reconstructed to 3840×2160, and is then scaled into the original window viewport. Black bars are restored on composition. The fallback uses the window image as original input.

At 1.00 the input is processed at its original size. Below 1.00 the input is first reduced to the requested intermediate size and processed with DLAA before being fitted to the window; DLSS itself does not downscale. The ratio is stored as integer percent (`streamline_sr_scale`) and applied with the SR revision. Changing the selected source resolution recreates SR resources. NVOF is cropped to the native content viewport and normalized motion is rescaled to that viewport's coordinates.

The bridge verifies SDK input ranges without silently changing the requested intermediate output. Private dimensions are limited to 8192. Telemetry exposes original input, actual DLSS input, intermediate output, window output, source and fallback reason. `srScaleBasis: source_output` distinguishes the corrected semantics from old sessions, which must be restarted once after updating components.

## Corrected-ratio GPU validation

`sr-ratio-001` verified native input 1920×1080 with intermediate outputs 3840×2160 (2.00×, 444 frames), 1920×1080 (1.00×, 181 frames), 960×540 (0.50×, 195 frames), and 2880×1620 (1.50×, 190 frames), including live switching back to 2.00×. The window remained 2560×1335. At 0.50× the actual DLSS input was also reduced to 960×540; at 1.00× and above it remained 1920×1080.

`sr-ratio-002` verified automatic window fallback under Nearest: 2560×1335 input → 5120×2670 intermediate output → 2560×1335 window at 2.00×, for 246 frames. Both emulator processes exited with code 0, SDK shutdown returned 0, and temporary settings were restored. FG remained background-gated, so the strict FG activation verifier still returned launcher exit 1. These tests verify GPU execution and dimensions, not visual-quality acceptance or active FG coexistence. See [corrected-ratio evidence](SR-ratio-evidence.json).

The corrected implementation passed 53 crate test invocations (one existing hardware test ignored), host and Windows-target checks for both Rust projects, frontend ESLint/type checks, and the production frontend build.

## Prior GPU validation, 2026-09-26 (before correcting ratio semantics)

Target: Ryubing 1.3.351 `475615f`, SHA-256 `022c6fcbe4741661995b8e6e5f032ae017f874f893a7adaab82c57ec9e89dd85`; RTX 5070 Ti Laptop; Stardew Valley startup sequence; NVOF requested in all five runs.

| Session | Configuration | Result |
| --- | --- | --- |
| sr-auto-001 | Bilinear, no AA | Native source; live 1.50 → 0.50 → 1.00 → 1.25 → 1.75 → 2.00 → 1.50 → Off; 1284 successful SR frames |
| sr-auto-002 | Nearest, no AA | Window fallback due to sampler mismatch; 1031 SR frames |
| sr-auto-003 | Bilinear, FXAA | Window fallback due to source synchronization mismatch; 1031 SR frames |
| sr-auto-004 | Bilinear, no AA | Native source; 1043 SR frames |
| sr-auto-005 | Final packaged layer, Bilinear, no AA | Native source; 1278 SR frames |

All emulator processes exited with code 0 and SDK shutdown returned 0. SR and FG were explicitly switched off before closing. The prior shutdown failures under other conditions are not declared fixed. Temporary configuration was restored. No emulator binary was rebuilt or changed.

Windows denied the foreground request in the final trial. All FG frames remained gated off with reason `background`. Accordingly the launcher's strict **FG activation** verifier returned failure (launcher exit 1), even though SR executed and emulator cleanup completed successfully. These runs do not validate active SR+FG coexistence, display quality, scanout cadence, long gameplay, or performance. No Vulkan validation layer was installed or used. See [machine-readable evidence](SR-auto-evidence.json) for counts and artifact hashes; raw sessions are local ignored build artifacts under `src-tauri/target`.

Validation also includes 52 passing crate test invocations (one existing hardware test ignored), host and Windows-target checks for both Rust projects without compiler warnings, frontend type checking, ESLint, and production frontend build. The local package script regenerates both component bundles and the embedded file hashes; rebuild the toolbox together with that manifest.
