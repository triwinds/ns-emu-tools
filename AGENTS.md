# Repository Instructions

- Commit messages must follow Conventional Commits, for example: `fix(eden): switch release source to Forgejo`.
- Unless the user explicitly requests otherwise, make changes only on the Rust side under `src-tauri`. Do not modify Python or other non-Rust parts by default.
- After modifying Rust code, run `cargo fmt`, then run `cargo check` for both the macOS environment and the Windows target environment before handing off the work.
- If either `cargo check` run reports any errors or warnings, fix them before handing off the work.
