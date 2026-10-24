# Experimental consolidated NR and inference resizing

The user requested ending repeated investigation of emulator errors already
observed with enhancement disabled. Raw historical failures remain; they no
longer block implementation. Eden 0xC0000409 remains outside the requested scope.

M5 adds default-off `--nr-consolidated`: independent command recordings share
one ordered queue submission, with completed-prefix fallback after discarding
a failed second recording. Borrowed-input retirement and final completion remain.
No cross-frame resource ring has been added.

M6 adds default-100 `--nr-inference-scale 50..100`: full P0 and source comparison
remain at original resolution, all model passes/Look share one reduced extent,
and the controlled total linear SDR correction is reconstructed onto P0.
Original alpha and zero-delta details remain exact. Normalized UV motion retains
the original viewport basis; depth remains synthetic. HDR and legacy NR readback
are rejected for reduced inference. Experimental bounds do not claim a model
minimum. Both flags require matching DLL capability markers.

Strict isolated 640x360 NGX execution verified 299 normal output/prefix hashes
and suffix recomputations identical between split and consolidated schedules.
Each 300-frame experiment discards one valid second recording and recreates its
feature. Consolidation reduces inference submissions 599→300 and intermediate
waits 300→1 (failure recovery). Warm synthetic CPU record/submit/wait median is
7.731→7.315 ms, with validation enabled; this is not a game-FPS claim.

320x180 dual-feature execution also completed moving-output, discard/recreate
and suffix checks. All four raw logs have zero validation errors; four loader
disable warnings are retained separately. All four processes timed out in core
shutdown after successful feature/parameter release and snippet shutdown.
Output execution passed; normal core retirement did not.

The shipped reconstruction SPIR-V passed Vulkan 1.1 validation and a strict
offscreen GPU test (19x3→37x5) of fine detail, exact zero delta/alpha, dark/white,
subnormal input, positive/negative correction, clipping and repeated use.
Zero core/sync warnings/errors; four loader notices. It does not test the NR
model at those small shader dimensions.

Rust formatting, component/toolbox host and Windows checks passed without
warnings/errors. Full regression: 226 passed, six hardware tests ignored by
default; reconstruction hardware test was run separately. Exact hashes and
unfiltered artifact references are in [summary.json](summary.json).

New normal Release game session uses two passes, consolidated submission and
75% inference. The user entered XC3 Everblight Plain (objective 51m). All 23
fixed-camera samples were active foreground with the expected 1724x967 original,
1293x725 inference, one inference submission and zero intermediate waits. Mean
application-present FPS was 30.101 (29.821–30.866); the game cap prevents an
overhead or speedup conclusion. A one-pass then two-pass live transition passed
at revisions 1/2, with actual feature counts 1/2 and foreground state throughout.
NR remained enabled; the final two-pass configuration was restored.

The user completed slow/fast camera turns and rock-edge occlusion, reporting
"未看到明显异常" for this 75% consolidated path. Manual observation passed;
no video was captured. The 45-second trace has 46 distinct samples, 32 active
foreground samples averaging 30.072 FPS; action timing was not synchronized.

Resize smoke passed at original→inference extents 1399x967→1049x725,
2560x1325→1920x993, then 1920x1080→1440x810. Each rebuilt two features and
reported eight private NR textures; two reconstruction textures allocated
22,937,600 / 57,671,680 / 35,389,440 bytes respectively. The initial 1724x967
window was not restored exactly; the final window used Eden's 1080p preset.
These counts do not include all Look/history/NVOF/SDK allocations or prove
long-run leak freedom. A rejected drag did not change the window; maximize
provided the expanded-size check.

Live NR+SR, NR+FG and NR+SR+FG smoke checks passed, with respectively 18, 17
and 18 valid foreground samples. FG reported observed generation and two SDK
presents. Mean application FPS was 30.063 / 30.075 / 28.680 at 1080p; these
are availability checks with validation disabled, not strict integration or
controlled inference-scale performance comparisons. SR/FG were restored off.

Normal UI exit balanced first-pass features/parameters 4 created / 4 released,
second-pass 5 / 5; all NR releases and snippet shutdown succeeded. Streamline
shutdown returned success and Vulkan devices/instances reached zero without
forced termination. The final Eden exit was the user-excluded 0xC0000409.
This game resource retirement passed; it does not waive isolated NGX core
shutdown timeouts. Video comparison, strict integration, stage timings and
memory/long-frame performance remain independent acceptance work.
