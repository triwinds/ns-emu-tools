# Independent XeSS FG diagnostics

The tool's independent AA+FG component is now documented in
[`docs/xess-component-integration.md`](../../../docs/xess-component-integration.md).
The commands below retain the separate FG diagnostic build and its evidence.

Opt-in Windows x64 test, independent of Streamline, NVOF, DLSS and the installed
graphics component. Defaults to a supported **non-NVIDIA** adapter. This is a
bounded synthetic execution/interop test. An additional opt-in game bridge below
uses placeholder guides for operation testing, not game quality acceptance.

The pinned XeSS SDK 3.0.2 reports FG **1.3.1** and XeLL **1.3.2**. FG requires
XeLL, including on AMD; no separate standalone XeLL feature is exposed. Non-Intel
GPUs support one generated frame per original frame (2x total).

## Prepare, build, run

From the repository root, using the existing MSVC and Vulkan SDK compiler:

```powershell
& src-tauri/tools/xess-fg-diagnostics/prepare-sdk.ps1
cargo build --locked --release --manifest-path src-tauri/tools/xess-fg-diagnostics/Cargo.toml
$exe = 'src-tauri/tools/xess-fg-diagnostics/target/release/xess-fg-diagnostics.exe'
$sdk = 'D:/py/ns-emu-tools/src-tauri/target/xess-fg-sdk-v3.0.2'
& $exe --sdk $sdk --output D:/py/ns-emu-tools/build/xess-fg-capabilities --capabilities
& $exe --sdk $sdk --output D:/py/ns-emu-tools/build/xess-fg-direct --adapter 1

# Vulkan compute -> same-adapter shared texture -> D3D12 -> XeSS FG.
# Select the adapter index from the capability report, not a hardcoded vendor ID.
$env:VK_LAYER_PATH = (Resolve-Path src-tauri/target/vulkan-sdk-1.4.363.0/Bin).Path
$env:VK_LOADER_LAYERS_DISABLE = '~implicit~'
$env:NS_XESS_FG_VULKAN_VALIDATION = '1'
& $exe --sdk $sdk --output D:/py/ns-emu-tools/build/xess-fg-vulkan --adapter 1 --vulkan
```

`prepare-sdk.ps1` downloads only hash-pinned Intel files and Microsoft's
Agility 1.619.6 NuGet package. Original notices/licenses remain under ignored
`src-tauri/target`. Original Intel DLL signatures are checked. Runtime keeps
verified DLL files open without write/delete sharing; build copies only verified
public headers. Agility runtime lives beside this diagnostic in `D3D12/` and uses
an independent device factory, without changing OS settings or Developer Mode.
Nothing is installed into the main graphics component.

The compiler is `src-tauri/target/vulkan-sdk-1.4.363.0/Bin/glslangValidator.exe`.
Use `prepare-sdk.ps1 -BinaryDirectory <directory>` when building to another target
directory. The selected binary always expects its verified `D3D12/` beside it.

Options: `--adapter N`, `--frames 90..600` (default 180), `--width 320..2560`,
`--height 180..1440`, `--source-fps 24..120` (default 40), `--capabilities`,
`--vulkan`, `--validation`. Output must be a new absolute directory. Test window
closes at completion; closing it early fails the run. GPU fence waits are bounded
at 30 seconds; uncertain completion terminates the child without unsafe teardown.

`--validation` requests D3D12's debug layer and **fails** if it cannot load. This
machine still reports `DXGI_ERROR_SDK_COMPONENT_MISSING`, including with the local
Agility debug DLL and outside the sandbox. No D3D12 validation success is claimed.
The independent Vulkan validation path above works, including synchronization
validation. Only four exact implicit-layer isolation notices are recorded
separately; all other warnings/errors fail the Vulkan run.

## What is verified

- DXGI and Vulkan adapters match by exact, unique LUID.
- D3D12 committed RGBA8 resource is imported as dedicated Vulkan memory.
- A shared D3D12 fence is imported as a Vulkan timeline semaphore. Values
  `2n+1` release Vulkan-produced color to D3D12; `2n+2` return ownership. The first
  frame needs no wait on an external producer; skipping a redundant zero-value
  wait also avoids this machine's AMD submit failure.
- Compute creates a translating checker/line scene entirely on the GPU. Color
  has no CPU staging transfer in Vulkan mode. Analytic pixel motion and constant
  depth are uploaded for the synthetic scene; no external optical flow runs.
