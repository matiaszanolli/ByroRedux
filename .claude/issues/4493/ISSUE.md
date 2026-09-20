# EXT-D1-2026-09-19-03: stale intra-doc link translate_climate_sky on the SUN_INTENSITY_PEAK contract doc

- **ID**: EXT-D1-2026-09-19-03
- **Labels**: low,terrain-exterior,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4493

**Severity**: LOW (doc rot) · **Dimension**: EXAL boundary
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D1-2026-09-19-03)

**Location**: `byroredux/src/env_translate.rs:992`

**Description**
The `SUN_INTENSITY_PEAK` contract doc's bullet cites `[`translate_climate_sky`]` — a function that does not exist anywhere in the tree (`grep -rn translate_climate_sky byroredux/src` → only this doc line). The bootstrap seed is produced by `apply_environment` calling `compute_sun_arc` + `translate_sky`/`translate_exterior_cell_lighting` (`scene/world_setup.rs:273-323`). A dead producer name on the one doc whose entire job is keeping three producers in sync is exactly the rot the doc exists to prevent; it is also a broken rustdoc intra-doc link.

**Suggested Fix**
Rewrite the bullet to name `apply_environment`'s `translate_sky`/`procedural_fallback_*` bootstrap seed.

## Completeness Checks
- [ ] **TESTS**: `cargo doc` warns gone
