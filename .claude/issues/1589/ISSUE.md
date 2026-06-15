**Severity**: LOW (import-time CPU-call overhead; F13 is alloc-churn) · **Dimension**: NIF Parse · **Status**: NEW (all 4)
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-06-14.md` (F10–F13, grouped)

## Description
Four missed `read_pod_vec`-family bulk-read adoptions on NIF import paths — per-element `read_u16/f32/bytes` push loops that should be a single bulk read (the wrapper is already used for the identical type elsewhere, often in the same file). None are double-allocations (`allocate_vec` pre-size is correct, so #831/#833 hold).

- **F10** `crates/nif/src/blocks/bs_geometry.rs:382-390` — Starfield BSGeometry *primary* triangle loop; the *LOD* path at `:504` already uses `read_u16_triple_array`. Hot import path. *Verified live: per-element `read_u16_le` ×3 + `triangles.push([a,b,c])`.*
- **F11** `crates/nif/src/blocks/extra_data.rs:1030-1036` — FO4 `BSPackedCombinedGeomData` triangle loop; the sibling `.csg` reader (`crates/nif/src/import/precombine.rs:139`) already bulk-reads.
- **F12** `crates/nif/src/blocks/collision/compressed_mesh.rs:166-175` — Skyrim+ Havok `big_verts: Vec<[f32;4]>` loop; `read_ni_color4_array` applies (used at `shape_compound.rs:31`).
- **F13** `crates/nif/src/blocks/legacy_particle.rs:392-398` — `read_bytes(32)` inside a per-particle loop allocates a throwaway 32-byte `Vec<u8>` every iteration (lifetime churn the peak-live dhat gates structurally cannot catch). *Verified live: `let chunk = stream.read_bytes(32)?; ... particles.push(arr);`*

## Impact
Per-element call overhead on import hot paths (F10 in particular). F13 churns a throwaway 32-byte `Vec<u8>` per particle — allocation lifetime the peak-live dhat gate cannot catch.

## Suggested Fix
Replace each with the matching bulk reader (F10/F11 `read_u16_triple_array`, F12 `read_ni_color4_array`). For F13, add `[u8; 32]` to the `impl_any_bit_pattern!` set + `read_pod_vec::<[u8;32]>` and a **lifetime-total** dhat assertion. Natural follow-ons to #1381.

## Related
#833 (`read_pod_vec` helper — these are missed adoptions, not erosions), #1381 (dhat alloc-counter coverage).

## Completeness Checks
- [ ] **SIBLING**: Grep for any further per-element push loops over POD types that have a matching bulk reader
- [ ] **TESTS**: F13 gets a lifetime-total dhat assertion (cross-link #1381); existing parse tests confirm byte-identical output after each swap
