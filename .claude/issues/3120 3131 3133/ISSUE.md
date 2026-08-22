# Issue #3120: REN-D3-01 — stale triangle.frag.spv

**Severity**: MEDIUM · **Domain**: renderer (shaders)

`2325c1de` (2026-08-17) added `RENDER_DEBUG_VOLUMETRIC_TERM = 8` and redefined
`RENDER_DEBUG_MODE_MAX = RENDER_DEBUG_VOLUMETRIC_TERM`. `composite.frag` was
recompiled; `triangle.frag.spv` was not — it still encodes the old bound of 7
(`OpUGreaterThan %bool %7625 %uint_7` vs fresh `%uint_8`).

Effect: `r.debug volumetric` (mode 8) hits the "unrecognised structured mode"
magenta early-out in the shipped `triangle.frag.spv`, even though source says
mode 8 is valid. Composite still displays correctly (reads froxel field
directly), so visually masked — but source/binary have drifted.

**Fix**: recompile with `glslangValidator -V -I. triangle.frag -o triangle.frag.spv`
(plain `-V`, NOT `-g0` — reflection test needs `OpName`). Then close the class:
add a test / build.rs gate that catches source/binary drift generally.

**Completeness**: re-run recompile-and-compare sweep over all 21 shaders;
add a regression test pinning this class of bug.

---

# Issue #3131: PERF-D5-01 — volumetrics_inject.comp combustion stencil runs unconditionally

**Severity**: MEDIUM · **Domain**: renderer (performance, volumetrics)

`main()` in `volumetrics_inject.comp` calls `transportCombustion` for every
froxel unconditionally. Its RK2 advection block (once temporal history is
valid) runs `incomingDynamicsFromNeighbors`, which does 6 neighbours × 3
trilinear RGBA16F 3-D texture fetches = 18 fetches, gated on
`combustionActivity(...) < 0.08` — i.e. precisely the froxels with NO
combustion pay the cost. In a scene with no fire, that's 100% of froxels.

The CPU already computes the right predicate
(`has_transport_emitter(fog_volumes)` + `combustion_active_until_seconds` in
`volumetrics.rs`) but never uses it to skip the stencil — `simulation_dt` is
always written into `fog_reference[3]` regardless.

**Fix**: One line in `VolumetricsPipeline::dispatch` — set
`frame_params.fog_reference[3]` (shader's `simulationDt`) to `0.0` when
`!has_transport_emitter(fog_volumes) && now > self.combustion_active_until_seconds`.
`transportCombustion` already gates its whole RK2 block on `dt > 0.0`, so no
shader change needed. Pin with a unit test on the predicate.

**Completeness**: check sibling per-froxel stencils in
volumetrics_inject/integrate.comp with no scene-level gate; add CPU predicate
unit test.

---

# Issue #3133: PERF-D4-01 — fog cluster rebuild is O(grid capacity) not O(live volumes)

**Severity**: MEDIUM · **Domain**: renderer (performance, SSBO upload)

`build_fog_volume_clusters` (`volumetrics.rs:380-401`) unconditionally does,
every frame with ≥1 visible fog volume:
- `entries.fill(GpuFogClusterEntry::default())` — 4096×8B = 32KB memset
- `indices.fill(0)` — 32768×4B = 128KB memset
- a 4096-iteration scalar loop recomputing `entry.offset = cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER`
  (a pure function of index that never varies)

Then both full 160KB arrays are `write_mapped` into CpuToGpu host-visible
memory every frame regardless of actual volume count.

`entry.offset` never changes — pure function of cluster_index. `indices.fill(0)`
is unnecessary: shader only reads `fogClusterIndices[cluster.offset + i]` for
`i < min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER)` and additionally checks
`volumeIndex >= fogVolumeCount` — slots past `count` are never read.

**Fix**:
(a) Initialise `entry.offset` once in `VolumetricsPipeline::new`, drop the
    per-frame loop.
(b) Delete `indices.fill(0)`.
(c) (larger, optional) touch only clusters a volume's AABB actually intersects.

(a) and (b) are safe, mechanical, independently testable.

**Completeness**: check sibling light-cluster path (`cluster_cull.comp` /
`upload_cluster_buffers`) for the same full-capacity-rewrite shape; add
regression test asserting `entry.offset` correctness after N frames without
the per-frame loop, and that count==0 clusters are never read.
