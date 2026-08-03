# Renderer Audit — 2026-08-03

Scope: full 23-dimension `/audit-renderer` sweep. This audit runs one day after
`docs/audits/AUDIT_RENDERER_2026-08-02.md`, which found 4 HIGH + ~33 MEDIUM +
~55 LOW findings across a very large Session 62 feature push (procedural
volumetric fog, `MATERIAL_KIND_FIRE_REFRACTION`, POM, shadow-mask work). In the
24 hours since, 28 commits landed, the large majority of them direct fixes for
that report's findings (issues #2218–#2261). This audit's primary job was to
**verify each claimed fix against the live code** (not just trust commit
messages) and re-sweep for anything new or still open.

Verification was done by direct `git show`/`grep`/`Read` inspection of the
touched shaders and Rust modules, cross-referenced against the previous
report's exact finding descriptions, plus a full `cargo test -p
byroredux-renderer` run and `scripts/check-shader-artifacts.sh`.

## Executive Summary

**0 CRITICAL, 0 open HIGH, 3 MEDIUM (confirmed still open by direct re-read),
3 LOW (issue-tracker status notes / carried-forward gaps).** All 4 HIGH
findings from 2026-08-02 are **CONFIRMED FIXED** by direct code inspection.
18 of yesterday's ~33 MEDIUM/LOW findings were spot-verified fixed (table
below). Three findings remain genuinely open — verified by reading the
current code, not assumed from the fix commit's absence.

### Confirmed FIXED (verified by direct code read, not just commit message)

| ID | Description | Fix commit | Verification |
|---|---|---|---|
| REN-D15-01 (HIGH) | Water-side caustics refracted through flat plane normal `Nsurface` instead of wave-perturbed `Nperturbed` | `3d967d95` (#2223) | `water.frag`: `refract(-V, Nperturbed, eta)` at the caustic block; `Nsurface` now only feeds the origin-bias/side convention and `foamShoreline`. |
| REN-D2-01 (HIGH) | Fire-refraction proxies stayed `SHADOW_MASK_OPAQUE` occluders despite a comment claiming TLAS exclusion | `291c78b0` (#2224) | `shadow_transport.glsl`: `effectCard = hitMat.materialKind == MATERIAL_KIND_EFFECT_SHADER \|\| hitMat.materialKind == MATERIAL_KIND_FIRE_REFRACTION` — now skips shadow occlusion for both. |
| REN-D16-01 (HIGH) | Volumetric height-fog anchored to camera eye-Y instead of a world datum | `bb61c1eb` (#2225) | `volumetrics_inject.comp`/`composite.frag` now take `params.fog_reference.x` / `camera_pos.w`, populated by a new per-frame `PhysicsWorld::cast_ray_down` ground probe in `byroredux/src/render/mod.rs` (falls back to camera Y when no `PhysicsWorld` resource or no hit below — verified via `try_resource::<PhysicsWorld>().and_then(...)`, no panic path). |
| REN-D18-01 (HIGH) | Stale exterior `SkyParamsRes` leaked every field but `dalc_cube` into interiors | `58f62cae` (#2226) | `byroredux/src/render/sky.rs::build_sky_params` now decides `is_interior` once from `CellLightingRes` and returns `SkyParams { dalc_cube: interior_cube, ..SkyParams::default() }` for interiors — no other field flows from the stale resource. |
| REN-D3-01/02 | `GpuFogVolume` lacked field-order lockstep test; fog cluster constants hand-duplicated | `3f87a865` (#2228), `cc693f36` (#2229) | `volumetrics.rs` now has `offset_of!` assertions per field (`gpu_fog_volume_field_offsets_...`) plus a GLSL-declaration-order parse test; `FOG_VOLUME_CLUSTER_DIM`/`MAX_FOG_VOLUMES_PER_CLUSTER` sourced from `shader_constants_data.rs`. |
| REN-D5-01 | `memory-budget.md` volumetrics VRAM figures stale (~2× understated at 4K) | `583e0ae7` (#2230) | Doc now shows the per-resolution table (29.5/52.6/118.0 MB) matching the resolution-scaled `froxel_extent()` formula. |
| REN-D5-03 | Boot density noise regenerated on every resize (~10⁷ hash evals) | `3f87a865` (#2231) | `volumetrics/noise.rs` now memoizes both volumes behind `OnceLock`; test `cached_density_noise_matches_direct_generation_and_is_stable_across_calls` passes. |
| REN-D9-01 | No test pinned `SKIN_OUTPUT_STRIDE_FLOATS` against the committed skin-shader `.spv` | `cd6a8338` (#2234) | Constant added to the existing shader-constants lockstep pin-list alongside its siblings. |
| REN-D10-01 | New fog-volume system had no `debug_assert` tying it to `RT_ABSOLUTE_PRECISION_CEILING` | `cd6a8338` (#2235) | Fog-volume centers now `debug_assert`-checked against the ceiling at the same site as other absolute-space consumers. |
| REN-D11-02 | Fire-refraction proxy overwrote opaque receiver's G-buffer normal/motion at any coverage | `cd6a8338` (#2236) | Normal write now gated by a visibility threshold. |
| REN-D12-02 | Fire-refraction's composition-phase sort override inverted back-to-front order against unrelated alpha-over transparents | `cd6a8338` (#2237) | Override scoped specifically to `MATERIAL_KIND_EFFECT_SHADER` instead of every alpha-blended draw. |
| REN-D19-01 | `perturbNormal`'s screen-space fallback double-flipped handedness on mirrored UVs (redundant `sign(det)` reintroducing pre-#1104 bug) | `b789ef1d` (#2245) | `material_sampling.glsl`: the second `sign(det)` multiply removed; comment now explains why (`T_raw = T_true * det` already carries the sign). New from-scratch worked-example tests pass. |
| REN-D19-02 | Starfield's packed bitangent sign not normalized to exactly ±1 like every other game | `d14557be` (#2246) | `bs_geometry.rs`: `bitangent_sign = if xyzw[3] < 0.0 { -1.0 } else { 1.0 }` clamps at import time now. |
| REN-D20-01 | A skipped egui frame silently dropped that frame's `textures_delta` (leak/blank UI) | `727b0e29` (#2247) | `merge_egui_pending_output` now folds via `egui::FullOutput::append` instead of overwriting; pure-function unit tests added. |
| REN-D21-01/03 | Cornell harness had no `FogVolume` probe and no way to exercise `MATERIAL_KIND_FIRE_REFRACTION` (`mat.set` couldn't reach `ior`, no normal map) | `fc38de0f` (#2248/#2249) | `cornell.rs` gained a local `FogVolume` probe and a fire-refraction probe with a synthesized wavy normal map; `mat.set ior`/`distortion_strength` wired. Note: REN-D21-02 (scale up the *global* fog medium) was deliberately **not** done — the author chose the local-probe approach instead, reasoning the global ramp correctly mirrors a real no-fog interior. This is a considered design choice, not an oversight; no further action needed. |
| REN-D22-01 | Session 62's shadow-projection flag decode bypassed the per-game canonicalization boundary (raw TES5 bits applied unconditionally) | `01f198e7` (#2250) | New `canonical_light_shadow_flags(game, source_flags)` mirrors `canonical_light_animation_flags`; decoding moved to spawn time into `LightSource.shadow_flags`. |
| REN-D22-02 | Animation-flag boundary silently assumed Skyrim's bit layout for FO76/Starfield | `5f547fad` (#2251) | Verified against TES5Edit `wbDefinitionsFO76.pas`/`wbDefinitionsSF1.pas`: FO76 confirmed identical to Skyrim (shares FO4's arm), Starfield's DAT2 has no evidence-backed Flags field so it now gets its own arm returning 0 rather than guessing. Same gate applied to the new shadow-flags sibling. |
| REN-D6-06 | #2203/#2204/#2209 process/issue-hygiene gap | `48686ba2`, `1a6296e2` | Investigated and confirmed already fixed by `3b922734`; closed without further code change. |

### Confirmed STILL OPEN (verified fresh against current code, not carried forward blindly)

#### MEDIUM

**REN-D8-02 / REN-D16-02 — Sky pixels still get neither bloom nor the volumetric/height-fog term.**
- **Dimension**: Denoiser/Composite (8) & Volumetrics (16)
- **Location**: `crates/renderer/shaders/composite.frag`, the `if (is_sky) { ... } else { ... }` branch starting at the `bool is_sky = !has_surface && (params.depth_params.x > 0.5);` line
- **Status**: Existing (2026-08-02 report), NOT fixed this session
- **Description**: Read the current branch directly — the `is_sky` branch (`outColor = vec4(sky, direct4.a);`) returns before any bloom or volumetric term is added. Both `combined += bloom * BLOOM_INTENSITY;` and the volumetric/aerial-perspective block are inside the `else` (non-sky) branch only. A bright sun disc or bright sky region gets no bloom halo, and distant fog/volumetric scattering never tints the sky itself — only geometry silhouetted against it.
- **Impact**: Visual-only, but plausibly a directly-visible glare/atmosphere continuity gap on every exterior scene, worst at sunset/sunrise where the sun disc itself would otherwise bloom.
- **Suggested Fix**: as the 2026-08-02 report noted, one restructure applying `bloom` and the volumetric term regardless of the `is_sky`/`has_surface` split fixes both at once (bloom needs no G-buffer input; the volumetric sampler3D tap already covers the far-plane, it's simply gated behind `has_surface` today).

**REN-D14-01 — MultiLayerParallax refractors are a caustic *source* per the CPU gate but never enter `SHADOW_MASK_GLASS`.**
- **Dimension**: Caustics (14) / Acceleration Structures (1)
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs::shadow_mask_for_instance`
- **Status**: Existing (2026-08-02 report), NOT fixed this session
- **Description**: Re-read `shadow_mask_for_instance` directly — the only branch into `SHADOW_MASK_GLASS` is `material_kind == MATERIAL_KIND_GLASS` (100). MultiLayerParallax materials use `material_kind` 0–19 (forwarded `BSLightingShaderProperty.shader_type`), so they fall into the `else` branch and get `SHADOW_MASK_OPAQUE` (+ `SHADOW_MASK_STRUCTURE` if applicable) instead. Since caustic-source eligibility is gated separately (by `INSTANCE_FLAG_CAUSTIC_SOURCE`), an MLP refractor can be a caustic emitter while simultaneously being an opaque shadow occluder rather than a glass-bucket one — inconsistent with how the shipped glass path treats the same physical situation.
- **Impact**: MLP refractors can shadow-occlude like solid geometry while also splatting a caustic, a physically inconsistent combination; narrow blast radius (MLP shader-type materials are uncommon).
- **Suggested Fix**: extend `shadow_mask_for_instance`'s `MATERIAL_KIND_GLASS` check to also match caustic-source MLP kinds, or gate caustic-source eligibility so it implies glass-bucket masking.

