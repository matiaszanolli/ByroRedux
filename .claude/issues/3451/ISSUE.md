# #3451 — TD1-2026-08-27-01: crates/renderer/src/mesh.rs newly crossed 2000 production LOC (1525 → 2049 in four days), taking the primary bucket from 4 to 5

Labels: `low,renderer,tech-debt,bug`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 1 — File / Function / Module Complexity · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD1-2026-08-27-01)

**Location**: `crates/renderer/src/mesh.rs` (2049 production / 2896 total LOC)

**Age**: `ae7179a3` (#3298 — resumable geometry-SSBO rebuild) and `cd1aa9e9` (#3372 — compacted-offset publication gate), 2026-08-26/27

## Description
The file now carries three distinct responsibilities that grew independently:

- (a) `MeshRegistry` proper — handle allocation, per-mesh metadata, LRU eviction;
- (b) the **global geometry SSBO lifecycle**, which is where all the new code went: `build_geometry_ssbo`, `rebuild_geometry_ssbo`, `advance_geometry_rebuild`, `rebuild_geometry_ssbo_atomic_fallback`, plus the free function `next_geometry_rebuild_chunk` and the generation counter;
- (c) the primitive-geometry helpers (`cube_vertices` and siblings, 167 LOC for the largest).

Unlike the other four primary-bucket members this is **not** a long-function problem — the largest function is 167 LOC, well under the 200-LOC extraction trigger, and the new chunked-rebuild code is unusually well documented (it explains in prose why the atomic path survives as the low-headroom fallback rather than being deleted). It is purely a file-cohesion crossing, and (b) is a self-contained state machine with its own generation counter, chunk cursor and fallback path — a clean extraction seam.

## Evidence
```
$ prod_loc crates/renderer/src/mesh.rs                    # SKILL Phase-1 helper
2049
$ git show 07a029ea:crates/renderer/src/mesh.rs > /tmp/old.rs; prod_loc /tmp/old.rs
1525
$ grep -n "fn build_geometry_ssbo\|fn rebuild_geometry_ssbo\|fn advance_geometry_rebuild\|fn rebuild_geometry_ssbo_atomic_fallback\|fn next_geometry_rebuild_chunk" crates/renderer/src/mesh.rs
152:fn next_geometry_rebuild_chunk(
1112:    pub fn build_geometry_ssbo(
1208:    pub fn rebuild_geometry_ssbo(
1348:    fn advance_geometry_rebuild(
1507:    fn rebuild_geometry_ssbo_atomic_fallback(
```
(All five re-confirmed present at publish time, 2026-08-28; file is 2896 total LOC.)

`gh issue list --search "mesh.rs in:title" --state open` returns nothing — the only prior `mesh.rs` Dim-1-adjacent issue is #1760 (CLOSED, two dead `pub fn`).

## Impact
Maintenance only. Worth filing now rather than after another growth cycle: this file gained +524 production LOC in four days across two issues, and the third primary-bucket member (`context/mod.rs`) is simultaneously regrowing +95 after its own #1749 split — the primary bucket went 4 → 5 members and is trending up, not down.

## Related
#3282 (`draw_frame`), #2256 (`volumetrics.rs`) — the two OPEN primary-bucket items; #3298 / #3372 (the two closed issues whose work landed here); #1749 (the `context/mod.rs` split now regrowing).

## Suggested Fix
Extract the global geometry SSBO lifecycle into `crates/renderer/src/mesh/geometry_ssbo.rs` — the five functions above plus `geometry_generation`, `ssbo_vertex_count`/`ssbo_index_count`, `geometry_dirty`, the rebuild cursor and `geometry_staging_pool` — leaving `mesh.rs` as `MeshRegistry` + primitives. Mechanical: the block already communicates with the rest of the file through a small, named field set.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other primary-bucket members; `context/mod.rs`'s post-split regrowth)
- [ ] **DROP**: If Vulkan objects move with the extraction, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (the existing chunked-rebuild / compaction-gate tests must move with the code and still pass)
