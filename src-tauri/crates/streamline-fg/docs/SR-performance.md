# SR tracking performance repair — 2026-09-26

Measured on the user's actual stationary TOTK scene, not the game menu, with the verified Ryubing 1.3.351+475615f build. FG was disabled to isolate tracking. The user manually entered the scene for each run.

| Configuration | Application FPS | Instrumented observer time per frame |
| --- | ---: | ---: |
| Original tracker, SR off | 7.47 | 107.09 ms |
| Same process and scene, tracking stopped | 30.02 | 0 ms |
| Repaired tracker, SR off | 30.00 | 6.92 ms |
| Repaired tracker, 1x SR + NVOF | 29.70 | 6.84 ms |
| Repaired tracker, 2x SR + NVOF | 29.80 | 6.73 ms |

The original observer consumed 800.44 ms per wall-clock second; descriptor handling accounted for 653.58 ms (81.7%). Timings sum callback observation work, including JSON construction and lock waits, but exclude underlying Vulkan execution, dispatch lookup, and source selection at present. They are not whole-machine CPU utilization. The matched in-process stop experiment establishes the practical impact independently of those timing boundaries.

The repaired tracker reduces observer time per application frame by 93.5%. The game caps near 30 FPS in this scene, so this is not evidence of zero residual overhead. Raw summaries and limitations are in [SR-performance-evidence.json](SR-performance-evidence.json); complete samples remain under ignored `src-tauri/target/sr-perf-gameplay-ab.json` and `src-tauri/target/sr-perf-fixed2-gameplay-samples.json`.

## Changes

- Descriptor templates retain only the bindings/elements used by the fingerprint-verified scaling shaders. Large game resource arrays are no longer serialized or cloned on every update. Offline diagnostic capture retains its complete array behavior.
- High-frequency pipeline/set bindings, draws and image barriers use structured state. Detailed draw JSON is generated only for swapchain candidates; barriers are expanded only for candidate source/destination images. Source barriers after the draw remain visible to the rejection checks.
- With SR off, production sessions suspend command and descriptor-update observation while retaining resource lifecycle metadata. Live re-enable clears previous frame/descriptor state and requires fresh evidence. Unknown paths continue to fall back; capacity bounds remain enforced.
- Online presentation is consumed once by the selector, instead of also accumulating duplicate post-present records.
- Compatibility failures now distinguish shaders, source format/binding, pass layout and presentation transitions.

## TOTK source-format repair

The failure snapshot proved that the shaders, coordinates and render-pass structure matched. The source image was `R8G8B8A8_SRGB` (43), sampled by the emulator through an `R8G8B8A8_UNORM` (37) image view. The previous requirement that the image itself be UNORM incorrectly rejected it.

The contract now accepts either RGBA8 image format only when the sampled view is UNORM. For an SRGB backing image, SR copies the original bytes into a private UNORM image before scaling, preserving the view's interpretation and avoiding the gamma conversion a direct SRGB blit would perform. Vulkan documents size-compatible image copies and blit format conversion separately: [vkCmdCopyImage](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdCopyImage.html), [vkCmdBlitImage](https://docs.vulkan.org/refpages/latest/refpages/source/vkCmdBlitImage.html).

Both tested SR modes used `native_source` with no fallback: 1x used 1920x1080, and 2x used 1920x1080 -> 3840x2160 -> the 2560x1335 window. The previous window fallback would have processed 5120x2670 at 2x.

## Diagnostic controls and validation

Normal launches do not enable timing or one-time candidate dumps. Explicit local diagnostics use `NS_STREAMLINE_SOURCE_MEASURE=1`; `NS_STREAMLINE_SOURCE_TRACK_ONLY=1` keeps tracking on with SR off solely for the comparison. Diagnostic `control.json` can set `sourceTrackingStop:true` to stop it permanently for that process. `NS_STREAMLINE_SOURCE_BENCH_OFF=1` disables source tracking from process start.

Rust tests cover the image/view format distinction, filtered template bindings, typed source/barrier order, clearing stale frame state, source lifetime, unsupported conditions and existing live controls. Component tests passed (30 library, 17 diagnostic, 9 launcher; one hardware-only test ignored). Host and explicit Windows checks passed for both Rust projects.

This was a short real-scene performance and operation test. Active FG plus SR was not part of this comparison; no pixel readback comparison or Vulkan validation-layer run was performed. The per-frame SR completion fence remains in place; this repair does not weaken GPU synchronization.
