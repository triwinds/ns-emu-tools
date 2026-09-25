# Streamline FG runtime

Windows x64 Vulkan frame-generation layer and the launcher used by the toolbox. This remains an experimental integration: it uses reference constants, zero motion vectors and constant depth. Window-operation protection and SDK capability checks still apply; SR is not implemented.

## Source layout

- `src/lib.rs`, `target_*.rs`, `live.rs`: Vulkan layer, frame generation, lifecycle and live control/telemetry.
- `src/main.rs`, `launcher.rs`, `session_verify.rs`: toolbox launcher and session verification.
- `src/support.rs`, `runtime.rs`: shared file validation and pinned runtime checks, with no dependency on the diagnostic host.
- `src/diagnostics/`: standalone Vulkan host, SDK routing experiments and diagnostic regression tests. These are compiled only with `--features diagnostics`.
- `sdk/baseline.json`, `bridge/`, `sdk-route/`: pinned SDK contract, C++ bridge and reproducible downstream runtime patches.
- `evidence/`: regression fixtures and compact final measurements; no bulk experiment logs.
- `docs/experiments/`: historical investigations, not runtime dependencies.
- `../streamline-target-policy.rs`: compatibility rules shared with the toolbox.
- `../../tools/streamline-sdk-audit`: optional read-only SDK/source auditing utility. The runtime does not depend on this tool.

## Build and package

Run from the repository root. The pinned SDK/header checkouts remain under `src-tauri/target`; see `sdk-route/README.md` for preparing the runtime. The SDK bridge build verifies its input headers against the runtime-owned baseline.

```powershell
cargo build --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features sdk-bridge --lib --bin streamline-layer-probe
& src-tauri/crates/streamline-fg/package-local.ps1
# Rebuild the toolbox after package-local updates its embedded artifact hashes.
cargo build --locked --release --manifest-path src-tauri/Cargo.toml --bin NsEmuTools
& src-tauri/crates/streamline-fg/stage-local.ps1
```

The launcher and DLL intentionally retain their installed names, `streamline-layer-probe.exe` and `streamline_probe_layer.dll`, along with the existing CLI/environment/Vulkan protocol. Renaming the source directory does not invalidate existing installations. `package-local.ps1` explicitly builds only those two runtime artifacts. `stage-local.ps1` verifies the toolbox's embedded hashes before staging the package beside the EXE; distribute both together. No SDK binaries are stored in this repository.

## Optional diagnostics

```powershell
cargo run --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --features diagnostics --bin streamline-fg-diagnostics -- --help
cargo test --locked --manifest-path src-tauri/crates/streamline-fg/Cargo.toml --all-features
cargo test --locked --manifest-path src-tauri/tools/streamline-sdk-audit/Cargo.toml
```

The default launcher no longer accepts SDK-host modes. The separate diagnostic binary retains them, including explicit `--target-probe` session verification. Historical commands that used the old combined executable for SDK host tests must use `streamline-fg-diagnostics` instead.
