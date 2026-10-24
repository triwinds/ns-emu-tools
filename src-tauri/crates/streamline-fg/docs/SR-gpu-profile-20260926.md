# SR GPU profile — 2026-09-26

The diagnostic build was run on the user's stationary gameplay scene with FG and 1× SR. The last 20 seconds contain 482 SR frames, all with FG active; 483 NVOF reports include the preceding frame because those queries are read on reuse. No SR fallback or last-frame history reset was reported. The 30.161-second telemetry interval averaged 23.90 application FPS / 47.77 presented FPS.

Input and DLAA output: 3840×2160; swapchain: 2560×1335. GPU: RTX 5070 Ti Laptop. Package `local-31ed06f02198-f168291eb7b6`; layer SHA256 `f168291eb7b69834a98ed035c23d363d87e2c5e7b52b621b7876037cf2c058ff`.

| GPU interval | Mean ms | P95 ms |
| --- | ---: | ---: |
| SR initial transitions + raw SRGB-compatible copy | 0.137 | 0.169 |
| SR input color blit / flip | 0.177 | 0.215 |
| Depth clear + motion preparation + transitions | 0.333 | 0.413 |
| DLSS/DLAA evaluation commands | **11.201** | **12.236** |
| Output blit + final transitions | 0.060 | 0.059 |
| SR total | **11.908** | **12.981** |
| NVOF input copy | 0.038 | 0.050 |
| NVOF execution + inter-queue gap | 2.101 | 2.969 |
| NVOF map transfer + dense conversion | 0.078 | 0.089 |
| NVOF total | 2.217 | 3.084 |

Mean can exceed P95 for output blit because of a small slow tail; these are the raw measured statistics, not a transcription error. NVOF execution is bounded by timestamps on the graphics queue around the semaphore dependency, so its interval deliberately includes inter-queue scheduling latency.

| CPU interval | Mean ms | P95 ms |
| --- | ---: | ---: |
| SR command recording | 0.268 | 0.402 |
| SDK evaluate call (included in recording) | 0.228 | 0.342 |
| SR queue submit | 0.014 | 0.027 |
| SR fence wait | **14.329** | **16.111** |
| NVOF preparation + submits, including previous reuse fence | 0.235 | 0.404 |
| FG input wait after presentation | 3.317 | 3.829 |
| Reflex begin/sleep | 0.173 | 0.280 |

CPU fence wait overlaps the GPU intervals and includes upstream dependencies: do not add it to GPU times. About 94.1% of SR GPU time lies inside DLAA evaluation. All other measured SR GPU work totals 0.708 ms, so removing copies cannot eliminate the roughly 11.2 ms DLAA workload. Async synchronization may improve overlap but does not remove that workload. This supersedes the earlier suspicion that redundant copying was the principal bottleneck.

CPU observer timing over the 30.161-second telemetry interval totals 45.65 ms per wall-clock second, approximately 1.91 ms per application frame. Descriptor observation contributes 23.75 ms/s, draw observation 15.16 ms/s, barriers 2.75 ms/s, render passes 0.91 ms/s, other observations 3.08 ms/s. The per-frame value is an estimate from telemetry frame rates. Observer timing includes observation and lock waits, but excludes underlying Vulkan calls, dispatch lookup, and present source selection; this is not whole-process CPU time. Instrumentation itself adds overhead.

## Instrumentation and validation

`NS_STREAMLINE_SR_TIMING=1` enables a six-entry timestamp query pool on graphics family 0, five consecutive GPU intervals, and CPU recording/submit/wait times. Queries are read after the existing SR fence without `WAIT`; missing results are reported as unavailable. Timestamp conversion respects valid-bit wrapping and the device's timestamp period. Queries are destroyed with the SR resources. No additional fence wait, queue idle, or memory barrier was added. Timestamps can still perturb scheduling, so these are instrumented measurements.

`NS_STREAMLINE_NVOF_TIMING=1` enables the preexisting GPU intervals and the added CPU preparation/submit record. `NS_STREAMLINE_SOURCE_MEASURE=1` enables observer counters. Profile records use buffered trace writes. Ordinary launches leave these environment flags unset.

64 tests passed; the hardware-only NVOF test remains ignored. Crate all-target/all-feature host and explicit Windows checks and application host/Windows checks passed without warnings. Both projects were formatted. Release package and tool EXE were rebuilt; all 11 installed artifacts and staged package were hash-verified.

Evidence: [JSON summary](SR-gpu-profile-20260926.json). Original session: `src-tauri/target/sr-gpu-profile-20260926-220856`. The launcher and aggregator scripts are `src-tauri/target/start-sr-gpu-profile.ps1` and `src-tauri/target/summarize-sr-gpu-profile.py`. No rendering optimization or quality reduction was applied in this profiling change.
