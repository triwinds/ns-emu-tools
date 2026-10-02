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

## Magpie FG comparison and fast-pan check (2026-10-02)

Reviewed the experimental fork at commit
`2c017d4be8311fac8584e2ff6187b1f03383bde1`, rather than relying on the older
September reference or release descriptions. Relevant pinned sources:

- [Renderer.cpp](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/Renderer.cpp):
  accepts a new capture ID, prepares guidance from the captured input before the
  effect chain, invokes FG after the chain, then publishes generated and real
  images through its own presenter. Exact duplicate-frame filtering is enabled
  by default but can be disabled in the FG effect settings.
- [FrameGuidanceService.cpp](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/FrameGuidanceService.cpp)
  and [FrameGuidanceTypes.h](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/FrameGuidanceTypes.h):
  shared motion is validated against the consumer's frame ID, extent and region;
  a binding/fallback change propagates a temporal reset. Motion resized for a
  consumer is also scaled in pixel units.
- [NvidiaOpticalFlowProvider.cpp](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/NvidiaOpticalFlowProvider.cpp):
  continuous pairs keep temporal hints. The first frame primes the reference and
  produces zero guidance. Bidirectional flow and cost are requested when
  available, but confidence is separate: the dense motion output remains the
  unmasked forward vector, even at low confidence.
- [DLSSFrameGenerator.cpp](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/DLSSFrameGenerator.cpp):
  directly evaluates D3D12 NGX, uses current-to-previous pixel motion with unit
  scale, reads the SDK's disable-interpolation output, and withholds generated
  images on reset or SDK-disabled frames. Depth is synthetic, not engine depth.

The Rust layer uses a Streamline Vulkan proxy swapchain, not this standalone
presenter. It forwards every application Present, including unchanged images;
the SDK owns interpolation and presentation. Dropping application Presents or
copying Magpie's NGX pixel scale into Streamline's UV contract would not be a
valid port. Both implementations estimate motion from original color and feed
FG final color after optional processing, so this ordering alone is not a defect.

The startup workaround currently disables NVOF temporal hints on every pair;
NR/SR temporal histories remain enabled. The reported rapid-turn ghosting also
occurs with NR/SR disabled and FG enabled. This narrows the necessary path to FG,
but does not identify temporal hints as the cause.

Added `hardware_fast_pan_with_and_without_temporal_hints`: a 1024x576 textured
plane accelerates from 8 to 64 pixels per frame, stops, reverses at 64 pixels per
frame, and explicitly resets. Both hint modes matched all 319,488 tested interior
pixels within one pixel on every non-reset pair; reset outputs were zero. This
checks sign, magnitude, stop/reversal and reset plumbing. It does not reproduce
rotation, disocclusion, untextured surfaces, game input cadence or generated-frame
quality. It therefore cannot establish that either hint mode fixes gameplay.

The next useful game evidence is raw consecutive color, motion and generated
output during the same rapid pan, including exact duplicate counts and frame
identity. Compare FG with zero guidance against FG with NVOF, then compare NVOF
hint modes without changing the NR/SR controls. Preserve application waits and
SDK resource lifetimes throughout; no production policy is changed by this review.

## Magpie-inspired local revision (2026-10-02)

The follow-up now compares consecutive original RGBA images exactly on the GPU.
It reads back one coherent word after a fence, skips NVOF execution for an exact
duplicate, and retains the last accepted reference and temporal seed. Continuous
new pairs use temporal hints by default; `NS_STREAMLINE_NVOF_TEMPORAL_HINTS=0`
remains an explicit comparison override. A boundary reset primes the reference,
clears guidance and invalidates the hint seed until the next evaluated pair.

Each unchanged 4x4 tile outputs zero motion. This is an exact static-color rule,
not confidence masking: changed tiles retain raw forward flow and UV scaling.
This extra guard addresses stale hints on static UI while restoring temporal
continuity. It does not identify HUD elements or provide engine depth.

