#version 450

// EXAL ground cover §11.1 bench (#4052) — the raster half's fragment stage.
//
// Never executed: `groundcover_bench.vert` collapses every blade onto a single
// clip position, so each triangle has zero area and dies at rasterisation.
// The pipeline still needs a fragment shader, and a real one (rather than a
// rasterizer-discard pipeline) keeps the vertex stage's outputs live the way
// the eventual blade pipeline's would.
layout(location = 0) out vec4 outColor;

void main() {
    outColor = vec4(1.0);
}
