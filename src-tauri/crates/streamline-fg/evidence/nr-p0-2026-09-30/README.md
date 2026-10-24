# Native Vulkan NR P0 — 2026-09-30

Status: the exact experimental P0 combination is now **accepted**, with explicit diagnostic-only NR/SR repairs. See [coexistence, resize and history acceptance](coexistence-history.md) and its [machine-readable evidence](coexistence-history.json). No NR installation, simulator injection, configuration switch or UI has been enabled. Work starts from commit `5916ea3e043eb91de1dda4c30736c8b02be81df9`; the only pre-existing untracked file was the integration plan. All implementation changes are under `src-tauri`.

The original [API trace and controlled initial-layout repair](layout-investigation.md) identifies missing initial transitions on the four NR-owned images. A default-off diagnostic workaround completes 603 evaluations with zero validation errors for both inspected DLL digests. The ordinary path still reproduces the four errors. The sections below preserve the initial investigation; the newer acceptance report supersedes their former coexistence/history blockers. Original [summary.json](summary.json) and [layout comparisons](layout-investigation.json) retain the historical results unchanged.

## Initial observed execution

Windows x64, RTX 5070 Ti Laptop GPU, NVIDIA driver 610.88, 640×360 SDR. Color is gamma-encoded RGBA16F, depth is synthetic constant R32F, motion is synthetic current-to-previous UV displacement in RG16F. These are owned test resources, not game color/depth/motion.

- The pinned static NGX core initializes successfully. Direct calls to NR `Init_Ext2` return `0xBAD00002`; that code alone does not establish the cause.
- Loading the minimal Rust `nvngx.dll` **after core initialization** allows NR snippet initialization, parameter population, feature creation/evaluation/release and explicit shutdown. The process image is not renamed and NVIDIA DLL bytes are not modified. This module is only a caller boundary; it does not implement NGX core exports and must not be installed as a global core replacement.
- Final release disassembly has a real `call r10` at RVA `0x10d0` for initialization/evaluation, with continuation at `0x10d3` and a result store at `0x10e2`. Thus the observed snippet return address stays inside the bridge. Other operations also use calls followed by this continuation. An ABI/version check precedes bridge use. Audit every rebuilt bridge; these RVAs apply only to the recorded binary digest.
- The final trial completes 3 NR-off control frames, 300 zero-intensity evaluations, 300 nonzero-intensity evaluations and 3 evaluations after another feature recreation. Every readback is finite and replaces the sentinel. Zero intensity exactly matches its synthetic input. The first nonzero frame has mean absolute difference `0.0076223082` from the matching zero-intensity frame; the next two differ by `0.0098923836` and `0.0087141346`. Each 300-frame cycle has 197 comparable moving pairs, all changing. The warmup frame has no valid motion; its first valid pair resets history. Static intervals and zero-motion resets are exercised, but their image quality and suitability for game history are **not verified**.
- Three features are released after fence completion; feature/capability parameter maps, snippet and core are then explicitly shut down. Error paths terminate the isolated child rather than assuming an uncertain GPU state is safe to reuse.

## Baseline validation failure

VVL 1.3.231 reports nine `VkCuModuleCreateInfoNVX.pNext` errors and the child terminates with `0xC0000005` on its first evaluation. An exploratory run without validation completes the output controls but cannot pass P0.

Official VVL 1.4.363.0, acquired in `copy_only=1` mode under workspace `target`, completes all 603 evaluations with core and synchronization validation enabled. It reports **four errors**, all `VUID-vkCmdDraw-None-09600`: NR-owned `nv.ngx.dlssnr.resource` images are expected in `GENERAL` while the validation state is `UNDEFINED`. Recorded handles `0x890000000089`, `0x8c000000008c`, `0x950000000095` and `0x9e000000009e` differ from the four host-owned input/output handles. Errors occur on the first submit after each feature creation. Both inspected NR DLL digests reproduce the same errors. Recording feature creation and its first evaluation in one command buffer, matching the inspected Vulkan reference, does not remove them.

