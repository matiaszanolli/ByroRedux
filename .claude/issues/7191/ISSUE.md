# #7191 — TOOL-D6-2026-11-21-02: texture-upscale folds case ASCII-only while the archives and engine caches fold Unicode

Labels: low,tech-debt,bug

- **Severity**: LOW (latent; real texture paths are ASCII)
- **Dimension**: Tool CLIs
- **Exposure**: a source stack mixing loose and archived non-ASCII paths
- **Location**: `tools/texture-upscale/src/lib.rs` `normalize_asset_path` (`to_ascii_lowercase`), which keys `source.rs` `AssetSource::open` / `SourceStack`
- **Status**: NEW (the #7168 / #7180 class; those fixed the engine only)
- **Description**: `Ä.dds` in one source and `ä.dds` in a later one stay distinct, so "later overrides earlier" does not happen and two manifest entries can result.
- **Suggested Fix**: fold through `byroredux_bsa::normalize_path` (the tool already depends on bsa) or `core::paths::fold_case`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
