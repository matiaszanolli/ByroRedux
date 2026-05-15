# Renderer Audit — Dimension 17: Water Rendering (M38) — 2026-05-14

**Auditor**: Claude Sonnet 4.6 (1M context)  
**Scope**: `--focus 17 --depth deep`  
**Prior audit refs**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` (F-WAT-01 through F-WAT-08)  
**Open-issue baseline**: `/tmp/audit/renderer/issues.json` — no water-specific open issues

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW**

The M38 water subsystem is architecturally sound. All 8 prior findings from the May 13 audit have been resolved or accepted, except one carryover (F-WAT-06). Four new LOW findings surface: missing propagation of the WATR `reflection_color` field, a constant hit-colour limitation, a CULL_MODE static-vs-dynamic state fragility, and a misleading coordinate-space doc comment.

**Important correction from May 13 audit**: `F-WAT-01` (rated HIGH — "refract sign wrong") was a **false positive**. The `-V` incident direction is correct per GLSL `refract(I, N, eta)` spec — I must point camera→fragment, and `-V` = `normalize(worldPos - cameraPos)` is exactly that. No fix was needed or applied.

| Sev | Count | IDs |
|-----|------:|-----|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 4 | F-WAT-06 (carryover), F-WAT-09, F-WAT-10, F-WAT-11, F-WAT-12 |

---

## Prior Finding Status (F-WAT-01 through F-WAT-08)

| ID | Title (May 13) | Status |
|----|----------------|--------|
| **F-WAT-01** | Refract `-V` sign wrong | **FALSE POSITIVE.** `-V` = camera→fragment = correct GLSL `refract(I,N,eta)` incident direction. Shader comment at `water.frag:25` also confirms the convention. No fix was needed. |
| **F-WAT-02** | Refraction miss returns sky tint under surface | **FIXED** (#1015). Miss fallback for refraction path is `push.deep.rgb`; reflection miss is `skyTint.xyz`. Both correct. |
| **F-WAT-03** | TLAS build skips `is_water` check | **FIXED.** `draw_command_eligible_for_tlas` in `predicates.rs:314-315` gates on `!draw_cmd.is_water`, covered by a dedicated unit test. |
| **F-WAT-04** | Grazing-angle normal clamp at 60% | **FIXED** (#1025). Gram-Schmidt projection into the `dot(N_perturbed, N) >= 0.05` half-space + hard fallback to geometric N if still below horizon. |
| **F-WAT-05** | No-resort contract unasserted | **FIXED.** `debug_assert!` at `draw.rs:1950`, 6 unit tests in `water.rs:519-610`. |
| **F-WAT-06** | Duplicate trig in WATR resolver | **CARRYOVER.** Still present (see findings). |
| **F-WAT-07** | Water bypasses MaterialTable | **ACCEPTED / INFO.** Tracked by #1067 (water shader material-buffer guard). |
| **F-WAT-08** | Dead vUV/vInstanceIndex interpolators | **FIXED** (#1036). Removed from both shaders in lockstep. |

---

## RT Pipeline Assessment — Water

- **Reflection rays**: origin + normal-offset bias, `reflect(-V, Nperturbed)`, miss → `skyTint.xyz`. CLEAN.
- **Refraction rays**: `refract(-V, Nperturbed, 1.0/ior)`, IOR 1.33 from push constant, miss → `push.deep.rgb`. CLEAN.
- **TLAS self-hit exclusion**: `is_water=true` gates the TLAS build predicate; water surfaces do not contribute shadow coverage for opaque geometry. CLEAN.
- **Hit-colour fidelity**: all hits return a constant (`mix(skyTint.xyz, vec3(0.65,0.70,0.75), 0.4)`) regardless of geometry type. KNOWN LIMITATION — see F-WAT-10.

---

## Findings

### LOW

#### F-WAT-06 — Duplicate trig in WATR resolver

- **Severity**: LOW
- **Dimension**: Water (M38)
- **Location**: `byroredux/src/cell_loader/water.rs:346-356`
- **Status**: CARRYOVER (from 2026-05-13)
- **Description**: `theta.cos()` and `theta.sin()` are computed twice (once for `WaterFlow.direction`, once for `mat.scroll_a/b`); `rec.params.wind_speed.abs().max(0.5)` is computed twice. The compiler may CSE transcendentals, but the duplication is a readability and maintenance hazard.
- **Evidence**:
  ```rust
  let theta = rec.params.wind_direction;
  flow = Some(WaterFlow {
      direction: [theta.cos(), 0.0, theta.sin()],   // computed here
      speed: rec.params.wind_speed.abs().max(0.5),  // computed here
  });
  let dir = (theta.cos(), theta.sin());             // computed AGAIN
  let speed = rec.params.wind_speed.abs().max(0.5); // computed AGAIN
  ```
- **Suggested Fix**: Cache into locals before both uses:
  ```rust
  let (cos_theta, sin_theta) = (theta.cos(), theta.sin());
  let speed = rec.params.wind_speed.abs().max(0.5);
  flow = Some(WaterFlow { direction: [cos_theta, 0.0, sin_theta], speed });
  let dir = (cos_theta, sin_theta);
  ```

---

#### F-WAT-09 — WATR `reflection_color` parsed but never propagated

- **Severity**: LOW
- **Dimension**: Water (M38)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:78` (parsed) + `byroredux/src/cell_loader/water.rs:298-306` (not transferred)
- **Status**: NEW
- **Description**: `WaterParams::reflection_color: [f32; 3]` is parsed from WATR DATA correctly. `WaterMaterial` (engine component) has no matching field; `resolve_water_material` never transfers it. The shader hard-codes the reflection hit colour: `mix(skyTint.xyz, vec3(0.65, 0.7, 0.75), 0.4)`, ignoring the per-record Bethesda authoring. Different water types (lava, chemical, muddy river) each have distinct WATR `reflection_color` tuning that is silently dropped.
- **Evidence**:
  ```rust
  // water.rs (ESM): field parsed
  pub reflection_color: [f32; 3],  // line 78

  // cell_loader/water.rs: never transferred
  mat.shallow_color = rec.params.shallow_color;  // transferred
  mat.deep_color    = rec.params.deep_color;     // transferred
  // mat.reflection_tint = ...  ← ABSENT
  ```
  ```glsl
  // water.frag:216 — hard-coded, ignores WATR tint
  return mix(skyTint.xyz, vec3(0.65, 0.7, 0.75), 0.4);
  ```
