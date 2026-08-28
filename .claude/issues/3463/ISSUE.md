# #3463 — PERF-2026-08-27b-01: memory-budget.md's vertex/index-pool row does not carry #3298's deliberate two-generation VRAM peak

**Labels**: medium, performance, memory, renderer, documentation, doc-rot
**Filed**: 2026-08-27 from `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`
**HEAD at audit**: `969d81c8`

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` — finding `PERF-2026-08-27b-01`
**Severity**: MEDIUM · **Dimension**: GPU Memory Pressure
**Location**: `docs/engine/memory-budget.md:455-466` and `:548` vs `crates/renderer/src/mesh.rs:56-70` and `:1246-1270`

## Description

`#3298` (`ae7179a3`) made the global geometry SSBO rebuild resumable by allocating the **full-size replacement generation while the old one is still bound and serving draws**. The code says so explicitly. The authoritative VRAM ledger does not — and `/audit-performance`, `/audit-renderer` and `/audit-safety` are all told to cite `memory-budget.md` rather than re-derive it, so a budget decision made from that page today is made against a peak that no longer exists.

## Evidence

The code states the trade-off (`crates/renderer/src/mesh.rs:66-70`):

```
/// That means two full geometry SSBO generations are resident in
/// device-local memory at once for the rebuild's duration. This is an
/// accepted trade-off (#3298): it smooths a multi-hundred-ms atomic
/// stall into several bounded per-frame chunks, at the cost of a
/// temporarily higher VRAM high-water mark.
```

and allocates both up front (`crates/renderer/src/mesh.rs:1256-1259`):

```rust
match Self::try_allocate_empty_geometry_buffers(
    device, allocator, vertex_size, index_size, rt_usage,
) {
    Ok((new_vertex_buffer, new_index_buffer)) => {
```

The old generation is only released at swap-in (`:1447-1452`). Meanwhile the ledger carries a single-generation figure:

```
| VERTEX_POOL_SOFT_CAP | 4 M vertices | ~416 MB (104 B/vertex) |
| INDEX_POOL_SOFT_CAP  | 16 M indices | ~64 MB (4 B/index)     |
...
| Vertex / index pools | ~208 MB | ~1.66 GB cap |
```

Two consumers are missing from the "Peak" column:

1. **The duplicate generation.** At the documented ~208 MB typical the peak is ~416 MB, not ~208 MB; at `VERTEX_POOL_HARD_CAP` + `INDEX_POOL_HARD_CAP` (~1.92 GB) the peak is ~3.84 GB — on its own past the page's stated `< 4 GB` engine budget target, before textures, BLAS or the froxel grid.
2. **A session-retained 64 MiB `CpuToGpu` staging buffer.** The rebuild lazily constructs its own `StagingPool` (`crates/renderer/src/mesh.rs:1355-1357`) at `DEFAULT_STAGING_BUDGET_BYTES` = 128 MiB (`crates/renderer/src/vulkan/buffer.rs:53`). Each chunk acquires a buffer of exactly `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB, and because 64 MiB is inside the 128 MiB retention budget the pool **keeps** it after the rebuild finishes, for the process lifetime. Pre-`#3298` the atomic path acquired one staging buffer the size of the whole pool, which normally *exceeded* the budget and was therefore evicted on release — so this is a new steady-state resident allocation, not a pre-existing one.

Verified at HEAD `969d81c8`: `memory-budget.md:548` still reads `| Vertex / index pools | ~208 MB | ~1.66 GB cap |`, and the page contains no "geometry SSBO rebuild" row at all.

## Impact

Not a runtime fault on the 12 GB dev card. The blast radius is decision-quality: `memory-budget.md` is cited as authoritative by `/audit-performance`, `/audit-renderer` and `/audit-safety`, and the same class of drift on the same page was filed and fixed three days ago as #3117 (volumetrics row understating the froxel grid ~24x, breaking the documented 4 GB ceiling). The 2x geometry peak is the largest single unrecorded VRAM consumer on the page.

## Related

`#3298`, `#3372`, `#2374`, #3117 (same doc, same drift class). Other live doc-rot on the same page: #3447 (GpuInstance/GpuCamera rows), #3431 (Ruffle wgpu device absent) — different rows, no overlap.

**Cross-reference #3443** (`REN-2026-08-27-D5-01`, the concurrent `/audit-renderer` HIGH on `mesh.rs:1244-1288` routing around `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`). **Fixing #3443 does not close this**: restoring the idle-threshold gate only forces the single-generation path *above* 256 MiB; every rebuild under that threshold still holds two generations, which is exactly the ~208 MB typical case the ledger describes. The 64 MiB retained staging buffer likewise survives that fix.

## Suggested Fix

Add a "Global geometry SSBO rebuild" row to the VRAM Rough Budget table with `2 x pool + 64 MiB` in the Peak column, and a note under the Mesh Registry section pointing at `GeometryRebuildInProgress`'s own doc comment as the source of truth.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other rows on `memory-budget.md` that describe a resource with a transient double-buffered generation)
- [ ] **TESTS**: A regression test pins this specific fix
