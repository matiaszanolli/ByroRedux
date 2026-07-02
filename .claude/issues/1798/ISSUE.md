# D7-NEW-01: Interior NPC/REFR spawn loop has no per-frame or per-NPC budget, unlike the exterior streaming path

**Issue**: #1798
**Labels**: medium,import-pipeline,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (D7-NEW-01)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D7-NEW-01)

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

