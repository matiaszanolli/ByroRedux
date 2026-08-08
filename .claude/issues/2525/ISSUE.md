# PERF-D8-NEW-02: Three per-element decode loops bypass the crate's own established bulk-read-then-map idiom for half-float/quaternion arrays

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2525
**Finding ID**: PERF-D8-NEW-02

**Severity**: MEDIUM
**Dimension**: NIF Parse Performance
**Location**: `crates/nif/src/blocks/extra_data.rs:377-385` (`BsPositionData::parse`), `crates/nif/src/blocks/node.rs:1080-1088` (`BsDistantObjectInstancedNode::parse`, transforms), `crates/nif/src/blocks/legacy_particle.rs:624-638` (`NiLegacyParticlesData::parse`, rotations)
**Status**: NEW

## Description
#1263 (NIF-D5-NEW-03) and #2032 (PERF-D8-01) both established the same fix shape for "array needs a per-element transform the raw bytes don't carry" (half-float decode, byte-swizzle, etc.): bulk-read the raw fixed-width values in one `read_*_array` call, then `.chunks_exact(k).map(transform).collect()`. Three call sites never got the memo and still do `allocate_vec` + a per-element loop of individual `read_u16_le()`/`read_f32_le()` calls: `BsPositionData::parse` (per-vertex half-float blend-factor array, FO4/FO76 cloth/dismemberment), `BsDistantObjectInstancedNode::parse` transforms (`Vec<[f32; 16]>`, 16 individual `read_f32_le()` calls per transform instead of one bulk read + `chunks_exact(16)`), and `NiLegacyParticlesData::parse` rotations (reads `w,x,y,z` and reorders to `[x,y,z,w]` per quaternion, could bulk-read + swizzle in the `.map()`). None of these are per-frame (all are one-time import-side parses, cached after first load), so the impact is bounded CPU overhead on cell-load / streaming-worker latency, not steady-state frame time.

## Evidence
Confirmed directly at all three locations — each still does a per-element read loop instead of the bulk-read-then-map idiom established at `crates/nif/src/blocks/bs_geometry.rs:410-446`.

## Impact
Extra per-element call overhead on the NIF-parse critical path for cell load / exterior streaming (a budget-bound path). Scales with vertex/instance count on FO4/FO76 cloth meshes and Starfield distant-object-instancing nodes; bounded by real-world content sizes, so this is a throughput/latency inefficiency rather than a correctness or memory-safety issue. dhat allocation-bound tests can't catch this class (allocation *count* is identical either way — the difference is N extra function-call/bounds-check/cursor-advance round trips instead of one bulk `read_exact`).

## Related
#1263 (NIF-D5-NEW-03, the original 3-site fix in `bs_geometry.rs`), #2032 (PERF-D8-01, the `BoneWeight` sibling fix in this exact dimension); the `node.rs` transforms site is also cited under PERF-D8-NEW-01 (this session) for its separate allocation-bound issue — fixing the bulk-read shape here does not by itself fix that finding, both changes are complementary.

## Suggested Fix
Apply the same `read_*_array(count * k)?.chunks_exact(k).map(transform).collect()` shape at all three sites, mirroring `bs_geometry.rs:421-446`. For `node.rs`'s `[f32;16]` case, use `read_f32_array(count * 16)?.chunks_exact(16).map(|c| c.try_into().unwrap())`. Since dhat bounds can't catch this class, propose a wall-clock or read-call-count regression test alongside the fix.

## Completeness Checks
- [ ] **TESTS**: A wall-clock or read-call-count regression test pins the bulk-read shape at all three sites (dhat allocation-count tests can't catch this class)
- [ ] **SIBLING**: All three sites converted consistently to the established idiom
