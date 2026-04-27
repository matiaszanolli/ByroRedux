# NIF-D5-04: FO4 / FO76 BSEyeCenterExtraData undispatched — head meshes lose eye anchors

URL: https://github.com/matiaszanolli/ByroRedux/issues/720
Labels: enhancement, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
FO4, FO76

## Location
- `crates/nif/src/blocks/mod.rs` — no dispatch arm

## Description
nif.xml line 8369: `<niobject name="BSEyeCenterExtraData" inherit="NiExtraData" module="BSMain" versions="#FO4# #F76#">`. Stores eye-pivot positions for FaceGen / dialogue camera framing.

## Evidence
2026-04-26 corpus sweep:
- `Fallout4 - Meshes.ba2` — 623 blocks
- `SeventySix - Meshes.ba2` — 2 blocks (FO76 mostly uses Starfield-style face meshes)
- Total: 625 blocks

## Impact
Dialogue / cinematic eye-tracking points to NIF origin, not the actual eye centroid. Visible as cross-eyed NPCs in close-ups.

## Suggested Fix
Add `"BSEyeCenterExtraData"` arm next to other `NiExtraData` aliases at `blocks/mod.rs:360-379`. Layout per nif.xml:
- `Num Floats: u32`
- `Float[Num]` (typically 4 floats — left+right eye XY in mesh space)

~20 LOC. Trivial dispatch addition.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-04)
- Bundle-adjacent: #710 (NIF-D5-03 BSPositionData — same `NiExtraData` alias pattern)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Other `NiExtraData` aliases at `blocks/mod.rs:360-379` follow same pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: `dispatch_tests.rs` byte-exact test; corpus regression — zero NiUnknown for `BSEyeCenterExtraData` post-fix
- [ ] **ALLOCATE_VEC**: Use `allocate_vec` for the float array (file-driven count)
