# Renderer Audit — Dimension 17: Water Rendering (M38)

**Date:** 2026-06-04
**Scope:** Single-dimension focus (`--focus 17`, depth `deep`). WATR resolver →
`WaterMaterial`/`WaterPush` → `water.vert`/`water.frag` → water-side caustic
synthesis (#1210 Phases A–E) → composite reassembly. Cross-references the
glass-caustic path (`caustic_splat.comp`) and the sun-direction source of truth
(`compute_sun_arc` → `CellLightingRes`/`SkyParamsRes` → `CameraUBO.sunDirection`
+ `GpuLight.direction_angle`).

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 0 |
| LOW | 0 new |
| INFO (pass-through) | 8 |

The M38 water subsystem is **structurally complete** and #1210 Phases A–E
**did fully land**: the per-frame-in-flight R32_UINT accumulator
(`water_caustic.rs`), the in-pass `imageAtomicAdd` from `water.frag`, the
`CameraUBO.sunDirection` plumb, and the composite-side sample all exist and wire
together. The prior "untracked, unimplemented" gap (REN-DIM17-01) is **closed by
implementation**.

The one open finding is a **sun-direction sign inversion** in the caustic
synthesis that shipped: `water.frag` treats the host's *to-sun* vector as a
*light-travel* vector, so the sun-visibility shadow ray fires downward into the
floor, the `refract()` incident vector is wrong-signed, and the Lambert weight
`dot(N, -sunRay)` clamps to zero exactly when the sun is overhead. Net effect:
water caustics produce ~zero output for any above-horizon sun — a
shipped-but-dead feature. The identical pattern is latent in
`caustic_splat.comp` (glass caustics).

**Severity calibration note:** the dimension agent initially rated this CRITICAL.
Per `_audit-severity.md`, CRITICAL is reserved for UB / crash / corruption /
AS-or-SSBO-index corruption. This is a shader-math sign error producing
wrong/absent output — "affects rendering correctness → at least HIGH". It is
filed here as **HIGH** (broad blast radius: all daytime exterior water), not
CRITICAL.

---

## RT Pipeline Assessment

The water RT paths (reflection/refraction via TLAS, caustic shadow + floor
trace) are structurally sound: `tMin 0.05` is consistent across the shadow ray,
floor ray, foam shoreline, `caustic_splat`, and `triangle.frag`'s refraction
loop (RT-01 / #1388); the refraction origin-bias steps `-N` into the water,
matching the transmission convention in `triangle.frag` and `caustic_splat.comp`
(and must **not** be flipped when fixing the sign bug). The only RT-side defect
is the sun-vector sign in the caustic block (REN-DIM17-02), which is a geometry
error, not a synchronization or AS-build error.

## Rasterization Assessment

Clean. WATR resolve, `reflection_color` propagation (#1069), dynamic `CULL_MODE`
(#1071/#1129), and `WaterFlow.direction` docs all re-verified against their prior
closures. No new pipeline-state, render-pass, or command-recording findings.

---

## Findings

### HIGH

#### REN-DIM17-02: water caustic sun-direction sign is inverted — caustics suppressed for overhead sun

- **Severity:** HIGH (rendering correctness; broad — all daytime exterior water)
- **Dimension:** Water (M38)
- **Location:** `crates/renderer/shaders/water.frag:533-602` (caustic block)
- **Source-of-truth:** `byroredux/src/systems/weather.rs:90-107` (`compute_sun_arc`),
  `byroredux/src/render/lights.rs:89-93` (`direction_angle` fill),
  `crates/renderer/src/vulkan/context/draw.rs:687-698` (`CameraUBO.sunDirection` fill)
- **Status:** NEW
- **Description:** The host's sun vector points **toward the sun**, not along
  light travel. `compute_sun_arc` builds `sun_dir` as `[cos θ, sin θ, tilt]`: at
  solar noon `θ≈π/2` → `[0, +1, ~0.15]` (straight up); night fallback `[0, -1, 0]`.
  This same vector is copied verbatim into both `GpuLight.direction_angle.xyz`
  (consumed by `triangle.frag`) and `CameraUBO.sunDirection.xyz` (consumed by
  `water.frag`). `triangle.frag` fires its directional shadow ray along `+L`
  (toward the sun) — correct. `water.frag` treats the same vector as
  light-travel and fires along `-sunRay` — three sign errors follow.
- **Evidence:**
  - `weather.rs:97-107`: noon → `[0,+1,~0.15]` (verified: `x=cos≈0, y=sin≈1, z=0.15`).
  - `triangle.frag:3083`: `L = normalize(lights[i].direction_angle.xyz)`;
    `triangle.frag:3136-3137`: `rayDir = normalize(L + jitter)` → toward the sun. Correct.
  - `water.frag:534`: `vec3 sunRay = normalize(sunDirection.xyz); // light-travel direction` (mislabel).
  - `water.frag:540`: shadow ray direction = `-sunRay` → points **down into the floor**;
    almost always hits terrain → `sunVisible` false → caustic branch skipped under a clear noon sky.
  - `water.frag:551`: `refract(sunRay, N, 1.0/1.33)` — GLSL `refract(I,N,eta)` requires
    `I` = propagation direction = `-sunRay`; passing `sunRay` with upward `N` gives
    `dot(I,N) > 0`, selecting the wrong refraction branch.
  - `water.frag:594`: `NdotSun = max(dot(N, -sunRay), 0.0)` — with `N` up and `sunRay` up,
    `dot(N, -sunRay) < 0` → clamped to **0** at the overhead-sun geometry where caustics should peak.
- **Impact:** Water-side caustics produce ~zero output for any above-horizon
  sun. The #1210 / M38 feature ships disabled-in-practice. The failure is a
  black/absent caustic — visually indistinguishable from "not yet implemented"
  and invisible to `cargo test`.
- **Suggested Fix (direction only — do NOT flip the refract origin-bias, which
  is correct):**
  - `water.frag:540` shadow ray dir `-sunRay` → `sunRay`.
  - `water.frag:551` `refract(sunRay, …)` → `refract(-sunRay, …)`.
  - `water.frag:594` `dot(N, -sunRay)` → `dot(N, sunRay)`.
  - Correct the misleading "light travel" comments at `water.frag:111`,
    `water.frag:534`, and `draw.rs:687` to "points toward the sun (light-incoming)".
- **Related (sibling, same change):** `caustic_splat.comp:258-279` carries the
  identical inverted pattern (`LtoG = normalize(direction_angle.xyz)`,
  `dot(N, -LtoG)`, `refract(LtoG, …)`) under the mislabel "Light-to-Ground".
  Glass caustics are latently broken the same way (less noticed: the only
  directional consumer is the sun and most test-content glass is interior). Fix
  both shaders in one change to keep the two accumulators on a consistent
  physical basis — they already share `CAUSTIC_FIXED_SCALE`.
- **Validation:** Per the project "no speculative Vulkan/shader fixes without
  RenderDoc or revert" policy, validate with a RenderDoc capture of an exterior
  water cell at noon — confirm `waterCausticAccum` is non-zero after the fix.

---

## INFO — Verification Pass-Throughs (checklist items confirmed correct)

- **#1210 Phases A–E landed** (contra the 2026-05-19 "unimplemented" status of
  REN-DIM17-01): accumulator (`vulkan/water_caustic.rs`, per-FIF R32_UINT,
  cleared **before** the render pass), `water.frag`'s in-pass `imageAtomicAdd`
  (`water.frag:602`), the `CameraUBO.sunDirection` plumb (`draw.rs:687-698`),
  and composite sampling all exist. Implementation present; correctness is the
  open issue (REN-DIM17-02), not absence.
- **Sun-direction plumb is unit-length, world-space.** `draw.rs:689` asserts
  `SkyParams.sun_direction` is already normalized; `compute_sun_arc` normalizes
  (`weather.rs:103-104`). `water.frag:534` re-normalizes defensively. OK.
- **`CAUSTIC_FIXED_SCALE` parity.** `water.frag:597` and `caustic_splat.comp`
  share the same fixed-point scale; composite divides both accumulators by one
  constant. Magnitude basis consistent — only the sign/geometry is wrong.
- **Refraction origin-bias convention is internally correct.** `water.frag:570`
  steps `-N*0.05` into the water for the floor trace (matching
  `fragWorldPos - N*0.15` in `triangle.frag` and `G - ns*0.1` in
  `caustic_splat`). Must NOT be flipped when fixing the sign bug.
- **tMin 0.05 consistency** across shadow ray, floor ray, foam shoreline,
  `caustic_splat`, and `triangle.frag` refraction loop (RT-01 / #1388). Verified.
- **TIR length-gate** (`length(refractDir) > 1e-4`, `water.frag:552`) present
  and correct for air→water; stays a cheap safety no-op once the incident sign
  is fixed (TIR cannot occur entering the denser medium).
- **Caustic source predicate excludes water by design.** `is_caustic_source`
  gates the glass path on `MATERIAL_KIND_GLASS` + MultiLayerParallax refraction;
  water is intentionally out, handled by the dedicated water accumulator.
  Architectural split intact.
- **Composite reassembly** divides each accumulator by `CAUSTIC_FIXED_SCALE` and
  adds to **direct** lighting before ACES tone-map. No double-count of the water
  vs glass accumulators, and the caustic is not leaked into the SVGF-denoised
  indirect path. OK.

---

## Dedup — Prior Known Findings

| ID | Title | Status this pass |
|----|-------|------------------|
| REN-DIM17-01 | Water-side caustics deferred to `water.frag`, unimplemented + untracked | **Closed-by-implementation.** #1210 Phases A–E implemented the deferred caustic. Replacement concern is correctness, filed fresh as REN-DIM17-02. Do not re-file. |
| F-WAT-06 | Duplicate trig in WATR resolver | **Fixed, re-verified** (`theta.sin_cos()` + single `speed`, `byroredux/src/cell_loader/water.rs:344-348`). |
| F-WAT-09 | WATR `reflection_color` parsed but never propagated | **Fixed, re-verified** (#1069; `tint_reflect.w` consumed at `water.frag:492`). |
| F-WAT-10 | `traceWaterRay` constant hit colour | **Documented limitation (#1070).** Out of DIM17-02 scope. No change. |
| F-WAT-11 | Water pipeline static CULL_MODE fragility | **Fixed, re-verified** (`vk::DynamicState::CULL_MODE`, `crates/renderer/src/vulkan/water.rs:190`; test `water_pipeline_dynamic_states_cover_documented_no_ops`, #1129). |
| F-WAT-12 | `WaterFlow.direction` Z-up doc comment wrong | **Fixed, re-verified** (`crates/core/src/ecs/components/water.rs:197-199`). |

---

## Prioritized Fix Order

1. **REN-DIM17-02 (HIGH)** — Restore water (and glass) caustics: flip the three
   sun-vector signs in `water.frag` and `caustic_splat.comp`, correct the three
   misleading "light-travel" comments. ~6 lines across both shaders; one issue
   tagged `M38` / `#1210-followup`. Validate via RenderDoc (noon exterior water
   cell, `waterCausticAccum` non-zero).

No other findings.
