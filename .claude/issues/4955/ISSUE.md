# #4955: REN-D3-2026-09-27-04: #4846's generated `RENDER_LAYER_ARCHITECTURE` / `FOG_VOLUME_SHAPE_*` reached only the named sites — sibling discriminant literals remain in GLSL and in the host mirror

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4955
- **Labels**: low,renderer,shaders,tech-debt,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D3-2026-09-27-04**._

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (flag constants)
- **Location**:
  - `crates/renderer/shaders/triangle.frag:1758` (`bool isArchitecturalGlass = renderLayer == 0u;`)
  - `triangle.frag:4395` (`&& renderLayer != 3u && …` — `RenderLayer::Decal = 3`)
  - `triangle.frag:860-862` (render-layer debug viz `layer == 0u/1u/2u`)
  - `crates/renderer/shaders/volumetrics_inject.comp:830-846` (`shape > 2.5` / `> 1.5` / `< 0.5` shape dispatch) and `:1100` (`center_shape.w > 1.5`)
  - host `crates/renderer/src/vulkan/volumetrics.rs:596,598,664-665,688` (`center_shape[3] < 0.5` / `> 2.5` / `< 1.5` / `< 2.5`)
- **Status**: NEW (a sibling gap of closed #4846; its SIBLING completeness box was never ticked)
- **Description**: `e26441c34` added generated `RENDER_LAYER_ARCHITECTURE` and `FOG_VOLUME_SHAPE_{SPHERE,ELLIPSOID,BOX,CONE}`, derived from the core `RenderLayer` / `FogShape` enums. It converted only the sites #4846 listed (`traceArchitecturalWindowGlass`, `localSkyAperture`, `draw.rs` `build_composite_params`).
  - The same discriminants are still hand-typed at the locations above.
  - `triangle.frag` compares `renderLayer` against literal `0u` and `3u` even though it already derives the layer through the generated `INSTANCE_RENDER_LAYER_SHIFT/_MASK`.
  - The skill rule is that `MATERIAL_KIND_*` / `INSTANCE_FLAG_*`-class discriminants come from `shader_constants_data.rs` and are never hand-written shader-side.
- **Evidence**: `grep -n 'renderLayer == 0u\|renderLayer != 3u' triangle.frag`; `grep -n 'shape > 2.5\|shape > 1.5\|shape < 0.5\|center_shape.w > 1.5' volumetrics_inject.comp`. Both lines were blamed to `6c56e3115` (2026-07-19), so they predate the fix.
- **Impact**: The values are correct today. Reordering `RenderLayer` or `FogShape` would silently break architectural-glass classification, coverage-only blending of decals and the froxel shape dispatch, while the #4846 tests stay green.
- **Related**: #4846 (closed), #2045, #4027, #4584.
- **Suggested Fix**: Emit the remaining `RENDER_LAYER_*` values (Clutter/Actor/Decal) and use them together with `FOG_VOLUME_SHAPE_*` at every listed site. Compare shapes with `==` on the ids instead of `x.5` thresholds, on both the GLSL and the host side. Add a source scan that bans `renderLayer [=!]= [0-9]u`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
