# #5437: NIFAL-D1-2026-10-08-01: The Phase-2 resolver table describes `resolve_unresolved_gloss_neutral_roughness`'s trigger backwards, in both the module doc and `nifal.md`

**Labels**: low,nifal,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5437

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-08.md` — `NIFAL-D1-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4 / FO76 (BGSM
  content)
- **Location**: `byroredux/src/material_translate.rs:42` and `docs/engine/nifal.md:797`. The predicate itself is at
  `material_translate.rs:1409-1433`.
- **Status**: NEW. The text was introduced by `c9b02ba4a` on 2026-09-12. It is not covered by #4246 (the `placement_lod`
  exemption rationale) or by #5279 (the "backstop" wording).
- **Description**: Both table rows say the resolver writes `Material::roughness` "when a gloss/smoothness map **resolved** with
  **no authored BGSM PBR scalars** to interpret it". The code does the opposite on both counts:
  - It returns `None` unless `bgsm_pbr_scalars_authored` is true.
  - It returns `None` unless `gloss_map_index == 0`, meaning no gloss map resolved.
  - It acts only when roughness is at or below the near-mirror floor.

  `nifal.md:823-824` states the gate correctly ("only acts on authored BGSM PBR scalars"), so the spec contradicts itself 26 lines
  apart. The function's own rustdoc (`:1383-1386`) is also correct.
- **Evidence**: `unresolved_gloss_neutral_roughness` contains the line `if !bgsm_pbr_scalars_authored { return None; }`, then
  `if gloss_map_index != 0 { return None; }`, then the floor test.
- **Impact**: A reader of the table would scope the resolver to legacy, non-BGSM content with a working gloss map. That is
  exactly the population it must never touch. Widening the gate to match the table would neutralise legacy roughness.
- **Related**: #3905, #3639, #4246, #5230.
- **Suggested Fix**: Reword both rows to: "when authored BGSM PBR scalars sit at the near-mirror floor and no gloss map
  *resolved* to modulate them (#3905; forced mirror panes exempt, #5230)". Fold this into the #4246 / #5279 doc pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`nifal.md` Phase-2 table + the `material_translate.rs` module-doc table (fold into #4246 / #5279 doc pass))
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the material merge path, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