Every application Present is still forwarded. Repeat/reset pairs set FG Off
with `eRetainResourcesWhenOff`, then subsequent unique pairs set On. Each frame
sets FG options only once and keeps Reflex pacing and SDK input-completion waits.
The Rust pause/Reflex layouts are checked against the pinned SDK by build-time
C++ static assertions and Rust ABI tests. SDK-generated images and their timing
remain owned by Streamline rather than Magpie's standalone presenter.

Validation: both hint modes passed the 1024x576 acceleration/stop/reversal sequence,
including exact repeats, a static UI patch, and alpha-only changes at the image
edge. The 321x193 and 2560x1335 translation/static/reset sequences passed strict
Vulkan core and synchronization validation with zero errors. At 2560x1335 all
3,172,416 interior samples matched the expected flow; unchanged UI was zero.
The copy/comparison GPU segment measured 0.238-0.714 ms in this synthetic run.
The added CPU fence wait is intentional and its gameplay cost still needs an
in-game measurement; synthetic timings include validation and upload/readback.

Real SDK exercise `sdk-fg-magpie-repeat-002` also passed: 120 alternating
new/repeat application frames reported 180 SDK presents, including 60 paused
repeats. Afterward 359 continuous application frames reported 717 SDK presents.
The child completed SDK input-lifetime waits and asynchronous native presents.
The earlier `repeat-001` sample lost foreground and is not valid recovery
evidence. Neither the successful synthetic SDK test nor motion-vector tests
establish that rapid camera rotation or startup ghosting is fixed in the game.

## Guidance and FG input alignment (2026-10-02)

This revision supersedes the static-tile guard and forward-only guidance above.
It follows the same pinned experimental Magpie fork, while preserving Streamline
Vulkan's input and presentation contracts.

NVOF now requests bidirectional flow and R8 matching costs when advertised by the
device. Session creation can fall back to bidirectional without costs, forward
with costs, then forward without costs. Unsupported capability bits are never
requested. Maps are copied into separate aligned buffer segments. The dense
shader samples raw S10.5 flow bilinearly at pixel centers and writes:

- `R16G16_SFLOAT` motion in pixels for FG;
- `R32G32_SFLOAT` motion in full-texture UV units for existing NR/SR consumers;
- a separate `R16_SFLOAT` confidence texture.

Confidence starts at `1 - forwardCost / 255`, or `0.65` without cost support.
With backward flow it is multiplied by forward/backward consistency, using
threshold `0.75 + 0.05 * length(forward)`, then by backward cost confidence if
available. A reference coordinate outside the image has zero confidence. Raw
motion is never masked by confidence. The initial alignment revision also removed
the unchanged-tile guard; the follow-up below restores it for FG only. Confidence is
stored separately; neither the Magpie FG call nor our public Streamline FG API
accepts this texture as an additional FG input.

Exact full-frame duplicates still bypass NVOF, retain the accepted reference and
temporal hint seed, and suspend FG with retained allocations. Resets clear all
three dense outputs and prime the reference. Only unique continuous pairs use
temporal hints. Application Presents and SDK input-lifetime waits are preserved.

FP16 storage images require `shaderStorageImageExtendedFormats`. The layer checks
device support and enables it in an owned copy of the application's core feature
structure; borrowed application feature memory is not modified. The frozen
target feature-chain allowlist remains in effect.

FG constants now use camera near `0.1`, far `1000`, and FOV `1.04719755` radians,
matching Magpie's defaults. For pixel motion, Streamline's normalization scale is
the inverse active tagged extent. Full-texture UV inputs keep the texture-to-
active-extent ratio. Compile-time bridge assertions check equal displacement
under both representations for cropped viewports, with independent axis scales.
These are default synthetic camera/depth parameters, not measured engine data.

The public `sl::DLSSGState` in the pinned 2.12 headers does not expose NGX's
per-output disable-interpolation result. Runtime telemetry therefore reports the
SDK's observed presentation count, pending/original-only/generated observations,
and explicit repeat/reset suspension, without treating an enable request as proof
of generation. This is asynchronous presentation feedback, not per-output
validity. Streamline continues to own generated-image selection and presentation.

