# #7182 — TOOL-D1-2026-11-21-01: `depth.dump` writes into `screenshots/` without the canonical-root check the other two confined writers make

Labels: low,tech-debt,bug

- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process on a debug build or an opted-in release. The escape needs a pre-planted `screenshots` symlink in the engine's cwd.
- **Location**: `byroredux/src/commands/depth.rs` `DepthDumpCommand::execute` (ac558feb7); contrast `crates/debug-server/src/system.rs` `write_screenshot` and `byroredux/src/commands/assets.rs` `safe_tex_dump_path`
- **Status**: NEW
- **Description**: the name filter is sound (`*.pfm`, ASCII alphanumerics and `_-.`), and the write uses `create_new`. But it writes `Path::new("screenshots").join(name)` after only `create_dir_all`. The screenshot and `tex.dump` writers also canonicalise the output root and require it to sit inside the cwd, so a symlinked `screenshots/` is refused by them and followed by `depth.dump`. It is a third copy of the confinement logic.
- **Evidence**: coordinator re-read `depth.rs:75-76` against `system.rs:260-265` and `assets.rs:515-517`. Its test (`depth_dump_rejects_paths_and_unsupported_devices`) covers the name filter only.
- **Suggested Fix**: one shared "single filename under `<dir>/`, canonical root inside cwd, `create_new`" helper used by all three writers, plus a symlinked-root test (the `confinement_rejects_a_symlink_out_of_the_root` pattern).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
