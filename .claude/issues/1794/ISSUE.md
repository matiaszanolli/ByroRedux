# PERF-D4-NEW-01: Per-frame bone_world fill + upload is fixed-stride O(used_slots × 144) — pays the full MAX_BONES_PER_MESH reservation per skinned mesh every frame; the code comment defers to an already-closed milestone

**Issue**: #1794
**Labels**: medium,renderer,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-01)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-01)

## Location
`crates/renderer/src/vulkan/scene_buffer/upload.rs:178-256` (`upload_bone_worlds` + `record_bone_world_copy`), `byroredux/src/render/skinned.rs:131-181`, `byroredux/src/render/mod.rs:308`, `crates/core/src/ecs/components/skinned_mesh.rs:52` (`MAX_BONES_PER_MESH = 144`)

## Description
Each `SkinnedMesh` slot occupies a fixed 144 × 64 B = 9216 B stride in `bone_world`, regardless of actual bound-bone count. Per-frame cost is three-fold, all O(used_slots × 144): CPU `bone_world.clear()` + `resize(required_slots, IDENTITY)` re-fills the whole array from empty every frame (`render/mod.rs:308`, `skinned.rs:131`); host-visible staging memcpy + flush of the full range (`upload.rs:190`); GPU `cmd_copy_buffer` of the same bytes plus a `skin_palette.comp` dispatch sized from it. Per-entity writes are bounded by `min(skin.bones.len(), 144)`, but the allocation, upload byte-count, and copy are the full 144-stride per used slot. Untracked debt: the in-code comment defers to "variable-stride packing (M29.5)", but ROADMAP marks M29.5 Closed with a narrower scope — GPU palette dispatch only — so no live tracker owns the packing work.

## Evidence
`skinned.rs:131` — `required_slots = (max_used_slot()+1) * MAX_BONES_PER_MESH`; resize always fills from empty since `bone_world` is cleared at `render/mod.rs:308`; `upload.rs:190` sizes byte count from the full strided array length.

## Impact
Scales with skinned-entity density, not bone count. At the project's own measured 260-entity FNV workload: ≥261 slots × 9216 B ≈ 2.4 MB/frame ≈ 144 MB/s sustained host-write + flush + GPU copy at 60 fps, most of it identity padding. Full-pool worst case (1365 slots) ≈ 12.6 MB/frame ≈ 755 MB/s. No dirty gate applies (animation legitimately changes every frame), so unlike instances/materials this cost is unconditional.

## Related
#1284, M29.5 (closed, narrower scope), memory-budget.md bone-palette row.

## Suggested Fix
Variable-stride packing — prefix-summed offsets sized by each mesh's actual bone count (or quantized buckets). Cheaper interim: build per-slot `vk::BufferCopy` regions covering only `skin.bones.len()` matrices. File a live issue so the debt stops pointing at a closed milestone.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

