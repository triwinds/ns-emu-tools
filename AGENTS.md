# Repository Instructions

- Commit messages must follow Conventional Commits, for example: `fix(eden): switch release source to Forgejo`.
- After modifying Rust code, run `cargo fmt`, then run `cargo check` for the host and Windows target environments before handing off the work. When validating a macOS target from Windows, use `cargo zigbuild` instead of plain `cargo check`.
- If either `cargo check` run reports any errors or warnings, fix them before handing off the work.
- For local delivery, build the executable directly in `src-tauri/target/release` with Tauri `--no-bundle` and keep the verified graphics component directory beside it. Stage component updates there as needed. Do not create installers, ZIP archives, or a new delivery directory under `output` unless the user explicitly requests them.
