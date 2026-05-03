**Severity**: LOW
**Dimension**: CPU Allocation Hot Paths
**Source**: AUDIT_PERFORMANCE_2026-05-01.md

## Locations
- [crates/renderer/src/vulkan/context/mod.rs:244-315](../../tree/main/crates/renderer/src/vulkan/context/mod.rs#L244-L315) (`to_gpu_material`)
- [byroredux/src/render.rs:786](../../tree/main/byroredux/src/render.rs#L786), [byroredux/src/render.rs:932](../../tree/main/byroredux/src/render.rs#L932) (call sites — `cmd.material_id = material_table.intern(cmd.to_gpu_material());`)
- [crates/renderer/src/vulkan/material.rs:306-313](../../tree/main/crates/renderer/src/vulkan/material.rs#L306-L313) (`MaterialTable::intern` early-return path)

## Description

For every DrawCommand, `cmd.to_gpu_material()` is called before `material_table.intern(...)`. The construction is a flat 60-field copy (~50 ns per call), but happens unconditionally — including for the ~97% of draws that hit the dedup early-return path in `intern`.

Total cost on Prospector baseline (1200 DrawCommands / 30 unique materials):
- 1200 × `to_gpu_material()` construction = ~60 µs/frame
- 1200 × 272 B byte-hash + HashMap lookup = ~50-100 µs/frame
- **Combined: ~150 µs/frame** in the `build_render_data` hot path.

## Impact

Quantifiable but bounded. Below the signal floor today; matters if `build_render_data` enters a parallel-scheduler design (D4-M1 in the 04-20 audit) where its 2 ms total budget is parallelized — the per-DrawCommand dedup work then becomes a serialization point on `material_table.intern`.

## Suggested Fix

Two options, ordered by complexity:

### Option (a) — Producer-side dedup (preferred long-term)

Intern materials once at `MaterialInfo`-resolution time (cell-load-once), stamp a stable `material_id` upstream, and have `DrawCommand` carry the id directly. Drops the per-DrawCommand `to_gpu_material()` + `intern()` calls entirely. Per-frame `MaterialTable::intern` becomes a no-op (just collect the materials produced at cell load).

Larger refactor (~200-400 lines touching `MaterialInfo` resolution, `DrawCommand` schema, NIF import, BGSM merge, render.rs DrawCommand emission). Right answer eventually, especially under parallel scheduling.

### Option (b) — Hash-cache on DrawCommand (preferred short-term)

Add `material_hash: u64` field on `DrawCommand`, computed at construction (one xxh3 of the same fields `to_gpu_material()` reads, ~30 ns). `intern` becomes:

```rust
pub fn intern_by_hash(
    &mut self,
    hash: u64,
    material_factory: impl FnOnce() -> GpuMaterial,
) -> u32 {
    if let Some(&id) = self.index.get(&hash) {
        return id;
    }
    let mat = material_factory();
    let id = self.materials.len() as u32;
    self.materials.push(mat);
    self.index.insert(hash, id);
    id
}
```

`to_gpu_material()` only runs on the miss path (~3% of calls). Per-frame cost drops from ~150 µs to ~30 µs. ~50 lines of code.

This option also aligns with the `R1-N5` deferred item from the renderer audit (`HashMap<GpuMaterial, u32>` → `HashMap<u64, u32>`, 33× key-storage reduction).

## Recommendation

Ship option (b) now (50 lines, immediate win). Plan option (a) for whenever the parallel scheduler design lands.

## Completeness Checks

- [ ] **UNSAFE**: N/A — both options are safe Rust
- [ ] **SIBLING**: Verify the `MaterialTable::intern` test suite still passes after the API change; the 9 existing tests in `material.rs` cover dedup correctness
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a collision-handling test for option (b) — two distinct materials whose `to_gpu_material()` outputs hash to the same u64. Real xxh3 collisions require crafted input but the test should still pass cleanly under either insertion order. Consider a `debug_assert` in `intern_by_hash` that the existing material at the looked-up id byte-equals the newly produced one (collision = hard fail in debug, silent miscoloring in release).
