# TD2-2026-10-15-13: `CellLightingRes` has no default/builder, so five production sites and eleven test fixtures spell all 25 fields

Labels: low,terrain-exterior,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-13`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: production `byroredux/src/cell_loader/load.rs:146` (`engine_default_interior_lighting`), `env_translate/fallback.rs:55`, `env_translate/sky_weather.rs:84`, `cornell.rs:612`, `cornell/oracle.rs:352`; `components/environment.rs:70` (`from_cell_lighting`); struct `components/environment.rs:27`; tests `systems/weather.rs:2224,2528,2957,3167`, `render/lights.rs:371,396,866`, `systems/audio.rs:1402`, `render/sky.rs:265`, `cell_loader/load.rs:1349`
- **Status**: NEW
- **Age**: fields added over `4cd98e4e2` (#4481/#4485), `db0ec5467` (#4926) and the fog/Skyrim XCLL work; the file moved in `6ef8976f2` (#5467)
- **Effort**: small
- **Description**: each literal ends in the same ten `None`s (`fog_clip`, `fog_power`, `fog_far_color`, `fog_max`, `light_fade_begin/end`, `directional_ambient`, `specular_color/alpha`, `fresnel_power`, `inheritance_flags`) and `fog_medium: FogMedium::from_legacy_ramp(100_000.0, 1_000_000.0, None)` appears verbatim in three production files.
- **Evidence**: clone scan: 9 identical windows `load.rs:155` / `fallback.rs:64`, 12 `cornell.rs:618` / `load.rs:152`.
- **Impact**: the next lighting field is 16 edits; a field forgotten in a fixture compiles but silently means "None".
- **Related**: #861 (the `from_cell_lighting` helper doc says exterior and the engine-default build the struct directly).
- **Suggested Fix**: `impl CellLightingRes { pub(crate) fn bare(ambient, directional_color, directional_dir, is_interior, fog_color, fog_near, fog_far) -> Self }` (ten `None`s and `fog_medium` computed once) and have the 5 production + 11 test sites use `..CellLightingRes::bare(..)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
