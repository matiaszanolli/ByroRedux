# Renderer Audit — 2026-05-19 (Dim 17 focused, Water M38)

**Scope**: Dimension 17 — Water Rendering (M38)
**Mode**: `--focus 17 --depth deep`
**Baseline**: [AUDIT_RENDERER_2026-05-14_DIM17.md](AUDIT_RENDERER_2026-05-14_DIM17.md) (4 LOW findings) — all five tracked items (F-WAT-06 / 09 / 10 / 11 / 12) have since been closed via the midnight-run batches + #1069 / #1070 / #1071 / #1129 / #1187. This pass verifies the closures and surfaces one new LOW item.
**Companion**: this morning's full 20-dim renderer audit at [AUDIT_RENDERER_2026-05-19.md](AUDIT_RENDERER_2026-05-19.md) reported only #1187 (doc-only) + #1129 (forward-compat test) since 2026-05-16; no Dim-17 runtime change. This focused pass goes deeper on the M38 checklist + cross-references today's #1199 cell-loader fix.

## Executive Summary

**0 CRITICAL · 0 HIGH · 0 MEDIUM · 1 LOW · 5 INFO**

| Severity | Count |
|----------|------:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |
| INFO | 5 |

### Headline

**REN-DIM17-01 (LOW)** — water-side caustics are untracked. `caustic_splat.comp:211-215` explicitly defers water-caustic to the water shader's responsibility ("the water-side caustic is the water shader's responsibility (M38)"). `water.frag` contains zero caustic implementation. The closure of #1070 (constant hit-colour limitation) was the last water-related architectural tracker; nothing remains in the open backlog pointing at the gap. Without a tracking issue, this perpetually slips through audits.

### Dedup status

**Prior carryovers — all FIXED since 2026-05-14**:

