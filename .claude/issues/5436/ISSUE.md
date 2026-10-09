# #5436: NIFAL-D8-2026-10-08-02: The loose `.mat` arm's `Undecodable`/`None` branches set `is_pbr` but not `external_material_resolved`, so the #4283 provenance the replaced CDB-miss fallback set is lost on this path (regression of #4283)

**Labels**: low,nifal,import-pipeline,bug,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5436

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-08.md` — `NIFAL-D8-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: #4283 is CLOSED; validation confirmed `apply_loose_mat` still writes only `material.is_pbr = true` in its `Undecodable`/`None` arms while `apply_cdb_pbr_fallback` (`cdb.rs`) also sets `external_material_resolved` — filed as a regression.

- **Severity**: LOW · **Dimension**: Shader-flags/Effects → Material · **Tier Violated**: single-boundary (a partial
  re-implementation of `apply_cdb_pbr_fallback`) · **Game Affected**: Starfield (Creation/mod)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:1690` (`material.is_pbr = true;` is the only routing write) and
    `:1703-1718` (the `Undecodable` and `None` arms);
  - compare `byroredux/src/asset_provider/material/cdb.rs:361-370` (`apply_cdb_pbr_fallback` sets both flags);
  - consumer: `byroredux/src/helpers.rs:140-142` (`effect_glass_carrier = … && (bgem_glass || (keyword_match &&
    external_material_resolved))`).
- **Status**: Regression of #4283, confined to the new loose-file path introduced by `c2f28e06c`.
- **Description**: Before #4277, a Starfield `.mat` path with no CDB row went to `apply_cdb_pbr_fallback`. That sets
  `external_material_resolved = true`, the format-agnostic provenance #4283 added so that Starfield effect-shader glass can take
  the keyword-promotion route. `apply_loose_mat` handles the same "external material present, nothing decoded" state, but writes
  only `is_pbr`. Only the `Decoded` branch gets the flag, indirectly, through `apply_cdb_material` (`merge.rs:217`).

  Per D8-01, every real file currently lands in the `None` branch. A loose file that cannot be decoded will always be possible,
  even after D8-01 is fixed.
- **Impact**: A Creation effect-shader material whose path or name matches a glass keyword is no longer promoted to glass. None
  of the 20 installed files is known to hit this (for example, `lasersight_white.mat` has no glass keyword), so the visible
  impact today is nil.
- **Related**: #4283 (closed), #5099 (the two provenance signals), D8-01, #5284.
- **Suggested Fix**: In the two fallback branches, call `apply_cdb_pbr_fallback(material, path)` (or set
  `external_material_resolved` beside `is_pbr`), so that "a `.mat` is present but carries no data" has one implementation.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`apply_cdb_pbr_fallback` and every other `is_pbr`-only write in `merge.rs`)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the material merge path, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
