# REN-D19-01: LAND terrain ships `bitangent_sign = +1` but its UV parametrization requires `−1` — every TX01 splat normal map is V-inverted

- **Severity**: MEDIUM
- **Dimension**: 19 — Tangent-Space
- **Location**: `crates/renderer/src/vertex.rs` — `Vertex::new_terrain` (`tangent: [1.0, 0.0, 0.0, 1.0]`); UV/position source `byroredux/src/cell_loader/terrain.rs` (the `for row` / `for col` vertex loop); consumer `crates/renderer/shaders/include/material_sampling.glsl` — `perturbNormal` Path 1
- **Status**: NEW
- **Description**: `new_terrain` hard-codes `w = +1.0`. The engine's handedness convention is fixed by `bitangent_sign` (`crates/nif/src/types.rs`), derived as `sign(dot(∂P/∂V, cross(N, ∂P/∂U)))` — i.e. the shader's `B = w · cross(N, T)` must reproduce +∂P/∂v. For the synthesized LAND grid it reproduces −∂P/∂v, so the reconstructed bitangent points the wrong way and the normal map's green (V) axis is inverted on all near-field terrain.
- **Evidence**: In `terrain.rs` the vertex loop builds `position = zup_to_yup_pos([origin_x + col·SPACING, origin_y + row·SPACING, height])`, and `zup_to_yup_pos` is `(x, y, z) → (x, z, −y)`, so `∂P/∂col ≈ +X` and `∂P/∂row ≈ −Z`. The same loop sets `uv = [col/32 · TILES, (1 − row/32) · TILES]`, so `∂u/∂col > 0` and `∂v/∂row < 0`. Chaining: `∂P/∂u = +X` ✓ (matches the stored `T = [1,0,0]`) and `∂P/∂v = (−Z)·(negative) = +Z`. `perturbNormal` computes `B = tangentSign · cross(N, T)`; with `N ≈ (0,1,0)`, `T = (1,0,0)`, `cross(N, T) = (0,0,−1)`, so `tangentSign = +1` yields `B = −Z` — the negation of the true `+Z`. The existing guard `terrain_vertex_carries_a_nonzero_tangent` asserts the exact tuple but only justifies the non-zero `xyz` (clearing `perturbNormal`'s `dot(T,T) > 1e-4` Path-1 gate, #2474); the `w` component's value was never derived.
- **Impact**: Every LAND cell that resolves a TX01 `_n` normal map (the `terrainSplatActive` loop in `triangle.frag` that calls `perturbNormal` per splat layer) shades with a mirrored V axis: north/south-facing micro-relief reads inverted (bumps as dents) while east/west relief is correct. Affects FNV / FO3 / Oblivion / Skyrim exterior ground wherever a splat layer has a normal map. Visual only. Secondary site, same root cause, much lower impact: `byroredux/src/cell_loader/water.rs` uses the same tuple on the water quad, producing a mirrored ripple pattern rather than wrong-looking relief. Worth fixing in the same change for convention consistency, not on its own merits.
- **Related**: #2474 (the closed zero-tangent terrain fix — guard intact); `bitangent_sign` / #1516.
- **Suggested Fix**: Change `Vertex::new_terrain`'s `tangent` to `[1.0, 0.0, 0.0, -1.0]` (and the water quad likewise), and extend `terrain_vertex_carries_a_nonzero_tangent` into a derivation guard asserting `w · cross(N, T) ≈ ∂P/∂v` for the LAND grid's actual `(row, col) → (position, uv)` mapping. Alternatively drop the `(1.0 - row/32)` V flip in `terrain.rs`; either change alone fixes it, both together re-break it.

## Completeness Checks
- [ ] **SIBLING**: Same tangent-sign fix applied to `byroredux/src/cell_loader/water.rs`'s water quad, which carries the identical hard-coded tuple and the same root-cause UV mirroring
- [ ] **TESTS**: `terrain_vertex_carries_a_nonzero_tangent` extended into a derivation guard asserting `w · cross(N, T) ≈ ∂P/∂v` for the LAND grid's actual `(row, col) → (position, uv)` mapping

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding `REN-D19-01`)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2822
