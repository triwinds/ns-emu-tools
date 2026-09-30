# Vulkan presentation-source identification trial

Date: 2026-09-26. This document records source identification and window lifecycle trials. Subsequently authorized GPU readback and single-frame output overwrite results are in [SR-gpu-trial.md](SR-gpu-trial.md); no simulator rebuild or DLSS SR evaluation is involved in these experiments.

## Extended verification

Six additional sessions captured 1,200 presents. All six exited with code 0; no forced termination was used. 1,080 presents matched the recorded blit contract. The other 120 are an intentional FSR negative control: no final legacy graphics draw was identified, so no source or replacement eligibility is claimed.

| Session | Game / configuration | Presents | Recorded contract matches |
| --- | --- | --- | --- |
| 005 | Stardew Valley, Bilinear, no AA | 120 | 120 |
| 006 | Stardew Valley, Nearest, no AA | 120 | 120 |
| 007 | Stardew Valley, FSR, no AA | 120 | 0, unsupported path |
| 008 | Stardew Valley, Bilinear, FXAA | 120 | 120 |
| 009 | Xenoblade Chronicles Definitive Edition, Bilinear, no AA | 120 | 120 |
| 010 | Stardew Valley, Bilinear, two live resizes, minimize/restore | 600 | 600 |

These are early startup/title samples, not a complete gameplay or image-quality test. Session 010 changes render extents from 1280x703 (66 presents), to 1584x935 (123), to 1084x695 (411). Minimize and restore were requested after present 313 with a two-second interval; matching resumed afterwards. This tests window lifecycle, not the emulator's pause/resume function. Computer Use initialization still fails with `CODEXHOST_STOCK_CODEX_PATH is required`; the window lifecycle trial uses process-scoped Win32 window operations, with no game input or screenshot inspection.

### Source coordinates and shader identity

The *current tested executable's* embedded Vulkan assembly was extracted locally and inspected, separately from the reference branch. `HelperShader.BlitColor` supplies four normalized floats in a 16-byte uniform buffer, set slot 0 / binding 1. They encode x1, x2, y1, y2 after destination flip handling. The observed static uniform-buffer descriptors resolve through buffer bindings and existing host mappings, with bounds checks; no new Vulkan memory mapping is performed by the probe. Snapshots are taken at descriptor update, not at GPU execution, and do not prove absence of later CPU/GPU changes.

- Stardew Valley: source 1920x1080, endpoints `[0,1,1,0]`.
- Xenoblade: source 1280x720, endpoints `[0,1,0,1]`.
- A fixed Y-flip would therefore be wrong for one tested title. Partial crops and both-axis flips are covered by numeric parser tests only, not live game samples.

Both sampled pipeline stages exactly match embedded SPIR-V resources:

| Resource | SHA-256 |
| --- | --- |
| ColorBlitVertex.spv | b559ef9b2f2c0796bd8e9706595690edc091fcbee127832ab5a91e48c9cd8625 |
| ColorBlitClearAlphaFragment.spv | c51c90364dfef8663959e0fa05f48e2142507973722a527627ddb06eb9783b39 |

Nearest uses the same shaders as Bilinear; their sampler differs. The diagnostic contract identifies this shared blit structure, not the filter mode. Shader fingerprints are pinned to this build and must not be treated as broad emulator compatibility.

### Layout and insertion evidence

For the observed final pass: source RGBA8 UNORM, swapchain BGRA8 UNORM; the render pass preserves GENERAL and explicitly clears the full destination to black with alpha 1. The source barrier makes color-attachment writes available to fragment sampling. After the four-vertex draw the render pass ends, followed by a GENERAL-to-PRESENT transition of the destination.

