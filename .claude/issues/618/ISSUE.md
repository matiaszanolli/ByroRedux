# SK-D1-02: BsTriShape vertex alpha channel dropped at import — ImportedMesh::colors is Vec<[f32; 3]>

## Finding: SK-D1-02

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Skyrim (hair tip cards, eyelash strips), all games with vertex-alpha-modulated effect meshes
- **Locations**:
  - Importer: [crates/nif/src/import/mesh.rs:270-278](crates/nif/src/import/mesh.rs#L270) — `[c[0], c[1], c[2]]` keep-RGB
  - Type def: [crates/nif/src/import/mod.rs:161](crates/nif/src/import/mod.rs#L161) — `pub colors: Vec<[f32; 3]>`
  - Sibling path: [crates/nif/src/import/material.rs:468](crates/nif/src/import/material.rs#L468)
  - Parse side: [crates/nif/src/blocks/tri_shape.rs:515-520](crates/nif/src/blocks/tri_shape.rs#L515) — full RGBA per nif.xml `ByteColor4`

## Description

`BsTriShape.vertex_colors` parses RGBA correctly per nif.xml `ByteColor4` (line 2122). The importer extracts only `[c[0], c[1], c[2]]` and discards `c[3]`. Downstream `ImportedMesh::colors` is typed `Vec<[f32; 3]>` so even if the importer were fixed, the renderer-facing field has nowhere to put the alpha lane.

Skyrim hair tip cards, eyelash strips, and several BSEffectShader meshes rely on vertex alpha as a per-vertex modulation against the alpha-test/alpha-blend mask. Currently dropped silently. The NiTriShape path through `extract_vertex_colors` (material.rs:468) has the same shape, so the fix is uniform across both geometry types.

## Suggested Fix

Two-step:

1. Promote `ImportedMesh::colors: Vec<[f32; 4]>` (RGBA).
2. Update both extraction sites (`mesh.rs:270` for BsTriShape and `material.rs:468` for NiTriShape) to emit the full 4-component value.
3. Add a vertex-color attribute to the renderer's `Vertex` struct (currently 3-channel) — or fold alpha into the per-instance `mat_alpha` only when `vertex_color_mode != Ignore`.

The third step is the larger change. As an interim, item (2) alone (still storing in a 4-component `ImportedMesh::colors`) preserves the data for future renderer consumption.

## Related

- D4-09 / #221 (open): NiMaterialProperty ambient/diffuse colors discarded — adjacent material-side gap.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: NiTriShape path at material.rs:468 has the same `[c[0], c[1], c[2]]` pattern; fix uniformly.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic BsTriShape with a non-1.0 alpha vertex → assert imported `colors[i][3]` matches input.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
