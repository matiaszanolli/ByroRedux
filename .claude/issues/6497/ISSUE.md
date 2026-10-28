# REN-D8-03: `water.vert` still documents `inColor` as "unused on water — kept for VAO compat"

Labels: low, shaders, water, documentation, doc-rot
Source: docs/audits/AUDIT_RENDERER_2026-10-27.md

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-27.md` — finding `REN-D8-03` (LOW, Dim 8 Water, doc-rot)
**Location (current)**: `crates/renderer/shaders/water.vert` (`layout(location = 1) in vec4 inColor;`, line ~33).

## Description
Since `7dd294ee3`, the exterior water mesh carries the bed field in `inColor`: r = depth / `WATER_BED_DEPTH_MAX`, gb = shoreward direction, a = the `WATER_BED_VALID_ALPHA` sentinel. `main()` reads it (`bedValid`, `bedDepth`, `uphill`). The declaration comment still says the opposite: "unused on water — kept for VAO compat".

## Evidence
```glsl
layout(location = 1) in vec4 inColor;       // unused on water — kept for VAO compat
...
bool bedValid = abs(inColor.a - WATER_BED_VALID_ALPHA) < WATER_BED_VALID_ALPHA_TOL;
float bedDepth = bedValid ? inColor.r * WATER_BED_DEPTH_MAX : 1.0e6;
vec2 uphill = bedValid ? inColor.gb * 2.0 - 1.0 : vec2(0.0);
```

## Impact
Invites a "cleanup" that drops the attribute and breaks the water bed / shore depth data. No runtime effect today.

## Related
`7dd294ee3`; `bed_vertex_color` in `byroredux/src/cell_loader/water.rs`.

## Suggested Fix
Rewrite the comment to name the bed encoding (r depth, gb shoreward direction, a validity sentinel) and point at `bed_vertex_color`. If the `.spv` is rebuilt for any reason, keep reflection (`OpName`) intact (plain `-V`).

## Completeness Checks
- [ ] **SIBLING**: `water.frag` and shader-pipeline.md / water docs describing the vertex layout are consistent with the corrected comment
- [ ] **SHADER-STRUCT**: Comment-only change does not require struct-sync, but `scripts/check-shader-artifacts.sh` still passes if the `.spv` is regenerated

