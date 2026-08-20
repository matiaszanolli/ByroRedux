# PERF-D4-01: the local-fog cluster rebuild is O(grid capacity), not O(live volumes)

**Issue**: #3133 — https://github.com/matiaszanolli/ByroRedux/issues/3133
**Labels**: `medium,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: MEDIUM
**Dimension**: SSBO Sizing & Per-Frame Upload
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D4-01)

## Location

- `crates/renderer/src/vulkan/volumetrics.rs` — `build_fog_volume_clusters` prologue (`:380-401`), the two uploads (`:2079-2088`), the `Box`ed staging arrays (`:784-785`)
- Consumer: `crates/renderer/shaders/volumetrics_inject.comp:519-536, 612-622`

## Description

Every frame in which at least one `GpuFogVolume` is visible, `build_fog_volume_clusters` unconditionally does:

```rust
entries.fill(GpuFogClusterEntry::default());   // 4096 × 8 B  = 32 KB memset
indices.fill(0);                               // 32768 × 4 B = 128 KB memset
for (cluster_index, entry) in entries.iter_mut().enumerate() {
    entry.offset = (cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER) as u32;
}                                              // 4096-iteration scalar loop
```

and the caller then `write_mapped`s **both whole arrays** (160 KB) into `CpuToGpu` host-visible memory, regardless of how many volumes were actually clustered.

**Two of the three steps are provably dead work:**

1. `entry.offset` is a **pure function of `cluster_index`** and never varies. It could be written once at pipeline construction and never touched again.
2. `indices.fill(0)` is **unnecessary**: the shader reads `fogClusterIndices[cluster.offset + i]` only for `i < min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER)` (`volumetrics_inject.comp:615-618`), and additionally rejects `volumeIndex >= fogVolumeCount` (`:619`). Slots past `count` are never observed.

## Evidence

Confirmed at HEAD:
```
crates/renderer/src/vulkan/volumetrics.rs:380:fn build_fog_volume_clusters(
crates/renderer/src/vulkan/volumetrics.rs:396:    entries.fill(GpuFogClusterEntry::default());
crates/renderer/src/vulkan/volumetrics.rs:397:    indices.fill(0);
crates/renderer/src/vulkan/volumetrics.rs:399:        entry.offset = (cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER) as u32;
```

Sizing: `FOG_VOLUME_CLUSTER_DIM = 16` (`crates/renderer/src/shader_constants_data.rs:438`) → `FOG_VOLUME_CLUSTER_COUNT = 4096`; `MAX_FOG_VOLUMES_PER_CLUSTER = 8` (`:441`) → `FOG_VOLUME_INDEX_COUNT = 32768`. Both staging arrays are `Box`ed fixed-size arrays — correctly **allocated** once, but fully **rewritten and re-uploaded** per frame. Per frame that is ~160 KB of memset plus ~160 KB of write-combined PCIe traffic plus a 4096-iteration scalar loop, for a `volume_count` that `MAX_GPU_FOG_VOLUMES` already bounds well below the grid capacity.

**Fog volumes are not rare.** `fog.rs:317` (`fog_volume_from_particle`), `:420` (`explosion_volume_from_particle`), `:474` (`fire_volume_from_particle`) and `:616` (`fog_volume_from_mesh`) classify ordinary torch / lantern / campfire / smoke emitters, so the non-empty branch is the **normal case in a lit interior**.

## Impact

A fixed per-frame CPU + upload tax proportional to **grid capacity** rather than **scene content**. Not a leak and not a hitch, but it is exactly the pattern `/audit-performance` Dimension 4's checklist names (*"a full-capacity memcpy of a near-empty buffer is the waste"*) — and unlike most such cases, the read bound is already enforced shader-side, so **the fix cannot change rendering**.

## Suggested Fix

(a) Initialise `entry.offset` once in `VolumetricsPipeline::new` and drop the per-frame loop.
(b) Delete `indices.fill(0)`.
(c) Touch only the clusters a volume's AABB actually intersects — reset `count` for those via the same range walk that fills them, or keep a small `Vec<u32>` of touched cluster indices from the previous frame and clear just those.

(a) and (b) are safe, mechanical and independently testable; (c) is the larger change.

## Related

- #2242 (`REN-D16-04`, CLOSED) documented why the **empty** branch may skip these uploads; this finding is the non-empty branch's mirror image.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the light-cluster path (`cluster_cull.comp` / `upload_cluster_buffers`) for the same full-capacity-rewrite shape
- [ ] **TESTS**: A regression test pins this specific fix (assert `entry.offset` is still `cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER` after N frames without the per-frame loop, and that a cluster with `count == 0` is never read)
