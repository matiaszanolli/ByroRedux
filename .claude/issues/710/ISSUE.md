# NIF-D5-03: FO4 / FO76 BSPositionData undispatched — actor positioning lost

URL: https://github.com/matiaszanolli/ByroRedux/issues/710
Labels: enhancement, nif-parser, high

---

## Severity: HIGH (cheap fix, large blast radius)

## Game Affected
FO4, FO76

## Location
- `crates/nif/src/blocks/mod.rs` — no dispatch arm next to the other `NiExtraData` aliases at lines 360-379

## Description
nif.xml line 8342: `<niobject name="BSPositionData" inherit="NiExtraData" module="BSMain" versions="#FO4# #F76#">`. Carries a per-vertex blend factor float array — used for procedural vertex morphing on physics-driven cloth and dismemberment.

## Evidence
2026-04-26 corpus sweep:
- `Fallout4 - Meshes.ba2` — 372 blocks
- `SeventySix - Meshes.ba2` — 2,589 blocks
- Total: 2,961 blocks

## Impact
Cloth simulation and FO76 vertex-blended deformation lose per-vertex data, falling back to default rigid behaviour. Visible regression on capes, flags, dismemberment effects.

## Suggested Fix
Add `"BSPositionData"` arm next to the other `NiExtraData` aliases at `blocks/mod.rs:360-379`. Layout per nif.xml:
- `Num Vertices: u32`
- `Vertex Data: Half Float[Num]`

~30 LOC including an entry in `extra_data.rs`. Trivial dispatch addition.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-03)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Other `NiExtraData` aliases at `blocks/mod.rs:360-379` follow same pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: `dispatch_tests.rs` byte-exact test; corpus regression — zero NiUnknown for `BSPositionData` post-fix
- [ ] **ALLOCATE_VEC**: Use `allocate_vec` for the half-float array (file-driven count)
