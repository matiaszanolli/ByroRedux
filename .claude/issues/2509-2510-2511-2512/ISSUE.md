# Issues 2509, 2510, 2511, 2512

All four are LOW-severity renderer audit findings (Dimensions 16-19). Domain: **renderer** → `byroredux-renderer` / `byroredux` (binary) crates.

---

## #2509 — REN-D16-2026-08-07-02: Per-froxel shadow-ray budget is up to 10 rays, not the documented single ray

**Severity**: LOW · **Dimension**: 16 — Volumetrics
**Location**: `crates/renderer/shaders/volumetrics_inject.comp:main`
**Status**: NEW (documentation/cost drift, not a correctness bug)

### Description
The design contract (and the file's own header comment, "shadow visibility is the standard 'trace toward light, miss = lit'") describes one `TerminateOnFirstHit` ray per froxel. Current code casts: 1 opaque sun ray, +1 glass-mask sun ray for interiors, and then up to `MAX_FROXEL_LIGHTS = 4` local lights × up to 2 rays each (opaque mask, then glass mask) = up to **10 ray-query traversals per froxel**. At the default grid (160×90×64 = 921,600 froxels) that is a worst case near 9.2M ray queries per frame from the injection pass alone.

### Evidence
`volumetrics_inject.comp:503-519` (sun opaque + interior glass), `:582-601` (`needsVisibility` opaque `traceShadowBinary` then `shadowPolicyUsesGlass` glass `traceShadowBinary`), `:552` `const uint MAX_FROXEL_LIGHTS = 4u`.

### Impact
No visual defect; a GPU-cost cliff in dense-light interiors that the checklist/design docs do not budget for. Also means any future "cost of volumetrics" estimate derived from the docs is off by ~10x.

### Related
M-LIGHT v2 shadow-policy work; #2205 (spot-cone guard in the same loop).

### Suggested Fix
Update the `volumetrics_inject.comp` header comment and the `VOLUMETRIC_OUTPUT_CONSUMED` doc block in `volumetrics.rs` to state the real per-froxel ray budget, and consider gating the second (glass) ray behind a cheap "did the opaque-architecture mask miss AND is a glass-capable light" precheck so the common case stays at 1 ray.

### Completeness Checks
- [ ] **TESTS**: N/A (doc-only change) unless the optional gating optimization is also implemented, in which case a perf regression test pins the cheap-precheck behavior

---

## #2510 — REN-D17-NEW-03: Stale line citation in the sun_angular_radius guard

**Severity**: LOW · **Dimension**: 17 — Soft Shadows
**Location**: `byroredux/src/render/sky.rs:104-107`
**Status**: NEW

### Description
The debug-assert's rationale cites `triangle.frag:2418-2425` for the tangent-plane-approximation derivation. That block now lives at `triangle.frag:3029-3060` (the legacy-WRS arm) with a second copy of the sampler at `triangle.frag:2916-2921` (the ReSTIR arm, which is the default-on path and carries **no** such derivation comment).

### Evidence
`sky.rs:105` — "Tangent-plane disk approximation valid only for α < ~0.05 rad (documented in triangle.frag:2418-2425)"; lines 2418-2425 of `triangle.frag` are now ReSTIR pHat/reservoir prose, unrelated to the sun disk.

### Impact
Doc rot only; a future reader tuning `sun_angular_radius` (or a per-cell / per-TOD override, which #1023 made a one-line host-side write) lands on unrelated code and may not find the α < 0.05 rad validity bound. Note the guard threshold (0.10) is already 2x the documented validity bound.

### Related
#1023 / REN-D20-002; the ReSTIR path at `triangle.frag:2916`.

### Suggested Fix
Repoint to the symbol rather than the line number (`triangle.frag`'s directional shadow-jitter block) and add a one-line back-reference in the ReSTIR arm at 2916 so the default-on path carries the same caveat.

### Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)

---

## #2511 — REN-D18-NEW-02: In-flight WeatherTransitionRes is never collapsed or cleared on a worldspace change

**Severity**: LOW · **Dimension**: 18 — Sky/Weather
**Location**: `byroredux/src/scene/world_setup.rs::apply_worldspace_weather` / `insert_procedural_fallback_resources`; consumed in `byroredux/src/systems/weather.rs::weather_system`
**Status**: NEW

### Description
`WeatherTransitionRes` is a one-shot state machine (`elapsed_secs`, `duration_secs: 8.0`, `done`) that blends the live `WeatherDataRes` toward `target` and, on completion, promotes `target` into `WeatherDataRes`. Nothing ever removes it — `cell_loader/unload.rs` explicitly documents that worldspace-scoped weather resources are *not* released on cell unload (#1199), and the only writers are the single `insert_resource` in `apply_worldspace_weather` and the `done = true` latch in `weather_system`. Two paths mishandle a transition that is still in flight when a second worldspace change lands:

1. **WTHR branch retarget** — `insert_resource(WeatherTransitionRes { target: new_weather, elapsed_secs: 0.0, .. })` overwrites the in-flight transition while leaving `WeatherDataRes` at the *original* source snapshot, so the frame of the switch pops backwards by `t * (oldTarget - src)`.
2. **Procedural-fallback branch** — `insert_procedural_fallback_resources` replaces `WeatherDataRes` with `procedural_fallback_weather()` but leaves the in-flight transition installed; `weather_system` keeps blending the procedural sky toward the *previous worldspace's* target and, on completion, promotes that target's weather over the procedural fallback — a climateless worldspace ends up permanently rendering the prior worldspace's weather.

### Evidence
```rust
// world_setup.rs::apply_worldspace_weather — WTHR branch
if world.try_resource::<WeatherDataRes>().is_some() {
    world.insert_resource(WeatherTransitionRes {
        target: new_weather, elapsed_secs: 0.0, duration_secs: 8.0, done: false,
    });                       // <- clobbers an in-flight fade; WeatherDataRes still holds the old source
} else { ... }
```
Reachability: `app_step.rs:542` calls `apply_worldspace_weather` on every exterior-destination transition, plus `scene.rs:414` and `debug_load.rs:394`. Two exterior door transitions inside the 8-second window are enough. `grep -rn "WeatherTransitionRes"` confirms no `remove_resource` call site anywhere in the tree.

### Impact
Case 1 is a one-frame colour pop, self-healing within 8s — cosmetic. Case 2 is persistent wrong weather (palette, fog distances, wind-driven cloud scroll, DALC cube) on a climateless worldspace until the next worldspace change. Both require two worldspace transitions within 8 seconds; case 2 additionally requires the second worldspace to have no CLMT/default WTHR, so vanilla content is effectively immune. No crash, no NaN.

### Related
Extends the M33.1 crossfade state machine hardened by #1101 / #1102 / #1103 / REN-D15-NEW-07, none of which addressed transition *lifetime* across a worldspace boundary. #1199 (worldspace-scoped weather resource lifetime) is the reason nothing clears it.

### Suggested Fix
Before installing a new transition (or a procedural-fallback `WeatherDataRes`), collapse any in-flight one: write the current blended snapshot into `WeatherDataRes` (or, cheaply, `lerp` at the live `t`) and set `done = true` / reset `elapsed_secs`. A `collapse_weather_transition(world)` helper called at the top of both branches of `apply_worldspace_weather` covers both cases in one place.

### Completeness Checks
- [ ] **TESTS**: A regression test drives two exterior worldspace transitions within 8s and confirms no backward colour pop and no persistent wrong-weather state

---

## #2512 — REN-D19-04: perturbNormal Path 1 multiplies by raw interpolated vertexTangent.w instead of clamping to +/-1

**Severity**: LOW · **Dimension**: 19 — Tangent-Space
**Location**: `crates/renderer/shaders/include/material_sampling.glsl:170` (`perturbNormal`); same pattern at `crates/renderer/shaders/include/lighting.glsl:128` and `crates/renderer/shaders/triangle.frag:2288`
**Status**: NEW

### Description
`.w` is exactly ±1 **per vertex** (guaranteed at import by `crates/nif/src/types.rs:154` `bitangent_sign` → `clamp_sign`, and by #2246 for the Starfield UDEC3 path). It is *not* ±1 **per fragment**: the varying is linearly interpolated, so any triangle whose three vertices disagree on handedness yields `w ∈ (-1, 1)`, hitting 0 at the mid-line. `perturbNormal` then builds `B = vertexTangent.w * cross(N, T)`, a shortened (or zero) bitangent, while `T` and `N` stay unit length — the TBN is no longer orthonormal and the V-axis component of the normal-map sample is attenuated toward zero. The POM sibling in the same file and the RT sibling both clamp first: `material_sampling.glsl:43` `tangentSign = vertexTangent.w < 0.0 ? -1.0 : 1.0;` and `include/ray_hit.glsl:191` the same.

### Evidence
```glsl
// material_sampling.glsl:169-171  (Path 1)
T = normalize(T - dot(T, N) * N);
vec3 B = vertexTangent.w * cross(N, T);   // raw interpolated w
mat3 TBN = mat3(T, B, N);
```
vs. the clamped form 127 lines above it in the same file (`:43`) and in `ray_hit.glsl:191`.

### Impact
Mixed-sign triangles are rare in authored Bethesda content (UV-seam vertices are duplicated, so a triangle normally spans one shell), but they are reachable through `synthesize_tangents` / `synthesize_tangents_yup`, where the sign is derived per vertex from *averaged* `tan_u`/`tan_v` accumulators — a vertex sitting on a UV fold can legitimately land on the opposite sign from its neighbours without the mesh duplicating it. Result is a band of washed-out normal-map relief (and, at `w ≈ 0`, a degenerate `mat3` column) along that seam. Cheap to make impossible; currently only 2 of 5 TBN reconstruction sites are hardened.

### Related
REN-D19-02 / #2246 (import-side ±1 clamp — this is the fragment-side residual it does not cover); REN-D19-01 / #2245.

### Suggested Fix
In `perturbNormal` (and for consistency `lighting.glsl:128`, `triangle.frag:2288`), replace the raw multiply with `float s = vertexTangent.w < 0.0 ? -1.0 : 1.0; vec3 B = s * cross(N, T);`, matching the POM and `ray_hit.glsl` sites, and note in the comment that the per-vertex ±1 guarantee does not survive interpolation.

### Completeness Checks
- [ ] **SIBLING**: All 5 TBN reconstruction sites (`perturbNormal`, `lighting.glsl:128`, `triangle.frag:2288`, plus the already-hardened POM and `ray_hit.glsl` sites) use the same clamped-sign construction
- [ ] **TESTS**: N/A shader-side; visual confirmation via a mixed-sign UV-fold test asset if available
