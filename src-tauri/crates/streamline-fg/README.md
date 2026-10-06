# Streamline FG runtime

Windows x64 Vulkan frame-generation layer and the launcher used by the toolbox. This remains an experimental integration: it uses reference constants and constant depth. Toolbox launches request native NVIDIA hardware optical flow for motion vectors, with zero-motion fallback when unavailable. See [NVOF integration](docs/NVOF.md). Optional source-based SR/DLAA and native NR have independent controls; NR pauses without valid motion. Window-operation protection and SDK capability checks still apply.

## Source layout

- `src/lib.rs`, `target_*.rs`, `live.rs`: Vulkan layer, frame generation, lifecycle and live control/telemetry.
- `src/main.rs`, `launcher.rs`, `session_verify.rs`: toolbox launcher and session verification.
- `src/support.rs`, `runtime.rs`: shared file validation and pinned runtime checks, with no dependency on the diagnostic host.
- `src/diagnostics/`: standalone Vulkan host, SDK routing experiments and diagnostic regression tests. These are compiled only with `--features diagnostics`.
- `sdk/baseline.json`, `bridge/`, `sdk-route/`: pinned SDK contract, C++ bridge and reproducible downstream runtime patches.
- `evidence/`: regression fixtures and compact final measurements; no bulk experiment logs.
- `docs/experiments/`: historical investigations, not runtime dependencies.
- `../streamline-target-policy.rs`: compatibility rules shared with the toolbox.
- `../../tools/streamline-sdk-audit`: optional read-only SDK/source auditing utility. The runtime does not depend on this tool.

## Build and package

Run from the repository root. The pinned SDK/header checkouts remain under `src-tauri/target`; see `sdk-route/README.md` for preparing the runtime. The SDK bridge build verifies its input headers against the runtime-owned baseline.

```powershell
cargo build --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features sdk-bridge --lib --bin streamline-layer-probe
& src-tauri/crates/streamline-fg/package-local.ps1
& src-tauri/crates/streamline-fg/package-runtime.ps1
# Publish the layer ZIP, manifest, source snapshot and checksums under its layer tag.
# Publish the stable runtime ZIP under its runtime tag once; reuse it on layer updates.
# Both tags are printed by package-runtime for triwinds/ns-emu-tools-runtimes.
& src-tauri/crates/streamline-fg/package-runtime.ps1 -EmbedManifest
# Rebuild the toolbox after embedding the published URLs and SHA-256 values.
cargo build --locked --release --manifest-path src-tauri/Cargo.toml --bin NsEmuTools
```

The launcher and DLL retain their installed names, `streamline-layer-probe.exe` and `streamline_probe_layer.dll`, along with the existing CLI/environment/Vulkan protocol. `package-local.ps1` builds them with `native-nr` and also builds the audited `nvngx.dll` caller bridge using the same dynamic CRT setting. `package-runtime.ps1` puts these three frequently updated binaries and their notices into a small layer ZIP; Streamline, DLSS, Reflex, NR and their notices go into a separate stable runtime ZIP. The generated combined manifest lists both downloads and all file hashes. By default packaging preserves the local launch manifest; use `-EmbedManifest` with the same revision arguments after publishing. `NrDirectory` is a maintainer build input; users never select DLLs. `BundleRevision` and `RuntimeRevision` independently select immutable layer and runtime revisions when repackaging. Existing archives are reused only when all members match; changed contents require a new revision. Corresponding layer source accompanies its Release. Runtime binaries are hosted as Release assets in the runtimes repository, not committed to this source repository.

Advanced settings are defined in `../streamline-advanced-settings.rs`, shared with the toolbox configuration. The toolbox passes validated `NS_STREAMLINE_ADVANCED_SETTINGS` JSON to the launcher; the launcher records and forwards it to the layer. NR exposes styles A/B/C, 0–200% intensity and independent tone/structure strengths, skin structure and intrinsic automatic masking. Null tone/structure strengths preserve the legacy link to intensity. SR exposes automatic exposure and a 0.25–4.00 exposure scale. FG exposes fixed or dynamic generation, 2–6× multipliers within the SDK-reported limit, a display-matched or 30–360 FPS dynamic target, low latency or Boost, and a 0/unlimited or 15–240 FPS original-frame cap through Reflex.