- **Impact**: Per-WATR reflection tint lost. All water bodies share the same neutral grey-sky reflection hit colour. Low visible impact for clear-water records; lava / chemical / deep-ocean water would look wrong when reflection rays hit scene geometry.
- **Suggested Fix**: Add `reflection_tint: [f32; 3]` to `WaterMaterial`. Propagate from `rec.params.reflection_color`. Grow `WaterPush` by 1 `vec4` (currently 112 B; Vulkan minimum push constant budget is 128 B — exactly 16 B = 1 vec4 of headroom). Sample multiplicatively on the reflection hit path in `traceWaterRay`.

---

#### F-WAT-10 — `traceWaterRay` returns a constant colour for all geometry hits

- **Severity**: LOW (known architectural limitation, needs tracking)
- **Dimension**: Water (M38)
- **Location**: `crates/renderer/shaders/water.frag:216`
- **Status**: NEW
- **Description**: Both reflection and refraction ray hits return the same constant regardless of the actual geometry hit:
  ```glsl
  return mix(skyTint.xyz, vec3(0.65, 0.7, 0.75), 0.4);
  ```
  This was an acknowledged design trade-off to avoid binding material/vertex/index SSBOs in the water pipeline. However, the comment does not record this as intentional debt, and there is no tracking issue for it.
- **Impact**: Refracted geometry (lake floors, sunken objects) and reflected geometry all look the same neutral grey. On cells with shallow/clear water where refracted geometry is prominently visible, this produces a visually uniform lake floor.
- **Suggested Fix**: Short-term: add a `// TODO(M38-Phase2): traceWaterRay returns constant; requires SSBO bindings in water pipeline to fetch hit albedo` comment and open a tracking issue. Long-term: plumb `rayQueryGetIntersectionInstanceCustomIndexEXT` + material SSBO lookup into the water pipeline descriptor set.

---

#### F-WAT-11 — Water pipeline static CULL_MODE fragility

- **Severity**: LOW
- **Dimension**: Water (M38)
- **Location**: `crates/renderer/src/vulkan/water.rs:377-384`
- **Status**: NEW
- **Description**: The water pipeline declares CULL_MODE as **static** (baked `NONE` in rasterizer state, absent from `dynamic_states`). All opaque and blend pipelines declare it dynamic. When the water pipeline is bound, the prior `cmd_set_cull_mode` from the opaque pass is discarded and the static `NONE` applies — which is correct. However, there is no assertion that the post-water pipeline sequence does not include any CULL_MODE-dynamic pipeline without a `cmd_set_cull_mode` call. Currently safe (UI is also static-cull), but fragile if a pipeline is inserted after water.
- **Evidence**:
  - `water.rs:377-384`: `dynamic_states` does not contain `vk::DynamicState::CULL_MODE`
  - `draw.rs:1936-1939`: comment asserts "subsequent UI pipeline also has cull static" — documented but not asserted
- **Suggested Fix**: Add `DynamicState::CULL_MODE` to water pipeline's `dynamic_states` and issue `cmd_set_cull_mode(cmd, CullModeFlags::NONE)` at the start of the water draw section. Makes the cull state explicit and eliminates the fragile "next pipeline also has static cull" invariant.

---

#### F-WAT-12 — `WaterFlow.direction` doc comment wrong coordinate space

