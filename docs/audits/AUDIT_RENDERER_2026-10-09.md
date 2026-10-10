**HEAD**: `3bcf6c8e8` · **Baseline**: `AUDIT_RENDERER_2026-10-08.md` (HEAD `00f580e09`) · **Audited**: Dims 2, 3, 5, 6, 7, 10, 11 (delta `00f580e09..3bcf6c8e8`: 18 commits on renderer-adjacent paths), the 3bcf6c8e8 (#5482) review, and the streaming-deep emphasis — the renderer side of cell load/unload (Dims 1 and 5) · **Unchanged since baseline (skimmed)**: Dim 1 (no commits under `acceleration/`; streaming release paths traced), Dim 4 (host-side state only), Dim 8 (fold helpers and a test only; EXAL content belongs to `/audit-exterior`), Dim 9 (no commits), Dim 12 (no commits)

# Renderer Audit — 2026-10-09 (streaming-deep suite, delta + #5482 review)

This run is part of `/audit-suite --preset streaming-deep`. The renderer delta is mostly fixes for the baseline's own findings, plus three new renderer changes:
- **#5482** (`3bcf6c8e8`): the presentation grade now applies exposure before the grade, and contrast gains a shadow toe.
- **#5369** (`2a7223121`, partial): an intensity-blind light-rig key and a deeper parked EMA tail.
- **#5211** (`e6e888f68`): an SVGF NaN guard.

**Method**
- Solo and synchronous; per-dimension notes are in `/tmp/audit/renderer/dim_*.md`.
- No engine, GPU process or validation run was launched.
- The IMGS contrast census used a throwaway read-only script (`/tmp/audit/renderer/imgs_census.py`) over the vanilla masters. Nothing in the tree was modified.

**Guards run**
- renderer lib: **1394 / 0 / 1** (baseline 1383 / 0 / 1).
- bin crate (rustc 1.96.0): **2743 / 0 / 55** (baseline 2705 / 0 / 55).
- `scripts/check-shader-artifacts.sh`: clean. 36 shaders plus the early variant, glslang 11:16.4.0. `presentation.frag.spv`, `svgf_temporal.comp.spv` and the `triangle` pair were rebuilt in their commits.

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 2 | D11-01 (#5482's toe still crosses zero for authored contrast > 1.556, 31 vanilla IMGS); D2-01 (#5369's rig key depends on light order, and the lights are sorted by an intensity score, so flicker among several lights re-keys the rig and voids mode 3) |
| LOW | 1 | D3-01 (today's two lane/grade changes left the five `CameraUBO` mirror comments, `shader-pipeline.md` and `renderer.md` stale) |

**Baseline closure**

| Baseline finding | State |
|---|---|
| REN-D5-2026-10-08-01 (32-bpp DDS masks) | **Fixed** (#5378, `cb6a5bf0b`; diff read, see Dim 5) |
| REN-D5-2026-10-08-02 (L8 luminance) | Existing **#5402** (open; `parse_dds` unchanged) |
| REN-D10-2026-10-08-01 (transmission handoff mask) | Existing **#5455** (open; `shadow_transport.glsl` untouched) |
| REN-D5-2026-10-08-03 (stale SAFETY comment) | Existing **#5457** (open) |
| #5211 (SVGF nearest-tap NaN) | **Fixed** (`e6e888f68`; read, correct) |
| #5210 (NIFAL doc/spec rot) | **Fixed** (`7d1de4ad5`; comment- and doc-only) |

**Streaming emphasis: no new renderer-side defect.** The unload release chain is sound. The one live hole on this path is the #5379 purge's bare `despawn_batch`, which is already reported this suite (CONC-D5-2026-10-09-01 / SAVE-D4-2026-10-09-01) and is not repeated here. The rest of the chain, traced:
- `release_entities_timed` drops a BLAS only when `rc == c`.
- Meshes and textures are queued for deferred destroy.
- Terrain-tile slots are freed on the CPU side and re-uploaded by a per-frame recorded copy.
- Skin and morph victims are queued for the eviction pass after the fence wait.
- LOD blocks carry unique global-only handles, so their unconditional `drop_blas` is safe.
- LOD water is uploaded with `rt_enabled = false` and is excluded from the TLAS (`draw_command_eligible_for_tlas` returns `!is_water`), so there is no BLAS to drop.
- `drain_streaming_state` → `flush_pending_destroys` calls `device_wait_idle` before draining.
- TLAS membership changes force a BUILD through `decide_use_update`, whose entity-id key cannot alias because ids are never reused.

## RT Pipeline Assessment

**AS (Dim 1).** No commits. The streaming release paths listed above are the only AS-adjacent code exercised by the delta.

**Ray queries and the direct-light EMA (Dim 2/10).** What #5369 got right:
- `restir_rig_static` keeps all three occluder gates of `caustic_scene_static`: rigid motion, instance-set change and `pose_dirty`. Moving occluders still drop to mode 1, so #4942's ghost protection holds.
- `triangle.frag` reads `dofParams.w` only as `> 0.5` and `> 1.5`, so mode 3 lands correctly.
- The `histPrev` clamp now matches the 256 cap.

What it got wrong: the rig key is order-dependent while the light array it walks is re-sorted by `gi_priority_score`, which is an intensity quantity (D2-01).

**Denoiser (Dim 7).** #5211's fix covers both NaN paths:
- The fallback tap now runs the same `isnan`/`isinf` rejection on both the indirect history and the moments.
- The current-frame indirect sample is sanitized at entry; the firefly `>` clamp is false for NaN, so it could not catch one.
- It is pinned by `temporal_pass_sanitizes_non_finite_samples_on_every_history_path`.

**Presentation (Dim 11, #5482).**
- The new order is exposure → saturation → contrast → brightness → tint → chroma compress → tonemap. Exposure is still applied exactly once, in presentation, from the same `exposureTex` texel FSR normalizes against, so the FSR contract is intact.
- Raw debug oracles return before sampling and exposure, so they are unaffected.
- The underwater tone is still `tonemap(underwater_color * exposure)`. The fog colour was ungraded before and after the change, so it stays consistent with the scene.
- The GLSL `gradeContrast` and the Rust `grade_contrast` are algebraically identical.
- The defect is in the curve's domain, not its order (D11-01).

## GPU-Struct & Memory Assessment

**Structs (Dim 3).**
- No layout change. `gpu_types.rs` changed rustdoc only.
- The two new defines (`GRADE_CONTRAST_PIVOT`, `GRADE_CONTRAST_TOE`) flow through `SHADER_DEFINES`.
- The size, offset and mirror pins are green.
- The `dofParams.w` lane changed meaning (mode 3), but the mirror comments did not follow (D3-01).

**Memory (Dim 5).** #5378 was read in full:
- The exact RGBA order (with an alpha mask) stays zero-copy.
- The exact BGRA order with `A = 0xFF000000` is zero-copied as `B8G8R8A8_*`. That format is mandatory for sampling, is paired in `format_for_color_space`, is listed in `format_has_alpha`, and is swapped by `average_rgb`.
- Every other 32-bpp layout, including X8R8G8B8, is expanded through `RgbExpand` with alpha forced to 255.
- The #4835 payload check is now shared through `uncompressed_expand_meta`.

The other two Dim 5 commits:
- #5273: the bounded `*_as_c_str()` accessors are correct.
- #5275: moves the warn re-arm to the end of a fully recorded upload pass. The per-frame growth retry that remains is PERF-D3-2026-10-09-01 (already reported).

## Findings

### MEDIUM

#### REN-D11-2026-10-09-01: #5482's shadow toe still crosses zero whenever the authored contrast exceeds `1 + TOE/PIVOT` (≈ 1.556) — 31 vanilla image spaces re-crush their deepest shades to black
- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation
- **Location**:
  - `crates/renderer/shaders/presentation.frag`, `gradeContrast`.
  - Its mirror `crates/renderer/src/tonemap.rs`, `grade_contrast`.
  - The constants `GRADE_CONTRAST_PIVOT` / `GRADE_CONTRAST_TOE` in `crates/renderer/src/shader_constants_data.rs`.
- **Status**: NEW. It is the residual of #5482, which is CLOSED by `3bcf6c8e8`.
- **Description**:
  - The blend is `mix(toe, stretched, w)` with `w = clamp(toe / TOE, 0, 1)`, which works out to `toe · (1 − (toe − stretched) / TOE)`.
  - As `x → 0`, `toe → 0` and `stretched → PIVOT·(1 − c)`, so `toe − stretched → PIVOT·(c − 1)`.
  - When `PIVOT·(c − 1) > TOE`, i.e. `c > 1.556`, the factor goes negative: the curve dips below zero and both tonemappers floor it to black.
  - The doc comments ("a curve that approaches black instead of crossing it") and the test's claim ("keeps every positive shade positive and the ramp strictly increasing") hold only for `c ≤ 1.556`.
  - `contrast_keeps_shadows_above_black_and_monotone` pins `c = 1.3` alone.
- **Evidence**:
  - A Python mirror of `grade_contrast` gives the exposed-radiance band that still presents as exactly zero, against the pre-fix floor `0.18·(1 − 1/c)`:
    - `c = 1.6`: 0.005 (pre-fix 0.068)
    - `c = 1.7`: 0.017 (pre-fix 0.074)
    - `c = 1.9`: 0.037 (pre-fix 0.085)
    - `c = 2.0`: 0.046 (pre-fix 0.090)
  - The curve is non-monotone for every `c > 1.556`.
  - A census of IMGS cinematic contrast in the vanilla masters (FO3/FNV `DNAM`, Skyrim/FO4 `CNAM`/`ENAM`, using the parser's own float indices) found these records above 1.556:

    | Master | Above 1.556 | Records |
    |---|---|---|
    | FalloutNV.esm | 9 of 67 | `UltraLuxeCasino` 1.9, `CaveTestBrightImageSpace` 1.9, `UnderworldImageSpace` / `CitadelLabImageSpace` / `CaveLLImageSpace` 1.7, four Metro/Urban sets 1.6 |
    | Fallout3.esm | 8 of 48 | `CitadelLabImageSpace` 1.7, `UnderworldImageSpace` 1.7, Metro/Urban 1.6 |
    | Skyrim.esm (SE) | 11 of 270 | `FrostmereCryptImagespace` 2.0, `DA16DreamImageSpace` 1.8, `ISSkyrimOvercastWarDAY` and `ISSkyrimStormSnowDAWN` 1.7, the storm-rain night set 1.65 |
    | Fallout4.esm | 3 of 293 | `ISWorldMapWeatherNIGHT` 1.65, `DiamondCityPastelDUSK` and `IstIS01` 1.6 |

  - IMAD contrast keys compose on top of these values (`value = base × mult + add`) and were not censused, so this count is a lower bound.
- **Impact**:
  - At `c = 1.9–2.0` (the Ultra-Luxe casino and Frostmere Crypt), every shade below about 0.04 exposed presents as exactly zero.
  - That is the same 0.03–0.04 ambient-only band #5482 set out to rescue. Those shades would have shown at roughly 0–15/255 on the toe.
  - At 1.6–1.7 (the FO3 DC Metro sets, Skyrim storm weathers) the residual band is small (0–3/255).
  - The effect is visual only. It reintroduces the "black holes" symptom in exactly the darkest-graded interiors and weathers.
- **Related**: #5482 (closed), #4840 (the ACES negative floor that turns the dip into black).
- **Suggested Fix**:
  - Weight the hand-over on the *stretched* value: `w = clamp(stretched / TOE, 0, 1)`.
  - `stretched ≤ 0` then returns the strictly positive toe, and the result always lies between two non-negative values. A mirror check over `c ∈ {0.5 … 2.0}` is positive and monotone across that range.
  - Extend the Rust test to the census range (`c` up to 2.0) instead of 1.3 alone.

#### REN-D2-2026-10-09-01: #5369's intensity-blind rig key is folded in array order, but the light array is sorted by an intensity-derived score — flicker among two or more lights re-keys the rig, so mode 3 rarely holds in the multi-light scenes it targets
- **Severity**: MEDIUM
- **Dimension**: Soft Shadows / Light Animation (ReSTIR direct EMA)
- **Location**:
  - `crates/renderer/src/vulkan/caustic.rs`, `fold_light_rig_geometry_key_for`, which uses the order-dependent FNV-1a step `fold_caustic_key_f32`.
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: the `restir_rig_key` loop over `lights` and the `restir_rig_static` gate.
  - The sort sites: `sort_lights_by_priority_with_ids` (`scene_buffer/light_history.rs`), called from `assemble_camera_and_lights.rs` and from `collect_lights` (`byroredux/src/render/lights.rs`).
  - The score `GpuLight::gi_priority_score` (`scene_buffer/gpu_types.rs`).
- **Status**: NEW. It is the residual of #5369 (CLOSED by "Fix #5369 (partial)"). Related: **#5370** (OPEN, HIGH — FO4 Institute speckle).
- **Description**:
  - The rig key deliberately excludes colour, intensity and cull radius, so that flicker keeps the deep parked EMA (history mode 3).
  - It folds the lights in the order of the uploaded array, and that array is sorted by `gi_priority_score = (r + g + b) × position_radius.w`. That score is exactly the intensity family the key excludes.
  - `flicker_intensity` (`byroredux/src/systems/light_anim.rs`) is per-entity seeded noise at about 12 Hz. Two flickering point lights of similar score therefore cross in sort order several times a second.
  - Each crossing changes the key, so that frame sets `restir_rig_static = false` and the mode falls to 1.
  - A mode-1 frame stores `histLen = min(histPrev + 1, 16)` (`triangle.frag`, the `historyCap` block). The deep tail then restarts from 16 after every swap and needs about 240 swap-free frames to reach the 0.008 floor again.
- **Evidence**:
  - `light_rig_geometry_key_ignores_intensity_but_not_geometry` covers a single light only, so a permutation is invisible to it.
  - The key loop runs after the post-combustion re-sort (`assemble_camera_and_lights.rs`, `scene_buffer::sort_lights_by_priority_with_ids(&mut frame_lights, …)`), whose comparator is `b.0.total_cmp(&a.0)` on the decorated score.
- **Impact**:
  - Rooms with several identical flickering fixtures keep their effective EMA well above the deep floor: candles and sconces from one LIGH base, and FO4 fluorescent banks.
  - That leaves the standing penumbra speckle #5369 set out to remove, and is plausibly part of why #5370 persists.
  - The single-light Cornell fire-lab A/B the commit measured cannot show it.
  - Visual only.
- **Related**: #5369, #5370, #4942, #5020.
- **Suggested Fix**:
  - Make the key order-independent: hash each light's geometry separately, then combine the per-light hashes commutatively (sum, or sort then fold).
  - Alternatively key by the CPU-side light identity (`light_ids`) rather than array position.
  - Add a two-light permutation case to the test.

### LOW

#### REN-D3-2026-10-09-01: today's two renderer changes left their lane and order descriptions stale — five `CameraUBO` mirrors and `shader-pipeline.md` omit history mode 3, and `renderer.md` / `shader-pipeline.md` still show the pre-#5482 presentation order
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (lane semantics) / FSR/Presentation (docs)
- **Location**:
  - The `vec4 dofParams;` comment in `crates/renderer/shaders/include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp` and `caustic_splat.comp`.
  - The `dof_params` row of the `GpuCamera` table in `docs/engine/shader-pipeline.md`.
  - The `presentation.frag` pass-table row and the presentation step in `docs/engine/shader-pipeline.md`.
  - The `presentation.rs` tree row and the presentation step in `docs/engine/renderer.md`.
- **Status**: NEW (related to #5022, which established the lane-description pin).
- **Description**:
  - Every mirror reads "w = history mode (0 moving, 1 parked, 2 parked + scene-static; …)". Mode 3 (#5369) is missing, although the Rust doc on `GpuCamera` and `restir_history_mode` describe it.
  - `every_camera_ubo_mirror_describes_dof_w_as_the_history_mode` matches that text as a prefix, so it stays green.
  - The presentation docs still give `tonemap(compressed * exposureTex)` and never mention the grade. Since #5482 the code is `exposed = scene × exposure` → grade (with the toe) → chroma compress → `tonemap(compressed)`.
  - `docs/engine/ui.md`'s `aces(graded * exposure)` is explicitly historical (pre-#3426), so it is not stale.
- **Suggested Fix**:
  - Append "3 parked + rig-geometry-static under flicker" to the five mirror comments and the table row, and extend the #5022 pin to require it.
  - Rewrite the two presentation passages to name the exposure → grade → tonemap order.

## Prioritized Fix Order

1. **D11-01.** A one-line weight change plus a test over the census range. It is the exact symptom #5482 closed on.
2. **D2-01.** An order-independent rig key. It decides whether #5369's main lever reaches real multi-light scenes; check it before working #5370's remaining leads.
3. **D3-01.** Comments, docs and the extended pin.
4. Carried open items: #5402 (L8 DDS), #5455 (transmission handoff mask), #5457 (SAFETY comment).

## Needs-RenderDoc / live validation

No engine or GPU process was launched.

- **D11-01.** A capture in the Ultra-Luxe casino (FNV) or Frostmere Crypt (Skyrim) with the `exposure`/image-health stats, to compare exactly-zero pixel counts before and after the weight change.
- **D2-01.** On a parked view of a multi-candle Skyrim/Oblivion interior or an FO4 Institute fluorescent bank, log `dof_params.w` per frame (expect mostly 1.0 today), then repeat with an order-independent key.
- **#5378.** An A/B of a Skyrim SE `.btr` vista seam was the baseline's request and is still owed.
- **Carried** from the baseline's list (all still unrun):
  - #5188 / #5187 / #5194 / #5215 fault-injection and sync-validation runs;
  - the FSR switch, forced dispatch failure and the default FSR Quality validation run;
  - the FP32 SDK permutation, which is untested.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 11, Order bullet**: still says presentation does "`tonemap(compressed * exposure)`". Since #5482 it is `exposed = scene × exposure` → saturation → `gradeContrast` (pivot `GRADE_CONTRAST_PIVOT` 0.18 exposed, toe `GRADE_CONTRAST_TOE` 0.1) → brightness → tint → chroma compress → `tonemap(compressed)`. Add `grade_contrast` to the host-mirror list.
- **Dim 3, lane-semantics bullet**: add "`dof_params.w` modes 0/1/2/3 (#5369)", and note that the #5022 mirror pin is a prefix match that cannot see a new mode.
- **Dim 2 / Dim 10**: the ReSTIR EMA bullet should record the 256 / 0.008 parked tail and `restir_rig_static` (the #5369 intensity-blind key) next to the 16 / 0.1 moving window.
- **Dim 7**: drop "#5211 still live" from any carried list; the fallback guard is fixed and pinned.
- **Dim 5, textures bullet**: #5378 closed the 32-bpp mask gap. L8 / `DDPF_LUMINANCE` (#5402) is the remaining accepted-format gap. 32-bpp files with no alpha mask now take the `expand` path.

## Guard posture

| Dim | Guards confirmed green | Blind spot |
|---|---|---|
| 1 | lib + bin full runs (`acceleration`, static BLAS recovery) | — |
| 2/10 | `restir_history_mode` tests, rig-key test, restir.rs 256/0.008 pin | the rig-key test uses one light, so it cannot see the order dependence (D2-01) |
| 3 | sizes, offsets, mirrors, `every_camera_ubo_mirror_describes_dof_w_as_the_history_mode` | prefix match misses mode 3 (D3-01) |
| 5 | DDS mask fixtures (BGRA zero-copy, X8R8G8B8 expand, empty masks), `a_submitted_skip_frame_does_not_rearm_the_episode_warn`, `device_names_read_through_bounded_accessors` | — |
| 7 | `temporal_pass_sanitizes_non_finite_samples_on_every_history_path` | — |
| 11 | `contrast_keeps_shadows_above_black_and_monotone`, `contrast_identity_and_flat_ends`, `presentation_grades_exposed_radiance_with_the_mirrored_contrast` | contrast is pinned at 1.3 only (D11-01) |

## Process notes

**Dedup.**
- Open issues came from the suite cache (`/tmp/audit/issues.json`, 147 open).
- Closed and all-state searches: "contrast toe", "gradeContrast", "crushed shadows", "contrast pivot", "rig key", "history mode flicker", "light order flicker", "restir history", "dofParams history mode", "presentation doc tonemap". Only the related items named above matched.
- #5482, #5369, #5211, #5210 and #5378 are CLOSED; #5370 is OPEN.

**Not re-reported** (already filed by this suite):
- CONC-D5-2026-10-09-01 / SAVE-D4-2026-10-09-01: the #5379 purge leaves GPU resources unreleased.
- PERF-D3-2026-10-09-01: the `dynamic_rgba` growth retry.
- EXT-D6/D1-2026-10-09-*: LOD prune and EXAL gating.

**Dropped candidates.**
- `gradeContrast` overflow to `inf`, and the resulting NaN from `mix`. It needs `c·log2(x/0.18) > 128`, i.e. contrast above about 8 on sun-disc radiance; the authored maximum is 2.0.
- The 0.18 pivot versus the meter's own exposed key (≈ 0.104 under `1/(1.2·2^EV100)`), and the uncarried IMGS "Contrast Avg Lum Value". Both are design choices documented in the code (`world.rs`, `shader_constants_data.rs`), not demonstrable defects.
- The ratio estimator not fully cancelling intensity under mixed visibility. With the deeper 0.008 floor, a light toggling inside a mixed cluster lags about 125 frames; this is the documented #5020 lag.
- #5378's X8R8G8B8 diffuse (9,326 Skyrim terrain-LOD atlases) now takes the `expand` path. Both upload paths skip `average_rgb` there, with a stale "16/24-bpp … UI/font atlases" rationale (`texture_registry/upload.rs`). Its only consumers are caustic sources and ground-cover models, neither of which uses terrain LOD, so there is no behavioural effect today.
- #5275: a frame whose dirty set holds only released or resized drops re-arms the warn without a copy. This is harmless.

To publish: `/audit-publish docs/audits/AUDIT_RENDERER_2026-10-09.md`. Suggested labels: `renderer` and `shaders`, plus `doc-rot` for D3-01.
