# NIF-D4-N05: extract_vertex_colors ignores its _mat parameter — 3× redundant property scan per NiTriShape

**Issue**: #438 — https://github.com/matiaszanolli/ByroRedux/issues/438
**Labels**: bug, nif-parser, medium, performance

---

## Finding

`crates/nif/src/import/material.rs:319-351` — `extract_vertex_colors` takes `_mat: &MaterialInfo` but ignores it:

```rust
pub(crate) fn extract_vertex_colors(
    scene: &NifScene,
    shape: &NiTriShape,
    geom: &GeomData,
    inherited_props: &[BlockRef],
    _mat: &MaterialInfo,  // <-- underscore-prefixed, unused
) -> Vec<[f32; 4]> {
    // ... inside:
    let mode = vertex_color_mode_for(scene, shape, inherited_props);  // <-- re-scans properties
    // ...
    // fallback branch walks property list a THIRD time for NiMaterialProperty diffuse
}
```

`mat.vertex_color_mode` was already computed in the preceding `extract_material_info` call at mesh.rs:102. The `_mat` parameter was apparently intended as the cache but never wired.

Additionally, the fallback material-diffuse path (line 341-349) walks the property list a THIRD time looking for `NiMaterialProperty`, even though `mat.emissive_color` / `mat.specular_color` are already set — but `MaterialInfo` doesn't store diffuse separately (it's read at material.rs:496-503 but discarded).

## Impact

3× property-list scan per NiTriShape. On large cells (Oblivion interiors with 1000+ shapes; FO4 exteriors with thousands of clutter items), this is measurable on the cell-load hot path.

Functional correctness is fine — just perf.

## Games affected

All.

## Fix

Two-part:

**(a) Wire `_mat` through**:
```rust
pub(crate) fn extract_vertex_colors(
    scene: &NifScene,
    shape: &NiTriShape,
    geom: &GeomData,
    inherited_props: &[BlockRef],
    mat: &MaterialInfo,  // drop underscore
) -> Vec<[f32; 4]> {
    let mode = mat.vertex_color_mode;  // use cached value
    // ...
}
```

**(b) Add `diffuse_color: [f32; 3]` to MaterialInfo**:
- Capture at material.rs:496-503 where it's currently read and discarded.
- Use it as the fallback in `extract_vertex_colors` instead of re-walking the property list.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check that all MaterialInfo callers handle the new `diffuse_color` field (likely only `Default` impl needs updating).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Perf regression test — time `extract_mesh` on a synthetic NiTriShape with 10 properties. Assert property-list scan count == 1 (instrument via counter).

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 4 N05.
