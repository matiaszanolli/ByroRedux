## Description

Two `// TODO: thread StagingPool through … (#242)` markers planted in commit `f97df27` (2026-04-13) — the **same commit** that closed `#242` (PERF-04-11-M5: `build_geometry_ssbo` bypasses StagingPool). The optimization issue closed but the consumer side never landed. Both call sites are reachable from shipped CLI flags / per-frame draw paths.

This is a 31-day-old live half-implementation, not a forgotten marker.

Verified live as of 2026-05-14:
```
byroredux/src/scene.rs:477:        None, // TODO: thread StagingPool through scene load (#242)
byroredux/src/main.rs:1151:                            None, // TODO: thread StagingPool through frame loop (#242)
```

Lifted from Dim 1 of `AUDIT_TECH_DEBT_2026-05-14.md` (TD1-001 + TD1-002 — also cross-referenced as TD5-005 stub side of the same gap).

## Findings consolidated

### TD1-001 — Scene loader passes `None` for StagingPool
- **File**: `byroredux/src/scene.rs:477`
- **Reachability**: every `cargo run -- mesh.nif`, `--bsa`, `--esm`, `--cell`, `--grid`, `--tree` invocation routes through `setup_scene` → `build_geometry_ssbo(.., None)`, forcing transient staging allocations on the geometry-upload path.

### TD1-002 — Per-frame geometry rebuild passes `None` for StagingPool
- **File**: `byroredux/src/main.rs:1151`
- **Reachability**: the branch fires whenever `mesh_registry.is_geometry_dirty()` (BLAS replacement, mesh streaming, NPC spawn). Hit every frame with dirty geometry; per-frame allocator churn instead of pool reuse.

## Proposed fix

`App` already owns a `StagingPool` (passed through `VulkanContext::new` for texture upload). Thread it through:

1. Change `setup_scene` signature to take `staging_pool: Option<&mut StagingPool>` (or `&mut StagingPool` if every call site has one).
2. Plumb the pool through `byroredux/src/scene.rs:477` → `mesh_registry.build_geometry_ssbo(..., Some(staging_pool))`.
3. In `byroredux/src/main.rs:1151`, pass `&mut self.staging_pool` into the rebuild branch.
4. Delete both `// TODO: thread StagingPool` markers.

Verify against the pool's `flush_if_needed` contract — staging upload completion has to land on the right command buffer before BLAS build reads the geometry SSBO. The existing texture path is the proven shape.

## Completeness Checks

- [ ] **UNSAFE**: none (StagingPool API is safe-Rust over `vk::CommandBuffer`)
- [ ] **SIBLING**: search for any other `// TODO: thread StagingPool` markers (`grep -rn 'thread StagingPool' byroredux/ crates/`) — verify only these two sites exist.
- [ ] **DROP**: no Vulkan object lifecycle change; StagingPool's own Drop owns the staging buffers.
- [ ] **LOCK_ORDER**: N/A — no RwLock scope change.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: add a regression that asserts `build_geometry_ssbo` consumes from the pool (e.g., counter on the pool before/after, or assertion that no fresh `gpu-allocator` block is allocated when the pool has capacity).

## Effort
small (~30 LOC across 3 sites: scene.rs signature, main.rs frame loop, build_geometry_ssbo arg threading)

## Cross-refs

- Audit report: `docs/audits/AUDIT_TECH_DEBT_2026-05-14.md` (Dim 1 + Dim 5 cross-ref)
- Closed driver: #242 (PERF-04-11-M5)
- Prior audit: `docs/audits/AUDIT_TECH_DEBT_2026-05-13.md` (TD1-001/002 + TD5-005, all unresolved)