| Prior ID | Topic | Status |
|----------|-------|--------|
| F-WAT-06 | WATR resolver trig duplication | ✅ FIXED — `theta.sin_cos()` consolidated at [cell_loader/water.rs:346](byroredux/src/cell_loader/water.rs#L346); `speed` computed once at line 347 |
| F-WAT-09 | WATR `reflection_color` not propagated | ✅ FIXED via #1069 (8fc12b99) — propagated through `WaterMaterial` + `WaterPush` (push constant grew to 128 B; #1087 string drift fixed in 61691170) |
| F-WAT-10 | `traceWaterRay` constant hit colour | ✅ DOCUMENTED via #1070 (51281f3d) — comment in `water.frag` records the M38-Phase-2 deferral |
| F-WAT-11 | CULL_MODE static-state fragility | ✅ FIXED via #1071 — `CULL_MODE` declared dynamic at [crates/renderer/src/vulkan/water.rs:190](crates/renderer/src/vulkan/water.rs#L190); caller emits `cmd_set_cull_mode(NONE)` before water draws; forward-compat coverage test via #1129 |
| F-WAT-12 | `WaterFlow.direction` Z-up doc comment wrong | ✅ FIXED — comment at [components/water.rs:198-199](crates/core/src/ecs/components/water.rs#L198-L199) now reads "Unit vector in world Y-up space. Y component is typically -1.0 for waterfalls" |

### #1199 cell-loader interaction

**Clean.** Today's #1199 fix (`unload_cell` no longer wipes `WeatherDataRes` / `SkyParamsRes` / `CellLightingRes` / `WeatherTransitionRes`) is **strictly positive** for water rendering:

- Grep across `crates/renderer/src/vulkan/water.rs`, `byroredux/src/render/water.rs`, `byroredux/src/systems/water.rs`, `crates/renderer/shaders/water.frag` returns **zero references** to any of the four wiped resources. Water rendering reads them indirectly only via the composite pass's underwater tint / fog application — which pulls from `CellLightingRes` and was previously broken across cell boundaries.
- Net effect on M40-streaming exterior cells: post-#1199, underwater fog distances correctly persist across cell unloads. Previously, the first cell-out-of-range event wiped `CellLightingRes` and the composite pass's underwater tint went to defaults. This is one of the four downstream invariants #1199 unlocked (Dim 9 / 10 / 18 / 20 all benefit).

No water-specific Dim-17 finding emerges from the #1199 fix; only an upstream improvement.

---

## RT Pipeline Assessment — Water (post-2026-05-14)

- **Reflection rays**: origin + normal-offset bias, `reflect(-V, Nperturbed)`, WATR-propagated `reflection_color` multiply on hit (#1069), `skyTint.xyz` miss fallback. CLEAN.
- **Refraction rays**: `refract(-V, Nperturbed, 1.0/ior)`, IOR 1.33 from push, `push.deep.rgb` miss fallback. CLEAN.
- **TLAS self-hit exclusion**: `!draw_cmd.is_water` gates the TLAS predicate ([predicates.rs:314-315](crates/renderer/src/vulkan/acceleration/predicates.rs#L314-L315) + dedicated unit test). CLEAN.
- **Hit-colour fidelity on reflection ray hits**: constant `mix(skyTint, vec3(0.65, 0.7, 0.75), 0.4)` regardless of geometry hit. KNOWN LIMITATION, documented in shader comment + #1070 closure note. M38-Phase-2 will need SSBO bindings in the water pipeline.
- **Caustic interaction**: **water is intentionally excluded** from `caustic_splat.comp` source predicate (only `MATERIAL_KIND_GLASS` + MultiLayerParallax with refraction; verified at [context/draw.rs:52](crates/renderer/src/vulkan/context/draw.rs#L52) `is_caustic_source` + tests at [draw.rs:2724-2824](crates/renderer/src/vulkan/context/draw.rs#L2724-L2824)). The water-side caustic is deferred to `water.frag` per the M38 architectural split documented at [caustic_splat.comp:213-215](crates/renderer/shaders/caustic_splat.comp#L213-L215). **Not implemented today** — see REN-DIM17-01.

---

## Findings

### LOW

#### REN-DIM17-01: water-side caustics deferred to water.frag are unimplemented and untracked

- **Dimension**: Water Rendering (M38) / Caustics
- **Files**:
  - [crates/renderer/shaders/caustic_splat.comp:211-215](crates/renderer/shaders/caustic_splat.comp#L211-L215) — documents the architectural split: "the water-side caustic is the water shader's responsibility (M38)"
  - [crates/renderer/shaders/water.frag](crates/renderer/shaders/water.frag) — no `caustic` / `Caustic` identifier anywhere except a comment at line 226 referencing the compute-pass caustic accumulator
  - [crates/renderer/src/vulkan/context/draw.rs:52](crates/renderer/src/vulkan/context/draw.rs#L52) `is_caustic_source` — explicitly gates on `MATERIAL_KIND_GLASS` + MultiLayerParallax+refraction; water is NOT a source
- **Symptom**: Underwater caustic lighting (sun rays bending through the water surface and concentrating into bright bands on the lake/pond floor) is absent. The renderer has the per-light caustic accumulator infrastructure in place but the water surface never contributes to it, and the water shader doesn't synthesise its own underwater-side caustic. Pools / lakes / rivers all look flat under the surface.
- **Cause**: M38 architectural split — caustic_splat handles glass + MLP (transmission caustics seen above the refractor); water-side caustics (transmission caustics seen below the refractor) were carved out as the water shader's responsibility because the compute-pass caustic accumulator samples from the camera's view direction and doesn't naturally handle the "looking through water at a lit floor" case. The architectural decision is documented; the implementation never landed.

  After #1070's closure (2026-05-14 batch), no open issue tracks the water-side caustic. The deferred work is currently invisible to the backlog.
- **Fix** (tracking-only, not implementation):
  - Open a tracking issue tagged `M38-Phase2` to surface the gap.
  - Audit-renderer.md Dim-13 checklist already mentions caustic source predicate; consider adding a Dim-17 checklist item explicitly asserting "water-side caustic implementation status (deferred / wired)" so future audits surface this without re-reading both shaders.
  - Implementation route: per-fragment-on-the-water-surface ray queries against a single shadow ray toward the sun, refract the result at the surface, splat into a R32_UINT accumulator gated to the underwater volume. See REN-D13-NEW-04 (audit 2026-05-09) for the multi-channel / multi-bounce constraints that also apply.
- **Estimated Impact**: visual fidelity on exterior pools / lakes / rivers on sunny TODs. Below FPS-signal threshold; medium visual impact.
- **Regression Risk**: ZERO (no implementation today; tracking-only fix has no code path).
- **Testability**: needs a sunny-exterior bench scene with water — `cargo run --release -- --esm FalloutNV.esm --grid 0,0 --bench-hold` over a water cell. Pre-fix: flat lake floor. Post-fix (when implemented): bright caustic bands.

---

### INFO (verification pass-throughs)

#### REN-DIM17-02: WATR resolver trig consolidation verified

- [byroredux/src/cell_loader/water.rs:344-348](byroredux/src/cell_loader/water.rs#L344-L348) — `theta.sin_cos()` returns both components in one call; `speed = rec.params.wind_speed.abs().max(0.5)` computed once. F-WAT-06 closed.

#### REN-DIM17-03: CULL_MODE dynamic-state hygiene verified (#1071)

- [crates/renderer/src/vulkan/water.rs:190](crates/renderer/src/vulkan/water.rs#L190) declares `vk::DynamicState::CULL_MODE`. Caller emits `cmd_set_cull_mode(NONE)` before water draws (per `#1071 / F-WAT-11` comment at line 188). Forward-compat coverage test `water_pipeline_dynamic_states_cover_documented_no_ops` at [water.rs:474](crates/renderer/src/vulkan/water.rs#L474) (#1129) pins this. The "next pipeline is also static-cull" fragile invariant from 2026-05-14 is gone.

#### REN-DIM17-04: WaterFlow.direction coord-space doc verified

- [crates/core/src/ecs/components/water.rs:197-199](crates/core/src/ecs/components/water.rs#L197-L199) — `"Unit vector in **world Y-up space**. Y component is typically -1.0 for waterfalls (falls are downward in Y-up)"`. F-WAT-12 closed; coordinate-space convention is now correctly stated.

#### REN-DIM17-05: water material distinct from glass via DrawCommand.is_water, not material_kind

- Water uses `material_kind=0` (default); glass uses `MATERIAL_KIND_GLASS=100` ([scene_buffer/constants.rs:193](crates/renderer/src/vulkan/scene_buffer/constants.rs#L193)). The two are disambiguated at the DrawCommand level via `is_water: bool`, not via material kind. R1 byte-Hash dedup can therefore never collapse water and glass because their `GpuMaterial` records differ in `shallow_color / deep_color / wave_*` fields. Verified clean.
- Note: this is a deliberate architectural choice (water shares the same `GpuMaterial` layout as opaque/refractive; the binding into the water pipeline is the distinction). If a future MATERIAL_KIND_WATER is introduced (e.g. to unify water with the caustic predicate per REN-DIM17-01), the dedup behaviour will need re-verification.

#### REN-DIM17-06: #1199 cell-loader fix is strictly positive for water rendering

- Water reads **zero** of the four wiped resources directly. The indirect dependency via the composite pass's underwater tint / fog (`CellLightingRes.fog_*`) was previously broken across cell boundaries; post-#1199 it persists correctly. This is one of the four downstream invariants #1199 unlocked (along with Dim 9 GI miss-fill, Dim 10 fog, Dim 18 volumetrics, Dim 20 soft shadows). No water-side action required.

---

## Verified-Clean Cross-Reference (updated from 2026-05-14)

| # | Checklist Item | Status |
|---|----------------|--------|
| 1 | WaterPlane ECS spawned from XCWT / cell water refs | ✅ |
| 2 | Vertex displacement amplitude bounded | ✅ (static quad; wave detail = fragment-only normal perturbation) |
| 3 | Fresnel — Schlick, F0 ~0.02 | ✅ ([water.frag:391](crates/renderer/shaders/water.frag#L391) `F0 = push.misc.x`, default 0.02) |
| 4 | RT reflection — miss → sky | ✅ ([water.frag:398](crates/renderer/shaders/water.frag#L398) `skyTint.xyz` fallback) |
| 5 | RT refraction — IOR 1.33, miss → deep_color | ✅ (IOR from `push.timing.w`; miss → `push.deep.rgb`) |
| 6 | Refract incident-vector sign | ✅ (`-V` = camera→fragment, GLSL spec) |
| 7 | SubmersionState boundary strobe prevention | ✅ (depth gate `depth > 0.0`) |
| 8 | Cell unload water entity cleanup | ✅ (mesh dropped; water BLAS excluded via `blas=false`; #1199 doesn't touch this path) |
| 9 | Shadow casting — water excluded | ✅ (`!draw_cmd.is_water` in TLAS predicate + unit test) |
| 10 | Two-sided via dynamic CULL_MODE NONE | ✅ (#1071 + #1129 coverage test) |
| 11 | Sort key — water after opaques | ✅ (structural order in `draw.rs`) |
| 12 | Material slot — water vs glass no dedup | ✅ (is_water bool, not material_kind) |
| 13 | TLAS self-hit exclusion | ✅ |
| 14 | Grazing-angle normal clamp | ✅ (#1025 Gram-Schmidt + hard fallback) |
| 15 | WaterDrawCommand instance_index assertion | ✅ (`debug_assert!` + 6 unit tests) |
| 16 | WATR reflection_color propagation | ✅ (#1069) |
| 17 | F-WAT trig dedup | ✅ |
| 18 | Underwater tint/fog | ✅ (Beer-Lambert in [composite.frag:547-558](crates/renderer/shaders/composite.frag#L547-L558); also benefits from #1199 fix) |
| 19 | Constant hit-colour on reflection rays | ⏸ DOCUMENTED LIMITATION (M38-Phase 2; #1070 closed) |
| 20 | Water-side caustic implementation | ❌ DEFERRED, UNTRACKED — REN-DIM17-01 |

---

## Prioritized Fix Order

### Tracking-only

1. **REN-DIM17-01** — open a tracking issue for water-side caustic implementation. Tag `M38-Phase2`. Implementation is medium-large effort (per-fragment ray query + refraction at water surface + R32_UINT accumulator splat); tracking the gap is a 10-minute task.

### Optional checklist hygiene

2. Add a Dim-17 checklist item to `.claude/commands/audit-renderer.md` asserting "water-side caustic implementation status (deferred / wired)" so future Dim-17 audits surface this without re-reading both shaders.

---

## Notes

- All four prior carryovers from the 2026-05-14 focused audit landed in the 2026-05-15→17 batch close-outs. The audit-publish→fix-issue cycle is working well for water.
- No allocation-hot-path findings; the dhat-infra gap is not load-bearing for Dim 17.
- No speculative Vulkan barrier changes proposed; the per-draw water pipeline binding is well-understood post-#1071.
- The Dim-17 renderer-specialist agent ran out of tool budget mid-investigation again (same pattern as Dim 15 this morning). Investigation continued in-thread by reading the 2026-05-14 baseline + spot-checking the closure status of each prior finding. All four carryovers verified FIXED by direct file reads.
