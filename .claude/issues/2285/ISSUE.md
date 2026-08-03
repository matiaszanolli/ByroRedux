# NIFAL-D6-07: finish_trimesh's index-bounds guard validates the merged total, not each source sub-buffer's range — corrupt NIF can splice unrelated geometry

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: MEDIUM
**Dimension**: Collision · **Tier Violated**: no-fabrication (defense-in-depth gap in the boundary the D6-03 fix itself introduced)
**Game Affected**: Skyrim LE/SE/DLC (`resolve_compressed_mesh`, multi-chunk merge) and any game with a multi-`data_ref` `bhkNiTriStripsShape`/`BhkMeshShape` (`resolve_tri_strips_data_refs`) — theoretical on corrupt/truncated NIFs only, not observed on vanilla content
**Location**: `crates/nif/src/import/collision/shape.rs:591-605` (`finish_trimesh`), consumed by `resolve_compressed_mesh` (`shape.rs:484-582`) and `resolve_tri_strips_data_refs` (`shape.rs:361-407`)
**Status**: NEW

## Description

`resolve_compressed_mesh` merges `big_verts` then each chunk's quantized
vertices into one flat `all_verts`/`all_indices` pair before calling
`finish_trimesh`; `resolve_tri_strips_data_refs` does the same across a
shape's `data_refs`. `finish_trimesh`'s only bounds check validates each index
against the *final merged* vertex count, not the count of the specific
sub-buffer it was decoded from. A corrupt/truncated NIF whose
`CmsBigTri.v1/v2/v3` exceeds `data.big_verts.len()` but is still less than the
eventual `all_verts.len()` (because later chunks pushed enough vertices to
cover the gap) passes the guard unchanged — it silently indexes into a
*different* chunk's vertex data, connecting two unrelated pieces of geometry
instead of being dropped as corrupt. This is exactly the failure mode
`finish_trimesh`'s own doc comment claims to prevent ("a corrupt tail cannot
poison otherwise usable authored geometry") — it prevents
degenerate/globally-out-of-range poisoning, but not cross-buffer splicing.

## Evidence

```rust
let vertex_count = vertices.len() as u32;   // the FINAL merged total
indices.retain(|[a, b, c]| {
    a != b && b != c && a != c && *a < vertex_count && *b < vertex_count && *c < vertex_count
});
```

## Impact

No known vanilla-content trigger — every real archive's per-buffer indices
are correctly local by construction. Corrupt/adversarial-NIF robustness gap
in the same class the surrounding code already defends against (`#1409`,
`#1779`, `#1385`): the next malformed or hand-edited NIF that trips it gets a
garbage triangle silently merged into a real static's collider instead of the
intended graceful `None` → synthesized-fallback path.

## Suggested Fix

Track each sub-buffer's own vertex-count offset alongside `base`, and
validate `*a < base + local_count` (or, simpler, validate/retain each
source's own index slice before pushing it into `all_indices`), so
`finish_trimesh`'s existing global check becomes a pure belt-and-suspenders
pass rather than the only one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other block parsers with multi-buffer merges)
- [ ] **TESTS**: A regression test pins this specific fix

## Filed as

GitHub issue #2285, labels: medium, nif-parser, bug.
