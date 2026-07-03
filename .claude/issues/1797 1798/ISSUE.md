# #1797: D6-03: All skinned BLAS builds/refits in a frame are serialized on one shared scratch buffer

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D6-03)
**Labels**: bug, renderer, medium, vulkan, performance
**State**: OPEN

## Location
`crates/renderer/src/vulkan/acceleration/blas_skinned.rs:417` (per-refit barrier), `:278-283` (per-build barrier in first-sight batch); consumed at `context/draw.rs:1835-1899`

## Description
`blas_scratch_buffer` is a single allocation sized to the max single-build demand. Because every skinned BLAS build/refit reuses the same scratch address, the Vulkan spec requires an AS_WRITE→AS_WRITE barrier between each pair — N dirty skinned entities produce N fully serialized AS builds per frame, each self-emitting the barrier. Small skinned BVHs (5-15K triangles per body part) individually underutilize the GPU; back-to-back serialization with full-pipe AS-stage drains prevents overlap. The barrier correctness chain (#642/#644/#983/#1095/#1140/#1300) is complete and intact; nothing tracks the throughput cost of the serialization.

## Evidence
`refit_skinned_blas`'s first statement is `record_scratch_serialize_barrier`; the refit loop calls it once per dirty entity; scratch sizing is grow-to-max-single-build, not per-build slots.

## Impact
GPU skin-chain time scales linearly with dirty-entity count with no overlap. On crowd scenes the `gpu_skin_blas_refit_ms` bracket absorbs the full serial sum plus per-barrier drain. Idle crowds are already saved by #1195/#1196; this is the moving-crowd ceiling only. Confidence: quantify before fixing — the #1194 GPU timer brackets exist for this (`skin.coverage` → `gpu_skin_blas_refit_ms` vs `refits_attempted`).

## Related
#642, #983, #1300 (correctness chain), #1194 (measurement hook).

## Suggested Fix
Sub-allocate the scratch buffer into K aligned slots, round-robin builds, emit the serialize barrier only every K builds; K=1 fallback under memory pressure.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---

# #1798: D7-NEW-01: Interior NPC/REFR spawn loop has no per-frame or per-NPC budget, unlike the exterior streaming path

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D7-NEW-01)
**Labels**: bug, import-pipeline, medium, performance
**State**: OPEN

## Location
`byroredux/src/cell_loader/references.rs:224` (`load_references` ref loop) → `byroredux/src/npc_spawn.rs` (`spawn_npc_entity` @319, `spawn_prebaked_npc_entity` @354); driven from `byroredux/src/cell_loader/load.rs:301` (`load_cell_with_masters`) and `cell_loader/transition.rs:237` (`load_interior_cell`)

## Description
The exterior streaming path has an explicit per-frame cell-count budget (`MAX_CELLS_SPAWNED_PER_FRAME = 2`, `main.rs:1181`, enforced `main.rs:1212-1227`) to avoid frame-time spikes from bulk main-thread spawning. No equivalent exists for interior transitions: `load_references` iterates every `PlacedRef` in one synchronous `for placed_ref in refs` pass, spawning each static + NPC inline — no batching, no yield, no cap. Nuance: even the exterior "budget" is cell-granularity — each individual cell's `load_references` call (also reached via `cell_loader/exterior.rs:403`) is itself an unbudgeted burst. So the true gap is "no sub-cell spawn budget anywhere"; the interior path additionally lacks even the cell-level throttle because it is a single-cell blocking load. `spawn_npc_entity` makes ~28 synchronous NIF-load call sites per NPC.

## Evidence
`main.rs` documents the exterior budget's rationale but it is never applied to `load_references`/`spawn_npc_entity`; no `Instant::now()` timing exists in `cell_loader/load.rs` or `references.rs` to even measure the stall. Same-root-cause sibling: `references.rs` ends the cell load with a single synchronous batched texture flush + fence-wait (`flush_pending_uploads`, ~`references.rs:969-987`) — an intentional batching win (#881), but on top of the unbudgeted spawn loop it means a large interior cell pays its entire NIF-parse + spawn + BLAS-build + texture-upload cost in one frame with a hard fence stall and no yield.

## Impact
An unmeasured multi-hundred-ms-to-multi-second frame-time spike on every interior transition into an NPC-dense cell (door walk-in, save-load-apply reload, fast travel). Architecturally distinct from and precedes #1698 (a post-load Rapier/ECS-scheduler settle-storm confirmed by `docs/audits/AUDIT_RUNTIME_2026-06-26.md`) — the two compound on entry to a crowded cell (load-time freeze, then post-load stall) but are separate mechanisms.

## Related
Existing: #1698 (adjacent, not a duplicate — post-load); #881 (the batched texture flush).

## Suggested Fix
Extend a `MAX_CELLS_SPAWNED_PER_FRAME`-style budget to interior NPC spawning by chunking `load_references`'s ref loop across frames with a resumable cursor; at minimum add `Instant::now()` timing around the NPC-spawn portion so the cost becomes visible before investing in chunking.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other hot-path loops / other dirty gates)
- [ ] **TESTS**: A regression test pins this specific fix
