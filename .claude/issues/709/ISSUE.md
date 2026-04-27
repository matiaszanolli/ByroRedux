# NIF-D5-02: Starfield SkinAttach block undispatched — paired 1:1 with BSGeometry

URL: https://github.com/matiaszanolli/ByroRedux/issues/709
Labels: enhancement, nif-parser, high

---

## Severity: HIGH

## Game Affected
Starfield (skinned content + face meshes)

## Location
- `crates/nif/src/blocks/mod.rs` — no dispatch arm

## Description
Not in nif.xml (Starfield-era addition). Appears 1:1 with `BSGeometry` on character / face meshes — every FaceMeshes BA2 NIF has exactly one `BSGeometry` and one `SkinAttach` per facegen mesh.

## Evidence
2026-04-26 corpus sweep:
- `Starfield - Meshes01.ba2` — 3,341 occurrences
- `Starfield - FaceMeshes.ba2` — 13,713 occurrences

The 1:1 ratio with `BSGeometry` on `FaceMeshes.ba2` is the structural signal — both are paired on every skinned mesh.

## Impact
Skinning broken on Starfield even if NIF-D5-01 (`BSGeometry`) lands. Without `SkinAttach` parsing, bone palette / partition table data is lost.

## Suggested Fix
Reverse-engineer in tandem with NIF-D5-01. Likely carries the bone palette / partition table that historically lived in `BSDismemberSkinInstance` + `NiSkinPartition`. Multi-session work via `examples/trace_block.rs` against a captured SF face NIF.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-02)
- Bundle: NIF-D5-01 BSGeometry, NIF-D5-08 BoneTranslations

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Compare against NiSkinInstance + NiSkinPartition + BSDismemberSkinInstance for inheritance patterns
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact dispatch test; corpus regression on FaceMeshes.ba2
