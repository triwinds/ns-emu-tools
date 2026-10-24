# Independent XeSS component

The `xess-component` feature builds Native AA and FG together, without
`sdk-bridge` or an external optical-flow runtime. UI, storage, build steps,
validation evidence and current quality limits are documented in
[`docs/xess-component-integration.md`](../../../docs/xess-component-integration.md).

- `package.py` stages hash-verified Intel/Agility files, licenses, the launcher
  and layer into `src-tauri/target/release/xess-package`. It updates the app's
  embedded manifest; build the Tauri app **after** packaging, with `--no-bundle`.
- Existing sidecars are verified before replacement. DELETE access to all
  members is acquired before removing any file; active launchers block updates.
- `smoke_game.py --name <new-name> --mode aa|fg|both` runs the packaged component
  separately from the UI. Pass `--game <ROM>` and `--emulator <EXE>` explicitly,
  or use this workspace's previous local Nextendo/Xenoblade 3 test defaults.
  It changes no emulator settings. Close the emulator normally after sampling.
- `evidence/integration.json` contains compact results and delivery hashes.

The main app installs to its own `graphics/xess/runtime/<version>` store and
saves XeSS preferences/sessions separately from DLSS. Installation uses a verified
installed version, sidecar or download cache first, then automatically downloads
the pinned Release through the configured downloader, proxy and GitHub mirror.
The progress dialog supports cancellation; archive and member SHA-256 checks
must pass before an atomic cache/install publication. A failed download keeps
existing files. Changed cache directories are retained as `.damaged-*`.

The independent launcher defaults to normal graphics launch mode; the app and
smoke helper also pass `--graphics-launch` explicitly. Normal exits do not run
the Streamline diagnostic acceptance gate, which does not recognize XeSS frame
events. `--target-probe --verify <session>` remains an explicit diagnostic action.
The UI uses the target's exit record for its result and reports launcher errors
separately. A launcher exit alone never confirms that a spawned target exited.

To prepare an online release after `package.py`, run `python
src-tauri/tools/xess-component/release.py` from the repository root. Assets
(runtime ZIP, source ZIP, manifest and SHA256SUMS) are written under
`build/xess-release/<tag>`, and `xess-release.json` pins the matching version,
download URL and archive hash. Publish these assets to the runtimes repository
before building the app. Regenerate this pin whenever packaging changes.
Local delivery remains the `--no-bundle` executable and verified component
directories in `src-tauri/target/release`.
