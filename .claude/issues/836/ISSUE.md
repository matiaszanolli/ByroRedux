# SK-D5-NEW-02: BSTriShape data_size 'irrational' WARN spam — false positive on SSE skin reconstruction path

## Description

When the BSTriShape header reports `data_size > 0` but `vertex_size_quads == 0 && num_vertices == 0 && num_triangles == 0`, the sanity check at `tri_shape.rs:467-501` logs a WARN every time. This is the **legitimate Skyrim SE skinned-body case** — the shape carries `VF_SKINNED` and the real packed vertex buffer lives on the linked `NiSkinPartition` (the `try_reconstruct_sse_geometry` path at `import/mesh.rs:718-722`, fix #559).

The descriptor on the BsTriShape block legitimately ships `0/0/0` because the geometry is elsewhere. The warning's "irrational" branch is taken because `derived_stride` is `None` when `num_vertices == 0`, so it falls back to the (also zero) descriptor stride and the per-vertex loop iterates zero times — which is correct, because `data_size > 0` here is just the persisted size of data that lives on a sister block.

## Location

`crates/nif/src/blocks/tri_shape.rs:467-501`

## Evidence

67 occurrences in a single Meshes0 parse (sample sizes `70144 / 71936 / 98304 / 145920 / 220160 / 364032 / 7680 / 15360 / 229888` — all powers-of-2-ish, all consistent with packed SSE skin vertex buffers).

```
WARN  byroredux_nif::blocks::tri_shape] BSTriShape data_size mismatch:
      stored 70144 vs derived 0 (vertex_size_quads=0, num_vertices=0,
      num_triangles=0) — trusting data_size-derived stride
      (irrational; falling back to descriptor stride)
```

The block's `data_size != 0` gate at L468 does not inspect `vertex_attrs` or `VF_SKINNED`. Same trigger appears in `dlc02\landscape\trees\treepineforestashl02.nif` (2 occurrences in a single 35-block tree NIF — those are the BSLODTriShape distant-LOD shapes, also sharing the SSE-skin payload pattern).

## Impact

WARN-level log spam every cell load (~tens of warnings per cell in Whiterun / dragon-spawn cells) drowns out actual parser warnings. The comment at L450-454 explicitly exempts the `data_size == 0` case for BSDynamicTriShape facegen content (#341), but doesn't extend the exemption to the symmetric `data_size > 0 + num_vertices == 0` SSE-skin case.

## Suggested Fix

Extend the gate at L468 to also skip the warning when `num_vertices == 0` (since the per-vertex loop won't run anyway):

```rust
if data_size != 0 && num_vertices != 0 {
```

Net behavior identical (per-vertex loop is gated on `num_vertices`); only the spurious warning goes away.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify the `data_size == 0` exemption at L450-454 still applies after the gate tightens — both branches handle the "geometry lives on a sister block" case
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a parse test on a real SSE skinned-body NIF (Meshes0 has 67 candidates) — assert no `data_size mismatch` warning

## Source Audit

`docs/audits/AUDIT_SKYRIM_2026-05-05_DIM5.md` — SK-D5-NEW-02