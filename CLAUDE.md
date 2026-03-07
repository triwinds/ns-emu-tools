# Repository Instructions

- Commit messages must follow Conventional Commits, for example: `fix(eden): switch release source to Forgejo`.
- Unless the user explicitly requests otherwise, make changes only on the Rust side under `src-tauri`. Do not modify Python or other non-Rust parts by default.
- After modifying Rust code, run `cargo fmt` and `cargo check` before handing off the work.
- If `cargo check` reports any errors or warnings, fix them before handing off the work.