- **Severity**: LOW / INFO
- **Dimension**: Water (M38)
- **Location**: `crates/core/src/ecs/components/water.rs:191`
- **Status**: NEW
- **Description**: The doc comment says: *"Z component is non-zero for waterfalls (typically `-1` — falls go down in world Z-up before the Y-up swizzle)"*. The engine runs Y-up; `WaterFlow` is populated after the Z→Y swizzle. For waterfalls in Y-up, the downward component is Y, not Z. The comment's "world Z-up before the Y-up swizzle" is misleading — by the time `WaterFlow` is populated, the swizzle has already happened.
- **Impact**: No runtime impact — actual values at `cell_loader/water.rs:348` are correct for Y-up. Misleads future maintainers about the coordinate space.
- **Suggested Fix**: `"Y component is typically -1.0 for waterfalls (falls are downward in world Y-up space); horizontal currents keep Y=0."`

---

## Verified-Clean List

| # | Checklist Item | Evidence |
|---|----------------|---------|
| 1 | WaterPlane ECS spawned from XCWT | `spawn_water_plane` reads `xcwt_form`, maps to `WaterMaterial + WaterKind`. |
| 2 | Vertex displacement amplitude / NaN | N/A — water.vert is a flat static quad; wave detail is fragment-only normal perturbation. |
| 3 | Fresnel — Schlick, F0 ~0.02 | `water.frag:391`: `F0 = push.misc.x` (default 0.02). Correct Schlick formula. |
| 4 | RT reflection — miss → sky | `water.frag:398`: miss fallback = `skyTint.xyz`. |
| 5 | RT refraction — IOR 1.33, miss → deep_color | IOR from `push.timing.w`; miss fallback = `push.deep.rgb`. |
| 6 | Refract incident-vector sign | `-V` = camera→fragment = correct GLSL incident direction. |
| 7 | Refraction miss fallback | `push.deep.rgb`, not sky. |
| 8 | SubmersionState strobe at boundary | Depth gate `depth > 0.0` prevents strobe; state only flips when camera truly crosses AABB. |
| 9 | Cell unload — water entity cleanup | `unload_cell` drops mesh; water BLAS is excluded (`blas=false`); entity despawned cleanly. |
| 10 | Shadow casting — water excluded | `!draw_cmd.is_water` in TLAS predicate; water does not shadow opaque geometry. |
| 11 | Two-sided via static CULL_MODE NONE | Pipeline bakes NONE; comment at `draw.rs:1936-1939` explains the transition. |
| 12 | Sort key — after opaques | Water draws structurally ordered after all opaque+blend batches in `draw.rs:1919`. |
| 13 | Material slot — water vs glass no dedup | Water uses `material_kind=0`; glass uses `MATERIAL_KIND_GLASS=100`. No collision. |
| 14 | TLAS self-hit exclusion | `predicates.rs:314-315` + unit test. |
| 15 | Grazing-angle normal clamp | Gram-Schmidt + hard fallback (#1025). |
| 16 | WaterDrawCommand instance_index assertion | `debug_assert!` + 6 unit tests. |
| 17 | WATR trig duplication | Carryover F-WAT-06. |
| 18 | Dead interpolators | Removed (#1036). |
| 19 | Underwater tint/fog | Beer-Lambert tint in `composite.frag:547-558`; sky-branch tinted at `:335-338`. |
| 20 | F-WAT-01 retraction | `-V` sign confirmed correct per GLSL spec. Prior HIGH finding was a false positive. |

---

## Prioritized Fix Order

| Priority | Finding | Effort |
|----------|---------|--------|
| 1 | **F-WAT-06** — trig dedup in WATR resolver | 2-line local cache |
| 2 | **F-WAT-12** — coordinate-space doc comment | 1-line edit |
| 3 | **F-WAT-09** — propagate WATR `reflection_color` | Small–medium (new field, `WaterPush` grow, shader change + SPIR-V recompile) |
| 4 | **F-WAT-11** — CULL_MODE dynamic state hygiene | Small (add dynamic state + one `cmd_set_cull_mode` call) |
| 5 | **F-WAT-10** — constant hit colour | Large / deferred (M38 Phase 2; needs SSBO in water pipeline) |

---

## Methodology Notes

- Full read of `water.frag`, `water.vert`, `water.rs` (Rust), `cell_loader/water.rs`, `systems/water.rs`, `components/water.rs`, `records/misc/water.rs`, `unload.rs`. Partial reads of `render.rs`, `draw.rs`, TLAS files, `composite.frag`.
- For F-WAT-01: verified GLSL `refract(I, N, eta)` spec — I is the incident direction (camera→surface); `-V` where V=`normalize(cameraPos - worldPos)` is exactly camera→surface. Prior audit's premise was wrong.
- Cross-referenced against `issues.json` — no water-specific open issues at audit time.

---

*Generated by `/audit-renderer 17` on 2026-05-14. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-14_DIM17.md`*
