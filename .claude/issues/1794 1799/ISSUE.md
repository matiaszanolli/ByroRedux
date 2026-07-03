# #1794: PERF-D4-NEW-01: Per-frame bone_world fill + upload is fixed-stride O(used_slots × 144)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D4-NEW-01)
**Labels**: bug, renderer, medium, performance
**State**: OPEN

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

---

# #1799: PERF-D5-NEW-01: Legacy 16-slot WRS reservoir arrays stay live on the default ReSTIR path

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-01)
**Labels**: bug, renderer, medium, pipeline, performance
**State**: OPEN

## Location
`crates/renderer/shaders/triangle.frag:1967-1983` (declaration + unconditional init), `:2237-2250` (legacy streaming writes), `:2558-2673` (legacy pass-2 reads)

## Description
#1369 retired a larger reservoir array (dropping the third `resRadiance` array, 320 B → 128 B) because per-thread reservoir storage was "the dominant per-thread footprint suppressing WRS occupancy," landing at `resLight[16]` + `resWSel[16]`. Session 49 then made ReSTIR-DI (a single scalar reservoir) the default shadow path, but kept the 16-slot legacy WRS arm compiled in for a runtime A/B toggle (`DBG_DISABLE_RESTIR`, a dynamically-uniform branch, not a compile-time constant). The arrays are declared and zero-initialized before `useRestir` is even computed (`:1980-1983` init vs `:1998` compute), so the compiler must budget their registers/local memory on every invocation — including the ~100% of production frames that take the ReSTIR path and never read them.

## Evidence
`NUM_RESERVOIRS = 16` at `:1967`; unconditional init loop `:1980-1983`; `useRestir` at `:1998` from a runtime-uniform flag; only the `!useRestir` path touches the arrays afterward.

## Impact
Up to ~32 extra live registers (or spilled local bytes) per fragment thread in a shader that already carries the full RT uber-path — silently re-eroding a portion of the #1369 occupancy win on a path that gets zero benefit. Blast radius: every lit fragment, every frame, every game. (Footprint smaller than a naive read: #1369 already halved the array set.) Confidence: MEDIUM — storage-lifetime analysis is code-verified; the magnitude of the occupancy hit needs Nsight/RenderDoc SASS confirmation. ALU/register-only, no pipeline-state/barrier change.

## Related
#1369, `DBG_DISABLE_RESTIR` toggle.

## Suggested Fix
Promote the legacy-WRS arm to a compile-time toggle through the existing generated-constants channel (the mechanism #1758 used for skin workgroup size); A/B then costs a shader recompile instead of taxing every production frame.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader passes / other reservoir arrays)
- [ ] **TESTS**: A regression test pins this specific fix
