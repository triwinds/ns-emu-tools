# Look scope, static history and cached recomposition

This record covers the 2026-10-05 M1 implementation, the static/per-tap foundations
of M2 and pure Look recomposition from M3. It does not complete the supplemental
pipeline plan or accept real-game quality, performance or strict validation.
See [summary.json](summary.json) for source, shader, build and log hashes, current
checks, the frozen-debug game session and the separate final Release build.

The latest component regression passed 216 tests, with 5 hardware tests ignored by
default. The separate fixed-input Look GPU test passed on the RTX 5070 Ti Laptop
GPU with zero core/synchronization errors or warnings. Four loader installation
notices are reported separately. Toolbox/component formatting and host/Windows
MSVC checks passed without Rust warnings or errors; preset migration, frontend
build/typecheck, changed-file lint and all three temporal SPIR-V variants passed.

The strict Eden session was a debug build with validation, readback and frame
timing enabled. It preceded the final history-clock and applied-configuration
Look-preparation edits. NR was disabled after the user reported 6 FPS; the
remaining approximate 11–12 FPS observation is not a controlled measurement.
The final raw validation log has 53 errors and 10 warnings. The retained SDK
summary of zero does not match it. Explicit verification rejected the session,
including failed process exit and incomplete presentation retirement. None of
these messages is filtered or exempted to declare acceptance.

After the user stopped computer use with Escape, no more window operations were
performed. The final Release DLL and launcher were frozen in
`src-tauri/target/nr-followup-20261005/release-build`. The prepared launch script
was only dry-run checked; it has not launched or measured the final build:

```powershell
& src-tauri/target/nr-followup-20261005/launch-performance.ps1 -Nr off
# Repeat the same saved-game scene with -Nr one and -Nr two.
```

The script disables validation/readback/frame-trace switches/GPU timestamps,
checks frozen binary hashes and uses a new session directory each time. Some
existing frame events can still be logged; this is not a zero-logging baseline.
These local target artifacts and bulk logs are not committed. Rebuild the final
sources when reproducing on another machine. The implemented configuration and
remaining persistence, suffix recomputation, protection/diagnostics, submission
and inference-size work are described in the
[implementation record](../../../../../docs/dlss5-generic-nr-pipeline-implementation.md).

The subsequent real-game run retained 20 independent samples per stage: off
30.04 FPS, one-pass/static Look 30.08, two-pass/static Look 30.13, off again
30.07. Validation modules were absent; readback/timestamps were disabled and
SR/FG stayed off. The initial battle ended before the one-pass view; later
phases shared an idle camera. The 30 FPS cap prevents estimating spare GPU
budget or proving zero NR cost. Motion-video and strict acceptance remain open.

The final quiet build separately confirmed 12 active two-pass/static Look
snapshots, 304 advancing observations, zero trace-byte growth and zero frame
success/present bookend records. Lifecycle/failure events remain available.
The script now defaults to this quiet build; the prior FPS build is retained.

Full Eden simulation pause stops Present, so Look settings apply on resume;
immediate recomposition without Present remains unimplemented. The known
0xC0000409 emulator exit issue is excluded at the user's explicit request,
with raw exit records retained. The user again stopped computer use with
Escape; no further UI operations were performed.
