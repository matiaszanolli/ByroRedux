# #7180 — PAR-D6-2026-11-21b-02: four other caches still fold their keys ASCII-only while the archives fold Unicode

Labels: low,import-pipeline,bug

- **Severity**: LOW (duplicate parse and upload only; reads resolve)
- **Dimension**: I/O & Paths
- **Location**:
  - `crates/renderer/src/texture_registry/mod.rs` `normalize_path` (the texture registry key);
  - `byroredux/src/scene/nif_loader.rs` (the `SceneImportCache` key);
  - `byroredux/src/asset_provider/material/provider.rs` (`normalize_material_path(..).to_ascii_lowercase()` keys);
  - `byroredux/src/asset_provider/archetype_idle.rs` (`idle_key`).
- **Status**: NEW (siblings of #7168, closed)
- **Description**: `Ä.dds` and `ä.dds` are two cache entries for one archived file, the #7168 class. No persisted (save) key is involved.
- **Suggested Fix**: fold through the same Unicode lowercase, and extend #7168's binary-side agreement test to these keys.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