Live protocol 1 retains independent revisions: `nrOptions` accompanies `nrRevision`, `srOptions` accompanies `srRevision`, and `fgOptions` accompanies `revision`. Invalid options reject the entire feature update; legacy switch commands retain tuning. NR tuning changes reset all downstream temporal consumers. FG rejects unsupported live multipliers; unsupported startup selections suspend generation and report the reason. Telemetry advertises `advancedSettingsSupported` and the measured `maximumGenerated`. The verified DLL exports a capability marker so older installed packages cannot silently ignore tuned startup settings. These changes require a newly packaged layer release; the embedded online manifest must only be updated after its assets are published.

Toolbox checks query the runtimes repository's published `streamline-layer-*` Releases, including experimental prereleases, and read the newest matching `<tag>-manifest.json`. Runtime-only and AIO Releases are excluded. The manifest declares `schema_version: 1` and `launcher_protocol: 1`; incompatible protocols or changed stable binary contracts require a toolbox update. Upload the two ZIPs before making the layer Release public. Future compatible small-package updates need only a new layer Release and its combined manifest, without rebuilding the toolbox. The check reports the installed and available versions without altering any installation. Installation verifies both archives and every file, reuses stable archives by SHA-256, then atomically selects a separate immutable installation for that emulator. The selected manifest persists across toolbox restarts. Network failures leave existing installations and running game snapshots usable.

The `native-nr` build uses bounded deferred NR → DLAA completion by default when frame generation is off, native inputs are available, no FG timeline work remains, and the frame does not request readback. Borrowed game inputs finish copying before presentation returns; private processing and native presentation retire before reuse or teardown. Set `NS_STREAMLINE_DEFER_PRESENT=0` to restore the synchronous tail in diagnostics. The toolbox exposes NR as an experimental, default-off option after automatic runtime installation. Strict Vulkan validation still fails for the game; see the [measurements](evidence/nr-tail-2026-10-01/README.md) and [validation review](evidence/nr-tail-2026-10-01/validation-review.md).

## Toolbox use

NR Look controls operate after the model under the SDR sRGB contract. Neutral settings bypass all added dispatches, and Look-only changes preserve NR history while resetting downstream color consumers. Optional spatial Look is default-off: it separates broad lighting and detail with a guided 5×5 sparse stencil, then optionally weakens negative local changes on the bright side of input edges. Radius is 1–32 NR working pixels; halo suppression defaults to zero. The band stage completes its neighbour reads before per-texel composition mutates NR output. It allocates at most one extra private RGBA16F image, retains it across tuning changes and retires it with fenced NR resources. A preparation failure preserves basic Look and reports a sticky error until recreation. Spatial settings require the `NS_EMU_NR_SPATIAL_LOOK_V1` layer marker and `nrSpatialLookSupported` live capability. See [implementation and fixed-input GPU validation](../../../docs/dlss5-generic-nr-pipeline-implementation.md). Game-video and performance acceptance remain pending; rebuilding the toolbox alone does not update an installed layer.

NR now compares its actual private gamma input, including native crop/flip/codec conversion, and reuses the final output on exact repeats. Its color-observation ID is separate from Present/FG IDs and is not an emulator simulation-frame ID. Repeats skip NR and Look; tuning changes wait for a new input while preserving pending resets, and NR can still be disabled during that wait. The input fence retires borrowed images and exposes only a four-byte comparison result; one additional RGBA16F image retains the previous input. All NR paths now pay this input-submit/fence and copy/compare cost. Live and evidence fields distinguish output activity from evaluation and report reuse/pending controls.

