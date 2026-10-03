# #5185: EXT-D5-2026-10-02-05: watal.md §2 still says Starfield pigment concentrations are normalized in the shader, and records neither #5151's metric lift nor the open unit of the noise-UV tile

**Labels**: low,terrain-exterior,water,documentation,doc-rot,game:starfield
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**: `docs/engine/watal.md:340-343`; the code is at `byroredux/src/env_translate.rs:905-917` and `crates/plugin/src/esm/records/spatial_units.rs:151-158`.
- **Status**: NEW. Stale since #4285 (closed); the missing record dates from #5151 (closed).
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Starfield
- **Description**:
  - The §2 text reads: "Its authored pigment concentrations remain in their vanilla 0..20 range and are normalized in the shader against the shared `STARFIELD_WATER_CONCENTRATION_REFERENCE`".
  - Since #4285 the ÷20 runs at the WATAL translate (`env_translate.rs:905-913`, comment "normalized … HERE, at the WATAL translate boundary, not in `water.frag`"). The shader only clamps (`water.frag:563`).
  - §2 has no mention of #5151 (Starfield DNAM metres → BU: depth, underwater fog near/far and noise falloff ×70, absorption ÷70).
  - The open item #5151 left behind is recorded only in a code comment (`spatial_units.rs:157-158`): the noise UV tile sizes at DNAM 120/124/128 "stay unlifted until a capture settles their unit". The skill's promotion rule wants open items in watal.md §2.
- **Impact**: The spec places a per-game unit conversion at render time, which is the defect #4285 fixed. A reader cannot find out from WATAL that the Starfield noise tile unit is unresolved.
- **Suggested Fix**:
  - Rewrite `watal.md:340-343` to say the normalization happens at the translate.
  - Add a #5151 sentence on the metric lift and its boundary (`spatial_units::normalize`, Starfield-gated, FO76 untouched).
  - List "Starfield noise-UV tile unit (DNAM 120/124/128)" as open in §2.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it
