# NIF-PERF-04: ImportedScene Vecs grow from default capacity — Vec realloc churn during walk

## Description

`import_nif_scene_impl`, `import_nif_with_collision_impl`, and `import_nif_impl` each construct an `ImportedScene` (or local Vec output) with `nodes: Vec::new()`, `meshes: Vec::new()`, etc. The walker pushes one entry per traversed node/mesh, so for a NIF with N traversal hits the Vec doubles ~log2(N) times — every doubling is a fresh allocation + memcpy of the full prefix.

The total walked-block count is bounded above by `scene.blocks.len()` (every node block produces at most one ImportedNode entry, every shape one ImportedMesh). Pre-sizing the Vecs from `scene.blocks.len()` would eliminate the realloc churn.

## Location

`crates/nif/src/import/mod.rs:666-673`, `:846-853`, `:753-761`

## Impact

For a 1000-block NIF (e.g. Megaton interior), ImportedNode growth path is 0→4→8→16→32→64→128→256→512→1024 = **10 reallocations per Vec**, ~2 KB total memcpy waste per Vec. Across the 8 Vecs in ImportedScene + per-mesh internal Vecs (positions, normals, tangents, uvs, indices, colors, skin), peak realloc cost per cell load is ~50-200 KB of unproductive memcpy.

Modest impact (~0.1-0.3 ms per cell), filed for completeness.

## Suggested Fix

```rust
let cap = scene.blocks.len();
ImportedScene {
    nodes: Vec::with_capacity(cap),
    meshes: Vec::with_capacity(cap / 4),  // ~25% of blocks are shapes
    // ... other Vecs sized similarly to expected yield
}
```

Most of these Vecs over-allocate slightly relative to actual yield (some blocks are filtered by `is_editor_marker` / APP_CULLED / unsupported subclasses) but trading some VM commit for zero realloc churn is the right tradeoff at typical NIF sizes.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Per-mesh Vecs (positions, normals, etc. inside `import/mesh.rs`) — verify they already pre-size from vertex_count (most do via `read_*_array(n)` which sizes exactly). Only mod.rs collection Vecs need this fix.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing import tests cover correctness; no behavioral change

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` — NIF-PERF-04