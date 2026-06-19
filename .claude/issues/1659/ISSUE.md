**Severity**: LOW · **Dimension**: NPC Equip + FaceGen
**Location**: parser `crates/nif/src/blocks/skin.rs:370-401` (`BsDismemberSkinInstance` + `BodyPartInfo`); import `crates/nif/src/import/mesh/skin.rs:36-44,135-143`
**Status**: NEW (confirmed still present 2026-06-18; documented limitation, never filed)

## Description
`BsDismemberSkinInstance::parse` reads per-partition `part_flag: u16` and `body_part: u16` into `partitions: Vec<BodyPartInfo>`, but `extract_skin_ni_tri_shape`/`extract_skin_bs_tri_shape` read only `inst.base.*` (bone_refs, skeleton_root_ref, data_ref). The `partitions` vector with its dismemberment flags is never surfaced into `ImportedSkin`, so Skyrim NPC armor renders over the full FaceGen body with no slot-based suppression (acknowledged in-code at `skin.rs:28-29`).

## Evidence
- `crates/nif/src/blocks/skin.rs`: `BsDismemberSkinInstance { base, partitions }`; `BodyPartInfo { part_flag: u16, body_part: u16 }` parsed in the `num_partitions` loop.
- `crates/nif/src/import/mesh/skin.rs:38-40` and `:137-139`: both extractors read only `inst.base.bone_refs` / `inst.base.skeleton_root_ref` / `inst.base.data_ref` — `partitions` is dropped.

## Impact
Cosmetic — armored Skyrim NPCs show body/skin clipping through equipped armor at seams. No correctness/UB issue.

## Suggested Fix
Surface `BodyPartInfo` partition flags onto `ImportedSkin` so a future slot-hiding/dismember consumer can hide FaceGen body sub-shapes whose `body_part_type` overlaps an equipped armor's biped slot. Track as Phase B.2.

## Completeness Checks
- [ ] **SIBLING**: Both `extract_skin_ni_tri_shape` and `extract_skin_bs_tri_shape` surface partition flags identically (NI + BS geometry paths)
- [ ] **TESTS**: A regression test pins a `BsDismemberSkinInstance`'s `BodyPartInfo` flags reaching `ImportedSkin`
