# Issue #28: Renderer: per-vertex inverse(mat3) produces NaN for zero-scale meshes

- **State**: OPEN
- **Labels**: bug, renderer, medium, pipeline
- **Location**: `crates/renderer/shaders/triangle.vert:25`

Per-vertex `transpose(inverse(mat3(model)))` is redundant (uniform scale)
and produces NaN when scale is zero.
