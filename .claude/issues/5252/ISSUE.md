# #5252: REN-D6-2026-10-05-01: #4441's boundary doc says Starfield material-reference stubs reach the keyword classifier, but since #5197 every CDB-hit stub is stamped `NO_SIGNAL_NEUTRAL` first

**Labels**: low,nifal,renderer,documentation,doc-rot,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5252

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-05.md` — `REN-D6-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: NIFAL Material (doc)
- **Location**: `byroredux/src/material_translate.rs` (the boundary contract: "BGEM and the Starfield material-reference stubs do not … the classifier arm is a live path for them"); `crates/core/src/ecs/components/material.rs` (`resolve_pbr` inline comment); versus `byroredux/src/asset_provider/material/merge.rs` `apply_cdb_material`.
- **Status**: NEW
- **Description**: `c2b67d81e` (#4441, 10-04) landed one day after `978d25c19` (#5197). It describes the stubs as unconditionally unclassified. `apply_cdb_material` stamps `PbrMaterial::NO_SIGNAL_NEUTRAL` metalness and roughness whenever the overrides are `None` on a CDB hit. Only CDB misses, and runs without the CDB loaded, still reach `classify_pbr_keyword`.

  The text exists to stop a reader deleting the classifier arm. Read as written, it also invites "restoring" the classifier for CDB hits, which would re-open #5197 (for example, `iris_iron_color.dds` turning into a 0.9-metal eye).
- **Suggested Fix**: Qualify it as "Starfield stubs whose `.mat` misses the CDB (or with no CDB loaded)", and name `NO_SIGNAL_NEUTRAL` as the CDB-hit outcome. Fold this into #5210's NIFAL doc pass if that lands first.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