This exposes a possible insertion point **after render-pass end and before the destination presentation transition**. An SR implementation would need its own compute/transfer access barriers; the application's fragment-sampling dependency alone does not cover added work. It must also preserve source coordinates, alpha, color conversion, letterboxing, resource lifetime, and presentation synchronization. The original application's swapchain usage is 26 (color attachment, storage, transfer destination), without transfer-source usage. Reading back the destination would require checking surface capabilities and explicitly requesting that additional usage at creation. Vulkan transfer commands require an appropriate layout and execution outside a render pass: [copy command specification](https://docs.vulkan.org/spec/latest/chapters/copies.html), [synchronization specification](https://docs.vulkan.org/spec/latest/chapters/synchronization.html).

### Prior approval gate (subsequently resolved)

The first GPU readback patch was rejected by automatic approval review because it could consume presentation semaphores and abort the diagnostic process on uncertain GPU completion. The user subsequently explicitly authorized GPU readback and replacement experiments. Those experiments are now implemented and tested; see [the GPU report](SR-gpu-trial.md) for results and remaining limits.

Original experiment design (readback and output overwrite now tested):

1. Launch an isolated test process with an explicit diagnostic flag, SDK/FG disabled, and a pinned executable. Retain the original application draw. Reject unsupported presentation extensions, multiple swapchains, unknown queues, mismatched shader/coordinate evidence, or unsupported format/usage.
2. At swapchain creation, request TRANSFER_SRC only if surface capabilities permit it. Allocate and record all private resources before taking ownership of any application wait semaphore. Failures during this preparation phase forward the original presentation unchanged.
3. After a short warmup, submit one private command buffer on the presenting queue, waiting on the original presentation semaphores. Transition source GENERAL and destination PRESENT to transfer-source layouts, copy both to coherent host buffers, and restore their original layouts.
4. Wait for the private completion fence with a ten-second bound. On success, save source RGBA and destination BGRA evidence, then forward presentation with the already-consumed waits removed. No displayed pixel replacement is performed in this initial experiment.
5. If submission/completion leaves semaphore ownership or in-flight resource state uncertain, terminate **only this diagnostic emulator process**, rather than forwarding an unsafe fallback. This can interrupt the run and lose its unsaved progress; it is the side effect requiring explicit permission.
6. Only after pixel/crop/alpha/black-bar comparison passes, implement a separately gated scaling-replacement trial, then source-input SR and SR/FG integration. Do not count the initial readback as any of those passes.

Single-frame GPU pixel reconstruction and output overwrite, including a post-readback comparison rejection, are now verified in the tested cases. Suppression of the original scaling draw, source-input SR evaluation, recovery from uncertain GPU submission, and SR/FG coexistence remain **unverified**. Existing present-source SR tests do not validate this new insertion route. Multi-swapchain, secondary-command-buffer/dynamic-rendering paths, long gameplay, gameplay pause/resume, partial crop, HDR, performance benefit, and SDK shutdown stability also remain unverified. Existing SDK-session shutdown failures are not resolved by these transparent-mode clean exits.

The analyzer always reports `replacement_verified=false` and `gpu_replacement_safe=false`, even for a matching recorded contract. Do not use this diagnostic contract as a production enablement gate.

## Result

On Ryujinx Canary 1.3.351 (`475615f`), Stardew Valley, Vulkan, Bilinear, internal scale 1, no AA, the layer identifies a submitted presentation draw and its sampled 1920x1080 source in every frame of two 120-present captures:

| Session | Swapchain | Destination viewport | Matched presents |
| --- | --- | --- | --- |
| scale-source-003 | 2560x1335 | x=93, y=0, 2374x1335 | 120/120 |
| scale-source-004 | 1280x703 | x=15, y=0, 1250x703 | 120/120 |

Both use one four-vertex draw per presented image, a sampled image at descriptor set slot 2 / binding 0, RGBA8 UNORM (Vulkan format 37), mip 0, layer 0, and a descriptor-declared GENERAL layout. Source images rotate; identification follows descriptor updates rather than a fixed texture handle or size guess. This is evidence of an observable scaling input, not a proof of shader semantics or replacement safety. The smaller window is a downscaling control, not a super-resolution result.

The command buffer containing that draw was submitted on the presenting queue; its signal semaphore appears in the corresponding present wait list and its destination matches the acquired swapchain image index. Unknown descriptor templates and descriptor copies: zero in both samples. Both sessions exited normally with the existing transparent-session verifier reporting clean_shutdown=true. These are startup/title-screen samples, not long gameplay or image-quality tests. Native UI inspection was unavailable because the Computer Use runtime failed to initialize with CODEXHOST_STOCK_CODEX_PATH missing.

## What was learned

- The final scale is shader drawing, not vkCmdBlitImage; intercepting blits alone does not work here.
- This version uses vkUpdateDescriptorSetWithTemplate, not ordinary vkUpdateDescriptorSets. Tracking template entries, offsets, strides, and image views resolves the sampled source.
- Destination viewport exposes letterboxing. It does not establish source crop or source Y orientation, which can be encoded in shader input buffers.
- The descriptor layout is not proof of the image's actual access history or synchronization. No replacement is enabled based on this report.

## Reproduction

Build this crate separately from the packaged component, so the installed manifest hashes remain intact:

```powershell
cargo build --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --lib --bin streamline-layer-probe --target-dir src-tauri/target/scale-probe-build
```

Launch with `--target-probe --scale-probe --target <absolute exe> --layer <absolute diagnostic dll> --session <new absolute directory> --game <absolute game>`. This flag rejects --fg and --sdk-off. It does not load Streamline, change Vulkan arguments, replace commands, or edit emulator binaries. Existing executable allowlist/hash checks remain in force.

The probe stops recording detailed commands after 120 presents or 250,000 events. `NS_STREAMLINE_SCALE_PROBE_FRAMES` accepts 1 through 3600; session 010 uses 600. Invalid values retain the default. A frame-limit completion record distinguishes complete capture from event-limit truncation. Normal lifecycle tracing can continue after this detailed-event limit. Shader artifacts are named by SHA-256 in the session's `shaders/` directory. Close the test emulator normally, then run `streamline-layer-probe --analyze-scale <absolute session directory>` to produce `scale-analysis-v2.json`. Reanalysis accepts identical output and refuses to overwrite differing evidence. Earlier analysis files were retained as `scale-analysis-v2.initial.json`. This analyzer is an offline candidate report, never an automatic enablement gate. Captures live under src-tauri/target and are not committed; a compact evidence summary is stored in [SR-source-probe-evidence.json](SR-source-probe-evidence.json).

Session scale-source-001 never reached Vulkan: the original portable configuration had graphics_backend=2, causing RendererHost to throw NotSupportedException. Tests temporarily used the already-verified string "Vulkan", disabled the exit-confirmation dialog, and changed window maximization for the size control. Those configuration fields were restored afterwards (including the original invalid backend value); no attempt was made to silently repair the user's configuration permanently. Session scale-source-002 established the four-vertex draw but lacked template decoding, so it is not counted as a source-identification pass.

## Boundaries before replacement

1. Resolve source crop/flip and shader identity from pipeline/shader/input-buffer evidence; do not infer them from viewport alone.
2. Track image barriers, subresources, lifetime, and command-buffer generations. Choose a point outside the render pass to evaluate SR; replacing vkCmdDraw inline is insufficient for an SDK that records its own compute/transfer work.
3. Preserve black-bar clearing, alpha/color conversion, semaphores, resource ownership, and fallback rendering; coordinate with the existing FG layer only after transparent-path validation.
4. Exercise resize during playback, multiple games, other filters, pause/resume, multiple swapchains, and SDK-enabled routing. Current analysis does not resolve dynamic rendering, secondary command buffers, descriptor copies, pipeline-layout compatibility, descriptor changes between recording and submission, or indirect synchronization chains.

The route is promising for a version-specific Bilinear adapter. Broad automatic identification and safe SR replacement remain unimplemented.

## Code validation

Host and x86_64-pc-windows-msvc cargo check passed without warnings for the toolbox and diagnostic component. cargo fmt completed. 42 component tests passed; one existing opt-in NVOF hardware test was ignored. New checks cover mapped-memory bounds/overflow, crop/flip/NaN/degenerate coordinates, shader drift, dynamic-uniform rejection, failed command recording, image/view/framebuffer destruction, command reset, and failed queue submissions. A diagnostic bug that previously converted negative Vulkan result codes to zero was fixed. The separate release diagnostic binary was rebuilt; the toolbox and packaged runtime were not replaced by this trial. Temporary graphics/filter/AA/window/exit-confirmation settings were restored after the last process exited. As in the initial trial, the original backend value 2 was restored rather than permanently repaired.
