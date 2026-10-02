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

Toolbox checks query the runtimes repository's published `streamline-layer-*` Releases, including experimental prereleases, and read the newest matching `<tag>-manifest.json`. Runtime-only and AIO Releases are excluded. The manifest declares `schema_version: 1` and `launcher_protocol: 1`; incompatible protocols or changed stable binary contracts require a toolbox update. Upload the two ZIPs before making the layer Release public. Future compatible small-package updates need only a new layer Release and its combined manifest, without rebuilding the toolbox. The check reports the installed and available versions without altering any installation. Installation verifies both archives and every file, reuses stable archives by SHA-256, then atomically selects a separate immutable installation for that emulator. The selected manifest persists across toolbox restarts. Network failures leave existing installations and running game snapshots usable.

The `native-nr` build uses bounded deferred NR → DLAA completion by default when frame generation is off, native inputs are available, no FG timeline work remains, and the frame does not request readback. Borrowed game inputs finish copying before presentation returns; private processing and native presentation retire before reuse or teardown. Set `NS_STREAMLINE_DEFER_PRESENT=0` to restore the synchronous tail in diagnostics. The toolbox exposes NR as an experimental, default-off option after automatic runtime installation. Strict Vulkan validation still fails for the game; see the [measurements](evidence/nr-tail-2026-10-01/README.md) and [validation review](evidence/nr-tail-2026-10-01/validation-review.md).

## Toolbox use

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

## Experimental present-source SR

Use the toolbox SR switch or `--sr-mode quality` alongside `--fg`. Default off. This version downsamples the final emulator image and reconstructs it to the same window size; it does not lower emulator rendering resolution. See [scope, tests and known exit failures](docs/SR-present-experiment.md).
