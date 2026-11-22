# #7190 — TOOL-D6-2026-11-21-01: texture-upscale's asset-path check is the `is_absolute` shape #5016 and #7142 retired (PLAUSIBLE, Windows effect)

Labels: low,tech-debt,bug

- **Severity**: LOW
- **Dimension**: Tool CLIs
- **Exposure**: an operator running `discover` on archives with crafted names, then `run`
- **Location**: `tools/texture-upscale/src/lib.rs` `validate_asset_path`, `output_png_path`
- **Status**: NEW
- **Description**: after normalisation it rejects only an absolute path and empty, `.` or `..` segments. A `c:` or `d:x` segment passes, and on Windows `PathBuf::push("c:")` replaces the output root. On Linux it is a literal `c:` directory. `byroredux_core::paths::is_bare_component` rejects `/`, `\` and `:` precisely for this.
- **Suggested Fix**: reject any segment that fails `is_bare_component` (the segments are already split), with tests for `c:/x` and `d:x`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
