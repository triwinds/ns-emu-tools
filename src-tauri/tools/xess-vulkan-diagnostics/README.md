# Isolated XeSS Vulkan Native AA / SR diagnostics

Standalone Rust/Vulkan executable. It loads the unmodified Intel XeSS 3.0.2 SR
DLL directly; no Streamline, emulator, swapchain, frame generation or XeLL is
loaded. The runtime identifies its **SR component** as **2.0.2**. This is distinct
from the SDK bundle version.

The test is intended to establish API execution, input conventions, bounded
GPU completion, synthetic image quality and isolated processing cost. It does
not establish emulator compatibility, in-game quality, FPS gain or end-to-end
latency. Analytic motion is more reliable than motion estimated from game
images, including in the degraded-input runs.

## Prepare and run (Windows x64)

From the repository root:

```powershell
& src-tauri/tools/xess-vulkan-diagnostics/prepare-sdk.ps1
# Requires glslangValidator. Default: existing Vulkan SDK 1.4.363.0 under
# src-tauri/target. Set GLSLANG_VALIDATOR for a different compiler location.
cargo build --locked --release --manifest-path src-tauri/tools/xess-vulkan-diagnostics/Cargo.toml
& src-tauri/tools/xess-vulkan-diagnostics/run-matrix.ps1 `
  -Evidence D:/py/ns-emu-tools/build/xess-vulkan-my-run
```

`prepare-sdk.ps1` verifies SHA-256 for the DLL, public headers and original
license/third-party notices. These external files remain under ignored target
storage. The Intel DLL was Authenticode-verified on the initial run. Runtime
loading checks the pinned DLL SHA-256 again; another version is rejected.
Headers are reference material, not bundled/modified source. No vendor binaries
are committed or installed into graphics components.

The matrix isolates implicit layers only in its child sessions and restores the
caller's environment afterward. Validation uses core and synchronization
validation. The four exact GENERAL/ID-zero loader warnings about intentionally
disabled NVIDIA Optimus/present, AMD switchable graphics and ReShade layers are
preserved separately as `loader_isolation_notices`; every other Vulkan warning
or error fails the report. SDK warning/error callbacks fail the report too.

Direct invocation supports:

```text
--dll <absolute libxess.dll> --output <new absolute directory>
--width 1920 --height 1080 --frames 144
--mode all|aa|quality|balanced --gpu-index 0
--validation --high-motion --no-jitter --flat-depth --temporal
--jitter-sign-x 1|-1 --jitter-sign-y 1|-1
--benchmark-only
--zero-motion-reset (requires --mode aa; zero jitter/MV, reset each frame)
```

The default adapter is Vulkan enumeration index zero, recorded in `report.json`;
use `--gpu-index` to select another adapter. Support is queried from the SDK
before creating the Vulkan instance/device. Required extension lists and device
feature chains are obtained at runtime. The diagnostic root enables FP16 storage
images; XeSS augments it with its required features. No NVIDIA vendor restriction
is applied. The initial run tested an RTX 5070 Ti Laptop at 1080p/1440p and an
AMD Radeon 610M at 640x360 (all three modes, core/synchronization validation).
Intel hardware has not been tested. Use `-GpuIndex` with `run-matrix.ps1` to
select a different adapter for its entire matrix.

## Scene and comparisons

All modes receive the same analytic scene, with point-sampled linear RGB input:

- Frames 0–47: static wires, fine checkerboard, concentric curves, a smooth
  background and an occluding textured foreground rectangle.
- Frames 48 onward: subpixel camera translation and independent foreground
  translation, with analytic current-to-previous motion excluding jitter.
- Frame 112: explicit color cut and history reset; later captures assess
  recovery. This is an explicit reset, not automatic scene-cut detection.

Jitter uses a Halton(2,3) sequence of length `ceil(8 * output/input ratio²)`.
Input samples use `(pixel + 0.5 - jitter) * output/input`; API jitter signs default
to `(1,1)`, calibrated against all four sign combinations and the known reference
for this direct compute-texture convention. Do not transfer a raster sample's
viewport-axis correction to this compute generator without checking the actual
sample locations. Sign overrides and actual values are recorded in the report.

Low-resolution motion is sampled on the jittered input grid and uses input-pixel
units, with analytic foreground/background depth. High-resolution motion is on
the unjittered output grid, uses output-pixel units and selects the foremost
surface in a 3x3 output-pixel neighbourhood; no depth texture is passed to XeSS.
Matched low/high runs check this contract. The flat-depth variant replaces the
low-resolution depth values with 0.5. The no-jitter variant supplies real
unjittered samples and zero offsets, without claiming new subpixel information.

Preset sizes come from `xessGetOptimalInputResolution`, rather than hardcoded
ratios. LDR input mode is enabled, all processing stays linear, and saved PNGs
are explicitly sRGB-encoded. Input, XeSS output, native point-sampled/no-AA,
jitter-aligned bilinear and a 64-spatial-sample reference are saved at frames
47, 95, 112 and the last frame. Metrics use unclamped linear RGB against that
reference and exclude eight outer pixels. The higher-resolution native baseline
is a quality reference; no native rendering speedup is inferred from it.

`--temporal` also records static consecutive-frame RGB MAE (frames 40–47) and
four-region, unscaled motion crops (frames 80–95). A constant output may have
zero consecutive-frame error while still being spatially aliased: interpret
that metric together with the reference comparisons.

```powershell
& src-tauri/tools/xess-vulkan-diagnostics/compare.ps1 `
  -Evidence D:/py/ns-emu-tools/build/xess-vulkan-my-run/standard-1080 -Frame 95
python src-tauri/tools/xess-vulkan-diagnostics/animate.py `
  D:/py/ns-emu-tools/build/xess-vulkan-my-run/standard-1080