Optional `nr.secondPass` is default-off and inherits first-pass model settings by default. It owns separate NGX parameters, feature, history and RGBA16F output; `P0 -> R1 -> R2` shares current depth/motion guides. Look runs once on the last successful `(input, output)` pair. Pass-count changes and explicit retry open a reset epoch, including on a repeated color. The first pass is fenced before recording the second; a failed unsubmitted second recording is discarded and its resources released before first-pass fallback. Failures stay sticky until explicit retry/toggle/recreation; submitted-work failures propagate. Disabling NR retires both instances. Two-pass use adds an intermediate CPU fence wait and can retain one spatial band per Look pair. It requires `NS_EMU_NR_TWO_PASS_V1` and `nrTwoPassSupported`. Actual passes, independent resource identities, resets, GPU/CPU pass timings and owned secondary allocation bytes are reported; SDK private memory is unknown. The isolated `streamline-nr-diagnostics --two-pass` test verifies real NGX independent instances, continuous history, independent second tuning, finite changing output and safe discard/recreation under strict validation. It requires the pinned runtime hashes, internal-layout repair and validation, and does not prove game, SR/FG handoff, visual quality or performance acceptance.

Build the frontend (`bun run build` in `frontend`) before rebuilding the toolbox. In **图形增强**, select the emulator and click **下载并安装画面增强组件**. The tool downloads the pinned runtimes Release using its normal download manager, verifies the archive and each payload, and installs NR with the other components. **下载并安装 NR 组件** restores a separately uninstalled NR component using the same cache. No DLL picker or folder beside `NsEmuTools.exe` is required. Selection checks stay read-only; installation initiates the download, with progress and cancellation. Verified caches work offline; damaged caches are replaced only after a successful download, with old files preserved. Model DLLs remain immutable managed versions, and game launches use private snapshots. Uninstall rejects altered or unowned managed files and leaves running copies intact.

Use **以画面增强启动**. NR, SR / DLAA and FG have independent controls; the NR slider saves 0–100% strength and applies it to a prepared live session. Installing NR after game startup requires another dedicated launch. NR pauses without valid NVOF and reports synthetic depth, actual input size, strength and running state. Normal launches omit diagnostic Vulkan validation and readback; their successful exit is not strict acceptance. The separate diagnostic modes retain their existing validation gates. Existing Ryujinx errors are outside the repair scope authorized by the user; they remain in the earlier raw evidence.

Native queue submissions, presentation and device-idle calls now share a device lock across the application and SDK downstream route; SDK hooks and worker waits execute outside that lock. Virtual PRESENT layouts are translated to TRANSFER_SRC only for proxy images also proven to originate from SDK `vkCreateImage` calls, preserving native swapchain layouts. Strict readback prefers coherent cached memory and copies mapped data into host RAM before analysis. See the [safety regression record](evidence/nr-safety-2026-10-01/README.md); upstream application resource-lifetime errors remain unresolved.

The native NR verifier also checks per-frame color/reset propagation and actual activation for all eight NR/SR/FG combinations. Continuous-consumer toggle evidence requires unchanged revisions and resource/fence identities; background FG requests do not count as active combinations. See the [combination test](evidence/nr-combinations-2026-10-01/README.md) for real FG results and the separate SDK clone initialization finding. Build native NR and NGX diagnostics with `$env:RUSTFLAGS='-C target-feature=-crt-static'` to match the pinned `nvsdk_ngx_d` runtime library; standard toolbox builds retain their existing CRT setting.

## Optional diagnostics

The pinned SDK's two fresh `dlfg-output_0/1` clone images now correct their first
`TRANSFER_SRC -> GENERAL` barrier to `UNDEFINED -> GENERAL`. The adapter requires
SDK creation, the captured BGRA8 resource contract, successful bind, exact debug
name and the observed first barrier shape; it consumes freshness on the first
barrier or image copy and removes records at destruction. It introduces no GPU
submission or CPU wait. `NS_STREAMLINE_SDK_OUTPUT_INIT=0` restores the original
SDK behavior for comparison; `NS_STREAMLINE_SDK_LAYOUT_TRACE=1` enables bounded
creation/bind/first-use evidence. See the [SDK layout investigation](evidence/sdk-fg-layout-2026-10-01/README.md).

