# #2696: NIFAL-D1-2026-08-12-01: Canonical `Material` doc cites a `grayscale_to_palette_scale` precedent field that does not exist on `Material`

- **Severity**: LOW
- **Dimension**: Material
- **Tier Violated**: none (documentation defect on the canonical type)
- **Game Affected**: all (doc only)
- **Location**: `crates/core/src/ecs/components/material.rs:256-260`
- **Status**: NEW
- **Description**: The #2284 rationale block says the six BSLSP shading scalars
  landed on `Material` "matching the existing `grayscale_to_palette_scale`
  precedent (see that field's doc …)". No such field exists on `Material` — the
  string occurs exactly once in that file, inside this comment. The authored
  scalar lives on the raw `ImportedMaterial` (`crates/nif/src/import/types.rs`,
  written by `byroredux/src/asset_provider/material.rs:1058`) and is
  raw-tier-parked — a *different* tier from the precedent claimed.
- **Evidence**: `grep -c grayscale_to_palette_scale crates/core/src/ecs/components/material.rs` → 1.
- **Impact**: A future audit reading the canonical type's own docs is told a field
  exists that does not, obscuring the genuine parked-at-raw-tier status. No
  runtime effect.
- **Related**: the accurate anchor is the "not yet plumbed to GpuMaterial" comment
  in `crates/renderer/shaders/triangle.frag`.
- **Suggested Fix**: Reword to say the precedent is parked one tier lower on
  `ImportedMaterial`, or land the field for real.

---
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-12.md` (finding `NIFAL-D1-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