- Before the 32-frame warmup ends, frames 0/16 are read back from D3D12 and every
  RGBA pixel is checked against the analytic scene. PNGs contain original Vulkan
  input, **not generated frames**. Readbacks/PNG writes do not enter measurement.
- Matching XeLL markers, resource tags, frame constants and present IDs are sent
  for each source frame. SDK present status must report FG enabled, interpolation
  success, and two queued frames for every measured frame.
- `events.jsonl` is flushed incrementally, even before a later GPU failure. Final
  `report.json` contains DLL/shader/binary hashes, adapter results and all events.

Source FPS uses a host cadence with XeLL enabled and its internal FPS cap disabled.
`sdk_queued_fps` counts SDK-reported queued frames. Neither is an independent
physical scanout measurement or a game performance result. The test serializes
GPU work and does not establish production pipeline latency/throughput.

Native XeSS FG has no public Vulkan API in this SDK. The synthetic test uses
known motion/depth; its results do not establish real-game quality.

## Experimental game bridge (zero motion / fixed depth)

Build a separate layer, without `sdk-bridge`, `native-nr`, or `xess-native-aa`:

```powershell
cargo build --locked --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --target-dir src-tauri/target/xess-fg-game --features xess-fg --lib --bin streamline-layer-probe
python src-tauri/tools/xess-fg-diagnostics/game_probe.py --name game-zero-example --max-edge 640
```

The local harness accepts `--game <XCI/NSP>` and `--emulator <EXE>`; its local
defaults reuse this workspace's earlier Xenoblade 3 path and existing Nextendo
executable. It changes no emulator settings. Select the test
GPU in the emulator first; Vulkan and D3D12 must match by exact LUID. The layer
is process-scoped and remains opt-in with `NS_EMU_XESS_FG=1`. Runtime paths are
`NS_EMU_XESS_FG_SDK` and `NS_EMU_XESS_FG_AGILITY`, both absolute and hash pinned.

The original application HWND receives the XeSS D3D12 proxy output. Vulkan WSI
runs on a private hidden shadow window; the game's images/acquire/present ABI
remain native. The bridge consumes the original present wait semaphores once,
copies the completed image on the GPU, waits for that copy, and presents the
hidden native image with no duplicate waits. This experiment currently requires
one swapchain per present, SDR RGBA8/BGRA8 UNORM, and the game's graphics queue.
Before presentation, a GPU readback verifies every guide pixel is zero motion
and depth 0.5. Upload plane offsets are aligned to 512 bytes independently of
row-pitch alignment, including odd guide heights. Uncertain GPU completion terminates the diagnostic process rather than releasing
possibly in-flight resources. Device teardown joins the control worker before
the loader can unload its code.

Motion is exactly zero, depth is 0.5, matrices are identity, and no jitter is
injected. There is no optical-flow estimator, Streamline runtime, SR, UI mask,
real camera/depth extraction, or production recovery path. XeLL is required by
FG; its markers at the present hook do **not** measure actual game input or
simulation latency. The game bridge serializes GPU work and may reduce source
FPS. `NS_EMU_XESS_FG_MAX_EDGE` defaults to 960 (320..2560): larger present images
are resized before FG for diagnostic cost control and stretched in the original
window. This is ordinary image resizing, not XeSS SR or native-resolution
quality validation.

Within the session directory write `xess-fg-control.json` as `{"enabled":false}`
or `{"enabled":true}` to switch interpolation without restarting. Disabling
interpolation keeps the same D3D12 bridge active; it is not a native-renderer
performance baseline. `NS_EMU_XESS_FG_INITIAL_OFF=1` starts that way.
The launcher's `target-exit.json` field `fg_requested` describes the legacy
Streamline `--fg` option, which stays false here; XeSS activation/results are
recorded independently in `xess_fg_*` events and SDK present feedback.

After normal exit, run `game_metrics.py <absolute-session-directory>`. It reports
source presents, SDK queued frames, bridge CPU wall time, lifecycle failures and
the exit code. It discards 32 frames after each switch/recreation. An optional
`gameplay-observation.json` supplies `conservative_min_elapsed_us` to exclude
menus/loading. SDK queued frames are not an independent scanout measurement.

```powershell
cargo fmt --manifest-path src-tauri/tools/xess-fg-diagnostics/Cargo.toml
$env:CARGO_INCREMENTAL = '0'
cargo check --locked --manifest-path src-tauri/tools/xess-fg-diagnostics/Cargo.toml
cargo check --locked --manifest-path src-tauri/tools/xess-fg-diagnostics/Cargo.toml --target x86_64-pc-windows-msvc
```
