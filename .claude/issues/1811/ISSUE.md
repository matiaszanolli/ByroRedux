# D6-04: Fixed per-frame skinning costs run even on fully-clean frames

**Issue**: #1811
**Labels**: low,renderer,vulkan,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D6-04)

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-04)

## Location
`byroredux/src/render/mod.rs:308,328`; `byroredux/src/render/skinned.rs:153-186`; `crates/renderer/src/vulkan/context/draw.rs:2630-2732`

## Description
The `pose_dirty` gate (`draw.rs:1712`) guards only the per-entity GPU compute dispatch. When `pose_dirty` is empty and no bind-inverse uploads are pending, the frame still pays, ungated: CPU identity refill of the whole live `bone_world` range + full per-entity bone-matrix reconstruction (`gt_q.get` + `to_matrix` per bone, `skinned.rs:153-181`), the pose-hash recompute over that slice (inherently un-gateable — it is the dirtiness signal), the full-range staging memcpy + device copy, and a full-range `skin_palette.comp` dispatch. Pass-4 `pool.sweep` (`skinned.rs:186`) also runs every frame.

## Evidence
`skinned.rs:153-186` unconditional per-frame reconstruction + sweep; `pose_dirty` gate (`draw.rs:1712`) only wraps the GPU dispatch, not the CPU rebuild/upload.

## Impact
For S live slots, ≈S × 9.2 KB per frame per sub-step — well under a millisecond at realistic slot counts (LOW), relevant only on dense crowd cells. The avoidable part is the matrix rewrite + upload + copy + dispatch on clean frames (the hash recompute is fundamental to the scheme).

## Related
#1379, #1195, #1284.

## Suggested Fix
Track frames-since-last-dirty and skip upload+copy+dispatch when ≥`MAX_FRAMES_IN_FLIGHT` with no pending uploads; stop clearing `bone_world` every frame, re-seeding identity only for freshly (re)allocated slots.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

