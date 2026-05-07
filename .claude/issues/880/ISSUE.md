# Issue #880 (OPEN): CELL-PERF-02: NPC spawn bypasses NifImportRegistry — every NPC re-parses skeleton + body + head from BSA bytes

URL: https://github.com/matiaszanolli/ByroRedux/issues/880

---

## Description

`spawn_npc` (`byroredux/src/npc_spawn.rs:355-377`, `:399-474`, `:481-622`) extracts and re-parses the same NIF files from the BSA on every NPC, never consulting the process-lifetime `NifImportRegistry` resource (#381) that exists for exactly this case.

`load_nif_bytes_with_skeleton` (`byroredux/src/scene.rs:1296-1321`) — the worker called by `spawn_npc` for skeleton + body + hand + head NIFs — has no cache consultation: it goes straight to `byroredux_nif::parse_nif(data)` every time.

For Megaton's ~40 NPCs × ~7 NIFs each (skeleton + upperbody + lefthand + righthand + head + …) = **~280 redundant parses per cell**. The skeleton path (`meshes\characters\_male\skeleton.nif`) is THE SAME for every male NPC.

## Evidence

```rust
// npc_spawn.rs:355-377 — extract + parse runs PER NPC, no cache check
let skel_path = humanoid_skeleton_path(game, gender)?;
let skel_data = match tex_provider.extract_mesh(skel_path) {
    Some(d) => d,
    None => { /* warn + skip */ }
};
let (_skel_count, skel_root, skel_map) = load_nif_bytes_with_skeleton(
    world, ctx, &skel_data, skel_path, tex_provider,
    mat_provider.as_deref_mut(), None, None,
);
```

```rust
// scene.rs:1296-1305 — load_nif_bytes_with_skeleton has no cache consultation
let scene = match byroredux_nif::parse_nif(data) {
    Ok(s) => s,
    Err(e) => { /* error + return empty */ }
};
let imported = {
    let mut pool = world.resource_mut::<StringPool>();
    let mut imported = byroredux_nif::import::import_nif_scene_with_resolver(
        &scene, &mut pool, Some(tex_provider),
    );
    // ...
};
```

Compare with `cell_loader.rs::load_references` which DOES check `NifImportRegistry::get` first (the three-tier `pending_new` shadow → registry read-lock → parse + insert pattern from #523).

## Why it matters

`NifImportRegistry` (#381) already exists, is sized for this exact use case, and survives cell transitions. NPC spawn predates the registry's adoption and never got plumbed through. For interior cells dominated by NPCs (TestQAHairM with 31 NPCs / 61 refs is the canonical audit case) this is the per-cell stall.

Compounds with **#879 (CELL-PERF-01)** and **CELL-PERF-03** (texture upload budget). All three stack on the cell-load critical path.

## Proposed Fix

Route `load_nif_bytes_with_skeleton` through `NifImportRegistry`:

1. Lookup keyed on lowercased model path (same key as `cell_loader.rs::load_references`)
2. Skeleton-bearing meshes (with `external_skeleton`) need careful handling for the `node_by_name` map — the parsed scene + meshes themselves are content-addressable, but the per-spawn `skel_map` returned from `load_nif_bytes_with_skeleton` is per-instance. Cache the parse + import; rebuild the per-instance map cheaply from cached node data.
3. The shared idle-clip pattern at `npc_spawn.rs:148-153` (`get_by_path` fast-path) is the local precedent for cache routing through this code path.

## Cost Estimate

Per-cell, NPC-dense interiors. Megaton: ~280 redundant parses/cell. Skeleton parse alone is ~50 KB × 40 NPCs = ~2 MB redundant parse memory + ~10 ms parse wall-clock × 40 (post-hot-cache) ≈ 100s of ms wall-clock cost.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit `byroredux/src/scene.rs` for other `load_nif_bytes*` functions that bypass the registry; verify `npc_spawn.rs::resolve_actor_meshes` (head/body/hand resolution) uses the cached path
- [ ] **DROP**: Cache entries are process-lifetime (#381 invariant); ensure the per-NPC `skel_map` rebuild doesn't leak transient String allocs
- [ ] **LOCK_ORDER**: `NifImportRegistry` already takes its own RwLock; preserve the existing TypeId-sorted acquisition order
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test — spawn 10 NPCs sharing the same skeleton, count `parse_nif` calls, assert exactly 1 (cold) or 0 (hot-cached); cell-transition test must continue to pass

## dhat Gap

Allocation impact is real (~2 MB redundant parse memory per Megaton load); needs dhat for quantitative regression guard. Wall-clock dominates; profile via `tracing` (file separate "wire `tracing` for cell-load critical path" follow-up).

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-02)
- Pairs naturally with: #879 (CELL-PERF-01), CELL-PERF-03 (cell-load trio)
- Builds on: #381 (NIF cache process-lifetime), #523 (three-tier lookup)
- Adjacent correctness: #841 (M41-PHASE-1BX body-skinning artifact under same path)
