# Native NR: coexistence, resize and history — 2026-09-30

The fixed experimental P0 combination is accepted: runtime A, the previously audited Rust caller bridge, RTX 5070 Ti Laptop, NVIDIA 610.88, VVL 1.4.363.0 and the explicit diagnostic NR/SR repairs. Continuous output, releases, live Streamline SR coexistence, shared-core shutdown and the missing-motion policy pass. Runtime B also passes the serialized NR-only worker/resize/history subset. This permits native backend work; it does not enable NR in the emulator or establish game image quality.

The [JSON report](coexistence-history.json) retains nine trials, exact input/runtime/EXE/layer hashes, GPU identity, validation configuration, warning reviews, output comparisons, shutdown events and raw-log hashes. The initial failing [P0 summary](summary.json) and [layout report](layout-investigation.json) remain historical evidence. Automated diagnostic files keep `p0_passed: false`; this curated report applies the output review and the audit of the unchanged bridge digest.

## Actual GPU coverage

All full runs use four batches: 640×360 → 960×540 → 1280×720 → 640×360. Each batch has an NR-off control, 300 zero-intensity frames, 300 nonzero-intensity frames, and feature recreation. A completed fence precedes every release/rebuild. Color is gamma RGBA16F, depth is constant R32F and motion is synthetic current-to-previous RG16F UV displacement.

| Session | NR evaluations | Missing-motion copies | NR-off copies | SR evaluations | Validation errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| `nr-p0-coexist-full-001` | 2412 | 0 | 12 | 36 | 0 |
| `nr-p0-coexist-history-001` | 1596 | 816 | 12 | 36 | 0 |
| `nr-p0-worker-history-alt-001` | 1596 | 816 | 12 | 0 | 0 |

The first run keeps the previous zero-motion reset experiment and proves the 300-evaluation continuous controls at each size. The accepted first-stage backend policy uses the second run: missing motion **pauses NR**, copies the original color exactly and preserves a reset for the first evaluation with valid motion. It does not interpret missing motion as a stationary scene or reuse old motion. Independent inspection of the JSONL checks all 816 copies, all valid-motion recovery resets and all finite/fenced readbacks. There are 20 NR activation/recovery resets across four batches.

Each full run creates and releases 12 features. Each 300-frame intensity cycle has 197 comparable moving pairs, all changing. Zero intensity and NR-off output controls match their inputs; nonzero controls respond to NR. The two history runs produce **2424/2424 identical output hashes and identical reset decisions**, despite different NR DLL digests and recording threads. The worker test initializes on the parent, uses one scoped worker at a time and joins before any subsequent NR access or shutdown. It demonstrates serialized thread migration, not concurrent NGX access.

SR evaluates six checker frames per phase: before NR evaluation, during a live nonzero NR feature at each of the four sizes, and after NR snippet shutdown. Every SR phase preserves the same six hashes as the unrepaired SR-alone control, including the changed checker pattern. SR uses its own input/parameter resources. This proves session and resource coexistence; **SR has not yet consumed NR output**. Actual FG is off.

Latest GPU verification uses `nr-p0-coexist-final-smoke-001` (short traced resize/history/coexistence) and `nr-p0-sr-control-final-001` (SR alone with normal cleanup). Both exit normally with zero validation errors and zero unreviewed warnings. Full runs preceded the final log-reporting/cleanup hardening and stricter recording-guard checks. The subsequent `safe_fallback` policy API is covered by state-transition tests and is not invoked by the GPU diagnostic; GPU failure injection remains unverified. Each session's EXE hash is retained separately from the final build hash. The bridge binary was unchanged.

## Shared NGX ownership

The initial coexistence trial ran NR and SR but failed on a second core shutdown after Streamline had already closed NGX. The corrected order is:

1. Initialize Streamline and merge its required extensions/features before device creation.
2. Attach Streamline to the same device; initialize NGX core, then the caller bridge and NR snippet.
3. Allocate separate NR parameter maps and exercise both consumers.
4. Complete NR fences, release features, destroy NR feature maps, shut down the NR snippet, then destroy its capability map.
5. Evaluate SR again after NR shutdown to verify that SR remains usable.
6. Let `slShutdown` close the shared NGX core, then destroy Vulkan objects and unload the retained libraries.

NR-only execution still explicitly closes the core. The child aborts on unknown submitted-GPU state instead of unwinding through possibly live DLL/device resources. Fence waits are bounded to 10 seconds; the parent allows 600 seconds for the combined test.

## Independent SR failures and controlled repair