```

Contact sheets use nearest-neighbour 2x crops of the original PNGs. GIFs are slow
inspection loops with a lossy 3-3-2 palette; use PNGs to judge color and fine detail.

## Timing, resources and checks

The scene generator and optional reference pass finish in a separate, fenced
submission before the timed submission starts. Two GPU timestamps enclose only
`xessVKExecute`'s commands. Readbacks, metrics, PNG encoding, reference generation
and presentation are outside that interval. CPU recording time is separate.
First 32 frames are excluded from the median/p95; every frame's timing and reset
status is retained. The separate 1,000-frame `--benchmark-only` runs omit all
reference rendering, readback, PNG writes and temporal captures throughout the
loop, reducing GPU clock/pacing disturbances from intermittent inspection work.
They establish timing only; use the image-producing runs for output acceptance.
GPU fence waits are bounded at 30 seconds; uncertain pending
submission state terminates the diagnostic process without destroying resources
potentially still in use.

Work is intentionally serialized; these are isolated pass times, not pipelined
game FPS or latency. Laptop clocks/power are not locked. Shader/DLL hashes,
device and raw driver versions, requirements, initialization time, SDK temporary
heap requirements and owned image allocations are recorded. SDK persistent
memory is not exposed by `xessGetProperties` and is explicitly excluded.

```powershell
cargo fmt --manifest-path src-tauri/tools/xess-vulkan-diagnostics/Cargo.toml
$env:CARGO_INCREMENTAL = '0' # avoids this workspace's hard-link cache warnings
cargo check --locked --manifest-path src-tauri/tools/xess-vulkan-diagnostics/Cargo.toml
cargo check --locked --manifest-path src-tauri/tools/xess-vulkan-diagnostics/Cargo.toml --target x86_64-pc-windows-msvc
```

Public ABI layouts are checked at compile time against the pinned C header
sizes/offsets. GPU validation and actual output readback are the acceptance
checks, rather than tests that merely mirror the FFI definitions.

## Independent in-game Native AA

The separate `xess-native-aa` layer build uses no Streamline or optical flow.
It is intentionally incompatible with the `sdk-bridge` feature. Build with:

```powershell
cargo build --locked --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --target-dir src-tauri/target/xess-native-aa --features xess-native-aa --lib --bin streamline-layer-probe
python src-tauri/tools/xess-vulkan-diagnostics/game_ab.py xess
```

`game_ab.py` is a local Xenoblade 3 harness and uses this machine's verified game
path/configuration. It launches the emulator, which writes its own portable data.
The optional `dlaa` command uses the existing NVIDIA route solely as a reference;
it is not part of the XeSS build. The user cancelled that comparison.

Native AA supplies zero motion/jitter and resets on every frame. This is a
spatial execution experiment, not a complete temporal integration. All real-game
results and limits are recorded in `docs/xess-native-aa-game-validation.md`.
