# Renderer Audit — 2026-05-13

**Mode**: deep · all 20 dimensions · orchestrator + per-dimension specialists
**Baselines**: per-dim audits 2026-05-11 / 2026-05-12 (DIM1, DIM3, DIM4, DIM5, DIM8, DIM8_v2, DIM9, DIM10, DIM11) + open issues snapshot (161 open at run start)
**Methodology**: every finding's premise was re-verified against current `main` HEAD; stale audit findings rejected per `feedback_audit_findings.md`.

## Executive Summary

| Severity | New | Carried-forward (open) | Total |
|----------|-----|------------------------|-------|
| CRITICAL | 0   | 0                      | **0** |
| HIGH     | 4   | 0                      | **4** |
| MEDIUM   | 13  | 1 (#952 hot-path fence) | **14** |
| LOW      | ~38 | several (#949, #957, #958, #961, #962, #963, #964, #948) | ~45 |
| INFO / Verified-Clean | ~20 | — | ~20 |

**Pipeline-area pass/fail snapshot**

- **Vulkan synchronization** — clean (the 2026-05-11 LOW set still applies; one MEDIUM hot-path window unfixed = #952).
- **GPU memory + drop ordering + per-FIF slot ownership** — clean. One MEDIUM defensive issue (no upper bound on `pending_vertices` growth).
- **Pipeline state, render-pass, command recording** — clean (2 stale doc comments only; render pass + record sequence verified balanced and correctly ordered).
- **Shader struct lockstep** — clean. `GpuMaterial` 260 B + GpuInstance 112 B byte-identical across 3 shaders. One LOW latent: `triangle.vert::CameraUBO` omits `skyTint` field.
- **Resource lifecycle** — clean. 13 subsystem `destroy` chains all invoked from `VulkanContext::Drop`. 4 LOW shape/clarity findings.
- **BLAS / TLAS** — clean. 1 carried LOW (#958: skinned-BLAS flag literal duplication).
- **RT ray queries** — solid; 1 MEDIUM (reflection cone uses non-V-aligned bias; black speckle on metals at grazing angles), 4 LOW.
- **Denoiser & composite** — clean (4 LOW: descriptor-pool size lag, post-resize history `initialize_layouts` skip, partial-failure state).
- **TAA** — clean. 0 findings — same posture as 2026-05-12 audit.
- **GPU skinning + BLAS refit** — clean (1 MEDIUM: `SHADER_READ` access mask gap on COMPUTE→AS_BUILD barrier — pinned by #661 already).
- **Caustic splat** — clean (1 LOW clarity).
- **Material table (R1)** — clean (3 LOW: dedup-ratio off-by-one in console output, redundant first-frame neutral re-upload, dead defence-in-depth warn).
- **Sky / weather / exterior lighting** — **HIGH** (sun-arc direction ignores `tod_hours`; non-default sunrise produces 40-min "below-horizon sun under sunrise sky"); 3 MEDIUM (cross-fade fog distance lag, missing latitude tilt, screen-space cloud parallax).
- **Tangent-space & normal maps** — clean (2 LOW doc-only).
- **Water rendering** — **2 HIGH** (refraction fires `-V` instead of `V`; refraction-miss paints sky under surface), 3 MEDIUM, 3 LOW.
- **Volumetrics** — **HIGH (latent)** — composite drops the `vol.a` transmittance term entirely; will under-fog the scene the moment `VOLUMETRIC_OUTPUT_CONSUMED` flips to true. 2 MEDIUM (HG `g` not clamped, hardcoded sun direction).
- **Bloom pyramid** — clean. All 7 findings are LOW (verified-clean / sentinels for future regressions).
- **M-LIGHT v1 stochastic shadows** — clean (1 MEDIUM: `sunAngularRadius` is a shader literal, not a UBO field — blocks per-cell tuning; 1 LOW future-RNG seed).

## Top Findings (full text)

---

### REN-D15-NEW-08 — HIGH — Sun arc ignores CLMT `tod_hours` for direction; only fades intensity at hardcoded 6h/18h

**Dimension**: Sky/Weather/Exterior Lighting
**File**: `byroredux/src/systems/weather.rs:294-330`
**Premise verified**: `weather.rs:227` correctly drives the colour interpolator from `build_tod_keys(wd.tod_hours)`, so palettes track CLMT TNAM hours per worldspace (FO3 Capital Wasteland sunrise 5.333h, FNV Mojave 6.0h). But the sun **direction** and **intensity** at lines 294-330 are computed against hardcoded constants:
```rust
let solar_hour = (hour - 6.0).clamp(0.0, 12.0);   // sunrise hardcoded to 6h
let angle = solar_hour / 12.0 * std::f32::consts::PI;   // arc spans hardcoded 12h
let sun_intensity = if (7.0..=17.0).contains(&hour) { 4.0 }
    else if (6.0..7.0).contains(&hour) { (hour - 6.0) * 4.0 }
    …
if (6.0..=18.0).contains(&hour) { [x/len, y/len, z/len] }
else { [0.0, -1.0, 0.0] }     // night sentinel
```
**Issue**: On FO3's canonical climate (sunrise 5.333h), the sun direction stays at the below-horizon sentinel for ~40 minutes of in-game *sunrise* while the sky gradient is sunrise-tinted — sky paints dawn while the world goes pitch-dark under a "below-horizon" sun. Symmetric ~1h dead window at sunset. #463 migrated the colour path off hardcoded values but left the arc literals.
**Fix**: Drive `solar_hour`, the arc-span denominator, and the intensity envelope from `wd.tod_hours.{sunrise_begin, sunrise_end, sunset_begin, sunset_end}`. Same shape as the colour interpolator at line 227.
**Test**: synthetic CLMT with sunrise=5.0h, sunset=19.0h; assert sun_dir.y > 0 across [5.5h, 18.5h] (vs current [6h, 18h]).
**Dup-check**: new (no open issue covers the arc/intensity literals).

---

### REN-D18-001 — HIGH (latent) — Composite drops volumetric transmittance entirely; only scattering is wired

**Dimension**: Volumetrics
**File**: `crates/renderer/shaders/composite.frag:358,412` ; `crates/renderer/src/vulkan/volumetrics.rs:7-8` (header doc)
**Premise verified**: header doc line 8 says `final = scene * vol.a + vol.rgb` (Frostbite §5.3 standard form). `composite.frag:358` does `combined = direct + indirect * albedo + caustic;` then line 412 does `combined += vol.rgb * 0.0;`. Even with the `* 0.0` removed, the line is purely additive — `vol.a` (cumulative transmittance written by `volumetric_integrate.comp:66`) is NEVER read.
**Issue**: Latent, fires the moment `VOLUMETRIC_OUTPUT_CONSUMED` flips to true (#928 disabled-path gate). When re-enabled, god-ray scattering will add to the scene but the receding-into-fog attenuation that should darken distant geometry will be missing — distant terrain stays at full radiance + glow on top, the inverse of the intended look.
**Fix**: Replace `combined += vol.rgb * 0.0;` with `combined = combined * vol.a + vol.rgb;` in lockstep with flipping `VOLUMETRIC_OUTPUT_CONSUMED = true`. Order matters: attenuate first, then add inscatter — inscatter is energy that arrived between camera and fragment and is NOT itself attenuated by `T_cum` (the integrate pass already weighted each slab's contribution by its own running transmittance).
**Test**: composite-output golden-image diff with synthetic high-scattering scene (~σ=0.05); a near-camera bright surface should fade to fog colour across a fixed distance, matching analytic `exp(-σ·d)`.
**Dup-check**: new (issue #924 covers the disabled-mix fallback in the aerial-perspective branch, NOT this missing multiply).

---

### F-WAT-01 — HIGH — Water refraction `refract()` called with wrong incident-vector sign

**Dimension**: Water (M38)
**File**: `crates/renderer/shaders/water.frag:365`
**Premise verified**: `vec3 Tdir = refract(-V, Nperturbed, 1.0 / max(ior, 1.0));` where `V = normalize(cameraPos.xyz - vWorldPos)` (fragment→camera). GLSL `refract(I, N, eta)` wants `I` as the incident vector (camera→fragment), so passing `-V` flips it back to fragment→camera — wrong direction for a downward-entering ray. Refracted ray fires *upward into the air column* instead of downward through the water.
**Issue**: Reflection on line 351 (`reflect(-V, Nperturbed)`) is geometrically symmetric so it survives the same flip; refraction does not. Only visible when refraction misses (cliff edges, sparse BLAS) or when comparing against ground-truth — a regression test would catch it via `assert_almost_eq` on a 45° incidence.
**Fix**: Pass actual incident ray `-V` (camera→fragment) per GLSL convention; the result `Tdir` is then naturally the outgoing refracted ray. Match the existing `traceReflection` convention in `triangle.frag`. Add a shader-side unit-style assert for a 45° test case.
**Test**: 45° incidence with IOR=1.33 should produce a refracted-ray `tan(θ_t) ≈ 0.564`; current code produces a ray going up not down.
**Dup-check**: new.

---

### F-WAT-02 — HIGH — Water refraction misses paint sky tint UNDER the surface

**Dimension**: Water (M38)
**File**: `crates/renderer/shaders/water.frag:177-206, 370-374`
**Premise verified**: `traceWaterRay` returns `skyTint.xyz` on miss for *both* reflection and refraction calls. Refraction misses are reachable at cell edges, over caves, or with sparse exterior BLAS.
**Issue**: When a downward refraction ray escapes the BLAS, the surface radiance going in is sky-blue rather than the cell's fog/deep colour. `absorbWaterColumn` then mixes `deep_color` based on `hitDist=maxDist`, so deep tint will dominate but the visible artefact is a faint sky glow instead of murk when the player looks straight down at shallow water near a cliff.
**Fix**: split `traceWaterRay` into reflection/refraction helpers, or take a `fallbackOnMiss: vec3` parameter. Reflection wants `skyTint`; refraction wants `push.deep.rgb` (or camera UBO `fog.rgb`).
**Test**: a synthetic scene with no opaque geometry below a water plane should render uniform deep/fog tint, not sky.
**Dup-check**: new.

---

## MEDIUM Findings (one-line)

| ID | Dimension | File | Issue |
|----|-----------|------|-------|
| REN-D1-NEW-06 | Vulkan Sync | `vulkan/context/draw.rs:231-242` | `reset_fences` precedes ~1850 lines of fallible recording — error-path deadlock window mirroring #908 (open #952) |
| REN-D2-005 | GPU Memory | `renderer/src/mesh.rs:213-261` | Global vertex SSBO `pending_vertices` has no upper-bound guard; degenerate streaming pattern grows unbounded |
| REN-D9-NEW-05 | RT Ray Queries | `shaders/triangle.frag:401` | `traceReflection` uses `tMin = 0.01` instead of the 0.05 used everywhere else; black speckle on metals at grazing angles |
| REN-D12-F1 | GPU Skinning | `vulkan/context/draw.rs:744-755` | COMPUTE→AS_BUILD barrier `dst_access_mask` lacks `SHADER_READ` for build-input read (covered by #661) |
| REN-D15-NEW-09 | Sky/Weather | `weather.rs:343` | Cross-fade fog NEAR/FAR uses *source* night_factor against *target* fog table |
| REN-D15-NEW-10 | Sky/Weather | `weather.rs:294-330` | Sun arc lacks per-worldspace latitude tilt; equatorial arc on every map |
| REN-D15-NEW-11 | Sky/Weather | shader cloud sample site | Cloud parallax is screen-space, not world-XY; rotating camera carries clouds with view |
| REN-D18-002 | Volumetrics | `volumetric_inject.comp` | HG asymmetry `g` not clamped to (-0.999, 0.999); g≈±1 produces NaN (today wired to 0.4 via host) |
| REN-D18-008 | Volumetrics | `vulkan/context/draw.rs:2116` | Sun direction hardcoded `[-0.4, 0.8, -0.45]` — interior cells will get sun-shaft injection that should not exist (latent until #928 flips) |
| REN-D20-NEW-01 | M-LIGHT | `shaders/triangle.frag:2386` | `sunAngularRadius` is a `const float` literal — cannot tune per-cell or per-TOD without shader recompile |
| F-WAT-03 | Water | `byroredux/src/render.rs` | Regular TLAS-build path does not check `is_water`; future TLAS code would silently reintroduce self-hits |
| F-WAT-04 | Water | `shaders/water.frag:341-343` | Grazing-angle normal clamp mixes only 60% toward geometric N; can still go below the plane |
| F-WAT-05 | Water | `byroredux/src/render.rs:1437` | `WaterDrawCommand.instance_index = idx as u32` contract relies on no re-sort post-#1437 — no assertion |

## LOW Findings (grouped, one-line)

| Dimension | Count | Sample IDs |
|-----------|-------|-----------|
| Vulkan Sync | 2 | REN-D1-NEW-07 (over-broad SVGF source mask, deferred), REN-D1-NEW-08 (UNIFORM_READ defence-in-depth, dup #963) |
| GPU Memory | 4 | REN-D2-001..004 (verified-clean documentation findings) |
| Pipeline State | 2 | REN-D3-001/002 (stale "76-byte Vertex" doc comments — actual size is 100 B post-M-NORMALS) |
| Render Pass | 3 | REN-D4-NEW-04..06 (mesh-id clear OK; UNIFORM_READ dup #963; gbuffer initialize_layouts dup #949) |
| Shaders | 2 | R-D6-01 (vert CameraUBO omits skyTint, latent), R-D6-02 (push-constant-block doc note) |
| Resource Lifecycle | 7 | REN-D7-NEW-07..13 (water shape, retained shader-modules clarity, sampler/render-pass order, framebuffer/swapchain destroy order verified) |
| Acceleration Structures | 1 | dup #958 (skinned BLAS flag literal duplicated at 4 sites) |
| RT Ray Queries | 4 | REN-D9-NEW-06..09 (reflection-miss Fresnel-weight short-circuit; PCSS-lite design state; checklist 1500u → 6000u doc fix; volumetric/caustic ray DBG_VIZ parity) |
| Denoiser & Composite | 4 | REN-D10-NEW-09..12 (descriptor pool size lags layout; post-resize history initialize_layouts skip; partial-failure state) |
| GPU Skinning | 2 | F2 (output_buffer lacks TRANSFER_DST for diagnostics), F3 (doc-comment clarity) |
| Caustic Splat | 1 | REN-D13-NEW-08 (first-use slot stale GENERAL→GENERAL barrier, clarity) |
| Material Table | 3 | REN-D14-NEW-01 (dedup ratio off-by-one in console output), 02 (first-frame redundant 260 B re-upload), 03 (dead defence-in-depth warn) |
| Sky/Weather | 5 | REN-D15-NEW-12..16 (wind_speed unwired; WeatherTransitionRes.done dead latch; pick_tod_pair dead branch; no-WTHR fallback gap; cloud-speed multipliers no test pin) |
| Tangents | 2 | R16-01 (DBG_FORCE_NORMAL_MAP 0x20 orphan dead code), R16-02 (audit-checklist bit-catalog stale: 0x100 / 0x200 wired but undocumented) |
| Water | 3 | F-WAT-06 (duplicate trig in WATR resolver), F-WAT-07 (water bypasses MaterialTable), F-WAT-08 (dead vUV/vInstanceIndex interpolators) |
| Volumetrics | 7 | REN-D18-003..010 (clarity / verified-clean / sentinels) |
| Bloom | 7 | REN-D19-001..007 (mip 0 = half-screen documentation hazard; barrier-count regression sentinel; non-normalised up-pass intentional; pre-ACES add verified; HDR source verified; resize binding-7 verified) |
| M-LIGHT | 1 | REN-D20-NEW-02 (cone-sample seed `i*7/i*13` will inject low-frequency flicker when M-LIGHT v2 merges sun into multi-light WRS) |

## Notable Verified-Clean Discoveries

- **Mesh-ID format upgraded** from R16_UINT (15-bit ID + bit-15 flag, 32767 cap) to **R32_UINT** (30-bit ID + bit-31 ALPHA_BLEND_NO_HISTORY flag, 0x7FFFFFFF cap) per #992. Audit checklist still references R16; downstream readers (TAA, SVGF) updated in lockstep.
- **TLAS instance buffer padding** is `max(2×, 8192)` (not `4096` as audit checklist claimed). Trade-off documented in-source: 8192 × 88 B × 2 FIF ≈ 1.4 MB BAR vs avoiding mid-cell-transition resize churn during M40 streaming.
- **GpuMaterial 260 B** size pin (`gpu_material_size_is_260_bytes`) and 65-field offset pin (`gpu_material_field_offsets_match_shader_contract`, #806) both in force. R1 closeout confirmed.
- **GpuInstance 112 B** byte-identical across `triangle.vert`, `triangle.frag`, `ui.vert` — and `caustic_splat.comp`. Per `feedback_shader_struct_sync.md` the recurring regression site `ui.vert` is currently in lockstep.
- **Bloom pyramid** at 10 barriers/dispatch-frame (down to 47% from pre-#931's 19) — regression sentinel REN-D19-003 will catch any reintroduction.
- **TAA** posture unchanged from 2026-05-12 — five prior closures (NaN guard #903, bit-31 mask #904, 5-tap MV-max #915, Halton-reset on resize #913, narrowed pre-dispatch barrier REN-D11-NEW-05) all still in force, anchored by in-source comments and `validate_set_layout` SPIR-V reflection at startup.

## Prioritised Fix Order

1. **F-WAT-01** (HIGH — water refract sign) — wrong-direction refracted ray, visually visible at any non-RT-perfect scene. **One-line fix.**
2. **F-WAT-02** (HIGH — water refraction-miss → sky under surface) — visible cliff-edge artefact. **Helper-split fix.**
3. **REN-D15-NEW-08** (HIGH — sun arc ignores CLMT TOD) — 40-min sunrise/sunset dead windows on FO3. Mechanical refactor of `weather.rs:294-330` to read `wd.tod_hours.{sunrise_begin..sunset_end}` like the colour path already does.
4. **REN-D18-001** (HIGH latent — composite drops `vol.a`) — *fix in lockstep* with the next attempt at flipping `VOLUMETRIC_OUTPUT_CONSUMED = true`. Ship it now so the gate flip is mechanical.
5. **REN-D9-NEW-05** (MEDIUM — reflection cone tMin=0.01) — black-speckle root cause; one-line shader fix.
6. **REN-D2-005** (MEDIUM — vertex SSBO unbounded) — defence-in-depth; soft-cap warn + hard-cap panic mirroring `scene_buffer.rs:1325` material pattern.
7. **REN-D1-NEW-06 / #952** (MEDIUM — fence reset window) — outstanding from prior audit; either move `reset_fences` to immediately before `queue_submit` (canonical Khronos order) or sibling-fix every `?` site between current reset and submit.
8. **REN-D15-NEW-09/10/11** (MEDIUM — fog cross-fade lag, latitude tilt, screen-space cloud parallax) — lower priority; clouds are most visible.
9. **REN-D14-NEW-01** (LOW dedup-ratio off-by-one) — one-line `unique_user_count()` swap; fix included because the data is what users monitor for R1 telemetry.
10. **REN-D20-NEW-01** (MEDIUM — sunAngularRadius shader literal) — required when M-LIGHT v2 lands; plumb through GpuCamera UBO.

## Files & Provenance

Per-dimension audit drafts (working files; cleaned up by orchestrator):
- `/tmp/audit/renderer/dim_1.md` … `dim_20.md`

Open-issues snapshot used for dedup: `/tmp/audit/renderer/issues.json` (161 OPEN issues, fetched 2026-05-13 18:40).

Prior renderer audits referenced for dedup: `docs/audits/AUDIT_RENDERER_2026-05-{11,12}_DIM*.md`, `docs/audits/AUDIT_RENDERER_2026-05-03*.md`, `docs/audits/AUDIT_RENDERER_2026-04-{25,27}.md`.