`nr-p0-sr-control-001` invokes no NR initialization or evaluation and still produces eight SR errors: four motion-image initial-layout mismatches and four exposure-clear `WRITE_AFTER_WRITE` hazards. Therefore these errors cannot be attributed to NR coexistence. Enabling `privateData` also resolves the device-feature error seen in the earliest combined trial; the existing requirement query did not list that core feature.

`--repair-sr-resources` is separate from the NR repair, default off and limited by the staged SR manifest hashes. It uses the same explicit procedure-lookup wrapper to transition matching fresh SR-owned RG16F motion images after binding and before first use. For tracked fresh 1×1 R16F exposure images, it adds an `ALL_COMMANDS` memory dependency to `TRANSFER_WRITE` before the real clear. The exposure layout and contents are not discarded. The SR-alone repaired control has zero errors, and the 36-frame combined runs also have zero errors.

`nr_layout.rs` now takes an independent dispatch/event interface, scopes candidates to a device and recording command, removes destroyed/stale candidates, and obtains dimensions from the current batch. Its guard cannot move across threads because it clears thread-local state on drop. It still supports only one diagnostic device per child; it is not linked into the production layer. Both NR and SR repairs preserve the ordinary driver/validation chain and never suppress messages. Unrecognized warnings still fail the run. The six remaining warnings are individually reviewed child-only implicit-layer exclusions, with the exact messages retained in JSON.

## Backend history foundation

`nr_history.rs` is a portable Rust state machine shared by the library and the GPU diagnostic. Its frame identity includes image, dimensions and a caller-provided mapping identity covering crop, flip, encoding and provenance. Enable/intensity/source changes, recreation, gaps and activity changes set NR/SR/FG resets. Missing motion pauses NR and retains its reset. SR and FG clear pending resets independently only when each actually consumes output, so a paused downstream consumer cannot lose its reset.

Tests cover invalid controls without history mutation, source/intensity/gap changes, recreation, disable/re-enable, missing-motion recovery and independent consumer acknowledgements. A confirmed safe failure before submission also signals downstream resets on the current fallback frame and the later NR recovery; paused consumers retain their signal. Submitted work with unknown state is not a safe fallback. Missing-motion policy is exercised on the GPU, but its SR/FG reset decisions are not yet attached to game consumers. The checker SR phases do not acknowledge NR-derived output.

## Reproduce

From the repository root, build `nr-coexistence`; `nr-diagnostics` alone remains available without the Streamline bridge. Use the existing fixed SDK locations described in the original README.

```powershell
cargo build --release --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features nr-coexistence --bin streamline-nr-diagnostics

& src-tauri/crates/streamline-fg/target/release/streamline-nr-diagnostics.exe `
  --runtime 'D:/py/ns-emu-tools/src-tauri/target/release/graphics-components/cache/aio-v1/runtimes/nvngx_dlssnr.dll' `
  --sha256 e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e `
  --bridge 'D:/py/ns-emu-tools/src-tauri/crates/nr-call-bridge/target/release/nvngx.dll' `
  --bridge-sha256 b3612653579d1af8a077921b425f00ed8e7bb574b1a0a80f58ea956d03dc2dc9 `
  --validation-dir 'D:/py/ns-emu-tools/src-tauri/target/vulkan-sdk-1.4.363.0/Bin' `
  --session 'D:/py/ns-emu-tools/src-tauri/target/nr-p0-new-coexistence' `
  --coexist-sr 'D:/py/ns-emu-tools/src-tauri/target/release/streamline-fg-package' `
  --repair-internal-layouts --repair-sr-resources --resize --pause-without-motion
```

The source runtime directory must contain the exact nine manifest-pinned files; they are copied and hash-checked into the new session. `--sr-control` excludes resize and tests SR without initializing NR. For the alternative DLL worker test, omit the coexistence/SR-repair flags, use runtime B's path and SHA256, and add `--record-on-worker`. `--trace-layouts` makes a short reproducer and cannot substitute for a full acceptance run. `--help` documents every mode and its constraints.

## Remaining game integration

Production work still needs per-device dispatch/lifetime ownership, real gamma/crop/flip preparation, independently verified NVOF coordinates, NR output feeding SR, FG tagging, independent NR launch/settings/state and all eight active combinations. A 10-minute emulator run, window lifecycle tests, game motion/history image review and separate performance sampling remain required. No ReShade, Feeder or RenoDX addon is used by this diagnostic.

Validation after Rust edits: application and crate `cargo fmt`; application host/Windows checks; crate all-features/all-targets host/Windows checks; NR-only Windows diagnostic check; final release build; 88 passing tests and one pre-existing ignored hardware NVOF test. All compiler checks are warning-free.
