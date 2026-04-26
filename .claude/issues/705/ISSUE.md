# Issue #705: O4-07: decal slots populated but emit as alpha-blend overlays rather than via depth-bias decal pipeline path

**Severity**: LOW
**File**: `crates/nif/src/import/material.rs:863-874` (decal extraction); `byroredux/src/render.rs:381` (only `is_decal` flag derived from shader-flag bits, not from presence of decal_maps)
**Dimension**: Rendering Path

`NiTexturingProperty` decal slots 0..=3 reach `MaterialInfo.decal_maps` and ride through `ImportedMesh`, but they don't currently bind any descriptor or surface in the fragment shader. Consumers expecting blood splatters / wall paintings / faction symbols on Oblivion architecture see only the base material.

The `is_decal` flag (which DOES drive the depth-bias path at `draw.rs:1244`) is set only from shader-flag bits 26/27, never from `decal_maps.len() > 0`.

**Fix**: Either:
- (a) Drop the extraction (matches pre-#400 behaviour, removes parsing cost), OR
- (b) Finish the round-trip: descriptor bindings + fragment-shader overlay loop + `is_decal = is_decal || !decal_maps.is_empty()` so the depth-bias path engages.

Currently the import-side cost is paid but the visual delivery is a no-op.

#400 closed the extraction half; this issue tracks the consumer half.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