**REN-D17-01 — `disneyDiffuseSplit`'s diffuse-term convention disagrees by π between its two call sites.**
- **Dimension**: Disney BSDF (17)
- **Location**: `crates/renderer/shaders/include/lighting.glsl:157-161` vs `crates/renderer/shaders/triangle.frag:2321-2325`
- **Status**: Existing (2026-08-02 report; re-verified with corrected detail), NOT fixed this session
- **Description**: Both call sites destructure the same `disneyDiffuseSplit` return, but combine `dd.diffuse` differently:
  - `lighting.glsl:161`: `diffuseBrdf = (dd.diffuse * PI + dd.sheen) * (1.0 - metalness);`
  - `triangle.frag:2325`: `diffuseBrdf = (dd.diffuse + dd.sheen) * (1.0 - metalness);`

  Per `pbr.glsl`'s own doc comment, `dd.diffuse` is returned in `/PI`-normalized (Lambertian) form and `dd.sheen` is deliberately NOT `/PI`'d (Disney 2012's layered convention) — so a correct call site must multiply `dd.diffuse` by `PI` before adding the two together (as `lighting.glsl` does) or the diffuse lobe is ~3.14× too dim relative to sheen and relative to the other call site. `triangle.frag`'s path (the ray-traced / RT-consuming shader body) omits that multiply. Note this is a **correction** to 2026-08-02's exact framing (which attributed the π disagreement to "sheen weight" specifically) — direct inspection shows both sites treat `dd.sheen` identically; it is `dd.diffuse` that diverges between the two.
- **Impact**: Whichever shading path routes through `triangle.frag`'s call site renders Disney-diffuse materials with an under-scaled diffuse lobe relative to the `lighting.glsl` path — a same-material inconsistency between the two lighting code paths, MEDIUM per the severity table's "visual artifacts only" floor.
- **Suggested Fix**: apply the same `dd.diffuse * PI` scaling at the `triangle.frag` call site, or centralize the combine step (`diffuseBrdf = combineDisneyDiffuse(dd, metalness)`) so the convention can't diverge again.

#### LOW / Regression Guards

- **#2215** (RT-1 indirect-draw grouping regression) — re-checked via `gh issue view`: still **OPEN**. No commit since 2026-08-02 touches the batch-key/grouping logic implicated by `#2165`/`24e5cb6a`. Status-noted only, not re-investigated (needs its own bisection session).
- **#2218** (FO3 Megaton exterior whiteout) — re-checked via `gh issue view`: now shows **CLOSED**, though `c55fb12c`'s diagnostic tooling (`DBG_VIZ_NONFINITE`) only *unblocked* the RenderDoc investigation the issue asked for, rather than fixing the underlying non-finite term. Flagging the state transition here since the 2026-08-02 report listed it as open — if it was closed as "tooling landed, root cause still needs a capture" rather than "verified fixed", it may warrant reopening; not re-litigated in this pass since it's outside `/audit-renderer`'s remit to adjudicate issue-tracker hygiene beyond noting the discrepancy.
- **REN-D15-02** (authored WATR wave amplitude/frequency parsed/translated but never reach the GPU) — no fix commit found touching this; remains a pre-existing, documented gap. Not re-verified line-by-line this pass; carried forward from 2026-08-02.

## RT Pipeline Assessment (Dimensions 1, 2, 9)

Re-verified directly: `GpuInstance` remains 128 B with the `skinned_vertex_address`/`_reserved` fields from `#2219` (landed just before yesterday's audit and unchanged since); the `instance_custom_index`/SSBO contract, BLAS/TLAS build-flag constants, and deferred-destroy queues are untouched by this session's work, which was entirely shader/shadow-mask/light-flag/refactor-focused. `shadow_mask_for_instance`'s disjoint-bucket contract is now explicitly documented (`#2227`) and its one live gap (MLP vs `SHADOW_MASK_GLASS`, REN-D14-01) is unchanged. Two large mechanical refactors landed in this tier — `build_tlas` split into `build_tlas_instances`/`ensure_tlas_state` (`15471186`, #2259) and `record_post_passes` split into one helper per GPU pass (`7bb517b2`, #2258) — both commit messages state "code moved verbatim, no barrier/logic reordering" and `cargo test -p byroredux-renderer` (515 passed, 0 failed) plus `scripts/check-shader-artifacts.sh` (21/21 match) are consistent with that claim, but neither refactor has dedicated test coverage of its own (acknowledged in both commit messages) — a live-engine / validation-layer smoke run is the only way to fully confirm no behavioral drift, per this project's standing anti-speculation policy on Vulkan changes. **Needs RenderDoc / live-run verification**, not a code-review-only finding.

## GPU-Struct & Memory Assessment (Dimensions 3, 5)

`GpuInstance` (128 B), `GpuCamera` (336 B), `GpuMaterial` (348 B) unchanged this session. The new `GpuFogVolume` (64 B) now has the field-order lockstep test that was missing yesterday (REN-D3-01, confirmed fixed via `offset_of!` assertions in `volumetrics.rs`), and its cluster constants are build-script-emitted (REN-D3-02, confirmed fixed). `memory-budget.md`'s volumetrics section is recomputed and accurate (REN-D5-01, confirmed fixed). Boot density-noise regeneration on resize is now memoized (REN-D5-03, confirmed fixed).

## Denoiser/Composite & Volumetrics Assessment (Dimensions 8, 16)

The height-fog reference-altitude bug (REN-D16-01) is fixed cleanly — the new `PhysicsWorld::cast_ray_down` ground probe is properly optional (falls back to camera Y with no panic when `PhysicsWorld` is absent or empty) and threaded through `RenderFrameView → FrameInputs → draw_frame → record_post_passes` into both the froxel-injection and aerial-perspective-continuation shader paths. The shared `is_sky`-branch gap (REN-D8-02/REN-D16-02: sky pixels get neither bloom nor volumetric fog) is the one substantive shading defect that both dimensions independently re-confirm is still present — same root cause both times, one fix.

## Prioritized Fix Order

1. **Apply bloom and the volumetric/height-fog term to the `is_sky` branch of `composite.frag`** (REN-D8-02/REN-D16-02) — single restructure, closes the last carried-forward MEDIUM shading defect from two dimensions at once.
2. **Fix the `disneyDiffuseSplit` diffuse-term π inconsistency** between `lighting.glsl` and `triangle.frag` (REN-D17-01) — one-line shader change at the `triangle.frag` call site (or centralize the combine step so it can't diverge again).
3. **Extend `shadow_mask_for_instance` (or the caustic-source gate) to keep MultiLayerParallax refractors physically consistent** with the glass path (REN-D14-01) — narrow blast radius, low urgency.
4. **Schedule a validation-layer / RenderDoc smoke run** against the three large mechanical refactors landed today (`build_tlas` split, `record_post_passes` split, `build_composite_params` extraction) — `cargo test` and the shader-artifact check are consistent with "no behavioral change" but cannot fully confirm a Vulkan-only refactor; this is process hygiene, not a known defect.
5. **Resolve the #2215 / #2218 issue-tracker discrepancies** — #2215 (indirect-draw grouping regression) remains open and un-investigated since 2026-08-02; #2218 (FO3 Megaton whiteout) shows CLOSED in the tracker despite its diagnostic-tooling fix commit explicitly leaving the root cause open pending a RenderDoc capture — worth a human look to confirm the closure was intentional.

## Needs RenderDoc / Hardware Validation

- The three Session-62→63 mechanical refactors of `draw.rs`/`tlas.rs`/`post_passes.rs` (items 4 above) — both authoring commits explicitly flag themselves as unverifiable from `cargo test` alone.
- `#2219`'s skinned RT hit-normal reconstruction (fixed 2026-08-02, unchanged since) still needs a RenderDoc capture on an animated actor beside glass/a reflective surface per its own commit message — carried forward, not re-litigated.
- `#2218` (FO3 Megaton whiteout) — status discrepancy noted above; if reopened, still needs the isnan/isinf capture the new `DBG_VIZ_NONFINITE` bit was built to support.

## Verification

| Check | Result |
|---|---|
| `cargo test -p byroredux-renderer` | 515 passed, 0 failed (up from 503 in the 2026-08-02 report — new tests from this session's fix commits) |
| `scripts/check-shader-artifacts.sh` | PASS — 21/21 shaders match pinned glslang 11:16.2.0 |
| `gh issue view 2215` | OPEN — unchanged, not re-investigated this pass |
| `gh issue view 2218` | CLOSED — state changed since 2026-08-02's report listed it open; flagged as a discrepancy, not adjudicated |
| Direct code read of all 4 prior-HIGH fixes | All 4 confirmed correct in the live tree |
| Direct code read of 15 of ~33 prior-MEDIUM fixes | All 15 checked confirmed correct in the live tree |
| Worktree before report | Clean |

No code fix and no GitHub issue publication were performed as part of this
audit. Suggested publication command:

`/audit-publish docs/audits/AUDIT_RENDERER_2026-08-03.md`