Validation: the 1024x576 acceleration/stop/reversal sequence passed with both hint
modes; the 321x193 sequence passed every fallback profile; and the 2560x1335
release translation/static/reset sequence passed with core/synchronization
validation enabled. No Vulkan validation errors or warnings were reported by
these hardware tests. Readback compares selected raw-flow samples, FP16 pixel
motion and independently calculated cost/consistency confidence, including the
image edges and static UI patch. Exact repeats and resets clear all dense outputs.
These checks establish input correctness, not gameplay ghosting quality.

Real SDK exercise `sdk-fg-magpie-pixels-002` accepted FP16 pixel-motion inputs and
completed 600 application frames with input-lifetime waits. The alternating
120-frame segment reported 180 SDK presents with 60 repeat suspensions; the next
359 continuous frames reported 718 SDK presents. This validates suspension and
recovery through the proxy presenter. Its synthetic motion texture is zero, so
nonzero unit conversion is checked by the bridge assertions and NVOF readback
tests instead. `pixels-001` never obtained foreground and is not a valid sample.
Host and explicit Windows all-feature/all-target checks completed without
warnings; library tests passed 79, with the three GPU tests exercised separately.

The first emulator launch (`game-Na4Lew`) exposed a route bug in the cost-format
query: a new system Loader entry could not interpret the existing proxy physical
device. The query now resolves through the same instance GIPA as every other
guidance capability call. A regression test supplies deliberately non-Loader
handles to verify route ownership and missing-function fallback. All three GPU
sequences were rerun successfully after this fix. The failed launch is not game
acceptance evidence.

### Startup UI flicker follow-up (2026-10-02)

Bounded foreground captures in `target/ui-ghost-capture-003` covered frames
650–1801 with FG active and no intervening focus boundary. The original color,
NR output and DLAA output did not contain the reported severe block corruption.
The NVOF field did retain roughly 900–1200 pixel vectors on exact-static text
edges and horizontal borders. In frame 950, all 997 large-motion lit pixels in
the text region and all 200 in the save-label region belonged to unchanged RGBA
tiles. These are invalid screen-displacement hints for those static regions;
they are distinct from low-confidence motion on a changing scene.

The default FG pixel-motion output now zeroes only exact unchanged 4x4 RGBA
tiles, including partial edge tiles. NR/SR's UV field and the separate confidence
texture remain raw, and continuous NVOF temporal hints remain enabled. This is
an intentional deviation from the Magpie alignment to address the observed
static UI regression. `NS_STREAMLINE_NVOF_STATIC_GUARD=0` keeps the raw FG output
for diagnostic comparisons. Hardware readback checks the separate FG/NR-SR
contracts, exact static UI, alpha-only changes, edge tiles and moving fields.

Capture can also be enabled with `NS_STREAMLINE_CAPTURE_REQUESTS=1`; the existing
session worker accepts `captureRevision` and `captureFrames` in `control.json`.
Each new revision requests 1–8 future frames, with 64 total per process. A frame's
selection is cached across NVOF/NR/SR/FG so a request arriving mid-frame cannot
produce mismatched stage captures. This opt-in diagnostic mode performs bounded
GPU readback and affects timing; it is not enabled in ordinary game launches.

Foreground follow-up `target/ui-ghost-capture-004` confirmed all captured
exact-static UI pixels with large raw motion had zero FG motion after the guard.
The user confirmed the startup block corruption and continuous flicker had
disappeared before any focus switch. The rotating autosave highlight was still
dimmer, which is a separate DLAA-stage issue.

### Historical DLAA animated-highlight experiment (2026-10-02)

Same-frame captures isolate the dimming before FG: at frame 950 in session 004,
the spinner input had a maximum encoded RGB channel of 255 and a 99th percentile
of 251; DLAA output dropped to 161 and 122. Static text retained its brightness.
This is animated-feature loss in temporal reconstruction, not a global exposure
change or a consequence of FG's exact-static motion guard.