The SDK-created, bound and exactly named FG images and NGX buffers also complete
shader-only `ALL_COMMANDS` barriers with compatible transfer read/write access.
This covers NGX clear, copy and buffer-fill dependencies while preserving layouts,
ranges, command count and waits. `NS_STREAMLINE_SDK_TRANSFER_ACCESS=0` disables
the adapter for comparison. The [transfer synchronization evidence](evidence/sdk-fg-transfer-2026-10-01/README.md)
records the same-build strict SDK-only off/on result; Vulkan validation warnings
and SDK textual warnings remain separate records.

`streamline-fg-diagnostics --sdk-fg --validation-dir <VVL Bin>` uses the pinned
VVL 1.4.363.0 for an SDK-only test with core and synchronization validation. Build
with `native-nr,diagnostics` and the dynamic CRT setting above; NR is unrequested
in these diagnostic children. This mode isolates FG from the older command and
resize experiments, waits up to five minutes for foreground, and preserves raw
validation errors. It does not bypass strict acceptance.

```powershell
cargo run --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features diagnostics --bin streamline-fg-diagnostics -- --help
cargo test --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --all-features
cargo test --locked --manifest-path src-tauri/tools/streamline-sdk-audit/Cargo.toml
```

The default launcher no longer accepts SDK-host modes. The separate diagnostic binary retains them, including explicit `--target-probe` session verification. Historical commands that used the old combined executable for SDK host tests must use `streamline-fg-diagnostics` instead.

## Experimental temporal NR Look

Optional `nr.look.temporal` is default-off. It reprojects raw log model-delta history with current-to-previous cropped UV motion, rejects input-color mismatch, invalid/out-of-bounds/large motion and dark pixels, and clips history to a guided 3×3 delta range. The elapsed color-observation interval sets exponential decay, capped by the configured history strength; repeated colors skip history advancement. Model resets, Look changes, gaps, nonpositive intervals and intervals over 250 ms seed a fresh Look history without making Look tuning reset NGX. Each prepared Look pair retains two RGBA16F delta images and one previous raw-input image until retirement; two-pass fallback caches can retain two independent groups. Temporal writes/read completion and raw-input copy precede spatial band and final composition. Preparation failure preserves other Look stages and stays sticky until recreation. It requires `NS_EMU_NR_TEMPORAL_LOOK_V1`/`nrTemporalLookSupported`. GPU fixed-sequence and core/sync validation pass, but estimated flow has no supplied occlusion/cost confidence, and real-game flicker/ghosting, NGX/SR/FG integration, performance and memory acceptance remain pending. See [P5 implementation and evidence](../../../docs/dlss5-generic-nr-pipeline-implementation.md).

Temporal Look supports both RG16F (the production NR UV guide) and RG32F motion
images. Both variants share `nr_look_temporal.comp`; rebuild the FP32 binary with
`glslc --target-env=vulkan1.1 nr_look_temporal.comp -o nr_look_temporal.spv` and the
FP16 binary with `glslc --target-env=vulkan1.1 -DNR_MOTION_FORMAT=rg16f nr_look_temporal.comp -o nr_look_temporal_fp16.spv`
from `shaders`, validate both with `spirv-val --target-env vulkan1.1`, and update
their paired entries in `motion.json`. The opt-in GPU test exercises both formats.

Look now has an explicit `scope` (`final_pass`, the legacy default, or
`chain_total`). Both scopes validate temporal history against the original
mapped NR input P0. Scope support requires `NS_EMU_NR_LOOK_SCOPE_V1` /
`nrLookScopeSupported`; neutral/bypassed Look remains bit-exact. Two-pass amount
zero preserves R1 RGB in final-pass scope and P0 RGB in chain-total scope, with
R2 alpha in both cases. Presets export schema 4 and migrate schemas 1/2/3.

