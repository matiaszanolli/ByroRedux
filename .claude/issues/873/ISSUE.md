# Issue #873 (OPEN): NIF-PERF-09: BSGeometry per-element read_*_le push-loops where bulk readers exist

URL: https://github.com/matiaszanolli/ByroRedux/issues/873

---

## Description

`crates/nif/src/blocks/bs_geometry.rs` has 4 sites that read POD arrays via per-element `stream.read_u32_le()` / `read_u16_le()` / `read_f32_le()` push-loops, despite the `read_u32_array` / `read_u16_array` / `read_f32_array` bulk readers existing for exactly this case. The #831 / #833 sweep that fixed `skin.rs` and `tri_shape.rs` missed `bs_geometry.rs`.

## Evidence

```rust
// bs_geometry.rs:425-435 — normals_raw, tangents_raw
let n_normals = stream.read_u32_le()?;
let mut normals_raw = stream.allocate_vec::<u32>(n_normals)?;
for _ in 0..n_normals {
    normals_raw.push(stream.read_u32_le()?);          // ← N function calls
}

let n_tangents = stream.read_u32_le()?;
let mut tangents_raw = stream.allocate_vec::<u32>(n_tangents)?;
for _ in 0..n_tangents {
    tangents_raw.push(stream.read_u32_le()?);          // ← N function calls
}
```

```rust
// bs_geometry.rs:467-482 — LOD triangles, 3N function calls
let mut tris = stream.allocate_vec::<[u16; 3]>(lod_tri_count)?;
for _ in 0..lod_tri_count {
    let a = stream.read_u16_le()?;
    let b = stream.read_u16_le()?;
    let c = stream.read_u16_le()?;
    tris.push([a, b, c]);
}
```

Plus meshlets at `:484-493` (4× `read_u32_le` per entry) and cull_data at `:495-508` (6× `read_f32_le` per entry).

## Why it matters

BSGeometry is the dominant geometry block on FO4/FO76/Starfield. A typical actor mesh has ~5–50 K vertices; `normals_raw` alone is then 20–200 K function calls (plus their bounds checks) where 1 `read_u32_array(n)` would suffice.

## Proposed Fix

Mirror the skin.rs cleanup:

```rust
let normals_raw = stream.read_u32_array(n_normals as usize)?;
let tangents_raw = stream.read_u32_array(n_tangents as usize)?;

// LOD triangles — pairs with NIF-PERF-10 (extend read_pod_vec to [u16; 3]):
let tris = stream.read_u16_triple_array(lod_tri_count)?;
```

For meshlets (4-tuple of u32) and cull_data (6 floats) — both are POD bag-of-fields and could go through `read_pod_vec` directly with a `#[repr(C)]` annotation on the structs.

## Cost Estimate

Per-FO4+/Starfield BSGeometry block; N×bounds-check overhead instead of 1. Cell-load critical path on FO4/FO76/Starfield cells.

## Completeness Checks

- [ ] **UNSAFE**: N/A (bulk readers already encapsulate the unsafe POD cast)
- [ ] **SIBLING**: Sweep all of `crates/nif/src/blocks/` for any remaining `for _ in 0..n { vec.push(stream.read_*_le()?) }` patterns; this audit found 4 in `bs_geometry.rs` plus the LOW NIF-PERF-12 sites in `header.rs`
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing FO4 / FO76 / Starfield NIF integration tests must continue to pass; bit-for-bit equivalence on parsed values

## dhat Gap

Expected savings are estimates; no quantitative regression guard exists today. This finding warrants a follow-up "wire dhat for BSGeometry parse" issue.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (NIF-PERF-09)
- Original sweep: #831, #833 (closed) — both missed `bs_geometry.rs`
- Pairs with: #issue-for-NIF-PERF-10 (`read_pod_vec::<[u16; 3]>`)