Session 005 tried the public `kBufferTypeBiasCurrentColorHint` input. The SDK
accepted its resource tag, but the spinner still darkened. The current
[NVIDIA DLSS programming guide](https://raw.githubusercontent.com/NVIDIA/DLSS/main/doc/DLSS_Programming_Guide_Release.pdf),
section 3.15, says this buffer is not supported by the latest models and only
preset F supports it. The attempted tag, dense mask, descriptors and bridge ABI
extension were removed; none are part of the resulting package.

Because the emulator source already contains composited UI, the present-source
SR diagnostic path can bound each reconstructed RGB pixel by the minimum and maximum of
nine bilinear samples from the current input's 3x3 neighborhood, expanded by
0.02 in encoded UNORM units. Coordinates use independent input/output axis
ratios and clamp at the image edges. Valid reconstruction inside those bounds
is unchanged; output alpha is preserved. This local spatial constraint can
restore a bright animated feature erased by history without resetting the
whole DLAA history or changing the NVOF/NR input contracts. It is not an engine
UI mask and may also constrain strong reconstruction changes in scene content.
Gameplay quality and performance still need to be assessed separately.

The pass uses the existing input and output images, after DLSS evaluation and
before presentation. An explicit GENERAL-to-GENERAL dependency orders the SDK's
writes before compute reads/writes, followed by the existing transfer dependency.
It shares SR's completion fence and adds no normal-launch readback or output
image allocation. It is disabled in ordinary launches; only the explicit
`NS_STREAMLINE_SR_COLOR_GUARD=1` diagnostic opt-in enables it.
Opt-in captures store the untouched SDK output in `sr-raw` and the protected
output in `sr`, from the same frame and submission.

The `hardware_sr_color_bounds` test passed core and synchronization validation
with GPU readback at 1x, approximately 1.4x, below 1x and approximately 2x.
It covers non-workgroup-aligned dimensions, edge sampling, artificially dark
and bright history, preservation of valid reconstructed values, unchanged input
bytes and output alpha. No validation errors or warnings were reported. The
library suite passed 81 tests; all four hardware tests are ignored by default.
Reproduce with `VK_LAYER_PATH` set to the local Vulkan SDK's Bin directory:

```powershell
cargo test --locked --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features native-nr --lib hardware_sr_color_bounds -- --ignored --nocapture --test-threads=1
```

Foreground session 006 accepted FG/NR/SR with two SDK presents in all four
captured frames, and no focus/reset boundary. In frame 950 the spinner input's
maximum/q99 encoded channel values were 255/253, untouched SDK output 180/141,
and bounded output 249/244. Exact-static FG motion stayed zero. However, the
user observed thinner spinner strokes. This workaround is therefore not game
acceptance evidence and is disabled by default. Its local upper bound can
remove reconstructed halo/edge color while restoring the bright core.

### Magpie SR comparison before the selected alignment

Reference remains the experimental SAOG0721 fork at
`2c017d4be8311fac8584e2ff6187b1f03383bde1`, specifically
[DLSSSRUpscaler.cpp](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/DLSSSRUpscaler.cpp)
and
[FrameGuidanceService.cpp](https://github.com/SAOG0721/Magpie/blob/2c017d4be8311fac8584e2ff6187b1f03383bde1/src/Magpie.Core/FrameGuidanceService.cpp).
These describe this fork's requested parameters, not a guarantee of the model
selected by an installed DLL/driver, nor the official Magpie main branch.

| Item | Magpie SR adapter | Emulator SR before alignment |
| --- | --- | --- |
| SDK | Direct NGX D3D11 | Streamline Vulkan, internally NGX |
| Mode/preset | Fixed Balanced, requests J; no separate DLAA selector | User mode with supported-range selection, this sample requested DLAA/M at 140% |
| Depth | Synthetic zero (ordinary SDR path) | Synthetic constant 0.5 |
| Motion | Current-to-previous source pixels in FP16; consumer resize scales both displacement axes | Current-to-previous UV in FP32; viewport crop and Streamline scaling convert to input pixels |
| Guidance source | Captured pipeline frame, adapted to each effect input extent | Unprocessed present frame; native/NR color input uses a cropped, resized guide |
| Jitter/exposure | Zero jitter, automatic exposure, neutral pre-exposure/exposure scale | Same |
| Optional bias | Uniform R8 mask 0.5 | None in the resulting normal package |
| Extra output constraint | None in this SR adapter | Diagnostic opt-in only; rejected as a normal fix after thin-stroke feedback |
| History | Initialize/resize/guidance reset or zero/real binding change | Initialize/source mapping/region/focus/flow/NR boundaries |

Magpie also writes NGX sharpness 0.3. The current Streamline header marks
sharpness unsupported/deprecated, so copying that number does not establish a
current-runtime fix. Likewise, the old bias mask is not a supported remedy for
the current J/M models per NVIDIA's guide. Both implementations use estimated
motion and synthetic depth rather than engine data. The SR loop is not globally
guarded by `isNewCaptureFrame` inside Magpie's renderer; the capture scheduler
and effect chain determine when it runs. Do not infer that every native SR call
is skipped merely because FG/NVOF filters duplicate capture frames.

The useful next controlled comparison is requested J versus M, then zero versus
0.5 depth while keeping the working FG static guard, input dimensions and color
pipeline fixed. The mode selected by the supported-range loop also needs to be
reported rather than treating the panel's requested label as its SDK mode.
Neither parameter difference alone was established as the cause of the dim spinner.


### Selected Magpie SR alignment (2026-10-02)

The selected parameters now use Balanced and preset J as the Rust configuration
and launcher defaults. The local test configuration also selects Balanced/J;
explicit user mode/preset choices remain supported. SR never silently substitutes
another mode to fit a ratio, including 1x. It retains the requested dimensions
and rejects ratios outside that mode's reported range.

SR depth is zero R32F. Jitter is zero and automatic exposure is enabled with
neutral pre-exposure/exposure scale. The experimental post-DLSS color constraint,
its shader and GPU test have been removed after the thin-stroke feedback. The
historical opt-in described above no longer enables a pass; SDK output is used
without this additional color modification.

NVOF now writes a separate raw RG16F current-to-previous source-pixel field for
SR. The working exact-static RGBA tile guard still applies only to FG's separate
RG16F field; NR continues to receive the original RG32F UV field. `sr_motion.comp`
samples the raw pixel field in the present-space viewport, clamps sampling to
cropped pixel centers to exclude letterbox motion, and scales displacement by
SR input width/crop width and SR input height/crop height. Streamline receives
RG16F input-pixel motion with inverse input dimensions as `mvecScale`, which its
adapter converts back into NGX pixel units.

SR motion availability is captured before NR consumes the NVOF semaphore.
Consumption no longer makes the still-valid field appear missing to SR or force
zero motion/reset on each NR frame. NR's completed fence or SR's NR handoff wait
orders the flow producer before the SR adapter; the adapter writes are ordered
before SDK evaluation by a GENERAL-to-GENERAL dependency.

Verification: 81 library unit tests and all four hardware tests passed. GPU
readback covers raw SR versus guarded FG motion, resets, duplicate/static frames,
fast pan with both temporal-hint modes, cost/backward fallbacks, and the actual
2560x1335 game extent. The cropped-motion test covers four non-workgroup-aligned
input sizes, per-axis scaling, direction, letterbox isolation and unchanged source
bytes. Core/synchronization validation reported no errors or warnings.

The isolated SDK test in `target/sr-magpie-alignment-002` uses the production
viewport-1 options/evaluate bridge. Balanced/J and zero depth successfully evaluate
three frames each at 1920x1080 -> 2688x1512 (140%) and 960x540 -> 960x540 (100%),
including resize/free/recreate and explicit history resets. Fences and checker
readbacks pass. Reproduce with `NS_STREAMLINE_SR_MAGPIE_TEST=1` and the existing
`--sdk-sr` diagnostic command. Synthetic execution does not establish the spinner's
appearance in the game; this alignment has not been accepted in a foreground game
comparison yet.
