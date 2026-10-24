# Pinned SDK downstream routing

The active integration uses `downstream-v3.patch`, applied to SDK commit `e8aaa6eaac968711fb62473d4ae8256dde20919b`. `patched-files-v3.json` verifies the modified source, and `runtime-v3.json` pins the seven deployed runtime DLLs. V1/V2 patches and old reports are archived in `../docs/experiments/`; runtime builds do not use them.

The SDK and Vulkan header checkouts are local build prerequisites, not vendored source:

- `src-tauri/target/streamline-sdk-v2.12.0`: frozen SDK checkout.
- `src-tauri/target/vulkan-headers-v1.4.341/include`: pinned Vulkan headers.
- `src-tauri/target/streamline-packman-cache`: SDK dependency cache.
- Visual Studio 2022 C++ toolchain.

From the repository root:

```powershell
& src-tauri/crates/streamline-fg/sdk-route/build.ps1
# For an existing verified experiment checkout:
& src-tauri/crates/streamline-fg/sdk-route/build.ps1 -Resume
```

The script creates/verifies `src-tauri/target/streamline-sdk-route-repro-v3`, builds the modified SDK routing components and runs the routing contract check. `stage-runtime.ps1 -Destination <new absolute directory> -ReferenceRoot <reference runtime root>` assembles the seven pinned DLLs from verified sources; it refuses an existing destination. The toolbox packaging script expects the assembled runtime at `src-tauri/target/streamline-route-runtime-v3`.

No binaries are redistributed by these scripts. See the runtime README for building the layer, packaging, and current limitations. Historical reports do not describe the current FG installation status.
