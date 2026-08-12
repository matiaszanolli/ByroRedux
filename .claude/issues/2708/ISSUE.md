# #2708: SF-2026-08-12-D9-02 - The REFR-overlay material resolver is a second, parallel external-material path that knows only `.bgsm`/`.bgem`, so Starfield `.mat` overlays resolve to nothing even after CDB Phase 2 lands

- **Severity**: LOW
- **Dimension**: 9 — external material flow
- **Location**: `byroredux/src/cell_loader/refr.rs:192-233` (`fill_from_bgsm`)
- **Status**: NEW
- **Description**: `fill_from_bgsm` dispatches on `path.ends_with(".bgsm")` /
  `".bgem")` and returns silently for anything else. Its own doc says "No-op when the
  path isn't a `.bgsm` / `.bgem`", so the omission is deliberate — but it means the
  engine has **two** external-material resolvers with divergent format coverage:
  `merge_external_material` (BGSM + BGEM + a `.mat` arm) and this one (BGSM + BGEM
  only). A Starfield REFR whose XATO/MSWP supplies a `.mat` path gets the path
  propagated into `ov.material_path` (and thence onto the spawned material) but no
  role fills, and there is no place for a future CDB lookup to hook in on this side.
- **Impact**: Zero today — Starfield content resolves no textures from either path
  (#2359), and `.mat` overlays on vanilla Starfield REFRs are rare. It becomes a real,
  silent per-REFR divergence the moment #2359 Phase 2 lands and the two resolvers
  disagree about what a `.mat` yields.
- **Related**: #2359, #2594, SF-2026-08-12-D9-01.
- **Suggested Fix**: Note the format gap in the doc comment now, and route both
  resolvers through one shared "resolve external material → roles" helper when Phase 2
  lands, rather than adding a second `.mat` arm here.

---
**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D9-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

