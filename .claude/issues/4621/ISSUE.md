# NIF-D4-2026-09-21-02: FO4+/Starfield BSSkin::BoneData bind transforms bypass both the #277 rotation sanitizer and #4549's finite gate

**Issue**: #4621
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: MEDIUM. A defence-in-depth gap on the skinning path; vanilla exposure is measured at zero.
**Dimension**: Geometry Extraction & Import Handoff (skinning)
**Game Affected**: Fallout 4 (`bsver` 130), Fallout 76 (155), Starfield (172+)
**Location**:
- `crates/nif/src/blocks/skin.rs:495-517`: `BsSkinBoneData::parse`, a raw 17-float slice per bone.
- `crates/nif/src/import/mesh/skin.rs:677-688`: `bs_bone_to_inverse_matrix`.
- Consumers: `skin.rs:258-271` (BSTriShape / `BSSkin::Instance`) and `:447-516` (Starfield `BSGeometry`).

## Description
#4549 put the non-finite gate, and #277 put `sanitize_rotation`, inside `NifStream::read_ni_transform` and `read_ni_transform_struct` (`crates/nif/src/stream.rs:752,776`). Together those cover `NiAVObject` and the legacy `NiSkinData` bones.

`BSSkin::BoneData` never goes through either reader. Its rotation, translation and scale are sliced out of a bulk f32 array and turned into the bind-inverse matrix unchecked. So a NaN, infinite or non-orthonormal bone reaches the bone palette, GPU skinning, and the skinned BLAS refit — the same chain #4549 closed for static transforms.

Confirmed at HEAD `ee6d3fb39`: `BsSkinBoneData::parse` (`skin.rs:495`) reads `num_bones`, calls `allocate_vec`, then `read_f32_array(num_bones * 17)` and builds each `BsSkinBoneTrans` directly from the flat chunk with no call to `sanitize_rotation` or any finite check. `grep -rn "sanitize_rotation("` across `crates/nif/src` shows it called only from `stream.rs:752`/`:776` and tests — never from `skin.rs`.

## Evidence (probe)
No vanilla bone is non-finite, and none is changed by `sanitize_rotation`.
- FO4 vanilla + DLC: 166,323 bones.
- FO4 including third-party: 267,328 bones.
- Starfield: 288,208 bones (27 of them carry scale ≠ 1).

## Impact
One corrupt bone poisons every vertex it influences. NaN positions make BLAS triangles inactive, ±inf gives unbounded AABBs, and raster shows exploded meshes. The exposure is corrupt or mod content, which was #4549's premise too.

## Related
#4549 (closed — the sanitizer this path was left out of), #4396/#4397 (closed — the animation-side siblings of the same finite-gate class), #277 (closed — introduced `sanitize_rotation`).

## Suggested Fix
- Apply `sanitize_rotation` and `sanitize_transform_translation_and_scale` per bone in `BsSkinBoneData::parse`, the parse-time boundary #4549 chose.
- Add a NaN-bone unit test.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D4-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other skin/bone readers that bypass `read_ni_transform`)
- [ ] **TESTS**: A regression test pins this specific fix (NaN/inf/non-orthonormal bone fixture)
