# NR internal layout investigation — 2026-09-30

The four baseline errors have a reproducible, narrowly scoped workaround in the **isolated diagnostic**. They have not been filtered. This report preserves the initial layout investigation. The subsequent [coexistence/history report](coexistence-history.md) accepts the pinned experimental P0 combination and verifies serialized worker recording and resize; production integration remains unverified.

## Evidence

`--trace-layouts` enables LunarG API dump above Khronos validation and runs three frames in each cycle: 3 NR-off controls plus 9 NR evaluations, with three feature creations/releases. Both layer manifests and DLLs are hashed in `nr-inputs.json`. The initial trace is `src-tauri/target/nr-p0-layout-trace-001/stdout.txt`; subsequent traces use `api-dump.txt`. Compact results and file hashes are in [layout-investigation.json](layout-investigation.json).

The unmodified trace has eight `vkCreateImage` calls: four host input/output images and four NR-owned images. The failing images are optimal-tiling, 640×360 RGBA16F, one mip/layer, usage `TRANSFER_SRC | TRANSFER_DST | SAMPLED | STORAGE`. Each is successfully created in `UNDEFINED`, allocated and bound. No `UNDEFINED -> GENERAL` image barrier is recorded for these four images. Their first image barriers specify `GENERAL -> GENERAL`:

| Image | Create result line | Bind call line | First image barrier's image line |
| --- | ---: | ---: | ---: |
| `0x890000000089` | 12178 | 12201 | 16326 |
| `0x8c000000008c` | 12656 | 12679 | 16208 |
| `0x950000000095` | 31849 | 31872 | 35832 |
| `0x9e000000009e` | 51179 | 51202 | 55162 |

Line numbers refer to the initial trace. The first image is allocated during feature creation, and the next during evaluation; the latter also supplies NVX view handles before the first kernel launch. Subsequent feature cycles allocate the other two images. The trace has 1,404 `vkCmdCuLaunchKernelNVX` calls, 12 queue submissions and **no `vkCmdDraw` or `vkCmdDispatch` calls**. The reported `VUID-vkCmdDraw-None-09600` therefore must not be interpreted as proof of a graphics draw. The failure is emitted by validation at queue submission while comparing initial command-buffer layouts against tracked image layouts.

This is much stronger evidence of a missing initial transition in the inspected snippet path than the original validation messages alone. It does not expose proprietary NVX kernel internals or establish a general defect across drivers/runtime versions.

## Controlled repair

`--repair-internal-layouts` supplies NR-only procedure lookup wrappers. For a matching image created within the current NR create/evaluate call, the wrapper records a real `UNDEFINED -> GENERAL` barrier immediately after successful `vkBindImageMemory`, on the already-recording command buffer. This occurs before image-view creation and before any kernel can access that new image. The regular loader/layer chain still executes the calls and validates the barrier.

The wrapper checks device, format, dimensions, tiling, usage, sample count, mip/layer count, flags, sharing mode, empty extension chain and initial layout. Candidate handles are tied to one NR call and cleared on leaving it. It does not use hardcoded handles or object names and does not touch host resources. It is default-off, restricted to the two recorded NR DLL digests, and linked only into `streamline-nr-diagnostics`, not the production Vulkan layer.

Do not repair by changing an already-recorded `GENERAL -> GENERAL` barrier after a kernel to `UNDEFINED -> GENERAL`: a transition from `UNDEFINED` may discard content that the kernel has already written. Do not set image creation's `initialLayout` to `GENERAL`, or suppress the validation message. The tested intervention performs the missing transition **before first use**.

## Results and limits

For each of the two inspected runtime digests, the full baseline reports four layout errors. With the experimental transition, 603 NR evaluations plus 3 NR-off controls complete with **zero core/synchronization validation errors**, zero unreviewed warnings, all feature releases and normal child exit. Exactly four transitions are added. Each 300-frame NR cycle has 197 moving pairs, all changing. Per-frame output hashes are compared against the corresponding baseline in the JSON report; this verifies that the intervention did not change the observed synthetic output.

The six remaining warnings are the previously reviewed loader notices about child-only implicit-layer exclusion. NVX kernel internals are not comprehensively covered by synchronization validation. Passing these checks is not evidence of game image quality, motion policy acceptance, multi-thread safety, resize compatibility, other formats or other runtime versions.

For reproduction, append `--trace-layouts` to the existing README command for a short failing baseline; use a new session and append both `--trace-layouts --repair-internal-layouts` for its repaired trace. Omit `--trace-layouts` and keep `--repair-internal-layouts` for the full 603-evaluation run. The ordinary default still reproduces and reports the runtime's four errors. A successful experimental run still records `p0_passed: false`.

## Integration decision

The preferred durable fix is for NR to initialize these owned resources correctly, or to document an initialization contract that produces those barriers. The local workaround demonstrates that native Vulkan is not blocked solely by these four messages. Before moving it into the production layer, the wrapper needs resource-lifetime tracking across threads/command buffers, explicit behavior for unrecognized resource paths and runtime changes, resize/minimize/recreation coverage, and Streamline coexistence tests. The diagnostic's single-thread and single-command-buffer assumptions must not be silently generalized.

Validation performed after the Rust changes: `cargo fmt`; application host and Windows MSVC `cargo check`; `streamline-fg` host and Windows MSVC `cargo check --all-features --all-targets`; eight existing NR diagnostic tests. All checks passed without warnings.
