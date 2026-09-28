# #4979: REN-D12-2026-09-27-03: `water.frag` and `groundcover_blade.frag` ignore every non-water structured debug view — lit HDR water and grass leak into raw correctness views

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4979
- **Labels**: low,renderer,shaders,water,terrain-exterior,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D12-2026-09-27-03**._

- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/shaders/water.frag:1289-1308` (only `RENDER_DEBUG_WATER_REFL/_TERM/_NORMAL` are handled; everything else falls through to `outColor = vec4(surfaceColor, alpha)`); `crates/renderer/shaders/groundcover_blade.frag` (no `renderDebug`/`RENDER_DEBUG_*` reference at all); draw site `crates/renderer/src/vulkan/context/geometry_pass.rs` (water block, then `gc.record_draw` inside the main pass)
- **Status**: NEW
- **Description**:
  - `triangle.frag` handles the converse: under the water modes, non-water surfaces paint flat 0.08 grey (`viewWaterDebug`, `docs/engine/watal.md`).
  - Nothing does the same for the triangle-side views on water and blades. Under `shadow_visibility`, `selected_light`, `direct_only`, `indirect_only`, `material_lobe`, `rt_lod`, `material_role`, `terrain_lod`, `facing_ratio` or `restir_light`, water and procedural blades write their normally lit, fogged HDR colour into attachment 0.
  - `render_debug_requires_raw_output(_, mode)` then routes the frame raw through composite, bloom, TAA, FSR and presentation. Presentation clamps it to [0,1] as if it were a categorical or scalar oracle value.
- **Evidence**: `grep -l 'RENDER_DEBUG_\|renderDebug' shaders/*.frag` returns presentation, composite, water and triangle, but not `groundcover_blade.frag`. `water.frag` tests only the three water discriminants. Ground-cover *model* shapes are unaffected because they draw with the triangle pipeline.
- **Impact**:
  - In exteriors, the "raw" oracle views show lit water and grass as bright false colours. A `shadow_visibility` or `restir_light` frame over a lake or meadow cannot be read in those regions.
  - The selected-ray probe on such a pixel reports "no fragment captured", which looks like a probe failure.
  - Cornell oracles are unaffected (no water or grass there). The doc promise in `renderer.md` ("These categorical/scalar views … are raw frame-graph oracles") is violated in exteriors.
- **Related**: #4867 (legacy-bit exclusivity, now fixed in `triangle.frag`); REN-D7-2026-09-27-04.
- **Suggested Fix**: Give `water.frag` and `groundcover_blade.frag` the symmetric rule: for any structured mode other than FINAL, COMPOSITE_TERM, VOLUMETRIC_TERM or their own modes, paint the same flat non-participant grey (or implement the view). Pin it with a source test that every main-pass fragment shader writing attachment 0 references `RENDER_DEBUG_FINAL`. The shader change needs `check-shader-artifacts.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
