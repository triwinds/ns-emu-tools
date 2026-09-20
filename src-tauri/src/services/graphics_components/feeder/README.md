# Experimental Feeder / RHI integration

The backend downloads a fixed experimental Windows x64 bundle. It does not infer runtime compatibility from a successful installation.

- Feeder: `v1.16.0-beta.6`, from `jlrouzies-fr/DLSS5-Feeder`.
- SR: `310.9.1`, NR: `310.8.0`, RenoDX DLSS5: `4.70`, from `RankFTW/rhi-repo` Releases.
- The three RHI versions follow the Feeder author's `tools/Install-DLSS5Feeder.ps1` source list checked on 2026-09-20. RHI is a community mirror, not an NVIDIA publishing endpoint. The NR SF variants and newer RenoDX releases are deliberately not substituted automatically.
- LumeniteFX commit: `f8cbbb4eccfcb7adf0d74bb358ba349272e3c1e9`.
- ReShade headers commit: `6db142b4b1a05c764222e5b0bd9a644b7ccfe1dc`.

`assets.json` freezes URLs and SHA-256 values. Release ZIP digests were checked against GitHub release asset metadata; shader digests were computed from the pinned upstream commits. Downloading uses the application's existing download manager, including cancellation. Packages are validated before any target writes; no upstream installer script is executed.

## Commands

1. Install an official Addon ReShade >= 6.8.0 through the existing graphics component commands.
2. `prepare_feeder_install(executable, graphicsApi)` downloads, validates and returns a preview with sources, blockers and a one-use plan ID. Preparation never deploys into the emulator directory.
3. `install_feeder(planId)` rechecks the executable, ReShade, cache and target snapshot before deployment.
4. `uninstall_feeder(executable)` removes only owned components and keeps ordinary ReShade. Modified INI/preset files are preserved and reported; modified binaries block removal. Runtime-generated logs/configuration are not claimed or deleted.
5. `repair_feeder(executable)` rolls an interrupted transaction back to its previous state.

The emulator must be stopped for writes. Unknown DLLs and competing consumers are blocked. ReShade's existing preset is not overwritten; its configuration switches to `ns-emu-tools-feeder/FeederPreset.ini`, with Lumenite Kernel before Feeder and provider 3. Initial files are backed up by digest; updates keep the initial backup. The `.ns-emu-tools-feeder` directory retains backups after removal for diagnosis.

GUI controls are available under Experimental Features > Graphics Enhancement, with emulator selection and separate ReShade / Feeder installation and removal. The developer example `cargo run --example feeder_probe -- install <absolute-exe>` exercises the Vulkan path and explicitly enables the current-user shared ReShade layer if needed. `remove <absolute-exe>` removes Feeder only, retaining ReShade. Do not run it against a live emulator.

## Verification

`cargo test --lib graphics_components` runs offline safety tests. `cargo test --lib pinned_public_packages -- --ignored --nocapture` explicitly downloads the real bundle and exercises an installation/uninstallation in a temporary directory, without real Vulkan registry changes.

Before reporting compatibility, verify shader compilation, useful depth, nonzero motion vectors while moving, NGX evaluation and neural consumer output on the target game/GPU/driver. Vulkan requires Smooth Motion to be off; this backend does not change driver settings. A Vulkan interop extension failure may require the upstream fallback layer, which is not registered automatically here.

## Ryujinx smoke test, 2026-09-20

Ryujinx 1.3.351, Vulkan, RTX 5070 Ti Laptop GPU / driver 610.88, Xenoblade Chronicles Definitive Edition 1.1.2:

- Feeder and RenoDX loaded; Kernel and Feed shaders resolved with provider 3 enabled.
- RenoDX reported `signed DLSSNR 310.8.0 D3D12 runtime initialized`, feature 18 creation, and repeated successful inline NR evaluations. Feeder delivered over 1,200 frames; colour-input and output probes changed.
- Depth probes at frames 600 and 1200 were flat zero. Useful depth is NOT verified.
- The runtime rebuilt at 2560x1335 after initially running at 1280x765. Its log recorded `CreateFeature` access violations with `renodx-dlss5.addon64` at the top of the module stack, then a `resource deadlock would occur` exception. Ryujinx exited. This combination is NOT accepted as compatible.
- The test components were removed through `uninstall_feeder`; ordinary ReShade was retained. The runtime-modified `ReShade.ini`, generated Feeder config/log and crash dump were preserved. Evidence copies are in `src-tauri/target/feeder-live-test/` (untracked build output).
