# NR suffix, persistence, color and proxy-layout evidence — 2026-10-05

This continues the [scope/static/cache record](../nr-scope-static-cache-2026-10-05/README.md).
The [machine-readable summary](summary.json) pins binaries, shader hashes and raw
local artifacts. The plan is **not complete**, and game integration/video quality
are **not accepted**.

Implemented: exact-repeat model suffix recomputation, explicit optical-flow-plus
conditional amplitude/presence, optional Oklab chroma decomposition, four color
protections, ten diagnostic views, schema 4 presets and independent DLL capability
gates. Source IDs and temporal observations do not advance during same-color
recomputation. Model/Look output caches preserve pristine prefixes and final raw
outputs; diagnostics never become inference inputs.

The shared fixed-input Look GPU test passed on the RTX 5070 Ti Laptop GPU with
both RG16F and RG32F motion binaries: zero core/synchronization errors or warnings,
four separately recorded loader notices. It covers neutral/bypass/zero/alpha,
color protection extremes and hue reversal, all diagnostic views, both scopes,
mode changes, absence/reappearance/opposite corrections, input cuts, invalid
motion, and cached recomposition without temporal advancement.

`src-tauri/target/nr-suffix-ngx-003` completed 300 synthetic moving frames with
two real NGX Features, one discarded recording and one recreated second Feature.
Fixed-input second-pass tuning 75 → 100 → 75 added three second Evaluates, zero
first Evaluates and zero source observations. R1 stayed bit-exact; returning to
75 reproduced the earlier output hash. Core/synchronization errors and unreviewed
warnings were zero. Six loader installation notices remain in the raw log.
Attempt 001 timed out during core shutdown; failed evidence is retained, while
002/003 completed. These results do not verify gameplay, SR/FG combinations or
global P0 acceptance.

The pre-proxy-fix Release game session used XC3 v2.1.0, Everblight Plain after
the first battle, objective approximately 61 m, 1718×950 present source. Strict
core/synchronization validation was enabled; readback, timestamps and per-frame
success logs were off. Look, SR and FG were disabled in all four stages.

| Stage | Distinct 1 s samples | Mean app FPS |
| --- | ---: | ---: |
| NR off before | 20 | 13.20 |
| One NR pass | 21 | 11.25 |
| Two NR passes | 20 | 10.32 |
| NR off after | 20 | 13.30 |

**All of these samples were marked `background`; this table is an invalidated
performance attempt, not a foreground comparison.** The sampler now rejects
background telemetry. They must not be compared to normal gameplay, the earlier
no-validation 30 FPS session, or used as the result of the later proxy fix.

NR-off strict validation already reported 54 errors and 10 warnings, including
10 SDK fake-swapchain TRANSFER_SRC/PRESENT layout mismatches. VVL suppresses
repeated IDs after ten messages, so counts are not numbers of faulty frames.
The wrong virtual image layout came from two required hooks being disabled with
optional source probes: proxy image registration and application barriers.
The repair keeps ownership tracking and layout translation independent of probing,
supports legacy and synchronization2 barriers, and preserves native images.
Submit2 queue serialization likewise remains enabled without optional probing.

The separate post-layout-fix session now records three SDK-created ordinary
proxy images per live chain and no fake-swapchain layout mismatch in its current
running snapshot. Shader stencil-extension/type, query-scope and depth-access
errors remain in the unfiltered log. NR-off presence alone does not prove every
remaining message belongs to the emulator; application/SDK attribution and a
clean completed strict run remain required. The latest Submit2 routing follow-up
is distinguished from the already loaded post-layout-fix DLL in the summary.

After restoring foreground and rejecting background samples, the same running
post-layout-fix session at 1724×962 measured 11.66 / 10.02 / 9.60 / 12.10 FPS for
off / one pass / two passes / off (20 / 20 / 21 / 21 one-second samples). All
accepted samples reported foreground, Look/SR/FG off, and the expected actual NR
pass count. These remain coarse strict-validation throughput, not stage timing
or a comparison against the earlier camera/size/background/no-validation runs.

All ten diagnostic views and the normal optical-flow-plus/Oklab/protection
composition applied in the game with two NR passes, chain-total scope and P0
guide. State snapshots record active continuous history and four temporal
textures occupying 56,623,104 Vulkan allocation bytes (54 MiB); the separate
prepared band remains cached after exiting low/high-frequency diagnostics.
The per-pixel history-weight screenshot is saved under the session's `screenshots`.
These are live contract/display checks, not motion-video quality acceptance.

