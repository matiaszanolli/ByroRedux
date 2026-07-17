# PERF-D8-01: BSGeometryMeshData::parse skin-weight loop bypasses the bulk read_pod_vec path its own doc comment specifies

**Labels**: medium, performance, nif-parser, bug

**Severity**: MEDIUM
**Dimension**: NIF Parse
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/nif/src/blocks/bs_geometry.rs:466-489` (loop), struct + doc comment at `:293-305`

## Description
`BoneWeight` is `#[repr(C)]` + POD-marked (`unsafe impl AnyBitPattern`), and its own doc comment states the parser should bulk-read it via `read_pod_vec::<BoneWeight>`. The actual parse body instead does `allocate_vec` + a per-element loop of two `read_u16_le()` calls — the exact pre-#873/#1589 pattern those sweeps were written to eliminate. Two sibling reads 20 lines later in the same function (`meshlets`, `cull_data`) already use the correct bulk path; #1589 converted 4 near-identical sites in the same cycle but missed this one.

Verified current: `crates/nif/src/blocks/bs_geometry.rs`'s skin-weight loop still does `stream.allocate_vec::<BoneWeight>(weights_per_vert)?` followed by a `for _ in 0..weights_per_vert { read_u16_le(); read_u16_le(); row.push(...) }` loop, rather than a single `read_pod_vec` call — while `BoneWeight`'s own doc comment (lines ~293-305) explicitly says it should bulk-read via `read_pod_vec::<BoneWeight>(n)`.

## Impact
CPU throughput cost only (no correctness/memory-safety impact — `allocate_vec`'s budget guard still bounds the allocation). Every skinned Starfield `BSGeometry` mesh pays thousands of extra small reads/pushes instead of one bulk `read_exact`, on the cell-load / streaming-worker parse path. **Not caught by existing dhat `heap_allocation_bounds*` gates** — the loop performs zero extra allocations vs. the bulk path (capacity is pre-reserved either way), so an allocation-count assertion can't distinguish the two; a call-count or wall-clock benchmark is needed instead.

## Related
#833 (NIF-PERF-02), #873 (NIF-PERF-09), #1589 (fixed 4 sibling sites, missed this one).

## Suggested Fix
Replace the inner loop with `stream.read_pod_vec::<BoneWeight>(outer_len * weights_per_vert as usize)?` then `.chunks_exact(weights_per_vert).map(|c| c.to_vec()).collect()` — must read `outer_len * weights_per_vert`, not `n_total_weights`, to byte-for-byte preserve today's truncating-division stream-position behavior. Guard with a `criterion` wall-clock benchmark or a `#[cfg(test)]` call-count assertion, since dhat allocation bounds can't catch this class of regression.

## Completeness Checks
- [ ] **SIBLING**: `meshlets`/`cull_data` reads in the same function already use the correct bulk path — verify the migrated code produces byte-identical stream position/output
- [ ] **UNSAFE**: `BoneWeight`'s existing `unsafe impl AnyBitPattern` invariant (POD layout) must still hold after the migration — no change expected, but confirm
- [ ] **TESTS**: A wall-clock/call-count regression test, since dhat allocation bounds cannot catch this class
