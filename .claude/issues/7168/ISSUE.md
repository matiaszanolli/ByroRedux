# #7168 — PAR-D6-2026-11-21-03: `canonical_mesh_key` folds ASCII only; `normalize_path` folds Unicode

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/core/src/paths.rs` `canonical_mesh_key` (`to_ascii_lowercase`) vs `crates/bsa/src/ba2.rs` `normalize_path` (`to_lowercase`)
- **Status**: NEW (pre-existing: the renderer's old key was ASCII-only too; #7145 moved it into the one rule)
- **Description**: reads are correct, because archive and loose lookups re-normalise. But `Ä.nif` and `ä.nif` get different cache keys in the renderer mesh cache and `nif_import_registry`, so one asset is parsed and uploaded twice.
- **Suggested Fix**: have `canonical_mesh_key` fold through the same function as `normalize_path` (move the fold to `core::paths`, which bsa can depend on, or the reverse).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
