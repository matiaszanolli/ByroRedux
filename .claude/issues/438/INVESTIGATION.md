# Investigation — Issue #438 (NIF-D4-N05)

## Domain
NIF import — `crates/nif/src/import/material.rs`.

## Root cause

`extract_vertex_colors` (`material.rs:439-471`) takes `_mat: &MaterialInfo` but never reads it:

1. **Line 448**: `let vertex_mode = vertex_color_mode_for(scene, shape, inherited_props);` — re-scans shape + inherited props for `NiVertexColorProperty`. But `mat.vertex_color_mode` was already set at `material.rs:910` in `extract_material_info`.
2. **Line 460-469**: fallback walks `shape.av.properties` + `inherited_props` AGAIN looking for `NiMaterialProperty` to get the diffuse color. `NiMaterialProperty` is already looked up at `material.rs:674` during `extract_material_info`, but the diffuse color is **discarded** there.

Caller at `mesh.rs:103,106`:
```rust
let mat = extract_material_info(scene, shape, inherited_props);
// ...
let colors = extract_vertex_colors(scene, shape, &geom, inherited_props, &mat);
```

Wiring is in place — just needs to be consumed.

## Fix

1. Add `diffuse_color: [f32; 3]` field to `MaterialInfo` struct.
2. Default = `[1.0, 1.0, 1.0]` (white, matches current fallback).
3. Capture `mat.diffuse` at `material.rs:674` area alongside specular/emissive.
4. Rename `_mat` → `mat` in `extract_vertex_colors`, use `mat.vertex_color_mode` and `mat.diffuse_color` — delete `vertex_color_mode_for` call at 448 and the property-list walk at 460-469.

The helper `vertex_color_mode_for` (line 477-492) stays — it's still used by tests and as the canonical lookup for `extract_material_info`.

## Scope
1 file. `Default` impl already handles the new field. No caller signature change.

## Perf impact

Before: 3× property-list scans per NiTriShape (once in `extract_material_info`, twice in `extract_vertex_colors` — mode lookup + diffuse fallback).

After: 1× scan in `extract_material_info` only. Vertex-color path reads cached values.