The user completed manual camera/occlusion actions but reported that the low
frame rate prevented assessment of trails. The strict session stopped presenting
on normal UI exit but stayed busy; its forced termination is recorded separately
and normal retirement failed (exit -1, no clean vkDestroyDevice). This is not the
excluded 0xC0000409 case.

A separate normal graphics-launch session now loads the final Submit2 follow-up
Release. Its recorded inputs confirm validation, readback and frame tracing off;
the launch also disables GPU timing. NR/Look/SR/FG start off. Title animation near
30 FPS is not gameplay performance evidence. The session waits for manual entry
into the comparable game scene, and cannot satisfy the strict-validation gate.

The user then entered gameplay after the first battle, objective 55m. Within the
same fixed camera in this normal session, off / one / two / off measured 30.0691 /
30.0844 / 30.0809 / 30.0771 app FPS (21 / 20 / 20 / 20 samples). All samples were
foreground with Look/SR/FG disabled and the expected pass count; NR input was
1724×962. Recorded process modules show the final probe DLL and no Vulkan
validation module. This camera differs from the strict run, and the 30 FPS cap
does not establish remaining GPU budget or zero NR cost. Two-pass plus mode with
chain-total Look, Oklab protection and diagnostics off subsequently applied; the
user's repeat motion assessment is pending.

A subsequent 45-second plus-mode status trace contains 45 distinct active
foreground samples at 30.0826 mean app FPS (29.7221–30.8006). It does not establish
when manual motion occurred. The game remains in this active configuration for
the user's assessment; no motion-quality result has been inferred from telemetry.

The user confirmed completing the repeated actions, without an explicit visual
assessment. The completion marker and following screenshot are retained, and a
specific anomaly observation is requested. Live optical-flow, static and plus
mode switches also passed foreground contract checks in the final build, then
plus was restored; these are separate from motion-quality acceptance.

The user subsequently reported no obvious anomaly in either plus or ordinary
flow after the requested camera/occlusion actions. These manual observations
passed; no video was captured and no superiority claim is made. Only 2 of 45
ordinary-flow trace samples were active foreground, so that trace is not a
foreground motion-performance comparison. Pause kept source ID 13183 and applied
revision 9 despite requested revision 10; resume applied amount 75/revision 10
and advanced source ID to 13280 at about 30.75 FPS. The first resume frame was not
captured. Fully stopped Present still cannot apply live Look controls.

The normal session released both NR features, completed SDK shutdown successfully,
and retired device/instance counts to zero. Its exit was 0xC0000409, retained under
the user's existing exclusion. This does not repair or waive the earlier forced
strict retirement failure.

A new strict-only call-attribution module keeps normal launches free of extra
shader/pipeline/render-pass/clear/query/draw wrappers. Two tests cover normal
profile gating, missing entry points, query arguments and callback context.
Latest full component regression is 223 passed with 5 default ignored; component
and toolbox host/Windows checks have no Rust warnings/errors. Its frozen layer is
`17bfa392b2c7fd7711aec18a2c67d172e7347fea182af00365323c8931f4b845`.
Initial strict startup messages are now tagged as vkCreateShaderModule (4 errors)
and vkCmdEndQuery (10 errors). Application entry tags do not prove ultimate fault
ownership. The user confirmed actual game entry; its snapshot retained 44
errors/10 warnings and zero proxy-layout errors. The user then requested ending
this repeated known-emulator investigation.

Rust formatting, component/toolbox host and Windows MSVC checks passed with zero
errors/warnings; component full regression, eight shared toolbox settings tests,
six toolbox preset tests, eight frontend preset tests, production types/build and
changed-file lint passed. Exact counts and log hashes are in the summary.

The user excludes Eden `0xC0000409` because it also occurs with enhancement off.
Raw exit/retirement failures are preserved; this exclusion does not waive other
validation errors. Historical M0 acceptance remains open. Per the user's
2026-10-06 clarification, known emulator errors no longer block M5/M6; their
default-off implementations and separate evidence are now available. Real
camera/occlusion/pause videos, long-running resource/retirement
checks, NR→SR→FG combinations, M6 inference-sizing acceptance and any independently justified
low-frequency temporal reconstruction remain pending. Live Look changes still
need Present events; a completely stopped renderer cannot process them.
