# 1× SR live profile — 2026-09-26

This is a first-stage wall-clock profile of the existing game process, not a GPU-kernel profile. No rendering code or release binaries were changed.

## Environment and method

- Session: `game-H1t2KX`; Ryujinx PID 33044, verified executable 1.3.351+475615f.
- Layer SHA256: `4ef3b68e780563400b12aeb0cfdeef0b5693555e25c597b85ddc3d5d55334812`.
- GPU: NVIDIA GeForce RTX 5070 Ti Laptop GPU.
- SR: DLAA, scale 100, native input and processing output 3840×2160; swapchain 2560×1335. NVOF motion enabled.
- Live controls changed in the same process. Each phase allowed 5 seconds to settle, followed by 16 seconds of collection; the first partially overlapping telemetry sample was excluded.
- The user was asked to keep the scene stationary. Scene identity and foreground state were not independently captured throughout every telemetry interval.
- Finally restored requested FG on, SR off, scale 100 with fresh control revisions. No game restart or forced termination.

## Results

| Requested phase | Mean app FPS | Samples | Qualification |
| --- | ---: | ---: | --- |
| FG only, initial | 29.97 | 15 | End status FG active; same-state log interval includes earlier frames |
| Both off | 29.97 | 14 | FG disabled, SR inactive |
| SR only | 22.89 | 14 | SR active, native source, no fallback; FG disabled |
| FG + SR | 24.74 | 14 | INVALID as combined-FG test: end status background, FG paused |
| FG only, return | 29.99 | 14 | INVALID as FG test: end status background, FG paused |
| FG + SR, retry | 21.71 | 15 | INVALID as combined-FG test: background, FG paused |
| FG only, retry return | 30.03 | 15 | INVALID as FG test: background, FG paused |

FG validity was checked against actual frame events, not just the requested toggle. The invalid rows must not support claims about combined FG performance.

## Measured presentation-thread intervals

These are elapsed CPU wall times derived from `vkQueuePresentKHR`, `target_sr_frame`, and `target_fg_frame` events. They include GPU waits and pending application work; they are not GPU execution durations. Contiguous blocks are grouped by actual FG/SR activity and discard their first 5 seconds.

| Actual state / log interval (microseconds) | Frames | Mean present entry → frame event | P95 |
| --- | ---: | ---: | ---: |
| Both off, 606631874–627586628 | 479 | 0.681 ms | 1.104 ms |
| SR only, 627666733–648666182 | 364 | 14.598 ms | 15.975 ms |
| SR active / FG paused, retry 735802307–756772693 | 345 | 14.340 ms | 15.220 ms |

For the first SR-only block, present entry → SR completion was 13.987 ms (P95 15.261 ms); the subsequent FG input wait was only 0.007 ms because FG was off. The approximately 13.9 ms increase over the off block is a presentation-path wall-time difference, not a measured pure DLAA cost or a guaranteed optimization gain.

Before this controlled experiment, an active FG+SR interval showed about 14.018 ms to SR completion and another 3.153 ms of FG input waiting. It is supporting historical evidence from this process, not a valid replacement for the failed stationary combined-FG comparison.

## Interpretation and limitations

SR can reproduce the slowdown with FG disabled. The first target for deeper profiling is therefore the NVOF → SR input preparation → DLAA → output copy → completion-fence chain, rather than attributing the entire slowdown to FG. The 4K SR workload is verified. The runtime blocks in `Sr::run` on its completion fence before presenting.

The 30 FPS cap hides available headroom; equal off/FG-only FPS does not establish zero FG or tracking overhead. The live process did not enable `NS_STREAMLINE_SOURCE_MEASURE` or `NS_STREAMLINE_NVOF_TIMING`, so this run cannot isolate CPU observer cost or individual GPU passes. No changes were made to synchronization or image quality based solely on these aggregate results.

A complete GPU profile still requires per-pass SR timestamp instrumentation and a restarted diagnostic process, with source-observer timing and existing NVOF GPU timing enabled. It should report input copy/flip, depth/motion preparation, DLAA evaluation, output copy, CPU command recording, and CPU fence wait separately. Foreground state must be verified per sample for the combined-FG comparison.

## Raw evidence

- `src-tauri/target/sr-profile-live-20260926.json`
- `src-tauri/target/sr-profile-retest-20260926.json`
- `src-tauri/target/sr-profile-timings-20260926.json`
- Original session `run/layer.jsonl` and `run/telemetry.json`.

Raw files live in the ignored build directory. The timing extraction script is `src-tauri/target/sr-profile-log.py`; the temporary control scripts are `sr-profile-live.ps1` and `sr-profile-retest.ps1` in that directory.
