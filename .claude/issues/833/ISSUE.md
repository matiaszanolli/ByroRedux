# NIF-PERF-02: chunks_exact().map().collect() doubles allocation in 7 bulk array readers

## Description

Each bulk reader allocates a temporary `vec![0u8; byte_count]` buffer for the `read_exact` call, then uses `buf.chunks_exact(N).map(...).collect()` to produce the typed output. This holds **two** allocations live simultaneously: the byte buffer (dropped at function return) and the typed output Vec.

For a 10K-vertex BSGeometry the temp buf is 120 KB and the output Vec is another 120 KB — peak 240 KB to read 10K NiPoint3s.

The bulk readers exist precisely to amortize fixed per-element overhead, so handing back a `Vec` is correct — but the temp byte buffer is unnecessary. Reading directly into a pre-typed `Vec<T>` for any POD `T` (which all of these element types are) costs the same `read_exact` call but avoids the second allocation.

## Location

`crates/nif/src/stream.rs:251-342`

Affected functions:
- `read_ni_point3_array`
- `read_ni_color4_array`
- `read_uv_array`
- `read_vec2_array`
- `read_u16_array`
- `read_u32_array`
- `read_f32_array`

## Evidence (representative)

```rust
pub fn read_ni_point3_array(&mut self, count: usize) -> io::Result<Vec<NiPoint3>> {
    let byte_count = count * 12;
    self.check_alloc(byte_count)?;
    let mut buf = vec![0u8; byte_count];           // alloc 1: 12*N bytes
    self.cursor.read_exact(&mut buf)?;
    Ok(buf
        .chunks_exact(12)
        .map(|c| NiPoint3 { x: f32::from_le_bytes([c[0], c[1], c[2], c[3]]), ... })
        .collect())                                  // alloc 2: 12*N bytes (Vec<NiPoint3>)
}
```

## Impact

Called ~17 times across the codebase (`tri_shape.rs`, `bs_geometry.rs`, `skin.rs`, `controller/morph.rs`). On a single FNV cell load with ~150 NIFs averaging 2 geometry blocks each, the redundant allocation churns **~2-5 MB through the heap allocator with no useful product**. ~0.5-1.5 ms per cell on the parse path. Especially impactful on FO4+ BSGeometry meshes (5-50K verts each).

## Suggested Fix

For `Vec<u16>` / `Vec<u32>` / `Vec<f32>` (already POD `T = elem`), allocate `Vec<T>` of capacity `count` and use `bytemuck::cast_slice_mut(&mut vec)` (the workspace already depends on bytemuck per #291) to read directly. For `[f32; 3]` / `[f32; 2]` / `[f32; 4]` (POD bag-of-floats), same trick — these structures are `#[repr(C)]` and bytemuck-castable.

Alternative: use `read_exact` into `vec.spare_capacity_mut()` then `set_len(count)`. Either path eliminates the second allocation entirely.

```rust
pub fn read_u16_array(&mut self, count: usize) -> io::Result<Vec<u16>> {
    self.check_alloc(count * 2)?;
    let mut out: Vec<u16> = vec![0; count];
    self.cursor.read_exact(bytemuck::cast_slice_mut(&mut out))?;
    // Endianness: NIF is little-endian; if host is BE swap here. NIF is LE-only by spec.
    Ok(out)
}
```

## Completeness Checks
- [ ] **UNSAFE**: bytemuck cast_slice_mut is safe; no manual unsafe needed
- [ ] **SIBLING**: All 7 readers in stream.rs:251-342 must be updated together
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing parse tests cover correctness. Add a microbenchmark comparing pre/post allocation count via `dhat`.

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` — NIF-PERF-02