Temporal `mode` defaults to `optical_flow`; explicit `static` accumulation
checks same-position original color and neighborhood stability without reading
motion. NR itself still requires the existing valid NVOF guide. `sampling`
defaults to legacy `bilinear`; experimental `per_tap` rejects each corner
before normalizing support (minimum accepted support 0.25). The extensions
require `NS_EMU_NR_TEMPORAL_MODES_V1` / `nrTemporalModesSupported`. Compile the
static variant with `glslc --target-env=vulkan1.1 -DNR_STATIC=1 nr_look_temporal.comp
-o nr_look_temporal_static.spv`, validate it and update `motion.json` too.

Pure Look changes on a repeated source recompose immediately from a private
pristine model-output cache, without NR Evaluate or temporal advancement.
`outputReused` continues to describe model reuse; `lookRecomputed` and
`finalOutputReused` distinguish recomposition from unchanged final pixels.
The cache adds one RGBA16F allocation per prepared Look pair, reported under
`lookCacheResources`. Model tuning on an exact repeat now recomputes the affected
suffix with NGX reset, preserving the source ID and skipping temporal observation.
Second-only changes reuse the first raw output; degraded chains conservatively
recompute the prefix when it may have been overwritten by Look. The existing
first-pass fence remains in place.

Experimental `optical_flow_plus` tracks conditional log amplitude and presence
separately. It uses 60/180 ms rise/fall, the existing `timeMs` for amplitude,
0.01 stops for presence, and one extra RGBA16F final-delta texture. It requires
`NS_EMU_NR_PERSISTENCE_V1` / `nrPersistenceSupported`; ordinary optical flow remains
the default. Compile the two variants with `-DNR_PERSISTENT=1`, and additionally
`-DNR_MOTION_FORMAT=rg16f` for the FP16 motion variant. Validate every Look SPIR-V
against Vulkan 1.1 and update `motion.json`.

Optional `algorithm=oklab`, `protection` percentages and `diagnostic` views require
`NS_EMU_NR_LOOK_EXPERIMENTS_V1` / `nrLookExperimentsSupported`. Oklab changes the
chroma decomposition; luminance controls retain their stops contract. Diagnostics
read pristine P0/R1/final caches and do not feed model inference. The ten views
include actual per-pixel temporal weight; no aggregate acceptance readback is
claimed. These algorithms passed fixed-input GPU checks but still need real
motion/occlusion videos and performance acceptance.

The SDK proxy's image tracking and PRESENT-to-TRANSFER_SRC translation now remain
active with optional source probes disabled, including synchronization2 barriers.
Submit2 queue synchronization also remains active without source probing. Only
SDK-created ordinary images belonging to a live proxy are translated; native
swapchain images retain PRESENT layout. The strict game baseline still reports
other renderer errors. Per the user's clarification, errors already present
with enhancement off no longer block M5/M6 implementation. Raw strict game
acceptance remains separate. See the
[suffix/persistence/color and baseline evidence](evidence/nr-suffix-persistence-color-2026-10-05/README.md).

Strict native-NR/SDK validation additionally tags application shader/pipeline,
render-pass/rendering, clear, query-end and draw entry points in raw callbacks,
without enabling source probes. Normal graphics launches omit these additional
wrappers. The tag identifies the entry point, not the final fault owner; the
wrapper preserves arguments and forwards the existing driver/SDK route. It does
not filter validation messages or change rendering to hide errors.

The target's SDK-only debug-messenger route escapes literal percent characters
for the pinned Streamline printf-style error callback. Capture is scoped to
`slSetVulkanInfo`; application callbacks and raw validation JSON remain unchanged.
This prevents a logging crash and does not make a failed validation session pass.

## Experimental NR scheduling and inference sizing

