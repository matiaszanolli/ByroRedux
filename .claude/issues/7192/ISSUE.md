# #7192 — TOOL-D6-2026-11-21-03: `discover` clobbers an existing hand-edited manifest non-atomically, and `--help` omits the manifest-trust note

Labels: low,tech-debt,bug

- **Severity**: LOW
- **Dimension**: Tool CLIs
- **Exposure**: any operator
- **Location**: `tools/texture-upscale/src/cli.rs` (`Commands::Discover` → `save_manifest`), `lib.rs` `save_manifest` (`std::fs::write`), and the clap help for `run --manifest`
- **Status**: NEW
- **Description**: `run` refuses to overwrite outputs without `--overwrite`, but `discover` overwrites the manifest the README tells users to review and extend by hand, with a plain `fs::write`. The README states that `[upscaler]` is executed, trusted input, but `run --manifest`'s help says only "Reviewed texture-set manifest."
- **Suggested Fix**: refuse an existing manifest without `--overwrite` and write through `byroredux_core::atomic_file::atomic_write`; extend the `--manifest` help with "executes `[upscaler].program`; run only manifests you wrote or reviewed".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
