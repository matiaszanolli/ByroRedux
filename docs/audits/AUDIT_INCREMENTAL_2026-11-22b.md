# Incremental Audit — 2026-11-22 (b)

**HEAD**: c504ac64f · **Baseline**: `AUDIT_INCREMENTAL_2026-11-22.md` · **Audited**: renderer D3/D6/D10 + descriptor lifecycle, safety (unsafe), gameplay Dim 5, save shape guard, concurrency (lock order), scripting Pattern-B drains · **Unchanged since baseline (skimmed)**: M-LIGHT measurement TSVs and PRD prose

## 1. Change summary

Scope: `--commits 5` (`git diff HEAD~5..HEAD`, 81 files, +5809 / −163). The first-parent window pulls in two merged PRs:

- **M-LIGHT Phase 4 sky apparatus** (#7222, #7266; Refs #6604, commits d079fd404..4adcb79b4). This covers:
  - the analytic sky environment;
  - the split-sum sky probe with a committed environment BRDF LUT (RLT-IBL-001..004, -010; `scene_buffer/env_brdf_lut.rs`, binding 26);
  - the portal-sky GI escape for bounded-aperture interiors (RLT-IBL-006; `sky_apertures.rs`, binding 27, `pathCrossesSkyAperture`);
  - traced sky-specular occlusion and its helper-lane skip (RLT-IBL-004, #7261);
  - the light-loop specular convention (RLT-IBL-011);
  - `GpuCamera` 368 → 384 B (`interior_sky`).
- **#7262** (5ede7b032): a Near Editor Location Sandbox centres its seat search on the editor placement. Patrol is declined with a cited reason.
- **#7264** (c504ac64f): every Pattern-B table row has a drain test in one crate or the other.
- Docs: RCG PRD rev 17/18 (#7238) and M-LIGHT measurement TSVs.

## 2. Routing map

| Files | Owners (`_audit-route.sh`) | Audited |
|---|---|---|
| `crates/renderer/shaders/include/{bindings,camera_ubo_members,env_brdf_lut,lighting,shader_constants}.glsl`, `triangle.frag`, rebuilt `.spv` | renderer, performance | D3 GPU-struct lockstep, D6/D10 shaders + lighting |
| `crates/renderer/src/vulkan/scene_buffer/*`, `context/{sky_apertures,frame_params,build_and_upload_instances,assemble_camera_and_lights}.rs`, `sky_cube.rs` | renderer, safety, concurrency, performance, nifal | descriptors / buffer lifecycle, unsafe, D3 |
| `byroredux/src/render/lights.rs`, `main.rs`, `scene.rs`, `material_dump.rs`, `cornell/*` | renderer (Dim 12), ecs, tooling | delta read |
| `byroredux/src/npc_spawn/ai_package{,/tree}.rs`, `systems/{sandbox,travel,random_conversation}.rs`, `crates/core/src/ecs/components/sandbox.rs` | gameplay, scripting Dim 5, concurrency Dim 7 | gameplay Dim 5, lock order |
| `byroredux/src/save_io/{serde_default_guard,registry_completeness}_tests.rs` | save | shape guard |
| `byroredux/src/systems/request_drain_tests.rs` | scripting, gameplay, ecs | Pattern-B drains |
| `scripts/m-light/*.py`, `scripts/gen_env_brdf_lut.py`, `docs/smoke-tests/m-light-portal-sky.sh` | renderer, runtime | light read |
| `docs/**` | tech-debt (doc rot) | doc-vs-code where code changed |

No `(unrouted)` paths.

Verified clean:
- **`GpuCamera` lockstep.** The struct is 384 B with `interior_sky` last, matching `vec4 interiorSky`. Every includer of `camera_ubo_members.glsl` / `bindings.glsl` was rebuilt, so no `.spv` is stale.
- **Bindings 26 / 27.** Written for every frame-in-flight set. The pool sizes derive from the bindings.
- **Unsafe.** The one new `unsafe impl NoUninit` has a SAFETY comment.
- **Helper lanes (#7261).** The occlusion ray is gated on `!helperLane`, and the derivative-dependent sampling stays above that gate.
- **Per-game branches.** None were added to the renderer.
- **`EditorPlacement` lock order.** It is read only through transient `world.get`, never held across another acquisition, so it stays a lock-order leaf.
- **`SandboxBehavior`.** Every construction site carries the new field. It is not saved, so the shape-baseline refresh needs no `FORMAT_MAJOR` bump.
- **Patrol decline.** Matches the GECK wiki.
- **#7264 table parse.** It sees all 21 markers in 16 rows.

## 3. Findings

### INC-R-01: The GI path's sky-aperture test compares an absolute ray origin against render-origin-relative planes
- **Severity**: MEDIUM
- **Dimension**: audit-renderer D6/D10 (shaders + lighting); silent divergence
- **Location**: `crates/renderer/shaders/include/lighting.glsl:598-611` (`pathCrossesSkyAperture`); caller `crates/renderer/shaders/triangle.frag:4880-4913`; upload `crates/renderer/src/vulkan/context/sky_apertures.rs:42`
- **Changed in**: 84104564e (RLT-IBL-006)
- **Status**: NEW
- **Description**: The host uploads each aperture plane's centre relative to the render origin. The shader then tests the GI path origin against it, but that origin is an absolute world position. The difference is the render origin, which `snap_render_origin` sets to `floor(camera / 4096) · 4096`. It is non-zero whenever any camera coordinate is negative or ≥ 4096, which is the common case for interiors. The aperture planes are therefore displaced by up to the render origin, and the escape test misses real openings or hits through walls.
- **Evidence**:
  - `sky_apertures.rs:42`: `let rel = centre - render_origin;` → `center: [rel.x, rel.y, rel.z, 0.0]`. `bindings.glsl` and the `docs/engine/shader-pipeline.md` binding-27 row both document the centre as relative to the render origin.
  - `triangle.frag:206`: `vec3 fragWorldPos = fragWorldPosRel + renderOrigin.xyz;`
  - `triangle.frag:4880`: `giOrigin = offsetRayOriginForDirection(fragWorldPos, …)`, then `pathOrigin = giOrigin`, then `pathEnvironmentRadiance(pathOrigin, pathDir)`.
  - `lighting.glsl:602`: `rotateBySkyApertureQuaternion(origin - aperture.center.xyz, …)`.
  - The composite's own aperture code works in absolute `camera − center`.
- **Impact**: In a bounded-aperture interior (interior sky mode 2) whose camera sits outside [0, 4096)³, escaping GI paths read the cell ambient instead of the portal sky, or read sky through solid walls. This is the RLT-IBL-006 behaviour itself. `docs/smoke-tests/m-light-portal-sky.sh` exercises only Show Sky (mode 1, no planes), so mode 2 has no live coverage.
- **Related**: #6604 (M-LIGHT umbrella), RLT-IBL-006
- **Suggested Fix**: In `pathCrossesSkyAperture`, subtract `renderOrigin.xyz` from the origin, or pass the render-relative origin from the caller. Pin the convention with a `shader_contract_tests` string check, and add a mode-2 smoke scenario.

### INC-R-02: Traced sky-specular occlusion subtracts sky radiance from an authored-cubemap environment
- **Severity**: LOW
- **Dimension**: audit-renderer D6 (shaders)
- **Location**: `crates/renderer/shaders/triangle.frag:3151` (`exteriorSkyProbe`), `:3209-3229` (`ambientFallback`), `:3305-3317` (the traced block)
- **Changed in**: 5f51748d5 / 84104564e (RLT-IBL-004)
- **Status**: NEW
- **Description**: `exteriorSkyProbe = jitter.w > 0.5 && exteriorSkyTint.w > 0.5` ignores `hasAuthoredCubemap`. For a material with an authored DDS cube, `ambientFallback`, and so `envColor` (at `probeShare` = 1 − rayFade), is that cube's sample, not the sky probe. Yet each blocked lobe sample still removes `probeShare · lobeWeight · exteriorSkyRadianceOr(lobeL, …)`, which is sky-cube radiance. The Lagarde-style `specularOcclusion` is also applied to it.
- **Evidence**: `if (exteriorSkyProbe && probeShare > 0.0 && reflectionGlassRayEnabled && !helperLane && rtLOD < RT_LOD_REFLECT …) { … envColor -= probeShare * lobeWeight * exteriorSkyRadianceOr(lobeL, ambientFallback, 0.0); }`. When `hasAuthoredCubemap` is true, `ambientFallback = textureLod(cubemaps[mat.envMapIndex], …)`.
- **Impact**: Exterior props with authored env maps (FO3/FNV `envMapIndex != 0`), when rough enough or beyond the reflection ray's LOD reach, have the wrong energy removed: authored-cube colour minus sky colour. `envColor` is not clamped after the subtraction, so a bright sky sample over a dim cube can drive it negative.
- **Related**: #6604
- **Suggested Fix**: Set `probeShare = 0` (or skip the traced block) when `hasAuthoredCubemap`, and clamp `envColor` at 0 after the subtraction.

### INC-R-03: `SkyCubeParams` lost its rustdoc to the new `SKY_CUBE_SUN_DISC_OFF` const
- **Severity**: LOW
- **Dimension**: audit-renderer / tech-debt (doc rot)
- **Location**: `crates/renderer/src/vulkan/sky_cube.rs:66-82`
- **Changed in**: 84104564e (RLT-IBL-010)
- **Status**: NEW
- **Description**: The const and its `///` block were inserted between the struct's doc block ("Host mirror of `include/sky.glsl`'s `SkyDome` …") and `#[repr(C)] pub struct SkyCubeParams`. Because the comment lines are contiguous, both doc blocks now attach to the const, and the struct has none.
- **Evidence**: Lines 66-75 hold the struct doc, 76-81 the const doc, then `pub const SKY_CUBE_SUN_DISC_OFF: f32 = 2.0;`, then `#[repr(C)]`.
- **Impact**: The rendered docs mis-describe the const and drop the struct's std140 rationale.
- **Related**: —
- **Suggested Fix**: Move the const and its doc above the `SkyCubeParams` doc block.

### INC-R-04: `GpuCamera` grew to 384 B, but two docs and two audit skills still carry 368 B and the old test name
- **Severity**: LOW
- **Dimension**: audit-renderer D3 / doc rot (and audit infrastructure)
- **Location**: `docs/engine/shader-pipeline.md:896`, `docs/engine/memory-budget.md:195`, `.claude/commands/audit-renderer/SKILL.md:86`, `.claude/commands/audit-safety/SKILL.md:279`
- **Changed in**: 84104564e (`GpuCamera` 368 → 384 B with `interior_sky`; the size test was renamed `gpu_camera_is_368_bytes` → `gpu_camera_is_384_bytes`)
- **Status**: NEW. Same class as the closed #3447 / #3450 / #3599.
- **Description**: The struct, its rustdoc (`gpu_types.rs:490`) and its pin (`gpu_instance_layout_tests.rs:60`) all say 384 B, but four other places were not updated:
  - The shader-pipeline binding table still says "`GpuCamera` (368 B)".
  - The memory-budget Camera UBO row still says 368 B per frame, 736 B total.
  - The audit-renderer and audit-safety skills still name the deleted `gpu_camera_is_368_bytes` as the guard.
- **Evidence**:
  - `_audit-validate.sh` now fails with `gpu_camera_is_368_bytes  audit-renderer audit-safety`, 1 suspect symbol.
  - `shader-pipeline.md:896`: `| 1 | 1 | UNIFORM_BUFFER | GpuCamera (368 B) | …`.
  - `memory-budget.md:195`: `| Camera UBO | — | 1 | 368 B (#3323) | 368 B | 736 B |`.
- **Impact**: Doc-only, but it turns the audit validator red. Until it is fixed, the validator exits at the symbol check, which hides its closed-issue advisory pass, and every `/audit-publish` path gate fails.
- **Related**: #3447, #3450, #3599
- **Suggested Fix**: Change the sizes at all four sites to 384 B (the budget doubles to 768 B) and the test name to `gpu_camera_is_384_bytes`. A test that reads the doc's size from `size_of::<GpuCamera>()` would stop this class from recurring.

### INC-G-01: The AI-package doc's Sandbox section still says the seat search always centres on the actor
- **Severity**: LOW
- **Dimension**: audit-gameplay Dim 5 / doc rot
- **Location**: `docs/engine/npc-spawn-ai-packages.md:240`; `byroredux/src/systems/sandbox.rs:61-62` (`SEAT_SEARCH_RADIUS` doc)
- **Changed in**: 5ede7b032 (#7262 edited §6 and §9 but not §5)
- **Status**: NEW
- **Description**: The doc says "Search *center* is always the actor's own spawn position". Since #7262, a Near Editor Location Sandbox centres on its `EditorPlacement`. `SEAT_SEARCH_RADIUS`'s doc still says the radius is measured from the actor's "current position".
- **Evidence**: `docs/engine/npc-spawn-ai-packages.md:240`, against `sandbox.rs`'s `travel::package_home(world, npc, behavior.near_editor_location, npc_g.translation)`. Lines 279-282 of the same doc now state the exception, so the doc contradicts itself.
- **Impact**: Doc only.
- **Related**: #7262
- **Suggested Fix**: Reword both to "the actor's own position, or its editor placement for a Near Editor Location package (#7262)".

### INC-G-02: The Pattern-B table-coverage test matches a label string, not the marker type
- **Severity**: LOW
- **Dimension**: audit-scripting Pattern-B drains / test-gap
- **Location**: `byroredux/src/systems/request_drain_tests.rs` (`every_pattern_b_table_row_has_a_drain_test`)
- **Changed in**: c504ac64f (#7264)
- **Status**: NEW
- **Description**: The test counts a table marker as covered when a whitespace-squashed source contains `drains("<Name>"`. Several things therefore satisfy it without a real drain check:
  - a case whose label names `X` but raises a different marker type;
  - a commented-out `// drains("X", …)`;
  - `OnResetEvent`, where only the presence of a test-function name in `obscript_objects.rs` is checked;
  - a parse that loses up to one of the 21 markers, since the `>= 20` floor still passes.
- **Evidence**: `let covered = cleanup.contains(&call) || here.contains(&call) || elsewhere…`, where `call = format!("drains(\"{marker}\"")`.
- **Impact**: The guard can pass while a listed marker has no behavioural drain test. This is the rot class #2672 / #7264 meant to close.
- **Related**: #7264, #2672, #6688
- **Suggested Fix**: Have both `drains` helpers record `std::any::type_name::<T>()` for the marker they ran, and compare the table against those names. Raise the floor to the exact row count, or derive it.

## 4. Missing tests

- `ENV_BRDF_LUT` (`scene_buffer/env_brdf_lut.rs`): nothing checks that the committed table matches `scripts/gen_env_brdf_lut.py`, or that its entries are sane (A, B ∈ [0, 1], A + B ≤ 1). Only the length is const-asserted.
- No test pins the shader-space convention `pathCrossesSkyAperture` shares with the host upload (INC-R-01). Interior sky mode 2 (bounded apertures) has no smoke scenario.
- `upload_sky_apertures` has no unit test for the count clamp at `MAX_COMPOSITE_SKY_APERTURES` or the header/plane offsets.
- No test covers the `hasAuthoredCubemap` × traced-occlusion interaction (INC-R-02).
- `ai_package/tree.rs`: no test that a tree Sandbox leaf with a type-3 Location input yields `near_editor_location: true`. Only the `false` case is pinned.
- `sandbox.rs`: no test for a `near_editor_location: true` actor with no `EditorPlacement`, which should fall back to its own position.
