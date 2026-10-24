# Present-source SR experiment

This first game integration reuses the existing Vulkan FG presentation boundary. It does not identify emulator-internal textures and does not create a separate high-resolution presentation window.

## Behavior

- Default off. Toolbox persists the SR switch and Quality/Balanced/Performance/DLAA mode; connected sessions apply SR changes live; offline settings become the next dedicated launch defaults.
- Launcher accepts `--sr-mode off|quality|balanced|performance|dlaa` with `--fg`. The live FG switch can still turn FG off while SR continues.
- Quality/Balanced/Performance downsample the already-presented image to the SDK's optimal input size, evaluate DLSS, then convert/copy the output back to the original swapchain size. DLAA uses equal input/output dimensions.
- This does **not** reduce the emulator's internal rendering workload. Text/HUD participate. This is source-resolution override, not native low-resolution-source reconstruction. A low-resolution emulator window feeding a larger output remains future work.
- SR runs on viewport 1, using the same frame token as FG viewport 0. Dedicated command buffers preserve the application's bound state.
- NVOF observes the original presented frame before SR. Its normalized UV motion is resized for SR and reused by FG; depth is constant and jitter is zero. Without NVOF, SR history resets every frame.
- The first version waits for an SR submission fence before presenting. This simplifies lifetime and semaphore ownership but adds latency. No performance or image-quality improvement is promised.
- Unsupported surface transfer usage or SR initialization falls back to the original image and reports failure. An error after recording/evaluation/submission follows the existing experimental FG fatal-error policy; it does not retry presentation with possibly consumed semaphores.
- Resize destroys old SR resources after device idle. Runtime telemetry reports evaluated input/output sizes, actual motion availability, and failure state; a saved switch is not evidence of activation.

Mode changes use independently revisioned live commands, wait for device idle at the presentation boundary, retire the old SR viewport/resources, and recreate at the new input size with history reset. Telemetry acknowledges the applied SR revision and reports actual mode, dimensions, and failure state. FG controls preserve SR requests and vice versa. Old running sessions must restart once with the new component.

## Deployment

Run `package-local.ps1`, then rebuild the toolbox. The package now includes hash-pinned `sl.dlss.dll` and `nvngx_dlss.dll`; every FG dedicated launch stages and initializes SR support, even when SR starts off. Existing installed bundles must be replaced through uninstall/install before testing the new settings. No SDK binaries are committed.

## Validation, 2026-09-26

Ryujinx 1.3.351, RTX 5070 Ti Laptop, Stardew Valley title-screen smoke:

- `src-tauri/target/sr-present-001`: 4546 successful SR evaluations, including 400 FG-active frames; Quality input 1707x890 to 2560x1335. No SR fallback or SDK execution errors. SDK shutdown returned zero and layer instance/device maps emptied, but process exited with access violation after teardown.
- `src-tauri/target/sr-present-002`: 6324 successful SR evaluations, including 1512 FG-active frames; exercised swapchain recreation and live SR telemetry. Process exit hit a managed `ObjectDisposedException` in the emulator's `AppHost.RenderLoop`.
- `src-tauri/target/sr-present-baseline-001`: SR disabled, FG-only control also exited with access violation. Exit stability remains unresolved; these sessions are **not** complete end-to-end passes. They establish real game-path evaluation, not visual quality or clean shutdown.
- Stopped/joined the telemetry worker before device shutdown to prevent it executing after layer unload; this does not resolve every observed emulator exit failure.
- Native screenshot/browser inspection tools failed to initialize (`CODEXHOST_STOCK_CODEX_PATH is required`); visual UI and image-quality inspection not completed.

Rust host/Windows checks passed without warnings; 35 component tests and 2 legacy configuration/mode tests passed (one existing opt-in NVOF GPU test remained ignored). Frontend type check/build and changed Vue component lint passed. The pre-existing `no-explicit-any` errors in `frontend/src/utils/tauri.ts` are unrelated to its added setting fields.

## Live switching validation

`src-tauri/target/sr-live-001` started with SR off. A single Ryujinx session acknowledged Quality, Balanced, Performance, DLAA, Off, then Quality again, with NVOF enabled. Input sizes were respectively 1707x890, 1485x774, 1280x668, and 2560x1335 for a 2560x1335 output. All six revisions were acknowledged; FG remained requested off. The self-launched title-screen process did not exit after normal close requests and was terminated for cleanup; this is not a clean-shutdown pass. This validates GPU execution and mode recreation, not visual quality. 36 component tests passed; one opt-in hardware test remained ignored.

## Next work

Provide a separate higher-resolution output or carefully virtualize presentation dimensions for genuine low-resolution input. Validate latency, motion artifacts, HUD quality, non-default modes, device loss, multiple swapchains, and clean application exit before broad compatibility claims.