This narrows the issue to NR internal resources, but does not prove a runtime defect, a validation false positive or the absence of an additional caller requirement. Errors are preserved and are **not** filtered or accepted. The six loader warnings are individually recorded and reviewed: the diagnostic deliberately disables implicit Optimus, NVIDIA present, AMD switchable-graphics and ReShade layers in its child. Unknown warnings and synchronization/lifetime warnings still fail the diagnostic.

The command returns failure when validation errors or unreviewed warnings remain, even after output readback and clean shutdown. `nr-result.json` separates execution from validation; automated `p0_passed` remains false because output review and the bridge audit are external acceptance steps. The newer curated report records acceptance for its pinned experimental combination. Streamline SR coexistence, shared core ownership and the missing-motion pause policy have now been tested; game integration and FG coexistence remain unverified. These results support continuing the native route.

## Reproduce

Run from the repository root in PowerShell. Use the hash-pinned SDK files already described by `sdk/nr-contract.json`. `NGX_SDK_DIR` may point to another copy of that exact NGX SDK; the build verifies the headers and static library. Existing production/Streamline builds do not require the NR SDK feature.

```powershell
cargo build --release --manifest-path src-tauri/crates/nr-call-bridge/Cargo.toml
cargo build --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features nr-diagnostics --bin streamline-nr-diagnostics

$nrBridge = (Resolve-Path src-tauri/crates/nr-call-bridge/target/release/nvngx.dll).Path
$nrDigest = (Get-FileHash -LiteralPath $nrBridge).Hash.ToLower()
& src-tauri/crates/streamline-fg/target/release/streamline-nr-diagnostics.exe `
  --runtime 'D:/py/ns-emu-tools/src-tauri/target/release/graphics-components/cache/aio-v1/runtimes/nvngx_dlssnr.dll' `
  --sha256 e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e `
  --bridge $nrBridge --bridge-sha256 $nrDigest `
  --validation-dir 'D:/py/ns-emu-tools/src-tauri/target/vulkan-sdk-1.4.363.0/Bin' `
  --session 'D:/py/ns-emu-tools/src-tauri/target/nr-p0-new-session'
```

The session directory must be new. `--init-only` probes initialization/cleanup without GPU evaluations. Omitting both bridge arguments tests the direct caller. `--without-validation` explicitly requests an exploratory run that cannot satisfy validation gates. Dimensions and frame counts are bounded; each fence has a 10-second deadline, and the parent bounds the child to 240 seconds, extended to 600 seconds for resize/coexistence. SHA mismatch, wrong PE architecture and bridge ABI mismatch fail explicitly. DLLs are copied only into the new diagnostic session. Layer environment changes apply only to the child; global registrations and emulator installations are untouched. The follow-up report gives the full coexistence/resize command.

## Evidence and checks

[summary.json](summary.json) retains the baseline, nine trial outcomes, the final runtime/driver/module hashes, the exact validation findings and warning reviews, output measurements, bridge disassembly audit and checks. Full logs, binary copies and PPM/PNG readbacks are in `src-tauri/target/nr-p0-*`; their exact paths and final file hashes are in the summary. These bulk files are not redistributed or committed. The current failure reproducer is `src-tauri/target/nr-p0-validation363-final-002`.

`cargo fmt` completed for the application and both independent crates. Application host and Windows MSVC checks passed. The `streamline-fg` crate passed host and Windows MSVC checks with all features and all targets; the bridge passed both target checks. All checks are warning-free. Tests: 78 passed in `streamline-fg`, 2 in the bridge; the pre-existing hardware NVOF test is ignored. Compilation and tests do not substitute for the failed GPU validation gate.

The FFI is independently authored from the pinned public NGX layouts and the inspected [Vulkan NR reference](https://github.com/AlrikOlson/bevy_dlss5/blob/ed5ea6264338b2e19d81dcecbed1e06ab22a01ed/src/ngx.rs). The caller-boundary design was compared with [AIO's D3D12 bridge](https://github.com/kibblerz/DLSS5-Reshade-AIO/blob/09301f5528e619e8b9ec17c257d167e2985f53b0/addon/src/nvngx-bridge.cpp); its Vulkan behavior was tested separately here. No upstream source or NVIDIA runtime is vendored. The portable SDK copy mode follows [LunarG's installation documentation](https://vulkan.lunarg.com/doc/view/1.4.363.0/windows/getting_started.html).
