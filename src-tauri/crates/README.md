# Native FG components

- `streamline-fg/`: deployed Vulkan layer and launcher. See its README for packaging and opt-in diagnostics.
- `streamline-target-policy.rs`: shared executable compatibility policy, compiled by both the toolbox and launcher.

The read-only SDK audit utility lives in `../tools/streamline-sdk-audit/`. Historical experiments live under `streamline-fg/docs/experiments/`; generated logs and local binaries are not source dependencies.
