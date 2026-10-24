# Experimental toolbox integration, 2026-10-01

The user authorized frontend integration and keeping the native Vulkan route,
while excluding Ryujinx source changes and its previously identified errors.
The tool now exposes local NR runtime import/removal, independent NR and SR/DLAA
controls, a 0–100% NR slider, FG startup preference, live FG control, and actual
NR/SR state. NR defaults off; old configuration preserves FG's startup default.
The normal `--graphics-launch` mode does not request diagnostic Vulkan validation
or readback, and its exit report explicitly denies strict acceptance.

## End-to-end backend runs

These use the same services as Tauri commands, invoked by explicit ignored GPU
tests. A copied test EXE and portable config under `src-tauri/target` isolate the
tool's component store from the user's toolbox configuration. Ryujinx's original
EXE, existing files and global Vulkan registration are unchanged.

- `nr-toolbox-smoke-001/.../game-5D06fy`: approved real runtime imported and
  copied into a private normal game session. Initial NR/SR/FG all off. NR at
  50%, 0% and 100% became active. The first batch stopped because repeated SR
  off status replaced its last completed revision, preventing acknowledgement.
  This was a telemetry defect, not evidence of a failed NR evaluation. All-off
  was applied and the game exited normally with exit code 0.
- `nr-toolbox-smoke-001/.../game-akvBAT`: same route after preserving completed
  SR acknowledgement fields. Seven phases each have eight one-second snapshots:
  NR 50%, NR 0%, NR 100%, DLAA only, NR + DLAA, NR + DLAA + FG request, all off.
  All independent revisions were acknowledged. NR and DLAA had actual activity
  in their enabled phases. FG remained paused for background focus in every
  sample; this run does not claim actual FG execution. All-off was confirmed,
  followed by normal exit 0 and successful NGX/Streamline shutdown records.
- `nr-toolbox-smoke-002/.../game-g2QNre`: no NR model imported. Initial FG off,
  NR off, DLAA on. DLAA actually ran from the native source, and
  `nrLiveSupported=false`. A live NR enable attempt was explicitly rejected
  with the import/restart instruction. This preserves SR use without NR.

No visual-feedback reply was received for these integration samples. Earlier
user-confirmed moving-picture and actual FG evidence remains in the combination
and SDK records. This session does not replace that evidence or claim formal
image-quality, resize, long-run, or zero-error game validation acceptance.

## Caller bridge rebuild audit

The audited bridge was rebuilt with the dynamic CRT, matching NR's pinned NGX
core. Its `.text`, `.data`, `.pdata` and `.reloc` raw sections are byte-identical
to the previously tested `b3612653...` bridge; 28 bytes differ in PE build/debug
metadata. ABI `0x0001004000480038`, real init/evaluate call RVA `0x10d0`, return
continuation `0x10d3`, and post-call result store `0x10e2` remain unchanged.
The new exact hash is pinned and exercised by both NR game sessions. Static CRT
output was rejected by packaging, and the dynamic CRT package build was repeated
to verify a stable hash. No acceptance hash wildcard was introduced.

## Checks and use

Rust formatting and application, layer, bridge host/Windows checks passed without
compiler warnings. Layer tests, native component ownership/tamper tests,
configuration migration/range tests, install tests and the SR acknowledgement
regression passed. Frontend type check/build and lint for the modified UI/type
files passed. The existing unrelated `no-explicit-any` errors in `utils/tauri.ts`
remain; no claim is made that repository-wide frontend lint passed.

Keep `streamline-fg-package` beside the built `NsEmuTools.exe`. In **图形增强**,
select the emulator, check/install the component, import the local pinned
`nvngx_dlssnr.dll`, choose independent enhancement settings, and click
**以画面增强启动**. Import after startup requires another dedicated launch.
The model DLL is not included in the distributable component package. Loaded
sessions have private copies, so component removal does not delete loaded files.
Modified/unknown managed files block removal and remain intact.

`summary.json` retains exact artifact hashes, full inputs/results, phase snapshots,
shutdown records and raw local evidence paths. Logs remain in ignored `target`.
