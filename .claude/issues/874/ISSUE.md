# Issue #874 (OPEN): NIF-PERF-10: chunks_exact(3).map().collect() triangle pattern survives in 3 sites — extend read_pod_vec to [u16; 3]

URL: https://github.com/matiaszanolli/ByroRedux/issues/874

---

## Description

After #833's `read_pod_vec` cleanup, the bulk readers themselves are optimal — but 3 *callers* in `skin.rs` / `tri_shape.rs` still do a double-pass: bulk-read `Vec<u16>`, then `chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect::<Vec<[u16; 3]>>()`. Two allocations + memcpy where one would do.

## Evidence

```rust
// tri_shape.rs:1453-1460 (NiTriShapeData)
let triangles = if has_triangles {
    let flat = stream.read_u16_array(num_triangles * 3)?;     // alloc 1: Vec<u16>
    flat.chunks_exact(3)
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect()                                             // alloc 2: Vec<[u16; 3]>
} else { Vec::new() };
```

```rust
// skin.rs:315-319 (NiSkinPartition)
let flat = stream.read_u16_array(num_triangles as usize * 3)?;
triangles = flat
    .chunks_exact(3)
    .map(|tri| [tri[0], tri[1], tri[2]])
    .collect();
```

Same pattern at `tri_shape.rs:765-768` (BsTriShape triangles).

`[u16; 3]` is bitwise identical to three contiguous u16s; a single bulk read into `Vec<[u16; 3]>` is correct.

## Why it matters

Triangle indices are present on every renderable shape. Skyrim/FO4/Starfield NPC bodies have 5–15 NiSkinPartition partitions × ~1000 triangles each. Per cell with ~50 NPCs: ~50–100 redundant `Vec<[u16; 3]>` allocations × ~6 KB = ~300–600 KB of unnecessary memcpy churn.

## Proposed Fix

Add `read_u16_triple_array(count) -> io::Result<Vec<[u16; 3]>>` on `NifStream` delegating to `read_pod_vec::<[u16; 3]>(count)`. `[u16; 3]` is POD, alignment 2 ≥ 1, all bit patterns sound — same path as the existing `[f32; 2]` / `[f32; 4]` / `NiPoint3` cases at `stream.rs:320-348`.

Then:
```rust
let triangles = if has_triangles {
    stream.read_u16_triple_array(num_triangles)?
} else { Vec::new() };
```

One alloc, one read_exact, one return. Same primitive unblocks the LOD triangle simplification in #issue-for-NIF-PERF-09.

## Cost Estimate

Per-shape; 1 extra alloc + 1 memcpy per call. Payload `num_triangles × 6 bytes`. Cell-load critical path on every game era.

## Completeness Checks

- [ ] **UNSAFE**: New `read_u16_triple_array` reuses `read_pod_vec`'s existing unsafe POD cast; no new unsafe needed
- [ ] **SIBLING**: Audit `crates/nif/src/blocks/` for other `chunks_exact(N).map().collect()` patterns; the dim5 audit identified `skin.rs:508-509` (BSSkin::BoneData 17-float layout) as a smaller-win candidate
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing per-game NIF integration tests must produce bit-identical triangle indices

## dhat Gap

Expected savings are estimates; no quantitative regression guard exists today. This finding warrants a follow-up "wire dhat for triangle-array reads" issue.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (NIF-PERF-10)
- Closes the caller-side gap left by #833 (closed)
- Pairs with: #873 (NIF-PERF-09 — uses the same `[u16; 3]` primitive for LOD triangles)
