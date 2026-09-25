# Optional Streamline SDK audit tool

This read-only development utility checks the pinned SDK and historical source-routing assumptions. It is not needed to build, install or launch the toolbox FG runtime and does not load graphics DLLs. Its P0 blocker report describes the original frozen investigation; it is not a live compatibility report for the implemented layer.

The shared SDK baseline is owned by `../../crates/streamline-fg/sdk/baseline.json`. Do not delete that file when omitting this optional tool.

```powershell
cargo run --manifest-path src-tauri/tools/streamline-sdk-audit/Cargo.toml --bin streamline-sdk-audit -- --help
cargo test --locked --manifest-path src-tauri/tools/streamline-sdk-audit/Cargo.toml
```

`P0-review.md` and `route-baseline.json` retain the historical review and source audit rules. They are not runtime dependencies.
