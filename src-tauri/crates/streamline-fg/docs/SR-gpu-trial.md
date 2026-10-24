# GPU source readback and one-frame output replacement

Date: 2026-09-26. The user explicitly authorized continuing GPU readback and replacement experiments after the earlier approval rejection.

## Result

On the pinned Ryujinx Canary 1.3.351 executable, the layer successfully read the pre-scale source and the same frame's swapchain image. A private GPU linear blit then reconstructed the original output exactly, including source Y orientation, RGBA-to-BGRA conversion and black borders. Three trials copied that reconstruction into the real swapchain image and read it back again: **original, reconstruction and written output were byte-identical**. All seven test processes exited normally with code 0; no forced termination or GPU-fatal path was exercised.

This is a **single-frame output overwrite after the original draw**, before native presentation. It does not suppress/replace the application's recorded draw, does not evaluate DLSS SR, and does not reduce application rendering work. It validates that the identified native-resolution source can be consumed and the presentation image can be rewritten in the tested conditions. It does not establish a production-ready automatic adapter.

| Session | Scenario | Result |
| --- | --- | --- |
| scale-gpu-001 | Stardew Valley, Bilinear, readback only | Both image copies completed; independent CPU bilinear reconstruction differs by at most 1/255 per channel, no pixels above 2/255 |
| scale-gpu-002 | Stardew Valley, Bilinear, 1920x1080 source to 2560x1335 output | One frame overwritten; max channel difference 0; written image equals private reconstruction |
| scale-gpu-003 | Xenoblade DE, Bilinear, 1280x720 source to 2560x1335 output | One frame overwritten; max channel difference 0; written image equals private reconstruction |
| scale-gpu-004 | Stardew Valley, Nearest | GPU readback/reconstruction completed, max difference 165/255; comparison rejected the writeback and retained original output |
| scale-gpu-005 | Stardew Valley, FSR | No matching source operation; skipped before GPU submission |
| scale-gpu-006 | Stardew Valley, FXAA, live resize and minimize/restore | Source identified, but required source-image dependency absent; skipped before GPU submission |
| scale-gpu-007 | Same window lifecycle, AA disabled | One frame overwritten after recovery, output 1584x935; all three images byte-identical |

Successful replacements occurred after 60 completed presents (002/003) and after 180 (007). Final application-present totals were 143, 135 and 362 respectively, so each continued presenting after the experimental frame. These captures show startup/publisher/loading screens, not long gameplay. The GPU readback PNGs were inspected; desktop scanout and sustained visual stability were not independently captured.

Stardew Valley uses normalized endpoints `[0,1,1,0]`; Xenoblade uses `[0,1,0,1]`. Both orientations were exercised on actual GPU images. Source and output alpha were opaque in the successful samples. Partial crops and fractional endpoints were not live-tested. The transfer-blit trial rejects nonopaque source pixels and noninteger endpoints; a general SR adapter will need shader handling for those cases.

## Synchronization and fallback behavior

The original application submission is retained. The first private submission waits on the original presentation semaphores, transitions the identified source from GENERAL and the destination from PRESENT to transfer-source layouts, copies both to coherent host buffers, restores the layouts, and waits for a private fence. Presentation is subsequently forwarded with the already-consumed waits removed. A supported surface is required before adding TRANSFER_SRC to the swapchain usage.

In replacement mode a second private submission clears a private BGRA image to opaque black and blits the source into the recorded viewport using the recorded source endpoints. After a completed fence, CPU comparison includes every channel of every pixel. The gate requires maximum channel error <= 2 and mean channel error <= 0.15 (8-bit units). Only a match permits a third submission to copy that private image into the swapchain, read the written image back, and restore PRESENT layout. Successful samples had zero error, not merely a value within tolerance.

Preparation failure or missing evidence leaves the original waits untouched. After the first completed submission, a rejected comparison still forwards the original image but removes the consumed waits. File/map errors after consumption never cause the original waits to be reused. An uncertain submit/fence result aborts only the diagnostic target process instead of freeing potentially in-flight resources or forwarding an unsafe fallback; the fence wait is bounded to ten seconds. That failure policy was authorized, but deliberate device-loss/timeout fault injection was not performed.

FXAA demonstrates why descriptor matching alone is insufficient: it produces the sampled image through compute and a global memory barrier (`COMPUTE_SHADER -> ALL_COMMANDS`, MEMORY_READ|MEMORY_WRITE), rather than the specific source-image barrier accepted by this trial. The current guard intentionally rejects that path. The identical resize/recovery sequence passes without FXAA, so the observed rejection is associated with the postprocessing dependency path rather than simply the new window size.

## Reproduction and isolation

Build the separate diagnostic layer/launcher as described in [SR-source-probe.md](SR-source-probe.md). Use a fresh absolute session directory:

```text
streamline-layer-probe --target-probe --scale-copy-probe --target <pinned exe> --layer <diagnostic dll> --session <new directory> --game <game>
streamline-layer-probe --target-probe --scale-replace-probe --target <pinned exe> --layer <diagnostic dll> --session <new directory> --game <game>
```

Both flags imply the read-only trace probe, require the pinned executable, and reject SDK/FG mode. The launcher explicitly clears these experimental environment switches on ordinary launches. They are not exposed in the toolbox UI and are not enabled in packaged sessions. `NS_STREAMLINE_SCALE_COPY_FRAME` sets the warmup present count (1..3000, default 60); the detailed capture limit must exceed it. Session 006/007 used warmup 180 and `NS_STREAMLINE_SCALE_PROBE_FRAMES=360`.

Artifacts: `source.rgba`, `present.bgra`, `scale-copy.json`; replacement trials also produce `gpu-reconstructed.bgra` and, only after a successful writeback, `replaced.bgra`. A skip produces `scale_copy_skipped` in `layer.jsonl`. The launcher exit code/transparent verifier describes target lifecycle, not whether the experiment passed; inspect the dedicated result fields. Compact hashed evidence is in [SR-gpu-trial-evidence.json](SR-gpu-trial-evidence.json). Full raw captures remain under the ignored `src-tauri/target/scale-gpu-*` directories.

The standalone diagnostic build was updated; toolbox binaries and the packaged runtime were not replaced. Temporary backend/filter/AA/window/exit-confirmation configuration was restored after all test processes exited. No emulator binary or global Vulkan registration was changed.

## Validation and remaining work

- `cargo fmt` for the toolbox and component; host and x86_64-pc-windows-msvc `cargo check` for both, no warnings.
- 48 test invocations passed across library/launcher/diagnostics, including four shared model tests run in both library and launcher; one existing opt-in optical-flow hardware test remains ignored.
- Real GPU evidence covers copy completion, orientation, color channels, alpha/black borders, byte-exact output overwrite, continued presentation, window rebuild/recovery, comparison rejection and unsupported-path skipping.
- Khronos validation-layer/synchronization-validation execution was **not** performed. No installed explicit validation layer was found in the queried registry locations; driver success and fence completion are not substitutes for a validation-layer run.
- Source-input DLSS evaluation, SR/FG coexistence, suppression of the original scaling draw, compute/global-barrier source tracking, multiple queues/swapchains, HDR, nonopaque sources, fractional crops, long gameplay, performance gains and prior SDK shutdown instability remain unresolved/unverified. The existing full-window present-source SR path was not modified by this experiment.

Next integration should preserve the native source descriptor, coordinates and destination viewport as a separate input contract for SR, expand resource/access tracking for the rejected paths, and then validate SDK-enabled routing. Do not turn the diagnostic matcher into automatic production enablement.