**输入尺寸缩放** is a standalone section above NR/SR/FG on the main panel.
Choose **按输入比例** (50–100%, default 100%) or **固定长边上限** (320–8192
pixels). `streamline_input_max_edge` defaults to 0. A nonzero cap supersedes the
percentage, preserves aspect ratio with integer rounding down, and never
enlarges smaller input. Cap 1920 maps 2560x1325 to 1920x993. Reduced inputs
must be at least 320x180. The old NR sizing keys migrate to these shared keys.

`--input-scale` / `--input-max-edge` require the independent SDK layer marker
`NS_EMU_INPUT_SCALING_V1`, not NR or its runtime. In normal graphics launches,
one shared SDR downsample produces private encoded RGBA16F color, including
crop/flip and mutable-SRGB byte reinterpretation. NR/SR read this smaller
source. With both off, the private color is enlarged back into the original
viewport; FG receives final presentation dimensions in every combination.
NVOF still analyzes original present pairs. Source remapping resets consumers,
wait semaphores are consumed once, and private submissions complete before
handoff. The full image, including UI and fine lines, participates in scaling.

Launch arguments seed the input settings. Sessions advertising
`inputScalingLiveSupported` accept `inputSizing: {scalePercent, maxEdge}` with a
monotonic `inputScaleRevision` in the session-local control file. Both values
are required and validated together; stale/invalid updates leave controls intact.
On the next present, pending GPU consumers retire before SR, NR and shared input
resources are rebuilt; NR/SR/FG/flow history resets at the same boundary. No
process environment mutation is used. Paused games apply on their next frame.
Actual source/input/output dimensions, preparation failures and
`appliedRevision` appear in `inputScale`. A failed preparation reports the
original input and an error; the GUI does not claim successful scaling.
The toolbox persists each change and waits for the renderer's revision before
confirming live application. Older sessions require one component update and
relaunch; subsequent input adjustments in a live-capable session need no restart.
Scheduling remains under NR **提交方式实验** and defaults to fenced passes.
Legacy `--nr-inference-*` options retain the old NR delta reconstruction for
diagnostics; normal shared scaling cannot combine with those or legacy readback.
Functional testing is handed to the user as requested.

`--nr-consolidated` keeps borrowed-input retirement and submits independent
first and second/tail command buffers together. A failed second recording is
discarded; the valid prefix is completed before feature release and fallback.
Default remains fenced passes.

`--nr-inference-scale 50..100` retains full-resolution P0, downsamples once for
all model passes and reconstructs the controlled total linear SDR RGB correction
onto the original base. Original alpha and zero-delta detail remain exact.
Default is 100. Reduced extents must be at least 320x180; this experimental bound
does not claim the model minimum. HDR and legacy NR readback are rejected for
reduced inference. Allocations and actual inference extent are reported. Both
options require matching DLL capability markers.

Isolated `--two-pass --consolidated` results match fenced chaining hashes and
pass discarded-tail recovery. Core shutdown timed out in both schedules, so
execution checks do not certify normal retirement. Offscreen reconstruction
passed strict GPU checks. Game/video quality, combinations and performance are
separate gates. See [scheduling/resizing evidence](evidence/nr-scheduling-resizing-2026-10-06/README.md).

## NR presets in the toolbox

The toolbox NR advanced dialog also supports a local NR-only preset library and
validated JSON import/export. Presets keep emulator/game/display labels, model,
two-pass and Look settings, and verified installed component/model version hashes.
Import is preview-only until explicitly applied; metadata describes installed files,
not the private snapshot already loaded by a game. Missing capabilities remain
explicit and the runtime checks still apply. A separate single-pass/Look bypass
keeps tuning values, while restoring defaults resets tuning. See the
[P6 format, migration and validation record](../../../docs/dlss5-generic-nr-pipeline-implementation.md).

## Experimental present-source SR

Use the toolbox SR switch or `--sr-mode quality` alongside `--fg`. Default off. This version downsamples the final emulator image and reconstructs it to the same window size; it does not lower emulator rendering resolution. See [scope, tests and known exit failures](docs/SR-present-experiment.md).
