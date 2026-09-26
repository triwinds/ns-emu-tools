# Native NVIDIA motion guidance for FG

The FG layer now uses a separate `target_nvof` backend. The historical
`target_motion` colour block matcher is no longer compiled or called. Its source
and paired shader are retained solely as experiment history.

## Reference and implementation

Reference: Magpie `experimental`, commit
`3841698348bfb246623d4acf791984c8b68a577b`:

- [NvidiaOpticalFlowProvider.cpp](https://github.com/SAOG0721/Magpie/blob/3841698348bfb246623d4acf791984c8b68a577b/src/Magpie.Core/NvidiaOpticalFlowProvider.cpp): hardware flow, balanced 4x4 grid, bidirectional consistency and cost confidence.
- [FrameGuidanceService.cpp](https://github.com/SAOG0721/Magpie/blob/3841698348bfb246623d4acf791984c8b68a577b/src/Magpie.Core/FrameGuidanceService.cpp): coherent frame inputs, history reset and explicit zero fallback.
- [Khronos optical flow specification](https://github.com/KhronosGroup/Vulkan-Docs/blob/main/chapters/VK_NV_optical_flow/optical_flow.adoc): native Vulkan session, queues, formats and direction contract.

This is an independent Rust/Vulkan adaptation. It calls `VK_NV_optical_flow`
directly, requiring no extra Optical Flow SDK DLL, CUDA, D3D11 or D3D12 interop.
The existing Streamline runtime is still required for FG.

- Input: full-resolution BGRA8 current and previous real frames.
- NVOF: separate optical-flow queue, 4x4 grid, MEDIUM performance level,
  forward flow only; backward flow and cost maps are not requested.
- Output: dense R32G32_SFLOAT, current-to-previous UV displacements. Decode the
  hardware signed 10.5 pixel vectors and divide by the input width/height;
  Streamline `mvecScale={1,1}` is retained. Magpie's direct NGX pixel-space
  contract is not copied into the Streamline UV contract.
- Production shader: decode and bilinearly expand forward flow, convert to UV.
  No confidence calculation, global atomic counters, or statistics readback.
- Reset: first frame, FG resume and source pause of at least 500 ms.
  Low-confidence samples do not trigger an invented scene-cut reset. Reset
  frames receive zero vectors. NVOF guidance is marked as dense and includes
  camera motion; zero-guidance mode does not claim camera motion.

## Synchronization and failure handling

1. Graphics queue consumes the application's binary present waits exactly once,
   copies the source, and signals a private semaphore.
2. The optical-flow queue waits for that copy and signals completion.
3. Graphics waits for NVOF, copies packed maps to a GPU buffer, densifies vectors,
   and signals the replacement present semaphore.
4. Return immediately after submission, forwarding the replacement semaphore
   to SDK present. The caller retains its SDK input-completion wait. On the next
   resource reuse, wait for the previous conversion fence. No CPU statistics
   mapping occurs.

Optional `NS_STREAMLINE_NVOF_TIMING=1` creates four graphics-queue timestamps.
The next reuse reads completed queries without WAIT. `target_nvof_gpu` reports
`copy_us`, `flow_and_queue_gap_us`, `map_copy_and_dense_us`, and `total_us`.
The middle interval includes optical-flow work AND cross-queue scheduling; it
is not a pure hardware-OF measurement. Timing is off by default and has no query
resources or timestamp commands when disabled. Unsupported timestamp queues
continue without telemetry.

Normal FG launches suppress hot Vulkan command hooks and per-frame NVOF events
before constructing JSON. Lifecycle/failure/fallback events remain available.
The three presentation events required by session verification are retained,
using a 64 KiB write buffer; lifecycle/error events flush it. Set
`NS_STREAMLINE_TRACE_VERBOSE=1` to restore full diagnostics; diagnostic hosts
retain their original verbose default. The local packaging script builds both
runtime artifacts with `--release`.

Guidance images use concurrent sharing between the graphics and optical-flow
families. Source swapchain images remain application-owned.

Missing extension/feature/queue, unsupported input extents/formats or session
initialization failures produce `target_nvof_fallback` and retain the already
cleared zero-motion guides. Device loss propagates. Errors after consuming the
application waits remain fatal; replaying those waits would be invalid.

## Enable and package

The toolbox FG panel exposes a persistent “NVIDIA 光流辅助” switch (default on).
It applies on the next FG launch: enabled adds `--nvof`; disabled omits it for
zero-motion guidance. It does not modify a running session. The preference is
stored in `setting.other.streamline_nvof`. Direct launcher use:

```text
streamline-layer-probe --target-probe --fg --reference-params --nvof ...
```

Omit `--nvof` for the zero-motion A/B baseline. The old `--estimate-motion` flag
is an alias for NVOF, not for the retired block matcher. `target-inputs.json`
records the requested backend; `target_nvof_ready`, `target_nvof_frame` and
`target_nvof_fallback` identify what actually ran.

Rebuild/repackage the launcher and layer together before reinstalling the FG
component. Editing these sources does not replace already installed DLLs or
running emulator processes. Rebuild the toolbox after regenerating package hashes.

## Original masked-flow baseline (2026-09-26)

GPU: NVIDIA GeForce RTX 5070 Ti Laptop GPU; driver 610.88.

- Shader: `glslangValidator -V --target-env vulkan1.2`, then `spirv-val`.
  Source and SPIR-V hashes are paired in `shaders/motion.json`.
- `cargo fmt`; host and explicit `x86_64-pc-windows-msvc` checks for both the
  application and FG crate (including all FG features/targets): passed.
- FG library tests: 13 passed; hardware test is ignored by default and was run
  explicitly.
- Hardware test at 321x193 and 1920x1080: static frames, signed horizontal and
  vertical translation, explicit history reset, unrelated-frame cut and
  recovery. Uses an actual producer semaphore and consumes replacement waits.
- Vulkan core and synchronization validation: no errors on the final runs.
- Translation/static interior pixels within one pixel of expected motion:
  33153/33153 at 321x193; 1885696/1885696 at 1080p.
- Unrelated-frame cut: 61713/61953 and 2073473/2073600 vectors rejected to zero,
  respectively; FG history reset asserted.
- Release 1080p synthetic sequence, validation disabled: 11 samples of CPU
  elapsed copy + NVOF + densification + fence wait, 5.45–14.19 ms (median
  6.75 ms). This is not GPU-only timing or a game performance benchmark.

Reproduce from the repository root:

```powershell
cargo test --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features sdk-bridge --lib hardware_translation_reset_and_static -- --ignored --nocapture
# Optional full-resolution run:
$env:NS_NVOF_TEST_1080P = '1'
cargo test --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features sdk-bridge --lib hardware_translation_reset_and_static -- --ignored --nocapture
# Optional validation: set VK_LAYER_PATH to a Vulkan SDK Bin directory, then
# set NS_NVOF_VALIDATION=1 before running the test.
```

## Limits

Game depth and camera matrices remain synthetic/reference inputs. Optical flow
is estimated from complete presented images, including UI. Synthetic tests prove
motion conventions and plumbing, not better game image quality or higher FPS.
Full-resolution bidirectional NVOF adds measurable latency; actual game A/B
captures and pacing measurements are still needed before calling this a quality
or performance improvement. This version does not add AMD OF, quality selection,
or Magpie's independent capture/presentation architecture.


## Magpie behavior alignment (2026-09-26)

The follow-up removes confidence masking and confidence-triggered history resets,
uses bilinear cost sampling, marks dense NVOF inputs accordingly, and defers
statistics readback until the previous resources are reused. The hardware test
now asserts that unrelated images retain nonzero raw flow despite mostly low
confidence, do not force history resets, and recover on the next static frame.
Explicit reset still clears output. Readback uses a separate command buffer and
waits on the returned semaphore to exercise asynchronous submission correctly.

The Vulkan layer still forwards every application present to Streamline. Magpie
can filter duplicate captured images and schedule generated textures itself;
that cannot be reproduced by dropping an application's present or inventing a
second pacing clock in front of the SDK swapchain. Duplicate-frame filtering and
a standalone presenter are not implemented in this change. No game A/B result
is implied by the synthetic hardware tests.

Validation of the aligned version: 321x193 and 1920x1080 GPU sequences passed
with core/synchronization validation enabled (zero errors). All 33,153 and
1,885,696 interior samples respectively matched the expected static/translation
flow within one pixel. The unrelated-image pair kept raw flow, reported mostly
low confidence, and recovered without an extra history reset. Library regression
suite: 13 passed, hardware test separately exercised.


## Production performance pass (2026-09-26)

Removed all global diagnostic atomics and confidence computation, then removed
backward/cost NVOF outputs used solely by those diagnostics. The dense shader
still consumes full-resolution-input, 4x4/MEDIUM forward flow with unchanged UV
conventions. One packed map copy remains instead of four. Motion output stays
R32G32_SFLOAT; SDK resource-lifetime waits and the three queue submissions remain.
These waits must not be removed without a separate resource-lifetime design.
Earlier confidence-test descriptions above document historical versions.

Hardware tests can use `NS_NVOF_TEST_GAME_EXTENT=1` for 2560x1335. With optional
GPU timing enabled they verify query results and print GPU segments separately
from CPU submission time. Synthetic timing is not an in-game FPS claim.

Production-pass validation: GPU tests at 321x193 (timing off), 1920x1080
(timing on), and 2560x1335 (release, timing on) passed with zero core/sync
validation errors. 14 library tests passed; the hardware test was run separately.
At 2560x1335 all 3,172,416 interior samples matched static/translation expectations.
The 11-sample GPU sequence measured 1.82-6.23 ms total, including optical-flow
queue scheduling; map copy plus densification measured 0.049-0.064 ms. These
include validation and synthetic CPU upload/readback between frames, not gameplay.
