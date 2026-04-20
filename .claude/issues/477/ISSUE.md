# Issue #477

FNV-3-L2: CellLoadResult.mesh_count counts cache-misses not spawned meshes

---

## Severity: Low (telemetry)

**Location**: `byroredux/src/cell_loader.rs:1003-1009`

## Problem

```rust
mesh_count: reg.len().saturating_sub(cache_size_at_entry)
```

This computes "new NifImportRegistry entries added during this cell load" — which includes failed parses and excludes cache hits. Repeat cell loads with 100% cache hit report `mesh_count = 0` despite spawning hundreds of entities.

## Impact

Misleading telemetry. A debug session that loads Prospector Saloon twice would show `mesh_count=784` first load, `mesh_count=0` second load, both identical scenes.

## Fix

Either:

1. **Rename** field to `unique_meshes_parsed` (makes current semantics clear).
2. **Redefine** — increment a counter on each `world.insert(entity, MeshHandle)` in `spawn_placed_instances`, expose as `entities_spawned`.

Option 2 is more useful for CI bench (FNV-5-F3 reference Prospector 809 baseline).

## Completeness Checks

- [ ] **TESTS**: Load same cell twice, assert `mesh_count` (or new name) is consistent
- [ ] **SIBLING**: Check all other `CellLoadResult` fields for similar stale-semantics issues

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-3-L2)
