**HEAD**: `7e9da5dcc` · **Baseline**: `AUDIT_RENDERER_2026-09-21.md` (HEAD `f97775ca8`) for Dims 1–4, 6–8, 10–12; `AUDIT_RENDERER_2026-09-26.md` (HEAD `078f650ec`) for Dim 5; `AUDIT_RENDERER_2026-09-20.md` (HEAD `052891f22`) for Dim 9 · **Audited**: Dims 1–12 (every dimension had commits on its Paths since its baseline: 10–32 each, 5 for Dim 5) · **Unchanged since baseline (skimmed)**: none

# Renderer Audit — 2026-09-27 (all 12 dimensions, deep)

`/audit-renderer`, default parameters (all dimensions, `--depth deep`). The orchestrator ran 12 dimension agents, three at a time (briefly four at a time, over the skill's cap). Each agent was read-only and guard-first, and ran at most `-j 4`. The orchestrator re-checked every HIGH and MEDIUM finding against live source before including it (the checks are listed under *Process notes*). No source, doc or skill was edited.

**Live evidence** (orchestrator, release build of HEAD, RTX 4070 Ti, `BYRO_VALIDATION=1` = core + sync validation). Five runs:
- `--cornell` at FSR native-aa, 90 frames.
- `--cornell --upscaler taa`, 90 frames.
- FNV `GSProspectorSaloonInterior` with skinned NPCs live, native-aa, 300 frames.
- **Pinned `--upscaler fsr3 --fsr-quality quality` reruns** (render 853×480 → output 1280×720) of Cornell (90 frames) and FNV Prospector (300 frames). These were needed because a saved `settings.toml` had silently turned the first "default" runs into native-aa (REN-D11-2026-09-27-01).

Results: **0 VUID and 0 SYNC-HAZARD in every run.** rt-integrity was clean everywhere: Cornell 31/31 and FNV 893/893 `tlas_emitted == tlas_eligible`, all `missing_*` = 0, 0 lights dropped, 0 cluster overflow. The 09-20 water push-constant-range pair no longer fires.

The only validation output is 9 `(SPIR-V Interface)` *performance* warnings at one pipeline creation. `groundcover_blade.vert` writes outputs that `groundcover_blade.frag` never reads (locations 6/7/13/16 float, 10/11/12/15/20 uint). This is valid SPIR-V, owned by `/audit-exterior`, and it repeats every session start, which makes real validation errors easier to miss.

Not live-covered: resize, `r.upscaler` preset switch, exterior grid streaming (ground-cover model tier), any water cell, and an animating Scaleform HUD.

## Executive Summary

| Severity | Count | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 2 | REN-D2-2026-09-27-01 (ReSTIR RIS weight: direct term of shadow-casting lights is the cluster *mean*, not the sum); REN-D6-2026-09-27-01 (FO4 spec-off BGSM → keyword-classified metal trim renders as zero-specular conductor, 33 vanilla shapes) |
| MEDIUM | 6 | D7-01 (ReSTIR direct EMA low-passes light flicker), D7-02 (SVGF scene-static misses spawn/despawn), D7-03 (TAA-mode water resolves as lake bed), D8-01 (**Regression of #4858**: window-glass sun portal query can never hit), D10-01 (BGSM SSS lobe unshadowed for every light), D11-01 (saved `render.upscaler` silently overrides the CLI default) |
| LOW | 36 | Test gaps, doc/ledger rot, one per-occurrence skin-slot leak, a latent timer-slot collision, hot-path std hashing, and others (see Findings) |

**The two HIGHs are both lighting-energy errors, and neither shows up in any `cargo test` guard.**

**REN-D2-2026-09-27-01** predates the window. The fresh-candidate loop in `triangle.frag` adds `w_i = pHat` and `M += 1` for each of the N enumerated cluster lights, then finalizes `W = ΣpHat / (M·pHat_y)`. With the uniform proposal the weight should be `N·pHat`. The shadowable direct term is therefore `(1/N)·Σ rad`. Single-light scenes, including every Cornell single-emitter check, are exact, so they cannot catch it.

**REN-D6-2026-09-27-01** was introduced by the #4836 fix (`853b2180e`, 09-26). A specular-disabled BGSM now defers metalness to the NIF side. On FO4 the NIF side is `classify_pbr_keyword`'s path-only `metal`/`iron` arm, which returns 0.9, while #4654 has already zeroed the specular term. Sanctuary residential trim and the wrought-iron fences are the confirmed vanilla population.

**The MEDIUM tier is mostly temporal-history problems** (D7-01/02/03), plus two correctness regressions in the delta window: D8-01, which came in with a fix, and D11-01.

**D8-01 is the cleanest example of a source-shape guard proving the wrong thing.** `ab255cfd2` rewrote `traceArchitecturalWindowGlass` as a `while (rayQueryProceedEXT)` candidate loop but kept `gl_RayFlagsOpaqueEXT`. That flag makes traversal commit hits itself, so no triangle candidate is ever produced, the loop body never runs, and the function always returns `false`. The guard `architectural_glass_portal_checks_every_candidate_layer` pins exactly that broken shape.

**Structural and sync health is good.** No `#[repr(C)]` struct drifts from its GLSL mirrors: `186234944` grew `GpuLight` from 64 to 80 B and the light header from 16 to 4112 B, with all four GLSL copies and the host sizing in lockstep. Every `.spv` is byte-reproducible from source (`scripts/check-shader-artifacts.sh`: 36 shaders plus the early-test variant). The `instance_custom_index` == instance-SSBO contract holds on both halves. The geometry-pool in-place compaction (`7e9da5dcc`) and the VRAM-scaled rebuild ceiling (`5226d73e2`) are sound. Every named guard that still exists passes, and none is `#[ignore]`d.

**Housekeeping.** 13 OPEN issues are fixed in code and can be closed after a glance: #4778, #4783, #4785, #4789, #4862, #4863, #4865, #4866, #4867, #4868, #4872, #4880, and #4864 (docs done, A/B not recorded). The 2026-09-24 renderer audit that filed #4827–#4880 was never committed to `docs/audits/`, so the next run has no file to use as its baseline for those issues.

## RT Pipeline Assessment

- **Acceleration structures (Dim 1): healthy.**
  - Flags match between build and refit at every site, and against `BlasEntry.built_flags`.
  - The TLAS instance index equals the instance-SSBO slot on both halves: `begin_frame_recording` grows the buffer and caps the map (#4833), and `build_and_upload_instances` uses the same filter under its `debug_assert_eq!`.
  - The instance-mask categorisation holds; #4576 and #4834 stay fixed.
  - The BLAS budget is re-derived at the end of a resize, after the upscaler is recreated.
  - `rt.integrity` runs every frame.
  - `7e9da5dcc` cannot invalidate a BLAS. A static BLAS keeps no reference to its source buffers once built and is never refit; skinned BLAS use per-mesh index buffers.
  - `5eb07a4f3` added a TLAS refit-identity rule (EntityId tie-break, and a BUILD when the membership set changes) that no test covers (D1-01).
- **Ray queries and shading (Dim 2): indexing and precision are clean; the estimator is not.**
  - The SSBO chain (`instance_custom_index` → `instances[]` → `materials[material_id]` → vertex/index SSBOs) holds.
  - The RT gate precedes every fragment ray query, and ray origins are scale-aware.
  - The relative/absolute coordinate split holds, and #4582, #4583 and #3980 stay fixed.
  - Every `exteriorSkyRadianceOr` caller is correctly gated today, but the interior gate lives only at the call sites (D2-04).
  - The Dim 3 lead on zero-filled reservoirs was traced and dropped: every reader gates on `M > 0 && W > 0`.
- **Denoiser (Dim 7).**
  - SVGF ping-pong, motion vectors, mesh-ID disocclusion, the no-history firefly clamp and dispatch coverage all hold. The composite order is correct (linear HDR, fog on the fully reassembled term, SSAO on indirect only, caustics into direct).
  - Two temporal-history gaps remain: ReSTIR direct radiance is EMA'd with no light-change invalidation (D7-01), and SVGF's scene-static signal ignores changes to the instance set (D7-02).
- **Volumetrics, caustics and water (Dim 8):** the checklist holds except the window-glass portal regression (D8-01) and the unfiltered nuclear dimmer (D8-02).

## GPU-Struct & Memory Assessment

- **Layout.** Every `#[repr(C)]` ↔ GLSL mirror is in lockstep. The guard sets pass: renderer lib 287/0/1-ignored (the ignored test is device-only and outside Dim 3); `gpu_material_size_claims` passes.
  - Blind spots: `GcDrawIndirect` is misclassified `ShaderLocal` (D3-05), and `GcCameraUBO` is a prefix mirror of `GpuCamera` pinned by block size only (D3-06).
  - `0e0d35b96` left `GpuCamera`'s `NoUninit` SAFETY comment false (D3-02).
- **Hot-path hashing.** `LightHistory` (`scene_buffer/light_history.rs`) introduced a std `HashMap`, and `upload_lights` a `DefaultHasher`, in the per-frame upload path. Three dimensions found this independently; it is merged as D3-03.
- **Memory (Dim 5, delta pass).**
  - `7e9da5dcc`: only `apply_compaction_plan` publishes offsets. The `copy_within` sweep never overwrites a range it has not moved yet. CPU readers between plan and publish are gated on `deferred_compaction`, and in-flight GPU buffers are untouched until the deferred swap.
  - `5226d73e2` works in bytes throughout, with a safe fallback when the budget reading is `None` or zero.
  - Nothing pins that the in-place branch actually runs (D5-01). memory-budget.md is stale on the rebuild transient, the light SSBO and the texture prefetch store (D5-02, D5-04, D3-01).
  - Of the 21 findings of 09-26: 19 are still OPEN and unchanged, #4880 is fixed but still open, and the unpublished finding 21 is mostly fixed (status table in the appendix).
- **Skinning (Dim 9).**
  - The palette plan and the first-sight `bind_inverses` path are sound.
  - A `MorphSlot` cannot be evicted while its entity lives, so the skill's "recreate" hazard does not exist.
  - One per-occurrence leak remains: the resize LRU rebase stamps every slot with 0, which `should_evict_skin_slot` treats as never evictable. A slot whose entity was despawned in the ~3 frames before a resize or preset switch is then never reclaimed (D9-01).


## Findings

### HIGH
#### REN-D2-2026-09-27-01: ReSTIR initial-candidate normalisation divides the shadow-casting direct term by the number of enumerated lights
- **Severity**: HIGH
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag` (fn `main`, ReSTIR stream `restirWSum += w_i; restirM += 1.0;` and finalize `restirW = min(restirWSum / (restirM * restirPHat), RESERVOIR_W_CLAMP)`, consumed by `frameContribution = rad * restirW * visibility`)
- **Status**: NEW. Not filed; searched `ReSTIR normalization`, `restirM` and `ReSTIR initial candidate M light count`. It is recorded as an open "estimator concern" in `docs/audits/PERFORMANCE_OPAQUE_EARLY_TESTS_2026-09-26.md` §"Separate normalization experiment, not retained", and noted "unchanged" in `PERFORMANCE_RESTIR_LIGHT_HISTORY_2026-09-26.md`.
- **Description**: Every shadow-casting light in the fragment's cluster is **enumerated** once as a candidate. The code adds weight `w_i = pHat_i` (luminance of `shadowableRadiance`) and increments `M` by 1 per candidate, then finalises `W = ΣpHat / (M · pHat_y)`. In RIS, the candidate weight is `pHat/p`, where `p` is the proposal pdf. Enumerating N lights is the uniform proposal `p = 1/N` with M = N, so the weight must be `N·pHat`. With the weight as written, `E[rad_y · W] = Σ_y (pHat_y/ΣpHat) · rad_y · ΣpHat/(N·pHat_y) = (1/N) Σ rad`. The estimator returns the **mean** of the shadowable lights' radiance, not the sum. Lights with `needsVisibility` contribute *only* through this estimate under ReSTIR (`if (!useRestir || !needsVisibility) Lo += shadowableRadiance;`), so nothing adds the rest back. The legacy-WRS arm, which the code's own tests cite as "the reference semantics being matched", normalises correctly: `W = resWSum / (K · w_sel)`, where K is the reservoir count and there is no candidate-count factor. Temporal and spatial combines add `M_r` in the standard way, so they carry the same 1/N scale forward rather than cancelling it.
- **Evidence**:
  - Two equal lights of radiance r: wSum = 2p, M = 2, W = 2p/(2p) = 1. Estimate r; truth 2r.
  - One light: W = 1. Correct, so single-light scenes and the Cornell single-emitter checks cannot see it.
  - The perf report's normalisation experiment "passed an added analytic total-energy test … It changed global lighting and was removed."
- **Impact**: In every cell with more than one shadow-casting light per cluster, the direct term of those lights is scaled by 1/N. The default `render/lights.rs` routes imported cell lights through full visibility, so this covers interiors in all games. Two effects follow:
  - Multi-light rooms render too dark on the direct term.
  - Because N is per-cluster, a light shared by adjacent clusters with different candidate counts contributes 1/N₁ versus 1/N₂. That is a potential cluster-grid-aligned brightness step (expected symptom, not observed; no engine run in this audit).
  - Any lighting tuning done since ReSTIR became default has been calibrated against the biased value.
- **Related**: #1369 (reservoir recompute), #2554 (far-field fade semantics), `restir_far_field_converges_to_unshadowed_radiance` (source-shape; no energy assertion).
- **Suggested Fix**: Weight fresh candidates by `pHat · N_candidates`, or equivalently treat the enumerated list as one initial sample with M = 1, per Bitterli Alg. 3/4. Add a CPU mirror test asserting the expected value equals `Σ rad` for N equal and unequal lights. As the perf report warns, gate the change on a transport-oracle A/B (Cornell `--cornell` multi-light plus one multi-light interior) because it rebrightens the whole ReSTIR term.

#### REN-D6-2026-09-27-01: A specular-disabled BGSM hands metalness to the NIF keyword classifier — 33 vanilla FO4 shapes (Sanctuary houses included) now render as zero-specular conductors
- **Severity**: HIGH. This is the floor for a wrong `Material` out of the NIFAL boundary. It is also a visible regression introduced by `853b2180e` on content that was a dielectric the day before.
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/asset_provider/material/merge.rs:651-681` (`merge_bgsm_arm`, the `if leaf.specular_enabled {` block and the unconditional `material.bgsm_pbr_scalars_authored = true;`). The keyword arm it falls back to is `crates/core/src/ecs/components/material.rs:1191` (`classify_pbr_keyword`, the path-only metal arm).
- **Status**: NEW. It is residual of CLOSED #4836 and #4654: the #4836 fix introduced it.
- **Description**: #4836 made the BGSM leave `metalness_override` untouched when `specular_enabled == false`, on the premise that "left untouched, all three keep whatever the NIF side classified … which zeroes the specular colour before `classify_legacy_pbr` runs and so classifies the surface as a dielectric." That premise holds only for the non-keyword arms. `classify_pbr_keyword`'s first arm returns `{metalness: 0.9, roughness: 0.55}` for any path containing `metal`/`steel`/`chainmail`/`iron`, and its second arm returns `{0.95, 0.25}` for `gold`/`silver`/`bronze`/`copper`, before specular is consulted. An FO4 NIF that inlines its diffuse in `BSShaderTextureSet` (15 of a 60-NIF DiamondCity sample; 17,980 of 34,995 vanilla NIFs carry both a `.bgsm` name and inline `.dds` strings) is keyword-classified at import. The spec-off merge then:
  - keeps that 0.9;
  - zeroes `specular_color` and `specular_strength` (#4654);
  - sets `bgsm_pbr_scalars_authored = true` anyway.

  The flag's own doc (`crates/nif/src/import/types.rs:722`) and the adjacent comment ("set at the exact site that merges them") say it means scalars were merged. It therefore also disables the overlay re-classification in `translate_material` and the normal-alpha heuristic. Shading then does `diffuseBrdf * (1 - metalness)` with the specular term × 0 (`lighting.glsl` `shadowableLightRadiance`), which leaves 10% of the diffuse and no highlight.
- **Evidence** (vanilla FO4, `material_dump` NIF-side values, which the spec-off merge now leaves final):
  - `Res01Modern01`–`06`, `Res01PlayerHouse`, `Res01PlayerHouseRoof`, `Res01PlayerHouseInterior` (`Intewall:11`), `Res01ModernCarport01`, `Res01ModernDormer01`: tex `trimmetalresidential01_d.dds`, metO 0.90 / rghO 0.55. Their BGSM `trimmetalresidential01.bgsm` has `spec_enabled=false`.
  - `Gate_ParsonsAsylum:1`, `FenceWIBollardPost01`, `FenceWIBollardStr01`, `CastleWallOutWedge02`: tex `wroughtiron01_d.dds`, metO 0.90. `wroughtiron01.bgsm` has `spec_enabled=false`, smoothness 1.0, so roughness takes the #3639 neutral 0.5.
  - DiamondCity `dextmetalrailings01` / `dextmetaldetails02`, `metalpanelslong01` (garage shell), gravel walls, and 4 SCOL precombines.
  - Total: 33 shapes in 26 NIFs.
  - Of the 467 spec-off FO4 BGSMs, 14 have a diffuse path that hits a metal/gold arm. That includes substring collisions such as `awesometales4_d` (`…someTALes` contains `metal`) and `comicbackgold_d`, but those magazine meshes were not found inlining the path.
  - Before `853b2180e`, the same shapes took `bgsm_metalness(specular_color, false)`, which is about 0 for white or near-white spec (a dielectric).
  - The #4836 fixture `merge_and_translate` seeds `NIF_METALNESS = 0.2` (`tests/bgsm_merge.rs:1473`), so no test reaches the keyword-metal input.
- **Impact**: Visibly dark, specular-less metal trim on every pre-war Sanctuary and Concord residential building and on wrought-iron fences and gates. The legacy sibling (a disabled `NiSpecularProperty` with a metal-keyword texture on Oblivion/FO3/FNV) has the same shape through walker.rs #696, but its population was not measured.
- **Related**: #4836, #4654, #696, #2609 (flag meaning), `classify_pbr_keyword` substring collisions (NIFAL scope).
- **Suggested Fix**:
  - When the leaf authors `specular_enabled = false`, resolve metalness to the dielectric the disabled block implies (`Some(0.0)`: FO4's spec-gloss model expresses a conductor only through specular), rather than deferring to a keyword guess.
  - Keep `bgsm_pbr_scalars_authored` truthful for that case (set it only where scalars are actually written, or document the new meaning).
  - Add a fixture with `NIF_METALNESS = 0.9` (keyword-metal) and a spec-off leaf.


### MEDIUM
#### REN-D7-2026-09-27-01: The ReSTIR direct-radiance EMA has no light-change invalidation, so flicker, pulse and toggles are low-pass filtered, and its parked mode keys on the bare camera flag, not SVGF's scene-static signal
- **Severity**: MEDIUM
- **Dimension**: Light Animation
- **Location**: `crates/renderer/shaders/triangle.frag` — the ReSTIR accumulate block (`bool cameraStatic = dofParams.w > 0.5; float historyCap = cameraStatic ? 64.0 : 16.0; float alphaFloor = cameraStatic ? 0.025 : 0.1;` … `accum = mix(prevAccum, frameContribution, alpha)`). The `reprojValid` acceptance (`sameSurface && rpHistLen > 0.0 …`) is where the history is kept. The flag is fed from `assemble_camera_and_lights.rs` `dof_params: [.., if camera_static {1.0} else {0.0}]`.
- **Status**: NEW. It predates the window (introduced `883f57cd7`, 2026-07-20). Dim 2 (REN-D2-2026-09-27-01) covers the ReSTIR normalisation only; this is a different defect. I searched issues for "restir temporal history", "light flicker", "accum history light" and "historyCap": no match.
- **Description**:
  - The direct term for every shadow-casting light (`if (!useRestir || !needsVisibility) Lo += shadowableRadiance;` routes all `needsVisibility` lights through ReSTIR) is `frameContribution = rad · W · V̄`. That is **radiance**, recomputed each frame from the light's current intensity.
  - It is then EMA-blended with the reprojected `prevAccum`. History is accepted whenever the surface ID, depth and normal match. Nothing compares the light's radiance or intensity to the previous frame, and `LightHistory::remap` deliberately keeps a light's history across colour/position changes (its own test `follows_identity_through_sort_animation_and_motion`).
  - The comment's premise, "once the camera is parked, direct-light noise is the only thing changing on a static receiver", is false for this engine. `systems/light_anim.rs` animates `LightSource.intensity` every frame: `flicker_intensity` uses 6 noise buckets per authored period (the 0.5 s default gives 12 Hz steps), and PULSE uses a sine.
  - The parked mode also reads the **bare** camera flag. SVGF's progressive mode was changed under #4046 to AND in `caustic_scene_static` (light rig included), so SVGF indirect drops its long history the frame a light changes, while ReSTIR direct keeps a 64-frame history with a 0.025 floor.
  - The same light change therefore propagates through indirect immediately and through direct over tens of frames.
- **Evidence**:
  - The EMA is a first-order low-pass with gain `α / |1 − (1−α)e^{−iω}|`.
  - A 12 Hz flicker at 60 fps has ω = 2π/5 ≈ 1.257. That gives gain ≈ 0.09 at α = 0.1 (camera moving) and ≈ 0.02 at α = 0.025 (parked). The flicker amplitude reaching direct lighting is about 9 % (moving) and about 2 % (parked) of the authored modulation.
  - A light switched off or dimmed while the camera is parked fades with τ ≈ 1/0.025 = 40 frames.
  - TAA (taa mode) adds α = 0.1 on top, but its 3×3 clamp follows a global brightness change quickly, so the ReSTIR EMA is the dominant attenuator.
- **Impact**: Flickering torches, candles and fires, the signature animated lights of every Bethesda interior, lose most of their flicker in shadowed direct lighting, most visibly when the player stands still. Scripted light toggles smear. Direct and indirect respond to one event on different time constants.
- **Related**: #4046 (SVGF got the scene-static fix), #2516 / #2478 (flicker rate and period plumbing that this filter undoes), REN-D2-2026-09-27-01, `light_history.rs`.
- **Suggested Fix**: Accumulate the **visibility** (the shadow ratio `V̄`, or `frameContribution / unshadowed rad·W`) and multiply by this frame's radiance when shading, so intensity animation passes through while shadow-ray noise is still filtered. Alternatively, reset `histLen` when the selected light's `rpRad` luminance differs from last frame by more than a relative ε. Either way, drive the parked cap from the same scene-static signal SVGF uses.

#### REN-D7-2026-09-27-02: SVGF's "scene unchanged" signal ignores draw-set changes (spawn/despawn/enable/disable of rigid instances), so a parked camera keeps ~1/256 α over GI that changed
- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite
- **Location**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: the draw loop (`let previous_source = … previous_rigid_models.get(&draw_cmd.entity_id).unwrap_or(m)`; `rigid_instance_moved |= previous_source != m;`) and `let caustic_scene_static = !rigid_instance_moved && pose_dirty.is_empty() && caustic_scene_key == self.prev_caustic_scene_key;` → `next_svgf_temporal_alpha(self.svgf_recovery_frames, caustic_history_valid)`.
  - Consumer: `svgf_temporal.comp` (`floorC = params.w > 0.5 ? 0.0 : params.x`).
- **Status**: NEW (#4046 added the light rig and did not cover the instance set).
- **Description**:
  - `scene_static` is built from rigid instances that **moved** (a first-sight instance compares `m` to itself and counts as unmoved), skinned poses, caustic-source placement and the light rig.
  - An instance that **appears** (first sight) or **disappears** (present in `previous_rigid_models`, absent this frame) changes nothing in that signal.
  - With the camera parked, `params.w = 1` removes the 0.2 α floor. `histAge` is capped at 255, so the neighbouring surfaces' GI then converges as an EMA with α ≈ 1/256 (τ ≈ 256 frames).
  - Pixels the object used to cover are fine: their mesh ID changes and history is rejected. The walls, floor and tabletop around it keep the object's colour bleed and contact occlusion in their indirect term for several seconds.
  - Streaming and cell loads are covered separately by `signal_temporal_discontinuity`. Gameplay-driven set changes are not: picking up or looting an item, `disable`/`enable`, corpse cleanup, and the spawning of any rigid object.
  - Emissive or material animation (no transform change) is likewise invisible to the key. This is a lesser case.
- **Evidence**: The instance set is not folded into `caustic_scene_key` anywhere. `current_rigid_models` / `previous_rigid_models` are swapped each frame in `draw.rs` (`std::mem::swap(&mut self.history.previous_rigid_models, &mut current_rigid_models)`), so the set difference is available for free.
- **Impact**: A multi-second GI ghost after the scene changes in front of a stationary player. This is the typical situation for looting, which happens with a parked camera.
- **Related**: #4046, #3995, #2468.
- **Suggested Fix**: In the loop, treat a first-sight rigid instance (`previous_rigid_models.get(..) == None` while `!camera_cut && !suppress_rigid_history`) as `rigid_instance_moved`. After the loop, treat `previous_rigid_models.len() != current_rigid_models.len()` (or any missing key) as a removal. Both are O(1) per draw on data already in hand.

#### REN-D7-2026-09-27-03: Under `--upscaler taa`, water pixels are resolved as the lake bed — its stable mesh ID, normal and motion — with full α = 0.1 history, and no reactive signal reaches TAA
- **Severity**: MEDIUM (opt-in path; floor = denoiser ghosting, a missing disocclusion signal)
- **Dimension**: TAA
- **Location**:
  - `crates/renderer/shaders/taa.comp`: bindings 0–8 have no mask input. The bypass gate is `offscreen || background || disocclusion || surfaceMismatch || alphaBlend`, with `alphaBlend = !meshIdHasStableHistory(currMid)`.
  - `crates/renderer/src/vulkan/water.rs` / `water.frag`: attachments 1–3 are masked off, and only 0/4/6/7 are written.
- **Status**: NEW (I searched "TAA water", "water mesh_id TAA", "TAA reactive mask" and "water ghosting").
- **Description**:
  - Alpha-blended `triangle.frag` draws set `MESH_ID_NO_HISTORY_BIT`, so TAA and SVGF bypass them.
  - Water writes no mesh ID, normal or motion by design (`water.rs` module doc), so the pixel keeps the **bed's** opaque stable ID, normal and motion.
  - TAA therefore accepts history for animated water: moving wave normals, RT reflection and refraction, and foam. It reprojects that history with the **bed's** motion vector, which is wrong by the parallax of the water depth whenever the camera moves.
  - FSR is told about this through the reactive and transparency masks water writes at full strength (#4864 tracks the value). TAA reads neither, although both attachments are still written in taa mode (#2180 / #4203).
- **Evidence**: `taa.rs` `write_descriptor_sets` binds curr HDR, motion, curr/prev mesh ID, prev history, output, params and curr/prev normal, and nothing else. `water.rs`: "attachments 1..=3 (normal, motion, mesh_id) … are masked off … so water never pollutes the G-buffer".
- **Impact**: In taa mode, water reflections and highlights smear and ghost under camera motion, and deep-water pixels reproject from the wrong place. The 3×3 γ = 1.5 clamp is loose on wavy water, so it limits the ghosting only partly. FSR mode (the default) is unaffected.
- **Related**: #4864, #2180, #4203; ground-cover blades have the analogous shape (see Needs validation).
- **Suggested Fix**: Bind the transparency (or reactive) mask attachment into `taa.comp` and fall back to the current sample, or raise α, where the mask exceeds a threshold. This mirrors what FSR does with the same data.

#### REN-D8-2026-09-27-01: `traceArchitecturalWindowGlass`'s candidate loop can never execute. The ray is `gl_RayFlagsOpaqueEXT` over all-OPAQUE geometry, so architectural window glass never establishes a sun portal
- **Severity**: MEDIUM. Visual; the default volumetric path in sealed interiors with glazed windows, in every game. It is partly masked at ray tier ≥ 1 by the rim fallback.
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` `traceArchitecturalWindowGlass` (~l.292–317); only caller is `main`'s sealed-interior sun arm (`bool sawGlass = traceArchitecturalWindowGlass(`, ~l.2989). Pinned by `crates/renderer/src/vulkan/volumetrics.rs` `architectural_glass_portal_checks_every_candidate_layer`.
- **Status**: Regression of #4858 (closed by `ab255cfd2`). That fix made the function unconditionally false, which is worse than the first-hit false-negative it replaced.
- **Description**:
  - #4858 said the terminate-on-first-hit query could commit a clutter pane instead of a farther architectural one.
  - `ab255cfd2` dropped TerminateOnFirstHit and loops `while (rayQueryProceedEXT(rq)) { if (rayQueryGetIntersectionTypeEXT(rq, false) != gl_RayQueryCandidateIntersectionTriangleEXT) continue; … if (layer == RENDER_LAYER_ARCHITECTURE) return true; } return false;`.
  - But the query still passes `gl_RayFlagsOpaqueEXT`, and every BLAS geometry is `vk::GeometryFlagsKHR::OPAQUE` (`blas_static.rs`, `blas_skinned.rs`). No instance sets `FORCE_NO_OPAQUE`: the only instance flag is `TRIANGLE_FACING_CULL_DISABLE` in `tlas.rs`, and no shader uses `gl_RayFlagsNoOpaqueEXT`.
  - Opaque triangle hits are committed automatically during traversal and are never returned as candidates. `rayQueryProceedEXT` returns true only for non-opaque triangle or AABB candidates, so it returns false on the first call and the loop body never runs.
  - The function also never inspects the committed hit, so it always returns false.
  - This is the only candidate-type loop in the shader tree (grep `CandidateIntersection`).
- **Evidence**:
  ```glsl
  rayQueryInitializeEXT(rq, topLevelAS, gl_RayFlagsOpaqueEXT, VISIBILITY_LAYER_GLASS, origin, 0.05, direction, tMax);
  while (rayQueryProceedEXT(rq)) {            // never true: all geometry opaque
      if (rayQueryGetIntersectionTypeEXT(rq, false) != gl_RayQueryCandidateIntersectionTriangleEXT) continue;
      ...
  }
  return false;
  ```
  - The call site feeds `visibility = sawGlass || (fogRayQualityTier > 0u && … && hasArchitectureRimAroundSkyRay(...))`.
  - The primary sun ray uses `VISIBILITY_MASK_ALL_OPAQUE`, so a glass pane does not block it. The glass pass is exactly what should turn "clear ray through a pane" into "lit".
- **Impact**:
  - In sealed interiors (no Show Sky bit), a froxel whose sun ray leaves through a glazed window is lit only if an authored LightShaft/SkyAperture covers it, or if the 4-probe architecture rim fallback happens to find a coherent plane.
  - The rim fallback is shed at ray tier 0 (`fogRayQualityTier > 0u`, #4793). There, every glazed window without an authored aperture is dark.
  - Large glazed openings (a pane wider than the ~272 BU probe diagonal, e.g. cathedral windows or glass walls) are dark at any tier, because all four probes pass through the glass. They use `VISIBILITY_LAYER_ARCHITECTURE` and miss.
  - Where the rim fallback does rescue a window, it costs 4 closest-hit rays per froxel that the single glass query was meant to short-circuit (a #4793 overlap).
  - Nothing catches this: the guard test asserts the loop's text.
- **Related**: #4858 (the fix that introduced this), #4793 (rim-ray cost), `0572bfd5a` (interior godrays), `traceArchitectureDistance` (its sibling uses the same flag correctly, because it only drains and reads the *committed* hit).
- **Suggested Fix**: Pick one:
  - Initialize with `gl_RayFlagsNoOpaqueEXT` so every glass triangle surfaces as a candidate. Return true on the first architecture-layer candidate and never confirm the others.
  - Keep the opaque closest-hit and re-trace from `t + ε` while the committed layer is not architecture, with a small bound.

  Replace the source-shape pin with one that also asserts the non-opaque flag. Confirm by a `VOLUMETRIC_TERM` A/B on a glazed-window interior at tier 0 and tier 2.

#### REN-D10-2026-09-27-01: BGSM translucency (SSS) lobe is added from the unshadowed radiance of every cluster light, including the sun
- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/triangle.frag` cluster light loop. The gate is `sssGate` (~L3291). `vec3 unshadowedRadiance = lightColor * atten;` is at L3310. The `if ((mat.materialFlags & MAT_FLAG_TRANSLUCENCY) != 0u)` block runs from L3336, with `Lo += sssTint * … * unshadowedRadiance` at L3372. Contrast `shadowableLightRadiance` (`include/lighting.glsl` L174), whose `bethesdaBackFactor` term at L338 is inside the shadowed function.
- **Status**: NEW. Pre-existing code (#1147 Phase 2b; the gate driver is #3574), not a delta regression. No open or closed issue covers it; searched "translucency shadow" and "subsurface unshadowed".
- **Description**: The checklist premise is that every direct lobe goes through `shadowableLightRadiance`, so that ReSTIR selection, the legacy-WRS subtraction and TLAS visibility all see it. The Skyrim back-light lobe (`bethesdaBackFactor`) and the rim lobe obey this; their comment says so explicitly ("keeps ReSTIR selection, visibility, and the legacy shadow subtraction byte-consistent"). The FO4-family translucency lobe does not. It is added straight to `Lo` for every light in the fragment's cluster, and no visibility term is applied:
  - under ReSTIR, the needs-visibility light's normal diffuse/specular term is withheld from `Lo` and re-enters shadowed via the reservoir, but the SSS term was already added unshadowed;
  - under the legacy estimator, pass 2 subtracts only `shadowableLightRadiance`, so the SSS term is never occluded either.

  The directional light is a member of every cluster (`cluster_cull.comp` sets `intersects = true` for `lightType > 1.5`, L259), so it takes this path too.
- **Evidence**:
  ```glsl
  vec3 unshadowedRadiance = lightColor * atten;           // L3310
  vec3 shadowableRadiance = shadowableLightRadiance(...); // shadowed path
  if (!useRestir || !needsVisibility) { Lo += shadowableRadiance; }
  ...
  if ((mat.materialFlags & MAT_FLAG_TRANSLUCENCY) != 0u) {
      ...
      Lo += sssTint * mat.translucencyTransmissiveScale
          * thicknessShape * turbMod * unshadowedRadiance; // no visibility
  }
  ```
- **Impact**: This is a light leak on `bgsm.translucency` content (`forward_bgsm_phase1_flags`; the shader comment dates the field to BGSM v≥8). Examples:
  - back-lit leaves or fabric sitting in a building's or terrain's shadow still get the full sun transmission;
  - translucent surfaces behind a wall get SSS from a lamp in the next room whose range crosses the wall.

  The result is also inconsistent across games. Skyrim's back-light lobe is shadowed and FO4/FO76's translucency lobe is not, which breaks the one-composition-convention invariant. The effect is visual only, and it is whole-surface on affected materials.
- **Related**: REN-D2-2026-09-27-01 (ReSTIR candidate weight) and REN-D7-2026-09-27-01 (EMA). Neither covers this term.
- **Suggested Fix**: Move the translucency lobe into `shadowableLightRadiance`, next to `bethesdaBackFactor`, so that ReSTIR pHat, the finalize visibility and the legacy subtraction all include it. `sssGate` then stays as it is (it is only the early-out). Add a `shader_contract` pin that `MAT_FLAG_TRANSLUCENCY`'s lobe lives inside that function. A before/after capture on an FO76/translucent-foliage scene is needed (see Needs validation).

#### REN-D11-2026-09-27-01: a persisted `render.upscaler` silently replaces the CLI default; the boot log line, and the determinism harness that scrapes it, report the pre-override value
- **Severity**: MEDIUM. There is no GPU fault, but it defeats the one signal every harness and audit uses to know which path of the CRITICAL-floor default was exercised.
- **Dimension**: FSR/Presentation
- **Location**:
  - `byroredux/src/boot/mod.rs`: `log::info!("Renderer upscaler selection: {}", renderer_config.upscaler)` immediately after `parse_renderer_config`.
  - `byroredux/src/main.rs` `install_universal_settings`: the `else if let Some(SettingValue::Choice(spec))` arm, `Ok(mode) => renderer_config.upscaler = mode`, which logs nothing.
  - `scripts/check-bench-determinism.sh` `run_once`: `selected_upscaler="$(sed -n 's/.*Renderer upscaler selection: //p' …)"`.
  - `byroredux/src/app_events.rs`: the `bench:` summary `println!`, which has no upscaler/extent token.
- **Status**: NEW. The same state was observed in `AUDIT_PERFORMANCE_2026-09-26.md` process notes ("Baseline selected the saved `render.upscaler = "fsr3/native-aa"`…") but never filed; no GitHub issue matches.
- **Description**:
  - `boot::run` parses the CLI and logs the selection at line ~337. `App::new` → `install_universal_settings` then loads `settings.toml` and, when `explicit_upscaler` is false, overwrites `renderer_config.upscaler` with the persisted choice, with no log.
  - The only trace of the real mode is the renderer's later `Frame extents: … (fsr3/native-aa)` / `Frame upscaler:` INFO lines, which contradict the boot line.
  - Precedence itself is intended (README §Player controls; `docs/engine/launcher.md` "Settings applied pre-device"). The defect is that the override is invisible and the log states something false.
- **Evidence**:
  - The live-run log shows `Renderer upscaler selection: fsr3/quality`, then `Frame extents: render=1280x720, output=1280x720 (fsr3/native-aa)`.
  - `~/.config/byroredux/settings.toml` contains `"render.upscaler" = "fsr3/native-aa"`, and `BYROREDUX_SETTINGS_PATH` is unset.
  - `FrameExtentSet::for_output` has no floor that could turn Quality into 1:1. The extents line prints `renderer_config.upscaler`, which confirms that the config value itself was native-aa.
  - None of these pass `--upscaler`, a settings path, or `BYROREDUX_SETTINGS_PATH`, so they all inherit the persisted value:
    - `scripts/check-bench-determinism.sh`
    - `scripts/renderer-eval.sh`, `renderer-eval-fnv.sh`
    - `scripts/bench-variability-envelope.sh`
    - `.claude/commands/audit-runtime/capture.sh` (`--game … --bench-frames … --bench-hold`)
    - most `docs/smoke-tests/*.sh`
  - `check-bench-determinism.sh` then stamps its per-run JSON with the pre-override `selected_upscaler`, next to render/output extents that disagree with it.
- **Impact**:
  - On any machine where someone once chose an upscaler in the pause menu, "no flag" no longer means FSR Quality.
  - Runtime-audit baselines, determinism manifests, eval captures and audit validation runs silently exercise a different reconstruction path: here native-aa, a render == output extent and a different jitter phase count / froxel grid / BLAS reservation.
  - Their logs claim Quality. Today's validation evidence for the "FSR default" path is affected.
- **Related**:
  - `.claude/commands/audit-fnv/SKILL.md`'s "the flag defaults to `fsr3`" premise.
  - `AUDIT_PERFORMANCE_2026-09-26.md` line ~114.
  - REN-D11-2026-09-27-02 (how such a value gets persisted).
- **Suggested Fix**:
  - Log the effective selection once, after `install_universal_settings`, naming its source (`cli` / `persisted settings.toml` / `default`). Move or re-emit the boot line so it is never pre-override.
  - Add an `upscaler=`/`render=`/`output=` token to the `bench:` line.
  - Have `check-bench-determinism.sh` and `capture.sh` either pass an explicit upscaler or point `BYROREDUX_SETTINGS_PATH` at a scratch file.


### LOW
#### REN-D1-2026-09-27-01: `5eb07a4f3`'s TLAS refit-identity rule (EntityId tie-break + membership-replacement → BUILD) has no test; the one updated test makes the tie-break vacuous
- **Severity**: LOW (test gap; the code traces correct)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas`: the `if use_update && !tlas.last_entity_ids.iter().copied().eq(…)` block and the post-record `last_entity_ids` refresh); `crates/renderer/src/vulkan/acceleration/predicates.rs` (`sort_tlas_instances_by_blas_address`, `.then_with(|| entity_ids_by_ssbo[… low_24() …])`); `crates/renderer/src/vulkan/acceleration/tests/predicates_tests.rs` (`tlas_instance_sort_key_is_independent_of_draw_order`)
- **Status**: NEW
- **Description**: `5eb07a4f3` added two behaviours:
  - The TLAS canonical order is now `(BLAS address, full EntityId)`, with the EntityId found via `tlas_entity_ids_scratch[instance_custom_index]`.
  - A same-address, same-count frame whose entity membership differs is forced from UPDATE to BUILD.

  Neither behaviour is exercised by any test. The only test touched was `tlas_instance_sort_key_is_independent_of_draw_order`, now called as `sort_tlas_instances_by_blas_address(&mut instances, &[0])`. Every instance in it has `instance_custom_index = 0` and a distinct address, so the tie-break is never reached. The membership rule is inline in `build_tlas`, so without a device it cannot be tested at all. `decide_use_update`, the pure decision helper the module doc names as the home of the BUILD-vs-UPDATE decision, still sees only `needs_full_rebuild`, the map generation and the address slices. A future edit could drop the entity check, break the tie-break, or refresh `last_entity_ids` on the UPDATE arm, and every test would stay green. The result would be the progressive refit-quality loss the commit exists to fix: legal, invisible, and a silent performance regression.
- **Evidence**: `grep -rn "last_entity_ids\|tlas_entity_ids_scratch" crates/renderer/src` finds no test reference. `predicates_tests.rs` has `&[0]` as the only entity table.
- **Impact**: Regression exposure only. Correctness today: I traced the tie-break index (always `< draw_commands.len()`, because the map compacts to ≤ i and #4833 caps it), the refresh happening only on BUILD, and the rollback invalidation (`invalidate_tlas_recording`, reached from all three `draw_frame` tail `Err` sites via `rollback_skin_frame_state`). All hold.
- **Related**: #3666 (the canonical sort this extends); #3991 / #2674 (record-time commit discipline).
- **Suggested Fix**: Fold the entity-sequence comparison into `decide_use_update` (pass `last_entity_ids` plus the current canonical ID sequence). Add tests for: equal-address instances ordered by entity ID regardless of input order; the same address multiset with a swapped entity forcing BUILD; an unchanged membership with permuted SSBO indices still selecting UPDATE.

#### REN-D1-2026-09-27-02: `refit_skinned_blas` says its index buffer is "the `MeshRegistry`'s global SSBO"; it is the per-mesh index buffer
- **Severity**: LOW (comment built on a false premise)
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` (`refit_skinned_blas`, the #3469 note above the `index_address` query); `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (dispatch collection: `mesh.index_buffer.as_ref().expect("skinned mesh requires a per-mesh index buffer")`, gated on `mesh.rt_capable`)
- **Status**: NEW (the comment was introduced by `dd7986793`, #3469)
- **Description**: The comment explains why the index device address is re-queried every refit instead of cached: "the index buffer is the `MeshRegistry`'s global SSBO … a stale-address GPU fault if any realloc site forgets to refresh". Every skinned BUILD/refit source is the mesh's dedicated index buffer. `rt_capable` meshes always own one, and global-only meshes are never `rt_capable` (`global_only_meshes_are_never_rt_capable`). That buffer is fixed for the mesh's lifetime and is never reallocated by the geometry-SSBO rebuild or compaction. The stated hazard therefore does not exist.
- **Evidence**: Call path `record_skinned_blas_refit` → `dispatches.push((…, mesh.index_buffer…buffer, …))` → `SkinnedBlasGeometry { index_buffer: idx_buffer, … }` → `refit_skinned_blas`.
- **Impact**: None at runtime. It misleads auditors, for example into believing compaction (`7e9da5dcc`) or the chunked rebuild can invalidate skinned BLAS inputs. The caching decision it justifies is harmless either way.
- **Related**: #3469; Dim 9 (skinning).
- **Suggested Fix**: Reword it to say the source is the mesh's dedicated index buffer, stable for the mesh's lifetime, and that the query is kept only because the scratch buffer is reallocated from three sites. Optionally cache the index address on the entry.

#### REN-D2-2026-09-27-02: The interior window portal samples the sky cube along the pane normal, so each flat pane shows one sky texel (including the sun disc when the normal faces the sun)
- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag` (fn `main`, window-portal block `vec3 skyColor = exteriorSkyRadianceOr(throughDir, exteriorSkyTint.rgb);`); pinned by `window_portal_orients_gate_ray_and_sky_sample_from_the_viewer` (`scene_buffer/shader_contract_tests.rs`)
- **Status**: NEW. `0572bfd5a` introduced the cube sample, and `3c9d44f20` (#4832) re-oriented it to `-N_bias`.
- **Description**: The portal's *occlusion* ray is deliberately fired along the pane's normal axis. That is #421: `-V` hits interior side walls at oblique angles. `0572bfd5a` then used that same direction for the *radiance* lookup. The rationale in its comment is "so a window sees the actual horizon, sun and clouds instead of one zenith swatch". For a flat pane, `throughDir = -N_bias` is constant (or near-constant under a normal map), so every pixel of the pane returns the same cube texel at LOD 0. The result is still one swatch per pane, now the horizon azimuth of the pane normal instead of the zenith, and it does not change with viewing angle. When the pane normal lies inside the baked sun disc or glare lobe (a west window at sunset), the whole pane reads the sun-disc radiance from every viewpoint. Seen through clear thin glass, the sky lies along the camera ray `-V`; thin glass does not bend it.
- **Evidence**: `vec3 throughDir = -N_bias;` feeds both `rayQueryInitializeEXT(windowRQ, …, throughDir, …)` and `exteriorSkyRadianceOr(throughDir, …)`. The test's required-strings list includes `"exteriorSkyRadianceOr(throughDir, exteriorSkyTint.rgb)"`, so the per-pane-constant behaviour is now pinned.
- **Impact**: Visual only. Interior windows show a flat, view-independent colour, and a pane blows out when its normal faces the sun. #4832's own note says the live sky effect is unverified, because the FNV panes tried do not discriminate.
- **Related**: #421, #3323, #4832, #925.
- **Suggested Fix**: Keep `throughDir` for the escape ray but sample `exteriorSkyRadianceOr(-V, …)` for the transmitted radiance, and update the #4832 pin accordingly. Confirm on a Skyrim or Vault 21 window capture.

#### REN-D2-2026-09-27-03: `interleavedGradientNoise` loses its spatial and temporal resolution over a session (float product `frameCount * 5.588238`)
- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/math_common.glsl` (fn `interleavedGradientNoise`); live caller in the default build: `triangle.frag` glass roughness-gated refraction scatter (`rn1`/`rn2`, seeded `cameraPos.w + 37.0` / `+ 53.0`). Compiled-out callers are in the `ENABLE_LEGACY_WRS` arm (candidate `u`, shadow disk `noise1`/`noise2`).
- **Status**: NEW. #1161 (closed) fixed only the u32→f32 cast (`frame_counter & 0xFFFFFF`), not the arithmetic. The 2026-09-23 report flagged this "same float-product shape" as out of its scope.
- **Description**: `fract(dot(fragCoord + frameCount * vec2(5.588238), magic.xy))` is evaluated in f32 with `frameCount` up to 2^24. f32 emulation over a 256×256 pixel block, at 60 fps:

  | Frames (60 fps) | Distinct outputs | Vertically adjacent pixels equal | Distinct over 64 consecutive frames (one pixel) | Mean |
  |---|---|---|---|---|
  | 100 k (~0.5 h) | 256 | — | — | — |
  | 1 M (~4.6 h) | 32 | 81 % | 32 | — |
  | 3 M (~14 h) | 8 | 95 % (52 % horizontal) | 8 | 0.43 (biased) |

  Higher frame rates reach each row proportionally sooner.
- **Evidence**: A numpy float32 mirror of the exact expression is in the audit notes (no repo files written).
- **Impact**: In long sessions, rough-glass refraction scatter turns into structured, banded and slightly biased noise that TAA/FSR cannot average away. The default ReSTIR, shadow jitter and GI paths are unaffected, since they already use the exact integer `hash2_pixel_frame`.
- **Related**: #4776 (same class, fixed for the froxel jitter), #1161.
- **Suggested Fix**: Seed the glass scatter from `hash2_pixel_frame(uvec2(gl_FragCoord.xy), uint(cameraPos.w) …)` like the other stochastic paths, or wrap the IGN frame term to `mod(frameCount, 64.0)` before the multiply.

#### REN-D2-2026-09-27-04: `exteriorSkyRadianceOr` gates only on the bake-ready flag, but since `0572bfd5a` the interior cube holds the outdoor sky; the #2226 interior gate lives unpinned at each call site
- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/include/bindings.glsl` (fn `exteriorSkyRadianceOr` versus `exteriorSkyDiffuseOr`); producer `byroredux/src/render/sky.rs` (fn `build_sky_params`, interior arm sets `portal_outdoor_sky: Some(outdoor.into())`) → `crates/renderer/src/vulkan/context/draw.rs` (fn `build_sky_cube_params`)
- **Status**: NEW (hardening / test gap)
- **Description**: In every interior, `build_sky_params` populates `portal_outdoor_sky`, falling back to the procedural outdoor sky on a direct `--cell` boot, and `build_sky_cube_params` bakes the cube from it. So `exteriorSkyTint.w > 0.5` inside interiors, and `exteriorSkyRadianceOr` returns *outdoor* radiance there. The diffuse twin self-gates (`if (jitter.w <= 0.5 || exteriorSkyTint.w <= 0.5) return fallback;`); the radiance helper does not. Today every non-portal caller adds `jitter.w > 0.5` itself, so there is no leak:
  - `raytrace.glsl` `_isExt`
  - `lighting.glsl` `pathEnvironmentRadiance`
  - `triangle.frag` `isExteriorGlass` ×2, `ambientFallback`
  - `water.frag` `reflectionMiss`

  But `every_sky_cube_consumer_gates_on_the_ready_flag` only counts helper calls. A new escape site that follows the helper's name ("exterior") and its doc ("the baked cube when the bake is live") would reintroduce #2226 in interiors with every guard green.
- **Evidence**: See the call sites listed above; the helper body is `if (exteriorSkyTint.w > 0.5) { … textureLod(skyCube, …) }`.
- **Impact**: None today; this is a latent interior sky-leak trap.
- **Related**: #2226, #1199, #4292, #3323.
- **Suggested Fix**: Add a region-bounded source pin requiring `jitter.w` (or an explicit portal allowlist) around every `exteriorSkyRadianceOr(` call. Alternatively, split out a `portalSkyRadianceOr` for the one intentional interior consumer and make `exteriorSkyRadianceOr` self-gate on `jitter.w`.

#### REN-D3-2026-09-27-01: `memory-budget.md` Light SSBO row still bills 64 B lights and a 16-byte header after the 80 B / 4112 B change
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `docs/engine/memory-budget.md:94` (Scene-Buffers table, "Light SSBO" row); `docs/engine/shader-pipeline.md:606` and `:663` (Set 1 binding 0 and volumetrics binding 3 rows: "`u32 count` + `GpuLight[]`")
- **Status**: NEW
- **Description**: `186234944` grew `GpuLight` to 80 B and added a 4112-byte header (`LightHeader`: count, 3 pads and `previous_to_current: [u32; MAX_LIGHTS + 1]`). `buffers.rs` sizes each slot as `size_of::<LightHeader>() + size_of::<GpuLight>() * MAX_LIGHTS`, which is 85,952 B. The memory-budget row still says an entry size of 64 B, 64 KB per frame and **128 KB** total. The real total is about 86 KB per frame and 172 KB for 2 FIF.
  - `shader-pipeline.md`'s dedicated `GpuLight` section was updated correctly (80 B, header described).
  - The two descriptor-table rows still describe the buffer as `u32 count` + `GpuLight[]`, which omits the 4 KiB remap array between them.
  - The Set 1 binding 0 row's "Used by: triangle, cluster_cull" also omits `caustic_splat`, `volumetrics_inject` and `water.frag`, which all read it.
  - The code is right; the docs are wrong.
- **Evidence**: `constants.rs` doc says "1023 lights × 80 bytes plus the 4112-byte remap header is about 84 KiB". `memory-budget.md:94` says `| 1023 | 64 B | 64 KB | **128 KB** |`.
- **Impact**: The budget ledger is under-billed by about 44 KB. Anyone laying out the light buffer from the binding table would place `lights[]` at offset 16 instead of 4112.
- **Related**: #3565 (the previous Light-SSBO row drift), #4872 (other ledger drift, open).
- **Suggested Fix**: Update the row to 80 B plus a 4112 B header, 86 KB per frame and 172 KB total. Say "count + 1024-entry previous→current remap + `GpuLight[]`" in both descriptor tables. Consider extending the `gpu_material_size_claims` scanner pattern to `GpuLight`.

#### REN-D3-2026-09-27-02: `0e0d35b96` made `GpuCamera`'s `NoUninit` SAFETY comment false — it deleted `[u32; 4]` while `render_debug: [u32; 4]` is a live field
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:648-651` (`unsafe impl NoUninit for GpuCamera`); the field is at `gpu_types.rs:573` (`pub render_debug: [u32; 4]`)
- **Status**: NEW (collateral of the #4870 sweep; the correct text was already in place before `0e0d35b96`)
- **Description**: #4870 item 3 asked for `CompositeParams`' SAFETY text to name its `[u32; 4]` lane, and `0e0d35b96` did fix that. The same commit also rewrote `GpuCamera`'s SAFETY in the opposite direction. The old text was "every field is `[f32; 4]`, `[u32; 4]`, or `[[f32; 4]; 4]`"; the new text is "every field is `[f32; 4]` or `[[f32; 4]; 4]`".
  - `GpuCamera` has carried `render_debug: [u32; 4]` since `8e7582ed4` (2026-08-16).
  - The no-padding conclusion still holds, because a `[u32; 4]` is also a 16-byte lane.
  - The stated premise of an `unsafe impl` is now false, which is what #3761/#3990 exist to prevent.
- **Evidence**: `git show 0e0d35b96 -- crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` shows the `-// SAFETY: every field is [f32; 4], [u32; 4], or …` / `+// SAFETY: every field is [f32; 4] or …` hunk.
- **Impact**: No runtime effect. A future reviewer who checks the premise will find it false, and the comment no longer describes the struct it vouches for.
- **Related**: #4870 (open; items 1, 3 and 6 are now fixed and item 4 remains), #3761, #3990.
- **Suggested Fix**: Restore the `[u32; 4]` mention, naming `render_debug`. Fold this into #4870's remaining work.

#### REN-D3-2026-09-27-03: `upload_lights` now runs a std SipHash `HashMap` and a `DefaultHasher` over a 4 KiB remap every frame (hot-path hashing rule)
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (upload path) / performance
- **Location**: `crates/renderer/src/vulkan/scene_buffer/light_history.rs:4,14` (`use std::collections::HashMap`; `scratch: HashMap<Identity, (u32, u32, u32, u32)>`); `crates/renderer/src/vulkan/scene_buffer/upload.rs:121-126` (`SceneBuffers::upload_lights`: `std::collections::hash_map::DefaultHasher` over `hash_light_slice(..)` plus `previous_to_current`)
- **Status**: NEW
- **Description**: `_audit-common.md`'s hot-path rule (#2923) requires the per-frame render path to use `FxHashMap`/`FxHashSet` end to end. `LightHistory::remap` runs every frame from `upload_lights`. It clears and refills a std `HashMap` keyed by `[u32; 4]`, with up to 1023 inserts and 1023 lookups at SipHash-1-3 cost.
  - The dirty-gate hash was previously a single `FxHasher::write` in `hash_light_slice`. It now wraps that in a SipHash `DefaultHasher` and feeds it the whole `[u32; 1024]` remap array (4 KiB) through `Hash`.
  - The remap array is also returned by value (4 KiB copy) and copied again into `LightHeader`.
- **Evidence**: `light_history.rs` imports `std::collections::HashMap`, while `descriptors.rs` `hash_light_slice` uses `rustc_hash::FxHasher`. `upload.rs:123` shows `let mut hasher = std::collections::hash_map::DefaultHasher::new();`.
- **Impact**: This is a small, bounded CPU cost per frame, not a correctness problem. It is the exact regression class the rule names, on a path that runs every frame.
- **Related**: #2923, #2036 (the light dirty-gate).
- **Suggested Fix**: Use `rustc_hash::FxHashMap` for `scratch`. Fold the remap into the gate with `FxHasher::write(bytemuck-style byte view of previous_to_current)` rather than `DefaultHasher` + `Hash`.
- **Also reported by**: Dim 4 (as REN-D4-2026-09-27-03, owner note: `/audit-performance`) and Dim 5 (as REN-D3-2026-09-27-03). Merged here; orchestrator confirmed `use std::collections::HashMap` in `scene_buffer/light_history.rs` and `std::collections::hash_map::DefaultHasher::new()` in `upload_lights` (`scene_buffer/upload.rs`), both from `186234944`.

#### REN-D3-2026-09-27-04: #4846's generated `RENDER_LAYER_ARCHITECTURE` / `FOG_VOLUME_SHAPE_*` reached only the named sites — sibling discriminant literals remain in GLSL and in the host mirror
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

#### REN-D3-2026-09-27-05: `GcDrawIndirect` is classified `ShaderLocal`, but the host sizes and strides its buffer from `sizeof(VkDrawIndirectCommand)` — by the table's own rule it is a counterpart
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (mirror classification)
- **Location**:
  - `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:1895` (`("GcDrawIndirect", ShaderLocal)` in `every_shader_struct_is_classified`)
  - `crates/renderer/src/vulkan/groundcover.rs:713` (`indirect: GROUNDCOVER_MAX_CHUNKS as u64 * 16 * GROUNDCOVER_INDIRECT_STREAMS`)
  - `groundcover.rs:1817` (`cmd_draw_indirect`, stride 16)
  - GLSL `groundcover_scatter.comp:67` (`struct GcDrawIndirect`)
- **Status**: NEW
- **Description**: `MirrorClass::ShaderLocal`'s doc (#4849) says that a byte-stride constant the host sizes a buffer from IS a counterpart. Such a struct must be `Guarded` by a test that pins its std430 size to that constant. Its sibling `GcDrawIndexed` was moved to `Guarded`, pinned to `size_of::<vk::DrawIndexedIndirectCommand>()`.
  - `GcDrawIndirect` is in the identical position: the scatter writes it and `vkCmdDrawIndirect` consumes it at a literal stride of 16. It stayed `ShaderLocal`.
  - `indirect_stride_matches_the_command` only asserts `size_of::<vk::DrawIndirectCommand>() == 16` (the ash side). No test reflects or parses the GLSL struct.
  - A misplaced doc comment makes this harder to spot. `groundcover.rs:2161` says "`cmd_draw_indirect` is issued with a hard-coded stride of 16…", but it sits on `push_block_fits_the_guaranteed_minimum`, not on the stride test.
- **Evidence**: See the locations. `every_shader_struct_is_classified`'s `ShaderLocal` check looks only for a Rust struct named `GcDrawIndirect` or `GpuGcDrawIndirect`, so the ash counterpart is invisible to it.
- **Impact**: The risk is low: the Vulkan struct is fixed by spec and the GLSL is four `uint`s. Still, the classification vouches for something false, and a field added to the GLSL struct would desynchronise the per-chunk stride with no failing test.
- **Related**: #4849 (closed), `GcDrawIndexed`'s guard.
- **Suggested Fix**: Reclassify it as `Guarded("name_diverging_glsl_rust_mirrors_stay_in_lockstep")`, with a stride row pinning the GLSL std430 size to `size_of::<vk::DrawIndirectCommand>()`. Replace the literal `16` with that `size_of`, and move the misplaced doc comment onto `indirect_stride_matches_the_command`.

#### REN-D3-2026-09-27-06: `GcCameraUBO` (`groundcover_blade.vert`) is a name-diverging prefix mirror of `GpuCamera` pinned by block size only, outside both discovery legs
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (mirror coverage)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert:210` (`layout(set = 1, binding = 1) uniform GcCameraUBO { mat4 gcViewProj; … vec4 gcJitter; }`); guard `crates/renderer/src/vulkan/groundcover.rs:2237` (`blades_project_with_the_jittered_camera_ubo`)
- **Status**: NEW
- **Description**: The blade vertex shader binds the scene camera UBO (set 1 binding 1) under a different block name, declaring the first 8 `GpuCamera` members (through `jitter`) with `gc`-prefixed names.
  - The only pin is `uniform_block_size_by_name(spv, "GcCameraUBO") == offset_of!(GpuCamera, jitter) + 16`, a size check.
  - `camera_ubo_glsl_copies_stay_in_lockstep`'s discovery leg matches only `uniform CameraUBO {`, and `every_shader_struct_is_classified` walks only `struct` lines. Neither can see this block.
  - A transposition among the same-typed `mat4` lanes (`gcViewProj`/`gcPrevViewProj`/`gcInvViewProj`) or `vec4` lanes would keep the size and pass. This is the same size-only class as #4778 (`VolumetricsParams`) and #3684 (`CameraUBO` before its order test).
- **Evidence**: `grep -rn 'uniform .*Camera' crates/renderer/shaders` shows 5 `CameraUBO` blocks plus `GcCameraUBO`. The test source lists the 5 `CameraUBO` files only.
- **Impact**: A field reorder in `GpuCamera`'s first 8 lanes, or a GLSL-side edit, would silently project ground-cover blades with the wrong matrix (for example the previous-frame VP, which gives wrong motion vectors and TAA smear) with every guard green.
- **Related**: #3684, #4028, #4778, #4296.
- **Suggested Fix**: Add `GcCameraUBO` to the camera lockstep test as a prefix mirror. Compare member names, allowing `gc` prefixes through `NameAlias`, plus order and type against the first 8 `GpuCamera` fields. Make the discovery leg match any `uniform …` at `set = 1, binding = 1`.

#### REN-D4-2026-09-27-01: `shader-pipeline.md`'s submission order omits the two new top-of-frame recordings and the early-test pipeline
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `docs/engine/shader-pipeline.md` ("Per-Frame Submission Order", "G-Buffer Layout"). Code: `crates/renderer/src/vulkan/context/begin_frame_recording.rs` (`begin_frame_recording`), `crates/renderer/src/texture_registry/dynamic_rgba.rs` (`TextureRegistry::record_pending_rgba_uploads`), `crates/renderer/src/vulkan/restir.rs` (`ReservoirBuffers::begin_frame`), `crates/renderer/src/vulkan/context/geometry_pass.rs` (`PipelineKey::Opaque { early_tests: true, .. } => self.pipeline_early`).
- **Status**: NEW (window commits `e2f99ad55` and `186234944`, both after the 09-24 audit). Related: open #4871 (earlier frame-order drift). #4871's groundcover_models step and #4602 flush-edge items have since landed as rows 5d and 22b.
- **Description**: `begin_frame_recording` now records two command sequences before step 2 (skin palette), and the doc names neither:
  1. **Dynamic-RGBA copies.** Per dirty texture: `ALL_COMMANDS → TRANSFER` with `SHADER_READ_ONLY → TRANSFER_DST` (src access `MEMORY_READ|MEMORY_WRITE`), then `cmd_copy_buffer_to_image`, then `TRANSFER → ALL_COMMANDS` back to `SHADER_READ_ONLY`. These are the HUD, Scaleform and ground-cover atlas updates that used to be one-shot submits (#3429).
  2. **Reservoir history clear.** `FRAGMENT|TRANSFER → TRANSFER|FRAGMENT` buffer barriers on the curr/prev reservoirs, a `cmd_fill_buffer` of the current slot, then `TRANSFER → FRAGMENT`.

  There are three smaller gaps in the same doc:
  - Step 6 lists `triangle.vert / .frag` only. Certified opaque draws now run `triangle_early.frag.spv` (`layout(early_fragment_tests)`) through `pipeline_early`, selected by `DrawCommand::allows_early_fragment_tests`.
  - Step 5d does not state the model tier's barrier (`COMPUTE → DRAW_INDIRECT|VERTEX|FRAGMENT|TRANSFER`), nor that it also writes the previous-model buffer's tail.
  - The G-buffer section says the pass is "Written by … (`triangle.frag` + `water.frag`)". It omits `groundcover_blade.frag`, which writes attachments 0/2/5/6/7 and masks off 1/3/4.
- **Evidence**:
  - `begin_frame_recording`: `self.texture_registry.record_pending_rgba_uploads(…)` then `self.reservoir_buffers.begin_frame(&self.device, cmd, frame);`.
  - `grep -n "record_pending_rgba_uploads\|begin_frame\|pipeline_early\|early_fragment" docs/engine/shader-pipeline.md` returns nothing.
- **Impact**: This doc is the authoritative barrier inventory that audits and sync reviews diff against. Two cross-submission image and buffer hazards handled at the top of the frame are invisible to it. This is the fourth consecutive audit where the doc did not move with a new recorded pass.
- **Related**: #4871, #4586, #4525.
- **Suggested Fix**: Add rows 1b (RGBA copies) and 1c (reservoir clear) with their barrier masks, name `pipeline_early` in step 6, and extend 5d and the G-buffer writer list. Widen `shader_pipeline_documents_every_record_pass_helper` beyond `post_passes.rs`, for example to every `record_*` / `begin_frame` recorder called from `draw_frame` and its phase files.

#### REN-D4-2026-09-27-02: The second instance-SSBO grow's safety comment says "before anything is recorded for `frame`"; since #4833 that is false, and the real invariant is unpinned
- **Severity**: LOW (latent; safe today)
- **Dimension**: Sync/Barriers
- **Location**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: the `#4199` comment above `self.grow_instance_ssbos(frame, gpu_instances.len() + model_tail)`.
  - `crates/renderer/src/vulkan/scene_buffer/upload.rs`: `SceneBuffers::ensure_instance_capacity`, which calls `update_descriptor_sets` on scene set bindings 4/18 under the SAFETY claim "the only command buffer that bound `set` has completed".
- **Status**: NEW
- **Description**: `1e6485313` (#4833) put a first grow in `begin_frame_recording`. The frame then records the RGBA copies, the reservoir clear, and all of `dispatch_skin_and_cluster` (skin, refit, AS barrier, TLAS build, cluster cull, ground-cover interaction and scatter) before `build_and_upload_instances` reaches the second grow. That grow can replace `instance_buffers[frame]` and rewrite `scene set[frame]` bindings 4/18 in mid-recording.

  This is legal only because nothing recorded before it binds `scene_buffers.descriptor_set(frame)` or names `instance_buffers()[frame]`. I verified that:
  - skin, cluster cull, scatter and interaction bind their own sets;
  - the sky bake binds the UPDATE_AFTER_BIND bindless set, and after the grow in any case;
  - the model tier reads the buffer after the grow.

  The call-site comment still claims the window is "before anything is recorded for `frame`", and no test pins the real invariant. The existing guards pin only that grow precedes upload and that the caustic set is rebound.
- **Evidence**:
  - Current comment: "This is after `sync_and_acquire_frame`'s fence wait and before anything is recorded for `frame`, which is the window the grow needs".
  - Recording order: `draw_frame` → `begin_frame_recording` (records) → `dispatch_skin_and_cluster` (records) → `build_and_upload_instances` (grow at the `model_tail` line).
- **Impact**: Suppose a future pre-upload pass binds scene set 1 (any compute pass that wants `instances[]` or the light SSBO through it). On the frames where the grow fires, that pass is invalidated by the descriptor update, since the set has no UPDATE_AFTER_BIND and the command buffer becomes invalid. Those frames are the rare exterior cases: the model-tier tail, or instance-count growth during streaming. The failure would be intermittent, exterior-only, and outside every current validation route.
- **Related**: #4199, #4833, #4413.
- **Suggested Fix**: Correct the comment to state the real invariant. Add a source-order pin that no `descriptor_set(frame)` or `instance_buffers()` use in `begin_frame_recording.rs` / `dispatch_skin_and_cluster.rs` precedes the second grow. Alternatively, fold the tail request into the first grow so the second one is a no-op in steady state.

#### REN-D5-2026-09-27-01: Nothing pins that the in-place compaction actually runs; its fallback is silent, so a layout drift would quietly bring back the 150–210 ms stall
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh/geometry_ssbo.rs` (`MeshRegistry::plan_geometry_compaction`, the `in_place` predicate and the `else` allocating branch; tests `in_place_compaction_moves_each_survivor_with_its_bytes`, `out_of_order_layout_falls_back_to_the_allocating_copy`)
- **Status**: NEW
- **Description**: `7e9da5dcc` keeps the allocating copy as a fallback for any layout that is not ascending and disjoint in slot order. The commit says the two branches produce identical pools and offsets, so a byte-for-byte test cannot tell them apart.
  - `in_place_compaction_moves_each_survivor_with_its_bytes` asserts payloads and lengths only. It passes unchanged if `in_place` is hard-wired to `false`.
  - The `else` branch emits no log or counter.
  - The invariant it depends on holds today: `upload_scene_meshes_batched` and `accumulate_global_geometry` append geometry and push slots in lockstep. A future upload path that reserves slots out of order would silently reinstate the ~400 MiB double allocation and its page-fault stall, and nothing would fail.
- **Evidence**: The #2678 test `repeat_compaction_without_a_new_drop_does_not_recopy` already pins pointer stability for the no-hole case (`reg.pending_vertices.as_ptr()`). The new in-place test takes no such snapshot across a real compaction.
- **Impact**: Perf regression with no signal. No correctness impact.
- **Related**: #2678 (the same pointer-stability technique), `7e9da5dcc`.
- **Suggested Fix**: In the in-place test, snapshot `pending_vertices.as_ptr()` / `pending_indices.as_ptr()` before the plan and assert they are unchanged after it. Add a once-per-session `log::warn!` (or a scratch-telemetry counter) on the allocating branch.

#### REN-D5-2026-09-27-02: memory-budget.md's VRAM rough-budget row still caps the rebuild transient at "+2× projected, ≤ ~512 MB"; the "Vertex / index pools" peak says 1.66 GB against a 480 MB cap
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md`, the "VRAM Rough Budget" table rows "Global geometry SSBO rebuild (#3298)" and "Vertex / index pools"
- **Status**: NEW (the first half comes from `5226d73e2`; the second is older drift in the same table)
- **Description**:
  - `5226d73e2` rewrote the `### Global geometry SSBO rebuild` section. It now says that with a budget reading "the doubling can reach 2× the `VERTEX_POOL_HARD_CAP` + `INDEX_POOL_HARD_CAP` figures". The summary table was not updated.
  - With `VK_EXT_memory_budget` (every desktop driver) the extra resident generation is up to one projected copy. That is at most 416 MB + 64 MB = 480 MB, so the two generations together reach about 960 MB. It is not "+2× projected, ≤ ~512 MB".
  - The same table gives the pools' peak as "~1.66 GB cap". The Mesh Registry rows put the caps at ~416 MB (4 M × 104 B) and ~64 MB (16 M × 4 B).
- **Impact**: Anyone doing 6 GB-target budget arithmetic from the summary table understates the transient by about 450 MB and overstates the steady pool by about 1.2 GB.
- **Suggested Fix**: Change the rebuild row to "+1× projected (the duplicate) while under 80% of the live budget; ≤ 256 MiB without a reading". Change the pools peak to ~480 MB, and re-derive the Peak total.

#### REN-D5-2026-09-27-03: `geometry_rebuild_row_matches_the_constants` still asserts "#3443 bounds the doubling below the 256 MiB threshold" — the opposite of what the pinned section now says
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs` (`memory_budget_doc_pin_tests::geometry_rebuild_row_matches_the_constants`, the trailing `#3443` assert, its comment, and the `.expect` message; also the `chunked_rebuild_consults_the_idle_threshold_before_duplicating` expect text ">= 256 MiB rebuild duplicates …")
- **Status**: NEW (made stale by `5226d73e2`)
- **Description**: The test comment reads "#3443 landed the idle gate, so the doubling cannot reach the hard caps. A page that doubles those is arithmetic against a path the code no longer takes". Its failure message says the row must record that the gate "bounds the doubling below the 256 MiB threshold". Since `5226d73e2` the section says the reverse, and the assertion only passes because the literal `#3443` is still there.
- **Impact**: The pin checks for a token, not a claim. The next reader who trusts the test's prose will double-count or under-count.
- **Suggested Fix**: Reword the comment and message to the headroom rule: bounded by the 80% line with a reading, by 256 MiB without one. Consider pinning the needle "2× the `VERTEX_POOL_HARD_CAP`".

#### REN-D5-2026-09-27-04: The streaming texture-prefetch store (256 MiB host RAM cap) has no memory-budget.md row
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `byroredux/src/asset_provider/texture_prefetch.rs` (`STAGED_BYTE_CAP`, `PrefetchStore::reserve` / `finish` / `clear`); `docs/engine/memory-budget.md` (no entry; `grep -i prefetch` returns nothing)
- **Status**: NEW (the Dim 5 "First step" requires a ledger row for every new resource owner; this one landed in `a3632909a`)
- **Description**: The store keeps up to 256 MiB of extracted DDS bytes (measured peak 111 MiB) until the cell apply completes, is cancelled, or is dropped. The cap is checked against *ready* bytes only. Reads still in flight (`Slot::Running`) each hold a full `Vec<u8>` on a stream-pool thread until `finish`, and a read that overshoots is fully extracted and then dropped. The transient can therefore exceed the cap by up to (stream-pool threads × largest texture).
  - memory-budget.md does ledger CPU-side owners (EsmIndex, CDB, `SwfPlayer::pixel_buffer`).
  - Only `docs/engine/archives.md` §"Texture prefetch" mentions this store.
- **Impact**: A 256 MiB-class host allocation in the streaming path that the RAM budget does not show.
- **Suggested Fix**: Add a CPU-side row to memory-budget.md: cap, clear points, the in-flight overshoot bound, and the `tex_prefetch_*` telemetry that reports it.

#### REN-D6-2026-09-27-02: `every_exterior_spawner_inserts_a_boundary_material`'s doc still says `scene.rs` is out of scope and omits the save / ground-cover exemptions
- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs:2431-2436` (test doc "Deliberately out of scope: `cornell.rs` and `scene.rs`"), `:2578-2591` (`SPAWNER_ROOTS` doc "two debug-scene files a crate-wide walk would pull in").
- **Status**: NEW
- **Description**:
  - Since `2b1b7fc5c` (#4856), `scene.rs` is itself a `SPAWNER_ROOTS` entry, scanned with a per-entity exemption for `cube`/`quad`/`red_tri`/`blue_tri`. The doc still describes it as excluded and implies only a crate-wide walk would reach it.
  - The guard doc names no exemption for save `restore_world` or EXAL ground cover. Those are recorded in `docs/engine/nifal.md` §3 (lines ~725-731) and `docs/engine/exal-groundcover.md` (the "Material-boundary exemption (#4304)" paragraph). Ground cover's spec does record the exemption.
- **Evidence**: `SPAWNER_ROOTS: [&str; 6] = ["cell_loader","cell_loader.rs","scene","scene.rs","npc_spawn","npc_spawn.rs"]`; the `name == "scene.rs" && [...].contains(&entity)` exemption sits at `:2510`.
- **Impact**: An auditor or a future edit trusting the doc will misjudge what the guard covers.
- **Related**: #4856, #4302, #4304, Existing #4917 (stripper).
- **Suggested Fix**: Rewrite the scope paragraph: cornell.rs is excluded by root choice; scene.rs is scanned with four named exemptions; point to nifal.md §3 for the save and ground-cover exemptions.

#### REN-D6-2026-09-27-03: #4566's Oblivion PBR-override ceiling (99%) sits below Oblivion's measured fill (100%) — the corpus guard is red at HEAD
- **Severity**: LOW. The test is opt-in and fails false; no runtime effect.
- **Dimension**: NIFAL Material (owner `/audit-nifal` Dim 9)
- **Location**: `crates/nif/tests/translation_completeness.rs:478` (Oblivion lane, `assert_pbr_override_ceiling(s, label, 99.0)`).
- **Status**: NEW. It was introduced by the #4566 fix `57b852717`.
- **Description**: `57b852717` claims each ceiling sits "~10pp above its measured fill". Oblivion's measured `metO`/`rghO` is 100.0%: every mesh has a `NiMaterialProperty`, hence `specular_authored`, hence a classifier signal. That matches the historical 100% in `AUDIT_NIFAL_2026-06-13/06-28`. So the ceiling can never pass on a machine with Oblivion data.
- **Evidence**: My run at HEAD printed `Oblivion meshes= 567 … metO=100.0% rghO=100.0%` and then panicked with `[Oblivion] metalness_override fill > 99.0% (got 100.0%)`. The other 7 rows fit their ceilings: FO3 94.3, FNV 96.7, SkyrimLE 92.4, SkyrimSE 93.8, FO4 99.4, FO76 15.8, Starfield 5.1.
- **Impact**: The upward-drift guard, and every assertion after it in the harness, cannot run green for anyone with Oblivion installed. The rows after Oblivion never reach their assertions.
- **Related**: #4566, #4393, #2707.
- **Suggested Fix**: Set the Oblivion ceiling to 100.0 (by construction), or better, assert the exact structural reason (`specular_authored` on every Oblivion mesh) and ceiling the other signal classes separately.

#### REN-D7-2026-09-27-04: Leaving a raw-output debug view resumes TAA and FSR against history stale by the length of the debug session
- **Severity**: LOW (debug tooling only)
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/render_debug.rs` `set_render_debug_mode` (only logs and assigns); `post_passes.rs` `record_taa_pass` (the raw-view early return leaves `history[f]` unwritten); FSR is skipped via `is_fsr_dispatch_active()`.
- **Status**: NEW. It is the unfixed half of closed #3632, whose fix (`1ce99197`) folded the raw-output predicate into `is_fsr_dispatch_active()` for **jitter**. The issue's second bullet ("the first frame back at `RENDER_DEBUG_FINAL` therefore dispatches with `reset: false` against reconstruction history that is stale", "`set_render_debug_mode` … does not call `signal_temporal_discontinuity`") still holds at HEAD.
- **Description**:
  - During a raw view, TAA does not dispatch, so its history slots freeze. The G-buffer mesh ID, normal and motion keep updating.
  - On return, `taa.comp` validates the stale `uPrevHistory` against **fresh** previous-frame mesh IDs and normals. Those describe a different frame, so the disocclusion test is meaningless for the first frame back.
  - FSR resumes with its last `reset_pending` value.
- **Evidence**: `render_debug.rs`: `if self.render_debug_mode != mode { log::info!(..); self.render_debug_mode = mode; }`. No raw-output transition tracking exists anywhere (grep for `was_raw` / `prev_raw` finds none).
- **Impact**: Roughly 10–20 frames of ghost from the pre-debug view when the camera moved during the debug session. It also contaminates any capture taken right after leaving a view.
- **Related**: #3632, #4513.
- **Suggested Fix**: In `set_render_debug_mode`, when `render_debug_requires_raw_output(flags, old)` differs from `render_debug_requires_raw_output(flags, new)`, call `self.signal_temporal_discontinuity(..)` (which already resets TAA, FSR and volumetrics).

#### REN-D7-2026-09-27-05: The dormant DOF-through-TAA path cannot produce bokeh — the lens offset is baked into both view-proj matrices, so motion vectors cancel it
- **Severity**: LOW (dormant: no production writer sets `Camera.aperture > 0`; only `Default` 0.0 and tests)
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` `dof_effective_view_proj` (its doc: "non-zero motion → reduced TAA weight → blur"). `assemble_camera_and_lights.rs` passes `vp = &effective_vp` into `GpuCamera.view_proj`, into `history.prev_view_proj`, and into the `camera_static` compare. `triangle.vert` computes `fragCurrClipPos = viewProj * worldPos` and `fragPrevClipPos = prevViewProj * prevWorldPos`.
- **Status**: NEW.
- **Description**:
  - TAA sub-pixel jitter is added to `gl_Position` **after** the motion clip positions are taken, so motion vectors are jitter-free.
  - The DOF lens offset instead lives inside `view_proj` and `prev_view_proj`, so each frame's motion vector contains the lens parallax.
  - TAA fetches history at `uv − motion` and re-aligns each world point, then blends at a **flat** α = 0.1: `taa.comp` has no motion-dependent weight. The per-frame parallax is therefore undone instead of integrated, and out-of-focus surfaces converge sharp, not blurred.
  - DOF also makes `camera_static` false every frame, which disables SVGF progressive accumulation and the ReSTIR parked cap.
- **Evidence**: `taa.comp`: `float alpha = params.params.x;` (host `let alpha = 0.1;` in `taa.rs`). No motion term in the blend.
- **Impact**: None today, because the path is dormant. If DOF is enabled (imagespace DOF is on the roadmap), it will look sharp and the doc will mislead whoever debugs it.
- **Related**: #4002, #2518 (`fsr_gated_dof`).
- **Suggested Fix**: Build the motion-vector matrices from the pinhole camera (keep a separate pinhole `prev_view_proj`) and apply the lens offset like the jitter, or correct the doc to state that DOF needs a dedicated pass.

#### REN-D7-2026-09-27-06: Denoiser/resolve doc rot not in #4874's list
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location / Evidence**:
  - `shaders/svgf_temporal.comp` header:
    - it says it blends "with a fixed α (paper value 0.2)", but α is `max(floor, 1/(histAge+1))` with a floor that drops to 0 under progressive accumulation;
    - "moments … that Phase 4 will use", but the à-trous pass is shipped and reads them;
    - "avoiding the need for a previous-frame depth/normal buffer", but the shader samples `prevNormalTex` (binding 10, #650).
  - `shaders/svgf_temporal.comp` in-body comment: "only GpuCamera's knee lane still takes the bare camera fact". The same claim appears in `build_and_upload_instances.rs` ("`GpuCamera`'s w lane that `triangle.frag` reads for its knee"). The knee is `dof_params.z`. The bare camera flag is `dof_params.w`, and its consumers are the ReSTIR history cap/floor (REN-D7-2026-09-27-01) and the GI seed. The `assemble_camera_and_lights.rs` `dof_params` comment names only the GI seed.
  - `src/vulkan/taa.rs` `TaaPipeline::dispatch`: the pre-barrier comment ("the previous composite (fragment reader of this slot as input texture)"), the `#2768` comment ("composite would sample the previous cycle's history as this frame's HDR"), and the post-barrier ("Expose the result to composite's fragment shader read", "dst = FRAGMENT for composite's read this frame") all describe the pre-#3572 consumer. The real consumer is `record_native_blit`, which does its own COMPUTE→TRANSFER barrier.
  - `docs/engine/shader-pipeline.md` § G-Buffer Layout:
    - "Written by the main render pass (`triangle.frag` + `water.frag`)" omits `groundcover_blade.frag`, and water writes only attachments 0/4/6/7;
    - the Mesh ID row's bit 31 "skip SVGF accumulation" omits that TAA and à-trous also skip it.
  - `docs/engine/renderer.md` TAA step 2: "Catmull-Rom 9-tap". `sample_history_catmull_rom` is the 5-tap optimised form. This sits next to #4874's γ = 1.25 on the same line.
- **Status**: NEW (sibling sites of open #4874; fold into it when published).
- **Suggested Fix**: A comment and doc sweep. Have the #4874 fix take these sites too.

#### REN-D8-2026-09-27-02: The nuclear surface-light dimmer scans the unfiltered scene volume list, while the grid now culls distant sources
- **Severity**: LOW. A narrow trigger: a live nuclear source farther than `grid_far + radius` from the camera while another combustion source burns near the camera. Visual only.
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` `append_combustion_surface_lights` (`let nuclear_source_active = source_volumes.iter().any(…FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR…)`, ~l.1935). Its caller `assemble_camera_and_lights.rs` passes the raw `fog_volumes`, and `post_passes.rs` `record_volumetrics_pass` filters through `filter_fog_volumes_for_grid` (`88c23887b`).
- **Status**: NEW. The scan itself predates the window. `88c23887b` established the contract it now contradicts ("A distant source must not arm the grid-wide simulation"), and applies that contract only to the dispatch and transport gate.
- **Description**:
  - Every combustion light decoded from this slot's moments is scaled by `NUCLEAR_COMBUSTION_SURFACE_LIGHT_SCALE` (0.22), and its cull radius by √0.22, whenever *any* nuclear volume exists anywhere in the submitted list.
  - A nuclear source that `filter_fog_volumes_for_grid` culls contributes no moments, because the field is camera-centred.
  - It still dims every nearby fire, explosion or smoke-emission light to 22 % with 47 % reach.
- **Impact**: A campfire or burning wreck near the player loses most of its field-derived surface light while a distant Fat Man cloud is alive (FO3/FNV/FO4 exteriors).
- **Suggested Fix**: Evaluate `nuclear_source_active` against the grid-filtered set, which is already in `volumetric_fog_scratch` from the previous use of the slot. Better, attribute the scale per moment bin by distance to a nuclear source.

#### REN-D9-2026-09-27-01: The resize LRU rebase to the "never dispatched" sentinel makes a just-despawned entity's SkinSlot, skinned BLAS and MorphSlot permanently unreapable
- **Severity**: LOW
- **Dimension**: Skinning (Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`finalize_screen_pass_state`: `self.frame_counter = 0;` then `for slot in self.skin_slots.values_mut() { slot.last_used_frame = 0; }` and the `morph_slots` sibling). `crates/renderer/src/vulkan/skin_compute.rs` (`should_evict_skin_slot`: `if last_used_frame == 0 { return false; }`). Guard `swapchain_recreate_rebases_skin_slot_stamps_when_it_zeroes_frame_counter` asserts the `= 0` choice.
- **Status**: NEW. The morph half was noted "not filed, needs a repro" in `AUDIT_RENDERER_2026-09-26.md` Needs-validation. This adds the SkinSlot and BLAS half and the mechanism. #2925 (closed) introduced the rebase.
- **Description**: `0` is the #643 sentinel that `should_evict_skin_slot` never evicts. After the rebase, a slot leaves the sentinel only when something re-stamps it:
  - a `SkinSlot` through the skin dispatch loop (the entity is drawn again);
  - a `MorphSlot` through `refresh_morph_slot_lru` (the entity still has a `MeshHandle`).

  An entity despawned outside `unload_cell` is never re-stamped. Examples are superseded outfit parts and other non-cell despawns, which the code comments say rely on "ordinary pool aging". If it was despawned within the `min_idle` (3-frame) window before the recreate, it is never re-stamped, and its slot is never evicted. `unload_cell` queues only its own victims, so nothing else reaches it. The #2925 comment claims the rebase "rescues … the idle slots — exactly the ones eviction exists to reclaim". It does the opposite: it exempts exactly those slots from eviction until a re-stamp that, for a dead entity, never comes. Before #2925 the stale stamp pinned a slot for "as many frames as the session had run". After it, a dead entity's slot is pinned forever.
- **Evidence**: `recreate_swapchain` → `recreate_screen_passes` → `finalize_screen_pass_state` zeroes the counter and rebases every stamp to 0. `set_upscaler_mode` also routes through `recreate_swapchain`, so an FSR preset switch triggers it too. In `record_skinned_blas_refit`'s eviction sweep, `should_evict_skin_slot(0, now, 3)` returns `false` forever, while `drop_skinned_blas` is reached only from that sweep, a remap, or a rebuild.
- **Impact**: Each occurrence leaks until shutdown:
  - one `SkinSlot` (output buffer of `vertex_count × 12 B` plus descriptor sets from the FREE_DESCRIPTOR_SET pool);
  - its skinned BLAS;
  - for morph entities, the weight buffer and a strong ref on the shared `MorphDelta`.

  The window is narrow (a despawn ≤3 frames before a resize or upscaler switch), but it recurs per occurrence and is invisible: `skin.coverage` just shows a higher slot count. No correctness or rendering impact.
- **Related**: #2925, #643, #4294. Dim 5 owns lifecycle and teardown.
- **Suggested Fix**: Rebase to a non-sentinel epoch value (e.g. `1`), not `0`. Live slots are re-stamped on the next frame by dispatch or `refresh_morph_slot_lru`. Dead and idle ones then age out on the normal `min_idle` threshold in the new epoch. Update the guard's assertion to pin "not the sentinel".

#### REN-D9-2026-09-27-02: `drop_skinned_blas`'s doc comment is attached to `commit_provisional_skinned_blas`
- **Severity**: LOW
- **Dimension**: Skinning (doc)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`, the `/// Drop a per-skinned-entity BLAS. Routes through pending_destroy_blas …` block immediately above `/// #3991 / #917 — clear this frame's provisional-insert list …` on `pub fn commit_provisional_skinned_blas`. `pub fn drop_skinned_blas` has no doc.
- **Status**: NEW. Introduced by `0025d8221` (#3991, 2026-09-07), which inserted the commit and rollback pair between the doc and its function.
- **Description / Evidence**: Rustdoc merges both paragraphs into `commit_provisional_skinned_blas`'s docs. That tells readers a list-clear "routes through `pending_destroy_blas` with a `DEFAULT_COUNTDOWN` countdown", and leaves the one function that actually does the deferred destroy undocumented. `drop_skinned_blas` is the lifetime-critical path for the "skinned BLAS never destroyed while referenced" contract.
- **Impact**: Doc rot on a lifetime contract. No runtime effect.
- **Suggested Fix**: Move the "Drop a per-skinned-entity BLAS…" paragraph onto `drop_skinned_blas`.

Existing, still present:
- **#4876** (open, REN-D9-2026-09-24-06). `shader-pipeline.md` step 1a still says morph-weight visibility "comes from step 5b's bulk barrier". Not re-filed.

Resolved since baseline:
- **REN-D9-02 of 09-20** (PickedUp two-site lockstep, no guard) is fixed by `7ebf84817`. Both halves are now pinned: `picked_up_placements_stay_hidden_even_when_animation_says_visible` in `static_mesh_fx_skip_tests.rs` and `picked_up_body_gets_its_first_skin_upload_only_when_dropped` in `bone_palette_overflow_tests.rs`.

#### REN-D10-2026-09-27-02: Froxel local-light budget is spent in cluster-list order, and the directional light consumes a slot
- **Severity**: LOW
- **Dimension**: Volumetrics (cross-dimension with Dim 13; the root cause is cluster-list composition)
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp`: `uint lightLoopCount = min(cluster.count, adaptiveLightCap);` (L3054), then `if (lightType > 1.5) { continue; }` (L3061). The list comes from `cluster_cull.comp` `main()` (atomic append, directional `intersects = true` at L259). The cap comes from `AdaptiveRayBudget::settings_for_tier` (`scene_buffer/ray_budget.rs`: `volumetric_light_cap` = 2/4/6/8 for tiers 0–3).
- **Status**: NEW. Pre-existing since `5798e4672` (2026-08-09); no issue found.
- **Description**: The froxel loop takes the first `adaptiveLightCap` entries of the fragment cluster's list and then skips the directional among them. Two problems follow:
  1. The directional is in every cluster's list, so whenever `cluster.count > cap` and the directional lands in the first `cap` entries, one of the local-light slots is spent on a light the loop discards. At tier 0 (cap 2) that halves the local in-scatter budget.
  2. The list order is `atomicAdd` order across the 32 lanes of `cluster_cull`, which is implementation-defined. The retained subset is therefore not priority-based. In practice it tends to follow light index, i.e. the `gi_priority_score` order, which makes the CPU sort silently load-bearing for fog, but no contract says so.

  `caustic_splat.comp` already solves the same problem correctly: its loop charges `budgetedLights < maxLights` only for lights that pass its rejection (L420).
- **Evidence**: see Location.
- **Impact**: In dense-light clusters, lamp in-scatter in fog, smoke and fire media uses fewer local lights than the tier grants, and which lights it uses is arbitrary. Visual only.
- **Related**: Dim 13 volumetrics; PERF-D5 #4807 (density caps; different).
- **Suggested Fix**: Iterate `cluster.count` and count only accepted local lights against `adaptiveLightCap`, as `caustic_splat` does. If the subset should be deterministic, state that cluster lists are unordered and select by score inside the loop.

#### REN-D10-2026-09-27-03: Residue of closed #4860 — `spawn_nif_lights` fabricates `SHADOW_OMNIDIRECTIONAL` into both flag words, and `Emitter::default()` still carries `ARCHITECTURE`
- **Severity**: LOW
- **Dimension**: Light Animation
- **Location**: `byroredux/src/cell_loader/spawn.rs` `spawn_nif_lights` (the `LightSource::from_legacy_world_units(... LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL, ... LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL)` call, comment at L1139–1140); `crates/core/src/lighting.rs` `impl Default for Emitter` (`visibility: VisibilityMask::ARCHITECTURE`, L243), inherited by `LightSource::default()`.
- **Status**: Residual of #4860 (CLOSED). The ESM FO3/FNV synthesis that #4860 named was removed in `ab255cfd2`. The NIF-direct twin and the `Emitter::default().visibility` item that #4860 also listed were not.
- **Description**:
  - A direct `NiLight` has no LIGH flags, yet it is stored with `flags = 0x1000` and `shadow_flags = 0x1000`. The comment ("Preserve its authored physical visibility explicitly at this boundary") has been false since `b9e961eeb`, because `for_legacy_local_light()` now ignores flags. The `light` console command (`commands/scene.rs`) prints `legacy_flags=0x00001000 shadow_flags=0x00001000` as if the lamp had authored them. That is the "fabricated diagnostic" class that `fallout3nv_zero_projection_flags_remain_zero_diagnostics` now forbids on the ESM side.
  - `Emitter::default()` (and therefore `LightSource::default()`) is the one construction path whose visibility is not `FULL`. It has no production caller today, which makes it a latent trap for the next procedural producer.
  - The directional light (`render/lights.rs` `params.z`) and `volumetrics.rs` `combustion_light_from_moment` both write `FULL` literally, which is consistent with the single policy.
- **Impact**: Diagnostics are wrong for every in-mesh NIF light. No rendering effect today.
- **Suggested Fix**: Pass `0, …, 0` for the NIF-direct flags (or a named "derived" constant that the console labels as such), and delete the stale comment. Make `Emitter::default().visibility` `VisibilityMask::for_legacy_local_light()` (`FULL`).

#### REN-D10-2026-09-27-04: `dielectricF0FromIor`'s `eta` floor does not prevent the mirror-class F0 its comment says it prevents, and four IOR floors disagree
- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` `dielectricF0FromIor` (`float e = max(eta, 1e-3);`, L150). The other floors are:
  - `evaluatePathBsdf` / `pathSpecularProbability` (`max(ior, 1e-3)`);
  - `triangle.frag` `GLASS_IOR = max(mat.ior, 1e-3)`;
  - `shadow_transport.glsl` `max(hitMat.ior, 1.0)` (L145);
  - `triangle.frag` GI glass `max(hitMat.ior, 1.001)`.
- **Status**: NEW (hardening; the #1253 guard exists but is ineffective).
- **Description**: The #1253 comment says the clamp keeps an uninitialised `mat.ior = 0` from yielding `F0 = 1.0`. But `((1-e)/(1+e))²` at `e = 1e-3` is 0.996, still mirror-class. The unclamped `eta = 0` was never a divide-by-zero either: the singularity is at `eta = -1`. The formula is also symmetric under `eta ↔ 1/eta`, so any floor below 1 maps small IOR values back up to high F0. Current reachability is nil in production:
  - `material_optical_scalar` returns `DEFAULT_DIELECTRIC_IOR` for every non-fire kind;
  - fire-refraction, the one kind whose `ior` lane is a 0–1 strength, returns before the F0 line and is excluded from the TLAS (`draw_command_eligible_for_tlas`).

  It is reachable only via `mat.set` or Cornell.
- **Impact**: Defence-in-depth that does not defend; inconsistent floors for one field.
- **Suggested Fix**: Floor at 1.0 (vacuum → F0 = 0) in `dielectricF0FromIor` itself, and drop the per-caller `max(…, 1e-3)`. Correct the comment.

#### REN-D11-2026-09-27-02: a CLI-seeded upscaler (`--upscaler` / `--fsr-quality`) is written to `settings.toml` by the next unrelated settings save, so a one-launch flag becomes the persisted default
- **Severity**: LOW (tooling/persistence; owner overlap with `/audit-tooling` settings-io handoff)
- **Dimension**: FSR/Presentation
- **Location**: `byroredux/src/main.rs` `install_universal_settings` (`if explicit_upscaler { settings.set(UPSCALER_SETTING_ID, …) }`); `crates/settings-io/src/lib.rs` `save_to_path` (`for entry in registry.entries() { settings.insert(…) }`); the save trigger in `main.rs`'s debug-UI output loop (`if settings_changed { settings_io::save(…) }`).
- **Status**: NEW
- **Description**:
  - With an explicit flag, the CLI value is written into the live `SettingsRegistry` so the menu shows it.
  - `save_to_path` serialises every registry entry, not just changed ones.
  - Any later settings change in that session (FOV, a key binding, HUD scale) therefore persists the CLI upscaler too.
  - This contradicts the README's "Explicit renderer CLI flags still win **for that launch**". It is a plausible origin of the `fsr3/native-aa` on this machine: `scripts/fsr-bench-matrix.sh` runs `--fsr-quality native-aa`, and a hold session with any menu touch would persist it. Not proven — the file's history is unknown.
- **Evidence**: The code path above. Nothing in `install_universal_settings` marks the seeded value as transient.
- **Impact**: This feeds REN-D11-2026-09-27-01: a bench flag leaks into every later flagless launch.
- **Related**: REN-D11-2026-09-27-01.
- **Suggested Fix**: Keep the CLI override out of the persisted layer. Either don't `set` the registry, or remember the pre-seed persisted value and restore it for `save`. Alternatively, persist only entries changed through the UI.

#### REN-D11-2026-09-27-03: the settings registry and the live upscaler diverge on three paths; two of them persist a mode that is not running
- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - `byroredux/src/main.rs`: the setting-change loop (`settings.set` → `PendingUpscalerSwitch::request`, then `settings_io::save`).
  - `byroredux/src/app_step.rs` `step_upscaler_switch`.
  - `crates/renderer/src/vulkan/context/resize.rs` `set_upscaler_mode` (the rollback arm, `self.renderer_config.upscaler = previous`).
  - `byroredux/src/commands/world_info.rs` `UpscalerSwitchCommand::execute` (queues only, never touches the registry).
  - `crates/renderer/src/vulkan/context/init.rs`: the #2480 startup promotion to `UpscalerMode::Taa`.
- **Status**: NEW
- **Description**:
  - (a) A menu upscaler change is saved to disk in the same UI tick it is staged, before `set_upscaler_mode` runs. If the rebuild fails and rolls back (#2156 arm), the renderer runs `previous` while registry and disk keep the failed mode, which is re-attempted at every launch.
  - (b) `r.upscaler` switches the renderer but leaves the registry, and so the menu, on the old choice. The next unrelated settings save writes that stale value back, reverting the operator's console choice on the next launch.
  - (c) The startup FSR-failure promotion to TAA does not update the registry either, so the menu shows an FSR preset while TAA runs.
- **Evidence**: The code paths listed above. None of `step_upscaler_switch`, `set_upscaler_mode`, the console command or the promotion writes `UPSCALER_SETTING_ID`.
- **Impact**: The menu and the persisted default can misstate the active reconstruction path, compounding REN-D11-2026-09-27-01. There is no GPU impact.
- **Related**: REN-D11-2026-09-27-01/02.
- **Suggested Fix**: Make the applied mode the single writer. After `set_upscaler_mode` returns (success or rollback) and after the init promotion, write the actually-active `UpscalerMode` back into the registry and save only then. Route `r.upscaler` through the same setter.

#### REN-D11-2026-09-27-04: the "frame parameters absent" native-blit branch hard-codes the scene image's source layout and bypasses #4538's exclusivity assert (sibling of #4592 not swept)
- **Severity**: LOW. It is correct today by TAA/FSR construction exclusivity, and the branch is unreachable while the jitter and dispatch gates share `is_fsr_dispatch_active`.
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs` `FrameUpscaler::record`, the `let Some(frame_params) = fsr_frame else { … }` arm: `record_native_blit(device, cmd, frame, inputs.scene_color, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)`.
- **Status**: NEW. Related closed issues: #4592 (the recovery branch fixed to `inputs.scene_color_layout`) and #4538 (the `debug_assert_eq!` in `record_fsr_barriers_before`).
- **Description**:
  - The bridge branch and the recovery branch both pass `inputs.scene_color_layout`.
  - The params-absent branch passes a literal `SHADER_READ_ONLY_OPTIMAL` as the source layout. It returns before `record_fsr_barriers_before`, so the #4538 `debug_assert_eq!(inputs.scene_color_layout, SHADER_READ_ONLY_OPTIMAL)` never runs on it.
  - The #4592 test (`the_recovery_blit_sources_from_the_scene_images_actual_layout`) inspects only the third call site by position, and its comment names this one ("bridge, params-absent, recovery") without checking it.
- **Evidence**: The literal quoted above versus the `inputs.scene_color_layout` argument on the other two call sites.
- **Impact**: None today. If FSR mode ever consumes a `GENERAL` input (the refactor #4538 guards against), this branch would record the #4592 VUID (`oldLayout-01197`) with no assert firing.
- **Suggested Fix**: Pass `inputs.scene_color_layout` here too. Extend the #4592 source pin to every `record_native_blit(` call site rather than `positions[2]`.

**Dim 11 — existing issues re-checked at HEAD (not re-filed):**
- **Existing: #4840** (ACES fed negative graded values). Still present: `presentation.frag` `main` has no floor between `graded = (graded - vec3(0.18)) * …` and `tonemap(graded * exposure)`.
- **Existing: #4878** (plan §1.4 "Auto exposure is not enabled"). Still present: `docs/engine/fsr3-upscaler-integration-plan.md` Exposure row.
- **Existing: #4864** (water/proxy reactive = 1.0 vs the plan's 0.9 rule). **Appears resolved by `ab255cfd2`**:
  - The plan's Reactive row now states that water and the screen-composition proxy write `1.0`.
  - `water.frag` and the triangle.frag proxy branch carry matching rationale comments.
  - The tail comment ("never a full 1.0") now reads as the alpha-coverage baseline only.
  - The issue's requested A/B or RenderDoc evidence was not recorded, so the closure call is the publisher's.
- **Stale-open, fix verified at HEAD — recommend closing**:
  - **#4863** (meter weighs non-finite / near-black texels): `exposure_meter.comp` skips `isnan`/`isinf`, excludes `l <= 1e-4`, caps at `1e4`, and falls back to `1e-4` on zero samples.
  - **#4865** (no cross-language FFI layout pin): the C++ `static_assert`s in `byro_fsr3.h`, plus `byro_fsr3_abi_layout` compared field-for-field in `dispatch_abi_structs_are_plain_and_pointer_width_stable`.
  - **#4862** (#4578 had no GLSL guard; mirror docs stale): the log-space order is pinned, the `tonemap.rs` `agx` doc is corrected, and the licence pin is implemented against `THIRD_PARTY_NOTICES.md`.
- REN-D7-2026-09-27-04 (a raw-view exit resumes TAA/FSR on stale history) is owned by Dim 7 and not re-filed.

#### REN-D12-2026-09-27-01: Debug-UI "GPU passes Σ" double-counts volumetrics — the new inject/integrate brackets are nested inside the outer volumetrics bracket
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/debug-ui/src/panels.rs:1263` (`gpu_total`); nesting at `crates/renderer/src/vulkan/context/post_passes.rs:829-839` (`record_volumetrics_pass`: `cmd_volumetrics_start` → `vol.dispatch(.., self.gpu_timers.as_mut(), ..)` → `cmd_volumetrics_end`) and `crates/renderer/src/vulkan/volumetrics.rs:1724-1802` (`cmd_volumetrics_inject_*`, `cmd_volumetrics_integrate_*` inside `dispatch`)
- **Status**: NEW (`88c23887b`)
- **Description**:
  - `88c23887b` added `volumetrics_inject` and `volumetrics_integrate` as child brackets inside the pre-existing `volumetrics` bracket.
  - `metrics_sample_system` publishes all three as sibling `gpu_pass_ms` rows.
  - The overlay sums every `Some` row into "GPU passes — Σ upper bound".
- **Evidence**: `let gpu_total: f32 = m.gpu_pass_ms.iter().filter_map(|(_, v)| *v).sum();`. The `volumetrics` row is ≥ `volumetrics_inject + volumetrics_integrate`, so the Σ adds the volumetrics cost roughly twice. `docs/engine/renderer.md` already states the rule for the geometry-phase children ("Do not add the children to their inclusive main-render parent"), and the geometry phases are correctly kept out of `gpu_pass_ms`. The volumetrics children were not.
- **Impact**:
  - The Σ is inflated by one full volumetrics cost on every frame with fog or volumetrics.
  - The hover caveat covers queue-drain overlap, not structural double-counting.
  - It misleads the CPU-Σ vs GPU-Σ comparison the panel exists for.
- **Related**: #2513 (Σ excludes inactive rows); `scripts/fsr_bench_report.py` `render_sum` uses a fixed key list and is unaffected.
- **Suggested Fix**: Exclude child brackets from the Σ. Either mark the children as sub-rows (`volumetrics.inject`), or skip names in a `CHILD_BRACKETS` list when summing. Add a unit test that the Σ over a synthetic snapshot with parent and children equals the parent-only sum.

#### REN-D12-2026-09-27-02: `render.debug probe` resolves the hit entity through a later frame's SSBO→entity map, so it can name the wrong entity
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**:
  - `crates/renderer/src/vulkan/context/render_debug.rs:45` (`selected_ray_hit_entity_id`);
  - caller `byroredux/src/app_frame.rs:1255-1258` (`apply_pending_debug_requests`);
  - map maintenance `crates/renderer/src/vulkan/acceleration/tlas.rs:500` and `:744` (`tlas_entity_ids_scratch`);
  - field doc `crates/renderer/src/vulkan/render_debug.rs:146` (`committed_hit_entity_id`).
- **Status**: NEW (`de808add3`)
- **Description**:
  - The probe is traced in frame N. Its record is read back in `sync_and_acquire_frame` of frame N+`MAX_FRAMES_IN_FLIGHT`, after that slot's fence.
  - The result is taken in the *next* `apply_pending_debug_requests`, which runs before that frame's `draw_frame`.
  - At that point `committed_hit_instance` (an instance-SSBO index) goes through `tlas_entity_ids_scratch`. That is the single manager-level map filled by the most recent TLAS gather, about two frames after the trace.
  - SSBO indices are the compacted per-frame draw order. `tlas.rs`'s own comment says "SSBO indices can change freely with raster order".
- **Evidence**: `selected_ray_hit_entity_id` does `accel_manager.tlas_entity_ids_scratch.get(instance_ssbo_index)`. Nothing keys the lookup to the frame the probe was armed in. `tlas_entity_ids_scratch.resize(draw_commands.len(), 0)` does not clear older entries either.
- **Impact**:
  - Any camera motion, streaming or cull change between the trace and the resolve can print a valid but unrelated entity id.
  - The field doc says the id is "absent if the matching TLAS membership has already changed". It is not absent; it is wrong.
  - This is the output used for the single-sided-wall and shadow-leak hunts, so an occluder can be misattributed.
  - With a static camera the order is usually stable, so the bug is intermittent.
- **Related**: `ab255cfd2`/`186234944` (the ReSTIR light-identity work, same diagnostic family).
- **Suggested Fix**: When a probe is armed (a one-shot, so the cost is bounded), snapshot `tlas_entity_ids_scratch` (or just the TLAS-gather generation) into the armed-request slot for that frame. Resolve against that snapshot at readback, or return `None` when the generation differs. Fix the field doc to match.

#### REN-D12-2026-09-27-03: `water.frag` and `groundcover_blade.frag` ignore every non-water structured debug view — lit HDR water and grass leak into raw correctness views
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

#### REN-D12-2026-09-27-04: `GeometryTimerPhase` hard-codes query slot 46 and active bit 23 with no range or uniqueness assertion — the next named bracket's natural slot collides with MainOpaque
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:421-437` (`GeometryTimerPhase::query_start` = `46 + 2*n`, `active_bit` = `1 << (23 + n)`), next to the named `Q_*` constants (last `Q_VOLUMETRICS_INTEGRATE_END = 45`) and `BIT_*` (last `BIT_VOLUMETRICS_INTEGRATE = 1 << 22`)
- **Status**: NEW (`0925f7926`)
- **Description**:
  - The named bracket constants run contiguously to slot 45 and bit 22. The five geometry phases occupy slots 46-55 and bits 23-27 through two literals inside `impl GeometryTimerPhase`.
  - The next bracket added the way every previous one was added (`Q_X_START = 46`, `BIT_X = 1 << 23`, `QUERIES_PER_FRAME = 58`, plus a table row) would silently alias MainOpaque's slot and bit.
  - Writing a timestamp twice into one unreset query per frame is VUID-vkCmdWriteTimestamp-None-00830.
  - `doc_table_slot_count_matches_queries_per_frame` only counts table rows. Nothing asserts that `Q_*` and phase slots are unique and `< QUERIES_PER_FRAME`, or that the `BIT_*` and phase bits are disjoint and fit a `u32`.
  - 28 of the 32 `active_bits` bits are now used.
- **Evidence**: `fn query_start(self) -> u32 { 46 + 2 * self as u32 }` and `fn active_bit(self) -> u32 { 1 << (23 + self as u32) }`, with no named base constant.
- **Impact**: This is a latent Validation-visible VUID and corrupted timings on the next bracket bump. The bracket set has grown five times in a month (#4210, SKYAL, #4315, #4618, `88c23887b`), so another bump is likely.
- **Related**: #4541 and #4210 (the same count-rot family).
- **Suggested Fix**: Introduce `Q_GEOMETRY_PHASE_BASE` and `BIT_GEOMETRY_PHASE_BASE`, derived from the last named constant. Add a test (or `const` assert) that collects every START slot (named plus phases), checks they are even, distinct and `+1 < QUERIES_PER_FRAME`, and checks every bit is distinct.

#### REN-D12-2026-09-27-05: Timer and telemetry doc rot bundle
- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:5-9`, `:2051-2060`, `:302-313`, rows 40-55 of the module table; `docs/engine/renderer.md:280`; `GpuTimerSnapshot::composite_ms` doc (`gpu_timers.rs` ~`:240`)
- **Status**: NEW (items a–d); Existing: #4811 (item e, still open, partially unaddressed)
- **Description**:
  - (a) The module header gives the live count (56 / 28), but its bump history stops at "again from 38/19 by the exposure meter (#4618)". It omits 40/20 → 46/23 (`88c23887b`: ground-cover models, volumetrics inject and integrate) and 46/23 → 56/28 (`0925f7926`: five geometry phases).
  - (b) The doc comment of `prose_outside_the_module_header_carries_no_bracket_counts` still says the header states "(40) … 20 start/end brackets". Its `rotted` list stops at 38/19, so it cannot catch 40/20 or 46/23 rot.
  - (c) `groundcover_models_ms`, `volumetrics_inject_ms`, `volumetrics_integrate_ms` and the five `main_*`/`groundcover_*_draw_ms` fields have no doc comments. Nothing on the struct says the phase fields never reach `SkinCoverageStats`, the bench line or the UI (only the `BYRO_PROFILE` log). Table rows 40-55 also drop the table's alignment and prose style.
  - (d) `renderer.md` step 3 says acquire is "Bracketed by a GPU timer so a FIFO-present block is attributable". No acquire bracket exists; `vkAcquireNextImageKHR` is host-side, and the measurement is the CPU `acquire_ms` in `CpuFrameTimings`.
  - (e) #4811: `composite_ms` still lists "+ bloom" as a composite input. Composite declares `bloomTex` unused since #2796, and bloom runs after composite. The sky-aperture mask is still not mentioned.
- **Impact**: This is the recurring count and doc drift the skill warns about. A reader following `renderer.md` looks for a GPU acquire timer that does not exist.
- **Suggested Fix**: Extend the bump history and the `rotted` patterns (40/20, 46/23), document the new fields and the phase fields' profile-only scope, reword `renderer.md` step 3 to "timed on the CPU (`acquire_ms`)", and close #4811 with the composite doc rewrite.


## Prioritized Fix Order

Correctness first, then safety and hardening, then docs.

1. **REN-D6-2026-09-27-01 (HIGH): cheap, confined to one module, content-verified.**
   - In `merge_bgsm_arm`, resolve a `specular_enabled == false` leaf to a dielectric (`metalness_override = Some(0.0)`), and keep `bgsm_pbr_scalars_authored` truthful.
   - Add a keyword-metal (`NIF_METALNESS = 0.9`) spec-off fixture.
   - Confirm with `mat.dump` on Sanctuary `Res01PlayerHouse` trim.
2. **REN-D8-2026-09-27-01 (MEDIUM, Regression of #4858): a one-flag fix.**
   - Initialise the portal query with `gl_RayFlagsNoOpaqueEXT` so triangles surface as candidates.
   - Replace the source-shape guard with one that asserts the flag and loop are compatible.
3. **REN-D2-2026-09-27-01 (HIGH) together with REN-D7-2026-09-27-01 (MEDIUM): same code block in `triangle.frag`.**
   - Fix the RIS weight (`N·pHat`, or treat the enumerated list as one M = 1 sample).
   - Accumulate *visibility* rather than radiance in the ReSTIR EMA.
   - Drive the parked history cap from the SVGF scene-static signal.
   - This relights every multi-light interior. Gate it on a transport-oracle A/B: Cornell two-emitter plus FNV Prospector / Skyrim BanneredMare, compared under `RENDER_DEBUG_DIRECT_ONLY`. Add a CPU mirror energy test (Σ rad for N equal and unequal lights).
4. **REN-D10-2026-09-27-01 (MEDIUM):** shadow-gate the BGSM translucency lobe, the way the Skyrim back-light lobe already is inside `shadowableLightRadiance`.
5. **REN-D7-2026-09-27-02 (MEDIUM):** count first-sight and removed rigid instances as `rigid_instance_moved`. O(1) per draw, on data already in hand.
6. **REN-D11-2026-09-27-01 (MEDIUM), then D11-02 and D11-03:**
   - Log the persisted-setting override.
   - Report the effective mode in the boot line and in the `bench:` line.
   - Make bench and determinism harnesses immune to `settings.toml` (for example via `BYROREDUX_SETTINGS_PATH`).
   - Stop one-launch flags from persisting.
7. **REN-D7-2026-09-27-03 (MEDIUM, `--upscaler taa` only):** give water a TAA no-history or reactive signal.
8. **LOW correctness and hardening:**
   - D9-01: rebase slot stamps to 1, not 0.
   - D12-04: range-check `GeometryTimerPhase` slots and bits before the next bracket collides.
   - D12-01: stop summing nested timer brackets.
   - D12-02: resolve the probe entity through its own frame's map.
   - D12-03: make water and grass respect debug views.
   - D8-02: filter the nuclear dimmer's volume list.
   - D10-02: froxel light-budget order.
   - D2-03: integer IGN hash.
   - D2-02: window-portal `-V` sample, once #4832 has a live capture.
   - D3-02: fix the SAFETY comment.
   - D3-03: FxHash in `LightHistory` / `upload_lights`.
   - D11-04: pass `inputs.scene_color_layout`.
   - D10-03 / D10-04.
9. **Test gaps:**
   - D1-01: TLAS refit-identity rule.
   - D5-01: in-place compaction actually taken.
   - D3-05 / D3-06: ground-cover mirrors.
   - D4-02: pin the second grow's invariant.
   - D6-03: the corpus guard is red at HEAD.
10. **Doc and ledger rot:** D3-01, D4-01, D5-02/03/04, D6-02, D7-05/06, D9-02, D12-05, D1-02, D2-04 (pin the interior gate).
11. **Issue hygiene:** close the 13 fixed-but-open issues listed in the Executive Summary.
12. **Cross-dimension (repo hygiene):** `screenshot_1790471263.png` was committed to the repo root in `de808add3`. Owner: `/audit-tech-debt`.


## Needs-RenderDoc / live validation

Observations only. No speculative Vulkan edits are proposed. Grouped by dimension, verbatim from the dimension auditors, with IDs remapped.

### Dim 1

- **`5eb07a4f3` TLAS UPDATE path.** Run a `BYRO_VALIDATION=1` grid crossing with NPCs. The UPDATE mode now runs on more frames: stable identity means fewer forced BUILDs. Confirm no VUID-03667 / 03708 / 03758 reports and no traversal-quality regression. The commit records a validation run, but on MedTek.
- **`5226d73e2` makes the chunked (two-generation) rebuild the common FO4 crossing path.** A sync-validation run through a crossing should confirm:
  - `restore_missing_static_blas_for_draws` builds global-only LOD BLAS only from the bound generation during the deferred-compaction window;
  - that no AS-build-input read of a deferred-destroyed generation appears.

  The code trace says both hold.
- **BLAS admission vs geometry duplication.** The live-budget duplication gate budgets 20% headroom for "streaming allocations during the copy", while BLAS admission is gated on the BLAS residency budget (≤ 1 GiB), not on live heap usage. On a 6 GB-class card both can grow during a multi-frame copy. This is a `/audit-performance` Dim 3 quantification item, not a finding.

### Dim 2

- **REN-D2-2026-09-27-01 magnitude and cluster seams**: A/B a multi-light interior with the normalisation fixed against HEAD (for example FNV Prospector, Skyrim BanneredMare), plus the Cornell harness with two emitters, and compare direct-only (`RENDER_DEBUG_DIRECT_ONLY`) luminance. Look for cluster-aligned steps under `RESTIR_LIGHT`.
- **Reservoir ownership under overdraw (observation, pre-existing)**: late-test pipelines run the fragment shader for fragments that are later depth-rejected, and each writes `reservoirsCurr[pixelIdx]`. A hidden surface drawn after the visible one can own the pixel's reservoir. The surface-ID gate then rejects reuse for the visible surface, which adds noise but no leak. `PERFORMANCE_OPAQUE_EARLY_TESTS_2026-09-26.md` leaves "overlapping-fragment history ownership" open. Measure with the early/late variants on an actor-dense interior.
- **Window portal live effect** (#4832 note): still unverified. Needs a Skyrim or Vault 21 window capture, ideally with REN-D2-2026-09-27-02's `-V` sample.

### Dim 3

- `186234944` `ReservoirBuffers::begin_frame` now `cmd_fill_buffer`s the current reservoir slot to 0 every frame. A zero reservoir encodes light index **0**, a valid light (the pinned directional), with surface id 0, rather than the 1023 sentinel. `remapReservoirLight` will then map it to the current directional's slot, so correctness rests on `M = 0` / `W = 0` nulling the reused weight in every temporal and spatial consumer. This belongs to Dim 2 (ray queries / ReSTIR) and should be confirmed there or in a validation capture. The barrier pair around the fill is also a Dim 4 item.

### Dim 4

- **Resize, preset switch and `set_upscaler_mode` rollback under `BYRO_VALIDATION=1`.** No live run covered them. Code tracing finds every size-derived resource recreated and rebound, but REN-D5-2026-09-26-07/08/12 (#4885, #4886, #4890) remain open on the same path.
- **Exterior grid streaming with the ground-cover model tier active.** This is the only route where the second `grow_instance_ssbos` replaces a buffer mid-recording (REN-D4-2026-09-27-02). It is also the route that exercises the model tier's EMIT barrier against the main pass and `groundcover_blade`'s partial G-buffer writes (mesh ID and normal left as terrain). Worth one validation plus sync-validation capture on FNV Lake Mead or Skyrim Tamriel.
- **Dynamic-RGBA in-frame copies with a live HUD or Scaleform overlay** (MenuXml HUD on FNV, Scaleform `--menu`). The live runs did not state whether an overlay was animating. A sync-validation run with an animating HUD would exercise the `ALL_COMMANDS` ↔ `TRANSFER` image barriers across the two slots.
- **Early-test pipeline correctness.** The certificate is sound by source reading. The visual A/B (`BYRO_DISABLE_OPAQUE_EARLY_TESTS=1`) is recorded in the commit's own perf doc and was not re-verified here.
- **Ground-cover perf warnings (owner `/audit-exterior`).** The live runs report 9 `SPIR-V Interface` performance warnings at one `vkCreateGraphicsPipelines` call: `groundcover_blade.vert` outputs at locations 6/7/13/16 (float) and 10/11/12/15/20 (uint) with no fragment input. This is valid per spec. It recurs at every session start and dilutes the validation signal. No Dim 4 finding is filed for it.

### Dim 5

- #4880 closure: a `BYRO_VALIDATION=1` fault-injection run (forced `vkAllocateMemory` / `vkCreateImageView` failure on the batched `flush_upload_batch` path) would confirm that no command buffer referencing a freed image or staging buffer is submitted. The code reads correct and the test is source-shape only.
- The resumable rebuild is now the normal FO4 crossing path, at about 400 MB duplicated over about seven 64 MiB chunks. The commit reports one sync-validation run (0 VUIDs). A 6 GB-card or constrained-budget run would confirm that the 80% line actually routes to the idle path under pressure; the dev card never exercises that branch.
- Host RSS after a large-exterior → small-interior transition: the in-place pools keep high-water capacity, and pages already written stay resident (bounded by ~480 MB). A `/usr/bin/time`- or `smaps`-style reading would show whether a `shrink_to` hysteresis is worth adding. This is documented in memory-budget.md as intended, so it is not filed.

### Dim 6

- **REN-D6-2026-09-27-01 visual confirmation.** Take one capture of Sanctuary (`Res01PlayerHouse`) trim and one of Parsons gate under direct sun, pre and post fix. Use `mat.dump` on the trim shape to confirm `metalness 0.9 / specular_strength 0` live, and to confirm that the shape→BGSM association inferred from matching diffuse paths holds.
- The FaceGen tint-by-`material_kind` role selection was not re-censused this run (no delta in `slot_role.rs` beyond doc `1a12670ae`). The aggregate `cross_game_translation_completeness` sample printed plausible per-game fill for tex/kind/normal/tangent before the REN-D6-2026-09-27-03 panic.

### Dim 7

- **REN-D7-2026-09-27-01 magnitude**:
  - On an FNV or Skyrim interior with a FLICKER light (vanilla candle/torch LIGH), sample a lit wall pixel's direct-only luminance (`RENDER_DEBUG_DIRECT_ONLY`) over ~120 frames. Compare parked vs moving vs `DBG_DISABLE_TEMPORAL`.
  - Expected if the analysis holds: flicker amplitude with temporal on ≈ 2 % (parked) / 9 % (moving) of the temporal-off amplitude.
- **REN-D7-2026-09-27-02**: With a parked camera, `disable` (or pick up) a coloured object on a tabletop. Time the indirect-only view's decay: τ ≈ 4 s expected at HEAD, ≈ 10 frames after the fix.
- **REN-D7-2026-09-27-03**: `--upscaler taa` on FNV Lake Mead or any open water. Strafe and compare against the FSR default for reflection smear.
- **Ground-cover blades under TAA** (owned by /audit-exterior; observation only): `draw_color_write_masks` leaves mesh ID and normal masked, so a blade pixel carries the terrain's stable ID and normal. TAA therefore has no blade-vs-terrain disocclusion, and trails depend on the colour clamp alone. SVGF is intentionally fed terrain GI there.
- **SSAO lag** (accepted design, #2798): the main pass samples AO written two frames earlier at `gl_FragCoord` with no reprojection. Under fast turns, AO halos lag silhouettes by two frames of motion. Worth one capture if AO "swimming" is reported.
- **SVGF on camera cut**: `signal_temporal_discontinuity` gives α = 0.5 for 8 frames, not a hard reset (`params.z` is untouched by design, per the `next_svgf_temporal_alpha` doc). Same-ID same-normal surfaces at the same pixel across a teleport keep 50 % of the pre-cut GI on frame 1. That is acceptable by design; noted for completeness.

### Dim 8

- **No water cell was in the live runs.** Needed:
  - water-pipeline creation and draws under `BYRO_VALIDATION=1` (FNV Lake Mead / Skyrim river);
  - `WaterCausticAccum`'s pre-pass clear → water.frag `imageAtomicAdd` (FRAGMENT) → composite read (FRAGMENT, next pass) dependency chain, device-confirmed.
- **REN-D8-2026-09-27-01 A/B**: a sealed interior with glazed windows and no authored aperture. Compare `VOLUMETRIC_TERM` at tier 0 and tier 2, before and after a fix. Count rim probes via `rt.masks`/timers.
- **`TransportFieldState` transitions** under sync validation: expiry write → known-empty skip → re-arm, including a forced submit failure (`BYRO_FSR_FORCE_DISPATCH_FAIL`-style) during the expiry frame.
- **Tier-0 rim shed (`e2f99ad55`)**: with the adaptive tier oscillating between 0 and 1, unmarked-opening godrays pop in and out. The temporal EMA may hide it; watch a tier transition live.

### Dim 9

- **What the orchestrator's runs showed.** The FNV Prospector run (skinned NPCs, full palette → dispatch → refit → TLAS chain) showed 0 VUID and 0 SYNC-HAZARD, with `missing_skinned=0`. That is consistent with the barrier set above.
- **Not exercised by that run:**
  - a resize or upscaler switch with skinned or morph entities mid-despawn (the REN-D9-2026-09-27-01 window; confirm with `skin.coverage` slot counts across a despawn-then-resize);
  - first-sight bind-inverse upload failure;
  - equip churn;
  - a mid-run cell transition.
- **Both-slots fence wait.** The cross-frame `bones_prev` read (raster reads the other slot's palette) relies on the both-slots fence wait in `sync_and_acquire_frame` to exclude a WAR hazard against the next frame's palette write. This is sound at `MAX_FRAMES_IN_FLIGHT == 2` and is listed in the `sync.rs` rider list (#4852). Any narrowing of that wait must re-derive it.

### Dim 10

- **REN-D10-2026-09-27-01 magnitude**: capture before and after moving the SSS lobe into `shadowableLightRadiance`, on a scene where the sun falls behind translucent BGSM foliage under an occluder, and on an interior with a lamp behind a wall adjacent to translucent content.
- **REN-D10-2026-09-27-02**: a `froxel` debug view on a dense-lamp interior at tier 0 vs tier 3, to confirm how much local in-scatter is lost. Whether atomic append order actually varies frame to frame on NVIDIA/AMD is not provable from source.
- **Carried**: the `FULL`-for-all collapse (`b9e961eeb`) has had no recorded live A/B for foliage comb artifacts through railings. The pre-flip rationale named "stable comb-like projections through railings"; the post-flip claim is that alpha coverage is resolved by `rayHitHasCoverage`. Candidates: FNV Prospector, Skyrim BanneredMare and Markarth SilverBloodInn, plus the Cornell L2/L5 oracles.

### Dim 11

- **Re-run the "FSR default" validation with an explicit Quality flag.** Use `--cornell --upscaler fsr3 --fsr-quality quality --bench-frames 90` under `BYRO_VALIDATION=1`, or `BYROREDUX_SETTINGS_PATH=<empty scratch file>`. The bare `--cornell` run almost certainly ran native-aa (REN-D11-2026-09-27-01), so the render < output boundary layouts (the CRITICAL-floor path) were not exercised this session. Confirm with the `Frame extents:` line.
- `BYRO_FSR_FORCE_DISPATCH_FAIL=1` under validation: the recovery depth-restore + GENERAL-output blit, carried.
- FSR consuming a per-frame-varying exposure texel under `--auto-exposure`, and the new meter sample policy on a mostly-void interior (everything excluded → `1e-4` → exposure clamps to `MAX_AUTO_EXPOSURE` 16). Is that the intended behaviour for a black loading frame?
- Live FSR boundary layouts and mask content (#4864's A/B); the FP32 SDK permutation remains untested (needs a device without `shaderFloat16`).
- A preset switch (`r.upscaler fsr3 performance` ↔ `taa`) under validation; the orchestrator did not run one.

### Dim 12

- The five BOTTOM_OF_PIPE phase timestamps and the PIPELINE_STATISTICS begin/end inside the main render pass. The live runs had no `BYRO_PROFILE`, so the statistics-query path (begin/end inside a render pass instance, host reset of a pool whose query was begun but whose readback was NOT_READY) was not exercised under validation. Worth one `BYRO_PROFILE=1 BYRO_VALIDATION=1` run.
- A submit failure after timestamps were recorded leaves `active_bits` set for queries that never executed. The next read of that slot then reports `0.0` as active. This was only observed in code; it depends on the error path.

## Stale skill premises (for the next `/audit-renderer` sync)

Verbatim from the dimension auditors, with IDs remapped. Cross-cutting items first:

- **Missing baseline.** The 2026-09-24 renderer audit (which filed #4827–#4880) is not in `docs/audits/` or in git history. Several dimensions had to reconstruct it from issue bodies. Future baselines for Dims 1, 4, 8, 9, 10, 11 and 12 should cite this report.
- **Guard names.** `gpu_light_is_64_bytes` → `gpu_light_is_80_bytes` (also in `/audit-safety`). `depth_capture.rs` has no `dependency_chain_tests`; its guards are `capture_ordering_tests` and `depth_format_guard_tests`. Dims 11 and 12 have no `Guard:` line.
- **Counts.** GPU timers: 56 queries / 28 brackets (not 40/20). `RenderDebugMode`: 16 user modes (not 13).
- **Frame shape.** The frame order is missing the exposure meter (composite → bloom → exposure meter → TAA → upscale → presentation), the top-of-frame dynamic-RGBA copies, and the `ReservoirBuffers::begin_frame` clear.
- **Default upscaler.** "FSR Quality is the engine default" holds only when no persisted `render.upscaler` exists (REN-D11-2026-09-27-01).

### Dim 1

1. **UPDATE decision.** The checklist says "the build/update decision keys on `last_blas_addresses` only". Since `5eb07a4f3` it also keys on `last_entity_ids` membership, next to `needs_full_rebuild`, `last_blas_map_gen` and `built_primitive_count`. `invalidate_tlas_recording` (from `rollback_skin_frame_state`) forces BUILD after an unsubmitted recording. The canonical sort is `(BLAS address, full EntityId)` via `tlas_entity_ids_scratch`.
2. **Paths.** The two halves of the load-bearing contract live outside the listed Paths:
   - `context/begin_frame_recording.rs`: `grow_instance_ssbos`, then `build_instance_map` with `instance_map_cap` (#4833).
   - `context/build_and_upload_instances.rs`: the SSBO writer and the #2913 `debug_assert_eq!`.
   - `scene_buffer/constants.rs`: `instance_map_cap`, the 24-bit const assert.

   Add all three.
3. **Mask bullet.** Record the `is_legacy_fx_card` arm (blended ∧ (kind 102 ∨ `dst_blend == 0`)) → EFFECT (#4576 / #4834). Baseline REN-D1-2026-09-21-01 is resolved, and a 2026-09-24 audit filed and closed #4833 / #4834 (report not in `docs/audits/`).
4. **New checklist item.** Geometry compaction (in place since `7e9da5dcc`) and the VRAM-scaled chunked rebuild (`5226d73e2`) cannot invalidate BLAS, for four reasons:
   - static BLAS are self-contained;
   - global-only LOD sources resolve at restore time from the bound buffer and published offsets, gated by `is_geometry_resident`;
   - skinned BLAS use per-mesh index buffers;
   - only `apply_compaction_plan` publishes offsets.

   The guard for the in-place path is `in_place_compaction_moves_each_survivor_with_its_bytes` / `out_of_order_layout_falls_back_to_the_allocating_copy` (`mesh/geometry_ssbo.rs`).
5. **Guard line.** `static_blas_recovery_runs_between_frames_not_in_the_render_driver` sits in `app_step::bench_subject_distance_tests` and needs `cargo test -p byroredux --bin byroredux`. The acceleration filter count is now 135.
6. **Barrier guard caveat.** `build_tlas_is_preceded_by_an_unconditional_as_write_to_as_read_barrier` pins order and gate only, not the dst access mask.
7. **Shrink bullet.** Per the 2026-09-26 report, stale premise 7: the TLAS shrinks run at the `draw_frame` tail on the *next* slot, and `shrink_tlas_to_fit` only records `tlas_shrink_pending`.
8. **Budget bullet.** Per the 2026-09-26 report, stale premise 2: the SDK term is `FrameUpscaler::sdk_memory_bytes`, and `screen_scaled_reservation_bytes` is `pub(super)` in `acceleration/predicates.rs`.

### Dim 2

1. **Dim 2 checklist, "Noise is frame-seeded interleaved-gradient"**: the default build now uses the exact integer PCG `hash2_pixel_frame` for:
   - ReSTIR candidate selection (fresh/temporal/spatial `u`)
   - the spatial disk (`dr`)
   - ReSTIR shadow jitter (`dRand`)
   - GI (`giRand`, `eventRand`, `bounceRand`, `lobeRand`)

   IGN survives only in the glass roughness scatter and the compiled-out legacy-WRS arm (see REN-D2-2026-09-27-03).
2. **Dim 2 checklist, "interior miss → cell ambient, not open sky"**: still true per site, but since `0572bfd5a` the sky cube is baked from `SkyParams::portal_outdoor_sky` in **every interior**. `exteriorSkyRadianceOr` therefore returns outdoor sky indoors, and the interior gate is each caller's `jitter.w > 0.5`, not the helper's (REN-D2-2026-09-27-04). The window portal now samples the cube (`exteriorSkyRadianceOr(throughDir, exteriorSkyTint.rgb)`), not `exteriorSkyTint.rgb` alone. `exteriorSkyDiffuseOr` self-gates on both flags.
3. **Dim 2 checklist, glass/portal**: add that the window-portal gate, ray, origin and sky sample are all oriented from the viewer-facing `N_bias` (`throughDir = -N_bias`, #4832), pinned by `window_portal_orients_gate_ray_and_sky_sample_from_the_viewer`. The "#821 portal does NOT use `N_bias`" premise is retired.
4. **Dim 2 checklist, ReSTIR**: add the light-identity remap. Reservoir light indices are translated through `previousLightToCurrent[MAX_LIGHTS + 1u]` (light SSBO header, 4112 B; `LightHistory::remap` in `scene_buffer/light_history.rs`, keyed by `GpuLight.history_id`), and current reservoirs are cleared per frame. Guards: `light_history::tests`.
5. **Dim 2 checklist, secondary-hit frames**: `getRayHitTangentFrame` now poses smooth normals and tangents from the current `bones[]` palette (`getHitVertexTransform`, set 1 binding 3 now visible to FRAGMENT), not `mat3(inst.model)`. `triangle.vert` emits `fragNormalTransform` (locations 11–13) for model-space normal maps.
6. **Dim 2 Guard line**: "the `depth_convention` tests" are not a module. They are the `shader_contract_tests` depth family (`the_shader_depth_convention_matches_the_engine_constant`, `every_depth_linearisation_goes_through_the_convention_header`, `depth_view_space_decodes_true_depth_from_inv_view_proj_alone`, `depth_decode_sites_read_no_fog_lanes`). `depthLinearize` no longer exists; the helper is `depthViewSpace(invViewProj, ndcXY, z)` (#4831).
7. **Dim 3 Guard line (cross-dim)**: `gpu_light_is_64_bytes` is now `gpu_light_is_80_bytes` (`GpuLight` gained `history_id`, `186234944`).
8. **Phase 1**: `scripts/check-shader-artifacts.sh` is the reliable stale-SPIR-V gate (it also rebuilds `triangle_early.frag.spv`). Recommend it as the standing first step for Dim 2 too; per-file `git log -1` date comparison is noisier.
9. **Cross-dim observation (Dim 8 / `/audit-exterior`, not filed)**: `water.vert` wave `phaseA`/`phaseB = dot(absolutePos.xz, dir) * spatial * 2π − t·…` is a `sin()` of absolute world XZ. It is pre-existing; `18ab4dba9` only flipped the time sign. It is smooth rather than a hash, bounded to about 2.6×10⁴ rad at the 2^20 ceiling (`uv_scale_a` ~ 1/256), and the phase error stays sub-BU. Its time term also grows unbounded with uptime, the #4930 class.

### Dim 3

1. **Guard line**: `gpu_light_is_64_bytes` is now `gpu_light_is_80_bytes`. The same stale name is in `.claude/commands/audit-safety/SKILL.md:173`. Also add these to the guard line: `every_remaining_uniform_block_size_matches_its_host_struct` (09-21 item 7, still unapplied), `composite_and_volumetrics_uniforms_match_rust_field_order`, `history_header_has_exact_std430_offsets` and `scripts/check-shader-artifacts.sh`.
2. **Paths — standalone mirrors**:
   - The list is `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`, `volumetrics_inject.comp`.
   - Actual `GpuInstance` copies are `bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp` and `groundcover_models.comp` (6).
   - `volumetrics_inject.comp` carries `GpuBoundaryInstance` (stride-guarded) plus a `GpuLight` copy, not a `GpuInstance`.
   - `GpuLight` copies are `bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp` and `volumetrics_inject.comp`.
   - `CameraUBO` copies are `bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp` and `caustic_splat.comp`, plus the prefix block `GcCameraUBO` in `groundcover_blade.vert` (D3-06).
3. **New checklist item — light SSBO header**: set 1 binding 0 (and the cluster, caustic and volumetrics sets) is `count, 3 pads, previousLightToCurrent[MAX_LIGHTS + 1], GpuLight[]` (4112-byte header). `GpuLight.history_id` is producer identity. `LightHistory::remap` maps the previous slot's list to the current one. `upload_lights`' dirty gate must hash the remap too.
4. **Instance-buffer rebind bullet**: confirmed that only the caustic set needs `rebind_instance_buffer` (09-26 item 8). Also record that the grow now runs **twice** per frame (`begin_frame_recording` before the TLAS map, and `build_and_upload_instances` with the model-tier tail), and that the TLAS map uses `instance_map_cap(instance_capacity(frame))` (#4833). Raster batches and the UI instance still compare against `MAX_INSTANCES` (#4726/#4722).
5. **Flag-constant bullet**: the list can add the now-generated `RENDER_LAYER_ARCHITECTURE`, `FOG_VOLUME_SHAPE_*`, `VERTEX_BONE_*_OFFSET_FLOATS` and `TONEMAP_OP_*`. The remaining literal sites are D3-04.
6. **Semantic-lane bullet**: add `GpuCamera.sky_tint.w` (sun angular radius) and `render_debug: [u32; 4]`, the only `u32` lane in `GpuCamera`, which D3-02 shows is easy to forget.
7. **MirroredPendingGuard**: "only `Reservoir`" is still true. Note that `ShaderLocal` `GcDrawIndirect` fails the table's own counterpart rule (D3-05).
8. **Issue hygiene**: #4778 is fixed in code but open. #4870 is partially fixed (see Existing).

### Dim 4

1. **Guard line.** "`dependency_chain_tests` (`egui_pass.rs`, `depth_capture.rs`)": `depth_capture.rs` has no such module. Its ordering guards are `capture_ordering_tests` (`finish_readback_runs_after_the_in_flight_fence_wait`, `record_copy_runs_immediately_after_the_depth_history_copy`) and `depth_format_guard_tests`.
2. **Frame shape.** This was already flagged in the 09-21 premise #5 and is still unsynced.
   - `begin_frame_recording` now also records dynamic-RGBA copies and the reservoir clear.
   - `record_groundcover_models` sits between `build_and_upload_instances` and `record_geometry_pass`.
   - The post-pass list must read "… composite → bloom → **exposure meter** → taa → upscale → presentation".
   - The tail is "probe FRAGMENT→HOST barrier → copy_depth_to_history (conditional) → depth_capture_record_copy → post passes → egui → screenshot → #4602 host flush edge".
3. **G-buffer bullet.** Add that `triangle_early.frag` (`pipeline_early`, `DrawCommand::allows_early_fragment_tests`) must keep all eight outputs, which `opaque_early_test_module_preserves_outputs_and_selects_execution_mode` pins. Also add that the ground-cover model tier has no pipeline of its own: it draws through the late opaque triangle pipeline.
4. **Checklist.** The rider list in `sync.rs` now has 13 items plus `images_in_flight`. Skill text that counts "six/seven" riders is stale.
5. **Resize bullet.** Carried from the 09-26 Stale skill premises item 3, which still holds. The real rebuild list is depth/depth-history, G-buffer, SVGF, reservoirs, caustic, water-caustic, bloom, volumetrics (whole pass), composite, egui, TAA and presentation, and SSAO. The exposure meter and ground-cover model tier rewrite their descriptors per dispatch. Ground cover rebuilds only its render-pass-bound pipelines, and only on a surface-format change.
6. **Doc test.** `shader_pipeline_documents_every_record_pass_helper` (added `0e0d35b96`) is the "doc moves with pass" gate the 09-21 premise #8 asked for. Its scope is `post_passes.rs` only; see REN-D4-2026-09-27-01.

### Dim 5

1. **Dim 5 checklist, geometry bullet.** Add: "`geometry_rebuild_needs_idle(projected, has_existing, live_budget)` gates duplication at 80% of the live `VK_EXT_memory_budget` DEVICE_LOCAL budget (`allocator::approaching_oom_line`). The fixed 256 MiB `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` applies only when the budget query returns `None`. The caller reads the budget via `VulkanContext::live_memory_budget` (unthrottled) only when a rebuild starts." Also add: "the CPU pools compact in place (`copy_within`) when survivors are ascending and disjoint; otherwise the allocating copy runs."
2. **Guard line.** Add `in_place_compaction_moves_each_survivor_with_its_bytes`, `out_of_order_layout_falls_back_to_the_allocating_copy` and `live_budget_gates_duplication_at_the_approaching_oom_line`. Note that the first cannot tell the two compaction branches apart (REN-D5-2026-09-27-01).
3. **Baseline item 14 (d/e).** `staging_guard_coverage_tests` / `no_file_outside_this_module_rolls_its_own_image_chain` are still allow-lists. `texture.rs` now has a source-shape unwind test (`dds_image_setup_failures_unwind_before_recording_copy_commands`), but it pins text order, not behaviour.
4. **The 09-26 report's finding 02 / #4880** is fixed in code (see status table). The next sync should drop it from the open list once it is closed.
5. **The 09-26 report's finding 21** was never published. The only live remainder is the rider-3 "blocking" wording and the `production_text` removal in the rider test (a Dim 3 / concurrency concern).
6. **memory-budget.md summary table** (VRAM Rough Budget): the rebuild and pool rows are stale (REN-D5-2026-09-27-02). Budget audits should read the `### Global geometry SSBO rebuild` section, not the summary row.

### Dim 6

1. **Guard line.** `every_exterior_spawner_inserts_a_boundary_material` is now per-function (#4856), and `SPAWNER_ROOTS` includes the sibling files `cell_loader.rs`/`scene.rs`/`npc_spawn.rs`, with scene.rs's four demo inserts exempted by name. The "documented exemptions: `cornell.rs`, save `restore_world`" list lives in `docs/engine/nifal.md` §3 (Cornell, `crates/save`, EXAL ground cover), not in the test. The ground-cover spec does record it.
2. **Still unapplied from the 09-21 baseline.**
   - `bgem_uses_thin_glass_behavior` is a production fn, not a test.
   - `resolve_pbr_is_idempotent` / `glass_behavior_preserves_authored_map_overlay` are `byroredux-core` tests, run with `--features inspect`.
3. **First step.** It omits three Dim 6 Paths: `systems/particle.rs`, `render/particles.rs`, `tangent.rs`. Add them, plus `crates/renderer/shaders/include/ray_hit.glsl`, which holds the secondary POM marcher and the tangent frame.
4. **Checklist MSWP / BGSM bullet.** Add the `specular_enabled` rule: #4654 zeroes spec and skips glossiness/roughness; #4836 gates metalness and the conductor tint. Add REN-D6-2026-09-27-01's caveat that the fallback is the keyword classifier, not a dielectric.
5. **Corpus guard.** Name `cross_game_translation_completeness` as the Dim 6 "check against real vanilla data" run. It is cheap (about 2 s, 200 meshes per game), but it is red at HEAD until REN-D6-2026-09-27-03 is fixed.
6. **Bindless-mask guard.** `bindless_index_bits_are_masked_at_every_textures_subscript` only sees a same-line `mat.<field>` inside `textures[`; a copy through a local escapes it. Note that in the guard description.

### Dim 7

1. **Order line**: "Order: composite → bloom → TAA/upscale" is missing the exposure meter. The actual order is composite → bloom → exposure meter → TAA → upscale → presentation (`record_post_passes`).
2. **Paths**: add `context/build_and_upload_instances.rs`. It hosts the `params.w` decision (`next_svgf_temporal_alpha` call, `caustic_scene_static`, `rigid_instance_moved`) and the SVGF/TAA param uploads. Also add `composite.rs` `prepare_sky_aperture` + `draw.rs` `build_composite_params` (the composite aperture cull, which the 09-24 audit already filed under D7), and `water.rs` / `groundcover.rs` `draw_color_write_masks` as G-buffer writers the temporal filters depend on.
3. **params.w bullet**: spell out what "scene-unchanged" is: `!rigid_instance_moved && pose_dirty.is_empty() && caustic_scene_key == prev` (light rig + caustic sources). Record the REN-D7-2026-09-27-02 gap (instance set).
4. **Double temporal accumulation**: the skill never mentions that direct light is already EMA-accumulated in `triangle.frag` (ReSTIR `prevAccum`, cap 16/64, floor 0.1/0.025 keyed on `dofParams.w`) before TAA or FSR. Add it as a Dim 7 interaction, with REN-D7-2026-09-27-01.
5. **TAA bullet** "un-jittered projection kept for motion vectors": true for the Halton jitter; false for the DOF lens offset (REN-D7-2026-09-27-05, dormant). The alpha-blend bypass also does not cover water, which keeps the bed's stable ID (REN-D7-2026-09-27-03).
6. **Guard line**: add `svgf_temporal_alpha_is_fed_the_combined_camera_and_light_rig_signal` (`build_and_upload_instances.rs`), `taa_comp_soft_clamps_geometry_reprojecting_from_sky_instead_of_hard_rejecting`, and `aperture_screen_bounds_preserve_brute_force_ray_hits` (`composite.rs`). Flag `an_svgf_parameter_upload_failure_latches_the_pass` as a raw-text scan that has not moved to `source_scan::production_text`.
7. **Composite bullet**: add the post-fog terms (underwater god-ray shafts, exterior precipitation, the `preResolveDither` of `volume_params.w = 1/1024`). All are linear HDR, pre-bloom.

### Dim 8

1. "inject = one `TerminateOnFirstHit` shadow ray per froxel" is still stale; the third audit in a row flags it. Per froxel the inject pass traces:
   - 1 opaque sun ray;
   - 1 full-traversal glass ray (no longer TerminateOnFirstHit, since `ab255cfd2`);
   - 4 architecture rim probes (only at tier > 0);
   - up to `MAX_FROXEL_LIGHTS` × 2 local rays, now skipped entirely for zero-scattering froxels;
   - the combustion boundary queries.

   The sun rays are skipped when `sunTermLive` is false.
2. The checklist should add the `88c23887b`/`e2f99ad55` machinery:
   - `filter_fog_volumes_for_grid` (runs before `requires_dispatch`);
   - `TransportFieldState` plus the `TRANSPORT_KNOWN_EMPTY_DT` (−2) sentinel next to `TRANSPORT_EXPIRED_DT` (−1);
   - the dirty-range cluster upload (`fog_cluster_write_range`);
   - `fogRayQualityTier` (`GpuFogVolumeUpload.count[1]`) and its tier-0 rim shed;
   - the inject/integrate GPU timers.
3. Fresnel and IOR are not constants:
   - "Fresnel base ≈ 0.02" → F0 is the authored WATR fresnel clamped to [0.001, 0.20], default 0.02.
   - "RT reflect/refract IOR ≈ 1.33" → the authored `WaterMaterial::ior` (`push.timing.w`), default 1.33.
4. "`WaterCausticAccum` … added to **direct**" → composite adds `albedo × (glass + water)/CAUSTIC_FIXED_SCALE` as a third term beside `direct` (`combined = direct + indirect*albedo + caustic`). It is albedo-modulated and not denoised.
5. Guard line:
   - Add `composite_and_volumetrics_uniforms_match_rust_field_order` (`composite.rs`).
   - Flag `architectural_glass_portal_checks_every_candidate_layer` as a source-shape pin that encodes REN-D8-2026-09-27-01.
   - Any ray-query candidate loop needs a non-opaque flag check, because the whole TLAS is `GeometryFlagsKHR::OPAQUE`.
6. Paths: add `context/assemble_camera_and_lights.rs` (the combustion drain), `context/draw.rs` `build_composite_params` (sky apertures) and `vulkan/composite.rs` `prepare_sky_aperture`. These interior-godray consumers moved in `0572bfd5a`/`e2f99ad55`.
7. The 2026-09-24 renderer audit's report (`docs/audits/AUDIT_RENDERER_2026-09-24.md`) is cited by issues #4828/#4837/#4858/#4875, but the file is not in `docs/audits/` or anywhere in git history. The next skill sync should not list it as a baseline file.

### Dim 9

1. **Morph bullet** (carried from 09-26 item 10, still unapplied). "An evicted slot must be *recreated* when the entity is next seen" is wrong. `MorphSlot` is created once at spawn (`try_spawn_morph_slot`) and never recreated. The live invariant is that liveness is stamped from entity liveness (`refresh_morph_slot_lru` ← `MeshHandle` presence), so a live entity's slot is never reaped. The new risk is the resize-rebase leak (REN-D9-2026-09-27-01).
2. **Guard line** (carried from 09-20 item 8, still unapplied). `bind_inverse_upload_failed_*` is in renderer `context/draw.rs` (`bind_inverse_upload_failed_is_reset_alongside_skin_dispatch_ran`), with siblings in `dispatch_skin_and_cluster.rs`, not `app_frame.rs`. `bone_palette_overflow_tests` is its own file, `byroredux/src/render/bone_palette_overflow_tests.rs`. Write the renderer guard command with `--lib` and the bin guards with `-p byroredux --bin byroredux`.
3. **Checklist addition.** Add "LRU stamp rebases on `frame_counter` reset must not use the `0` never-dispatched sentinel" (REN-D9-2026-09-27-01), and "the palette `boneWorld` descriptor range is the whole buffer, and the palette descriptor cache keys on ranges as well as handles" (#4829).
4. **Guard list.** Add `a_reused_plan_scratch_keeps_capacity_and_drops_the_previous_plan` (#4611), `palette_descriptor_key_tracks_each_descriptor_range` / `palette_dispatch_range_is_the_whole_buffer_not_the_frame_extent` (#4829), `release_shared_destroys_on_last_strong_ref_despite_a_weak` (#4838) and `palette_publish_barrier_covers_fragment_bone_readers` (#4853).
5. **Palette barrier bullet.** The skill says "COMPUTE → AS-build → FRAGMENT". The palette barrier's own dst is COMPUTE|VERTEX|FRAGMENT, because the raster and `ray_hit.glsl` `bones[]` readers consume the palette directly. The skin-output barrier's dst is AS_BUILD|FRAGMENT|COMPUTE. The bullet should name both edges.
6. **Record the 2026-09-24 run as the true D9 baseline** for the next sync. It is uncommitted, and its findings are #4829, #4838, #4852, #4853 (closed) and #4876 (open).

### Dim 10

1. "Shadow-visibility policy … NIF-direct and authored-projection lights → `FULL`; unflagged room lights → `ARCHITECTURE | DYNAMIC_ACTOR`; Starfield Light Type 1 → shadow spotlight, Type 2 → conservative", plus the "Open, device-gated: … `STATIC_PROP`" note. **Now**: `VisibilityMask::for_legacy_local_light()` takes no argument and returns `FULL` for every legacy light in every game (`b9e961eeb`, `2b1b7fc5c`). Projection and shadow flags are diagnostics only. Replace the bullet with: "one policy = `FULL`; a producer hand-picking a mask, or `Emitter::default()`'s `ARCHITECTURE`, is the regression".
2. "Shadow decode is permissive … FO3/FNV zero-authoring → `SHADOW_OMNIDIRECTIONAL` … the asymmetry is deliberate." **Now**: `canonical_light_shadow_flags` returns the authored bits unchanged, except Starfield Light Type 1 → `SHADOW_SPOTLIGHT`. It no longer synthesises anything (`ab255cfd2`, pinned by `fallout3nv_zero_projection_flags_remain_zero_diagnostics`), and it no longer affects rendering. The strict-animation half is unchanged. The "asymmetry" wording should become "animation is strict; shadow bits are pass-through diagnostics".
3. "GI scans only the first `GI_HIT_LIGHT_CAP` entries, so the sort is what makes that prefix meaningful." **Now** (since #4017, `e47d486c6`): `pathHitRadiance` scans all `lightCount` lights with a per-hit top-K (`GI_HIT_LIGHT_CAP` = 8). The sort matters only for the `MAX_LIGHTS` tail clamp and, implicitly, for the froxel cluster-prefix (REN-D10-2026-09-27-02). The code-comment half of this rot is open as #4877.
4. "Sun soft shadow: single-tap stochastic cone." **Now**: the ReSTIR finalize traces `rayBudget.directShadowSamples` (1..`MAX_DIRECT_SHADOW_SAMPLES`) cone taps per pixel per frame. It is still deterministic per pixel per frame (`hash2_pixel_frame`).
5. The Paths line should add `crates/renderer/src/vulkan/scene_buffer/light_history.rs` and `scene_buffer/upload.rs` (`upload_lights`: remap and dirty gate). The checklist should add: "LightHistory remap reads the previous FIF slot's identities over the exact uploaded slice; the dirty hash includes the mapping; every GpuLight producer sets a unique `history_id` or zero". The Guard line should add `light_history::tests`, `light_history_identity_survives_animated_priority_reordering` and `history_header_has_exact_std430_offsets`.
6. The Guard line's `shader_contract` description ("no unshadowed fill terms") overstates coverage. The two tests only ban `LIGHT_AMBIENT_FILL_FACTOR`/`lightAmbientFill` and an XCLL bypass; they cannot see the translucency lobe (REN-D10-2026-09-27-01).
7. Doc: `docs/engine/memory-budget.md` Light SSBO row (64 B, no 4112 B header). Already filed this run as REN-D3-2026-09-27-01; not re-filed.

### Dim 11

1. **Dim 11 has no `Guard:` line.** Proposed:
   - `cargo test -p byroredux-renderer --lib exposure|presentation|tonemap|upscal|fsr|post_passes`
   - `cargo test -p byroredux-fsr3-sys` (cross-language ABI + vendored-SDK contract)
   - bin `renderer_config_defaults_to_fsr_quality`
   - `scripts/check-shader-artifacts.sh`

   Note that no test covers the *effective* default (`install_universal_settings`).
2. **"FSR Quality is the engine default"** holds only for launches without a persisted `render.upscaler`. The effective precedence is CLI flag > `settings.toml` (`$XDG_CONFIG_HOME` or `~/.config/byroredux/settings.toml`, or `BYROREDUX_SETTINGS_PATH`) > Quality. The floor and every validation instruction should require an explicit `--upscaler`/`--fsr-quality` or a scratch settings path. The same correction applies to `.claude/commands/audit-fnv/SKILL.md`'s "the flag defaults to `fsr3`".
3. **Stale since the 09-21 baseline and still in the skill:**
   - `NO_EXPOSURE_RESOURCE_FALLBACK` no longer exists (slots are cleared to `DEFAULT_EXPOSURE`; the meter failure latch freezes the value).
   - `aces(graded * params.exposure)` is now `tonemap(graded * exposure)`, with the exposure from `exposureTex` and an ACES|AgX switch.
   - The order line omits the exposure meter (composite → bloom → exposure meter → (TAA) → upscale → presentation).
   - Paths should add `vulkan/exposure_meter.rs`, `shaders/exposure_meter.comp`, `src/tonemap.rs`, and the settings seed `byroredux/src/main.rs` `install_universal_settings`.
4. **The FFI checklist item** should record that `crates/fsr3-sys` now has a C↔Rust layout probe (`byro_fsr3_abi_layout`) plus C++ `static_assert`s. New FFI structs must join both.
5. **Issues #4862, #4863 and #4865 are OPEN but fixed at HEAD**, and #4864 is doc-resolved; sync them before the next dedup pass.

### Dim 12

1. "`QUERIES_PER_FRAME` (40 = 20 brackets)" is now **56 = 28 brackets**: 23 named, plus 5 `GeometryTimerPhase` BOTTOM_OF_PIPE sub-intervals of main render. Add the optional per-FIF PIPELINE_STATISTICS pool (`opaque_fragment_invocations`, `BYRO_PROFILE` only). Note that `active_bits` has 4 bits left.
2. "`RenderDebugMode`, 13 user modes" is now **16** (`USER_MODES`, Final + 15; `facing_ratio` and `restir_light` are the newest). The 09-21 report already flagged this, but it was not applied.
3. Dim 12 has **no `Guard:` line**. Proposed:
   - `cargo test -p byroredux-renderer gpu_timers`
   - `every_post_pass_is_gpu_timer_bracketed_or_deliberately_not`
   - `composite_frag_spv_debug_mode_guard_matches_render_debug_mode_max`
   - `legacy_visualization_bits_are_disabled_in_named_debug_modes`
   - `finish_readback_runs_after_the_in_flight_fence_wait`
   - bin crate: `bench_gpu_keys_match_the_reported_bracket_order`, `every_bench_gpu_key_is_printed_on_the_bench_line`, `cornell::tests`, `mat_set_tests`
   - plus `scripts/check-shader-artifacts.sh`
4. "Every reader" should list the actual fan-out: `fill_skin_coverage_stats` → `SkinCoverageStats`, then:
   - `BENCH_GPU_KEYS` and the bench line (`app_events.rs`);
   - `gpu_breakdown` (`systems/debug.rs`);
   - `metrics_sample_system` (`gpu_pass_ms`), which feeds the debug-ui, byro-dbg display and TUI generically, so they need no per-bracket edit.

   Two exceptions: `groundcover_bench_ms` is owned by `GroundcoverBench`, and the geometry phases are profile-log only by design.
5. Add a checklist line: **child (nested) brackets must not be summed with their parent**. This covers the geometry phases in main render (documented) and volumetrics inject/integrate in volumetrics (REN-D12-2026-09-27-01).
6. The raw-output gate list should say where each gate lives: Rust for TAA, exposure meter, bloom and upscale; shader for composite and presentation. The exposure meter is the gated "new post pass". Add that main-pass pipelines other than triangle (water, ground-cover blades) must honour the structured modes too (REN-D12-2026-09-27-03).
7. Paths should add `context/sync_and_acquire_frame.rs` (timer read, probe readback), `byroredux/src/app_frame.rs` `apply_pending_debug_requests` (probe entity resolve), `byroredux/src/systems/{debug,metrics}.rs` and `byroredux/src/main.rs` `BENCH_GPU_KEYS`.

## Guard posture

- Every named guard that still exists passes and none is `#[ignore]`d. Per-dimension filtered runs passed (0 failed in all):

  | Dim | Result |
  |---|---|
  | 1 | acceleration 135 |
  | 2 | shader_contract 114; ray_budget 6; light_history 6; core camera 30 |
  | 3 | 287 lib, 1 ignored device-only |
  | 4 | 73 + 68 + bin 1 |
  | 5 | 43 + 219 |
  | 6 | bin 78, core 2, renderer 2 |
  | 7 | 111 |
  | 8 | water 77, volumetric 71, caustic 45, froxel 8, combustion 5, fog_volume 7 |
  | 9 | 107 + 24, bin 12 |
  | 10 | 114 + 55 + 1, bin 48, core 7 |
  | 11 | lib 104, fsr3-sys 8, bin 12, settings-io 8 |
  | 12 | lib 15 + 22, bin 53 |

- The one red run is the opt-in real-data corpus test `cross_game_translation_completeness` (REN-D6-2026-09-27-03). The Oblivion ceiling of 99% is below Oblivion's measured fill of 100%.
- **Source-shape guards pinning broken or vacuous code:**
  - `architectural_glass_portal_checks_every_candidate_layer` pins the D8-01 regression.
  - `swapchain_recreate_rebases_skin_slot_stamps_when_it_zeroes_frame_counter` pins the `= 0` choice behind D9-01.
  - The #4832 window-portal test pins the pane-normal sample (D2-02).
  - The updated TLAS refit test passes `&[0]`, so its tie-break never runs (D1-01).
  - `bindless_index_bits_are_masked_at_every_textures_subscript` misses a copy made through a local (all current readers were checked by hand).
- `scripts/check-shader-artifacts.sh`: every `.spv` is byte-identical to a fresh compile. No GLSL edit in the window ships stale.

## Process notes

- **Orchestrator re-verification of every HIGH and MEDIUM** against HEAD source:
  - D2-01: the `restirWSum += w_i; restirM += 1.0` loop and `W = restirWSum / (restirM * restirPHat)`. Nothing downstream compensates: `frameContribution = rad * restirW * visibility` goes straight into the EMA.
  - D6-01: the `merge.rs` `if leaf.specular_enabled` block and the unconditional `bgsm_pbr_scalars_authored = true`; `classify_pbr_keyword`'s first arm returns 0.9. The population comes from the agent's FO4 sweep.
  - D7-02: first sight falls back to `unwrap_or(m)`, so the compare `previous_source != m` is false, and removals are never folded in.
  - D8-01: `gl_RayFlagsOpaqueEXT` with a candidate-only loop body. Per `GL_EXT_ray_query`, opaque triangles never become candidates.
  - D10-01: `Lo += sssTint … * unshadowedRadiance` with no visibility term.
  - D11-01: an upscaler was selected from the persisted setting, reproduced live.
  - D9-01 was also checked: `resize.rs` `slot.last_used_frame = 0`, and `should_evict_skin_slot` returns `false` on 0.
  - D7-01 and D7-03 rest on the agent's traced code paths. They are consistent with the ReSTIR block the orchestrator read for D2-01.
- **Execution notes:**
  - An accidental bare `byroredux --help` launch opened a real engine window (the binary has no `--help`). It was killed. It also surfaced the D11-01 lead.
  - Another Claude session was building in a separate worktree (`gamebyro-redux-perf`) during the audit. No engine instances collided.
- **Cross-dimension duplicates removed:** the `LightHistory` std hashing finding (D3, D4 and D5 → D3-03). D2-02 (the window-portal sky sample in `triangle.frag`) and D8-01 (the window-glass portal query in `volumetrics_inject.comp`) are distinct defects in the same feature family.
- The dimension auditors' full "Verified OK" lists are not reproduced here; the assessments above summarise them.

## Appendix A — Existing issues touched by this audit

### Dim 3 — Existing / still open (not re-filed)

- **Existing: #4722** — the UI overlay's `ui_instance_idx` is still gated on `MAX_INSTANCES` (`build_and_upload_instances.rs` `(idx < super::super::scene_buffer::MAX_INSTANCES).then_some(idx as u32)`), not on `instance_capacity(frame)`. It is still present at HEAD.
- **Existing: #4726** — scene draw batches are still not clamped to the grown capacity after a failed grow. `upload_instances` clamps to `instance_capacity`; the batches do not. Still present.
- **Existing: #4870** — partially fixed by `0e0d35b96`. The `skyTint.w` mirrors, the `CompositeParams` SAFETY, the `render_origin.w` rustdoc, the cone shape doc and the flags table (`DIFFUSE_ALPHA`/`LOD_BLOCK`, 160 B terrain tile) are fixed. Still stale: "five GLSL copies" at `gpu_types.rs:93` (there are six `GpuInstance` copies), plus the D3-02 collateral.
- **#4778 (OPEN)** — its substance is fixed. `composite_and_volumetrics_uniforms_match_rust_field_order` now compares the full `VolumetricsParams` member order, and the test cites #4778. Recommend closing it.

### Dim 5 — Status of the 21 findings from 2026-09-26

| # | Issue | Status at 7e9da5dcc |
|---|---|---|
| 01 | #4879 | OPEN, still present. `texture_registry/upload.rs` is untouched and `flush_upload_batch` has no `ref_count == 0` skip. |
| 02 | #4880 | **Fixed in code by `2b1b7fc5c` + `e26441c34`, issue still OPEN.** The allocate arm destroys the image. `create_image_view` now runs before the first `cmd_pipeline_barrier`. No `?` exit remains after recording starts; the staging buffer is handed to the caller. Pinned by the source-shape test `dds_image_setup_failures_unwind_before_recording_copy_commands`. #4854 is CLOSED. Recommend closing #4880 (fault-injection confirmation still pending, see Needs validation). |
| 03 | #4881 | OPEN, unchanged (`buffer.rs` untouched). Since `5226d73e2` it matters more: every FO4 crossing now takes the chunked path, so the mesh-side `geometry_staging_pool` cycles through 64 MiB chunks plus a smaller tail chunk on every crossing, and those acquires are what shrink the pool's capacity labels. |
| 04 | #4882 | OPEN, unchanged in substance. `5226d73e2` now reclaims before retrying a failed duplicate allocation. A failed atomic build still leaves `geometry_dirty` set and is retried every frame through the unwinding-free `buffer.rs` constructors. |
| 05 | #4883 | OPEN, unchanged (`context/resources.rs` untouched). |
| 06 | #4884 | OPEN, unchanged (`acceleration/` untouched). |
| 07 | #4885 | OPEN, unchanged (`recreate_descriptor_sets` untouched). |
| 08 | #4886 | OPEN, unchanged. |
| 09 | #4887 | OPEN, unchanged (`dds.rs` untouched). |
| 10 | #4888 | OPEN, unchanged. The `groundcover.rs` edit in `0e0d35b96` is comment-only. |
| 11 | #4889 | OPEN, unchanged. |
| 12 | #4890 | OPEN, unchanged. `resize.rs` only gained the `pipeline_early` destroy and re-assign. |
| 13 | #4891 | OPEN, unchanged. |
| 14 | #4892 | OPEN, still present. memory-budget.md still has no `dynamic_rgba` row (grep finds nothing). |
| 15 | #4893 | OPEN (bundle, unchanged). |
| 16 | #4894 | OPEN, unchanged. |
| 17 | #4895 | OPEN, unchanged. The `device.rs` delta is `timestamp_valid_bits` only, and `texture_compression_bc` is still neither required nor gating. |
| 18 | #4896 | OPEN, unchanged. No pin test was added. |
| 19 | #4599 | OPEN, unchanged in substance. `e26441c34` adds two more `.expect("allocator lock poisoned")` sites in `record_dds_upload`. Both are on the upload path, not reachable from teardown. |
| 20 | #4897 | OPEN, still present. `instance_map_scratch` is still absent from `shrink_frame_scratch`. |
| 21 | never published | **Mostly fixed by `2b1b7fc5c`.** Riders 9–13 now name the `groundcover_models` prepare/harvest, `SkinSlot::output_buffer`, the in-place skinned BLAS refit, `bind_inverse_upload_staging` and the `SkinSlot` / `destroy_slot` / `MorphSlot` immediate frees, and the test table pins them. Two things remain. Rider 3 (`sync.rs`) still says the terrain tile buffer is "overwritten by a blocking staged copy". `0e0d35b96` also dropped `production_text` from three owners in `frames_in_flight_contract_names_every_dependent_resource`: `bind_inverse_upload_staging`, `entry.accel` and the `groundcover_models` `# fence contract` check. Those needles can now match test text; that is Dim 3's call. |

Side note, not in the 21. `0e0d35b96` rewrote the memory-budget.md model-tier text (0/14/28 MiB per slot) and added the volumetrics noise-pair row (294,912 B), which appears to resolve open **#4872**. It also rewrote the deferred-destroy table (`DEFAULT_COUNTDOWN + 1`th tick). #4872 is still OPEN; it can be closed after a check. The Dim 3 light-SSBO row (64 B/light, against the actual 80 B plus a 4112 B header) is already filed there and is not re-filed here.

### Dim 8 — Existing issues still present (not re-filed)

- **#4774** (write/sample Z convention drift). `fieldT = 0.5 * (fieldFrontT + fieldBackT)` is unchanged in `volumetrics_inject.comp` `main`.
- **#4837** (`resolve_water_noise_and_rain` divides all four concentration lanes, oceanness included, by 20). The loop in `byroredux/src/env_translate.rs` is unchanged.
- **#4875** (volumetrics doc rot). The inject header still says "up to 14 ray-query traversals … ~12.9M", and `interior-godrays-status.md` still says "uncommitted".
- **#4782** (V-buffer history has no non-finite guard).
- **#4784** (full RK2 stencil when any emitter is active). `88c23887b` gated only the curl forcing on `activity > 0.0`; the backtrace and gather still run grid-wide.
- **#4864** (water writes reactive = 1.0). `ab255cfd2` reaffirmed it in a comment; this belongs to Dim 11.

### Dim 8 — Open issues that appear fixed in code (orchestrator: verify and close)

- **#4778** (VolumetricsParams size-only pin). `composite_and_volumetrics_uniforms_match_rust_field_order` (`e26441c34`) now pins member order and passes.
- **#4783** (out-of-grid emitters arm the stencil and linger). Fixed by `filter_fog_volumes_for_grid` plus `distant_transport_emitters_do_not_arm_grid_simulation` (`88c23887b`).
- **#4785** (sun ray traced when the sun term is zero). Fixed by `sunTermLive` (`90c779748`) plus the zero-scattering local-light gate (`e2f99ad55`).
- **#4789** (volumetrics timer attribution). `88c23887b` added `cmd_volumetrics_inject_*` / `cmd_volumetrics_integrate_*` and `VolumetricsFrameState` via `note_volumetrics_state`. Confirm that the bench/log side matches the issue's ask.

### Dim 12 — Existing-issue status updates (fixed in code, issue still OPEN — recommend closing)

- **#4866** (ground-cover model tier unbracketed): fixed by `88c23887b`. `cmd_groundcover_models_start/_end` is in `groundcover_models.rs`, `BIT_GROUNDCOVER_MODELS`, slots 40/41, wired to `SkinCoverageStats`, the bench line (`BENCH_GPU_KEYS`), `gpu_breakdown` and `metrics_sample_system`. Tested by `groundcover_models_bracket_reports_measured_duration`.
- **#4868** (`saturating_sub` / `timestampValidBits`): fixed by `0e0d35b96`. `snapshot_from_bits_with_valid_bits` computes `e.wrapping_sub(s) & timestamp_mask`, and `timestamp_valid_bits` comes from caps. Tested by `timestamp_valid_bits_make_wrapping_brackets_report_elapsed_time`.
- **#4867** (named modes vs legacy `DBG_VIZ_*` bits): fixed by `ab255cfd2`. `triangle.frag` uses `uint vizFlags = legacyDebugMode ? dbgFlags : 0u;` and every categorical site tests `vizFlags`. Pinned by `legacy_visualization_bits_are_disabled_in_named_debug_modes`. Composite and presentation decide raw output from the mode alone, which is consistent.
- **#4811**: still open (see REN-D12-2026-09-27-05e).

### Dim 7 — Cross-dimension leads (not Dim 7)

- **Hot-path hashing (#2923 rule)**: `scene_buffer/light_history.rs` `LightHistory::remap` builds a `std::collections::HashMap<Identity, …>` (`scratch`) every frame. It is called from `scene_buffer/upload.rs` in the per-frame light upload (`self.light_history.remap(frame_index, &lights[..count])`). The per-frame render path is supposed to be `FxHashMap` end-to-end. Introduced by `186234944`; Dim 2 or Dim 3 / `/audit-performance` owns it.

### Dim 12 — Cross-dimension observations (not filed here)

- `de808add3` committed `screenshot_1790471263.png` (167 KB) to the repo root. It is still tracked. This is /audit-tech-debt territory.
- Cornell's skinned-receiver `MaterialTextureHandles` (`cornell.rs:856-857`) uses `parallax_height_scale: 0.0, parallax_max_passes: 0.0`. The two #4552 sites use the named defaults. It is inert with no height map bound, but the inconsistency is not covered by the hygiene scan, which only looks for the 0.04/4.0 literals.

## Appendix B — Verified OK (regression-guard list, per dimension)

### Dim 1

- **Contract, TLAS side.** `build_tlas_instances` packs `vk::Packed24_8::new(ssbo_idx, shadow_mask)` from `instance_map[i]`. The 24-bit `debug_assert!` sits at the truncation site, and `MAX_INSTANCES < (1 << 24)` is const-asserted in `scene_buffer/constants.rs`.
- **Contract, map side.** `begin_frame_recording` runs `grow_instance_ssbos(frame, draw_commands.len())` *before* `build_instance_map(…, instance_map_cap(instance_capacity(frame)), |i| mesh_registry.get(…).is_some())`.
- **Contract, SSBO side.** The `build_and_upload_instances` loop `continue`s on the same `mesh_registry.get()` miss. `debug_assert_eq!(gpu_instances.len().min(mapped_cap), instance_map.iter().flatten().count())` pins the two sides (#2913, #4833 clip). Nothing mutates `mesh_registry` between the map build and the SSBO loop: only `dispatch_skin_and_cluster` sits between them, and it does not drop meshes.
- **SSBO tail.** The UI instance and the ground-cover model tier (`record_groundcover_models(…, gpu_instances.len())`) sit strictly after every TLAS-referenced index.
- **UPDATE bookkeeping.** The UPDATE decision uses `needs_full_rebuild`, `last_blas_map_gen`, the `last_blas_addresses` zip, `last_entity_ids` equality and `instance_count == built_primitive_count` (VUID-03708). The bookkeeping is committed after `cmd_build_acceleration_structures` records (#2674). `invalidate_tlas_recording` runs on all three tail `Err` sites. The instance array is exactly `instance_count`, and slot padding exists only in `max_instances`.
- **Build flags.**
  - Static: `STATIC_BLAS_FLAGS` = `PREFER_FAST_TRACE|ALLOW_COMPACTION` at both size-query and record, and `BlasEntry.built_flags`.
  - Skinned: `SKINNED_BLAS_FLAGS` = `PREFER_FAST_BUILD|ALLOW_UPDATE` at BUILD, refit and `built_flags`, with `validate_refit_flags` before UPDATE.
  - TLAS: `UPDATABLE_AS_FLAGS` = `PREFER_FAST_TRACE|ALLOW_UPDATE` at size-query and build.
  - These match the memory-budget.md flag table.
- **Geometry.** `R32G32B32_SFLOAT` @ offset 0 with stride `size_of::<Vertex>()` (static) or `SKIN_OUTPUT_STRIDE_BYTES` (skinned), `UINT32` indices (mesh-local, so the byte-offset global-pool source is correct), `GeometryFlagsKHR::OPAQUE` on every triangle geometry.
- **Transform.** `column_major_to_vk_transform` emits rows `(m0,m4,m8,m12)…`. `tlas_instance_transform` returns identity for `bone_offset != 0`. `TRIANGLE_FACING_CULL_DISABLE` is gated on `draw_cmd.two_sided`, and its comment is corrected (#4580 / #4801).
- **Instance mask.** `shadow_mask_for_instance` has a single precedence chain: refractive glass (incl. MLP with `multi_layer_refraction_scale > 0`) → GLASS; `EFFECT_SHADER` / `FIRE_REFRACTION` / `is_legacy_fx_card` (blended ∧ (kind 102 ∨ `dst_blend == 0` = Gamebryo ONE)) → EFFECT; Actor → DYNAMIC_ACTOR; else layer bucket. `mask_divert_cause` shares `is_legacy_fx_card`.
  - `dst_blend` defaults are 7 (`static_meshes.rs` `(6, 7)` fallback, `ImportedMaterial` / `MaterialInfo` defaults), so non-additive blends never falsely hit the additive arm.
  - Baseline REN-D1-2026-09-21-01 (#4576) and REN-D1-2026-09-24-02 (#4834) hold. REN-D1-2026-09-21-03 (#4581) is fixed: `telemetry.rs` now carries the retired-note comment.
- **Global-pool BLAS sources vs `7e9da5dcc` / `5226d73e2`.**
  - `restore_missing_static_blas_for_draws` takes global-only sources from `global_vertex_buffer` / `global_index_buffer` plus the published `global_vertex_offset` / `global_index_offset`. Both belong to the bound generation during a chunked rebuild, because offsets publish at swap-in in `advance_geometry_rebuild`.
  - Latecomers appended after a plan are refused by `scene_geometry_resident(…, deferred_compaction.mesh_count)`.
  - Restore builds are synchronous (`submit_one_time` + fence), so no GPU read of the old generation outlives the call.
  - Built BLAS carry no reference to their source buffers, and static BLAS are never refit.
  - The in-place `copy_within` path is correct: survivors are checked ascending and disjoint, and aliased or unordered layouts fall back to the allocating copy. Its output equals the old path's, and it runs only when `geometry_rebuild.is_none()`.
- **Shrink wiring.** `shrink_tlas_to_fit` / `shrink_tlas_scratch_to_fit` run at the `draw_frame` tail. `shrink_blas_scratch_to_fit` runs at `cell_loader/unload.rs` and at `recreate_swapchain_core`.
- **BLAS budget.**
  - `recompute_blas_budget_for_current_state` is the last step of the resize (`resize.rs`), after `upscaler.recreate`, and bills `sdk_memory_bytes`.
  - Init re-derives twice: with SDK 0, then again after `FrameUpscaler::new`.
  - `set_upscaler_mode` goes through `recreate_swapchain`, so a mode switch re-derives too.
  - `blas_budget_is_recomputed_after_the_upscaler_catches_up` pins the ordering (run by Dim 5).
- **`rt.integrity`.** The chain is `AccelerationManager::integrity_snapshot()` (`tlas.rs`) → `VulkanContext::fill_rt_integrity_stats` (`context/telemetry.rs`), called per frame from `app_events.rs` → `RtIntegrityStats::verdict` (`tlas_emitted == tlas_eligible`, all three `missing_*` zero, `tlas_build_succeeded`, `rt_flag`).
- **Stale TLAS after a failed build (#4779 / #4851).** Compute ray queries go through `ray_query_tlas` (build-gated). `water.frag` and `caustic_splat.comp` gate on `sceneFlags.x`. The AS_BUILD → FRAGMENT|COMPUTE barrier is unconditional on both arms. The pre-TLAS AS_WRITE → AS_READ frame-scope barrier is gated only on `accel_manager.is_some()`.
- **`StaticBlasWorkingSet` stamp vector (`88c23887b`).** The wrap is handled (`generation == 0` → `fill(0)`, restart at 1). `can_evict` still requires both non-membership and the idle window.
- **Hot-path hashing.** `skinned_blas` is `FxHashMap` (#4000 holds).
- **Documented ceilings.** (a) The `--grid` burst false-eviction note ("Deferred pending a `--grid` + low-VRAM-budget repro") is still present in `blas_static.rs`. `restore_missing_static_blas_for_draws`' doubling chunks also bump the shared `frame_counter` once per call. That only ages non-working-set entries, and deferred destroy (not the idle window) is the lifetime guarantee. Not quantified, so not filed. (b) The shared skinned scratch still serialises.

### Dim 2

- **SSBO chain**:
  - `rayQueryGetIntersectionInstanceCustomIndexEXT` indexes `instances[]` at every hit site; no `InstanceId`/`gl_InstanceID`.
  - Raster uses `gl_InstanceIndex` (`triangle.vert`, `water.vert`) and `fragInstanceIndex` → `instances[]`.
  - `materials[inst.materialId]`.
  - Vertex/index reads use `VERTEX_STRIDE_FLOATS` and the `VERTEX_*_OFFSET_FLOATS` constants. `VERTEX_BONE_INDICES_OFFSET_FLOATS` (12) and `VERTEX_BONE_WEIGHTS_OFFSET_FLOATS` (16) are generated in `shader_constants_data.rs` and match `Vertex` (`[f32;3],[f32;4],[f32;3],[f32;2],[u32;4],[f32;4],…`).
- **Skinned hits**: `getHitTriWorldPositions` reads skin output with mesh-local indices (no `+ vOff`), while bind-pose attributes use `vOff`. The rigid branch lifts to absolute with `+ renderOrigin`.
- **`b9e961eeb` posed frames**: `getHitVertexTransform` blends the current `bones[]` (set 1 binding 3, now `VERTEX|FRAGMENT` in `SceneBuffers` layout) with the same weight/index lanes and `MAX_BONES_PER_MESH` clamp as `triangle.vert` and `skin_vertices.comp`.
  - The inverse-transpose predicate differs between raster (`wsum >= 0.001 || NON_UNIFORM`) and RT (`boneOffset != 0 || NON_UNIFORM`). It only differs on orthogonal frames, where the direction after `normalizeHitDirection` is identical.
  - The `fragNormalTransform` varying (locations 11–13) is consumed only by `triangle.frag` MSN.
- **RT gate**: `rtEnabled = sceneFlags.x > 0.5 && …` feeds `directShadowRayEnabled`, `giRayEnabled` and `reflectionGlassRayEnabled`. The window portal (`isWindow && reflectionGlassRayEnabled`), glass reflect/refract, fire-refraction (`discard` when disabled), ReSTIR and GI all sit behind these. `water.frag` ray helpers early-return on `sceneFlags.x < 0.5`; the caustic block is gated `sceneFlags.x >= 0.5`; `groundcover_blade.frag` is gated.
- **Origins/tMin**: every `rayQueryInitializeEXT` uses a scale-aware `offsetRayOriginForDirection` origin with tMin 0. Shadow rays use `TerminateOnFirstHit` (`traceShadowBinary`). Shading rays resolve the closest hit (no TOFH). The portal origin now uses `N_bias`, so its bias side agrees with `throughDir`.
- **#4832 portal**: `V = normalize(cameraPos.xyz - fragWorldPos)` points toward the camera and `N_bias` faces the viewer, so `windowFacing = dot(N_bias, V) ≥ 0` and `-N_bias` leaves the pane away from the viewer. Correct.
- **Regression checks on the baseline LOWs**: #4582 holds (`legacyArm = !useRestir && !viewRestirLight`, legacy arm compiled out by default), and #4583 holds (`facing < 0.0 && frontFacing`).
- **Light-identity remap (`186234944`)**:
  - `previousLightToCurrent[MAX_LIGHTS + 1u]` sits at offset 16 in all four `LightBuffer` mirrors (`bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp`, `volumetrics_inject.comp`), and `LightHeader` matches.
  - The sentinel `restirY = 0xFFFFFFFF` masks to 1023 = `MAX_LIGHTS` and is rejected by `remapReservoirLight`; `INVALID` fails `< lightCount`.
  - Ambiguous or duplicate identities map to INVALID.
  - Current reservoirs are cleared per frame, so a stale two-frame index cannot be remapped with the one-frame map.
- **Zero-filled reservoir readers (Dim 3 lead)**: `ReservoirBuffers::begin_frame` does `cmd_fill_buffer(curr, 0)`, and create/resize also zero-fill, so an unwritten entry decodes to light index 0 (after `remapReservoirLight`: `previousLightToCurrent[0]`, possibly a real light), surface 0, M = 0, W = 0, histLen 0. The only readers are the two `reservoirsPrev[]` reads in `triangle.frag`:
  - Temporal selection requires `rp.M > 0.0 && rp.W > 0.0`.
  - Temporal radiance reuse requires `rpHistLen > 0.0` (zero-fill packs 0) and never uses the light index.
  - Spatial reuse requires `rn.M > 0.0 && rn.W > 0.0`.
  - The `lights[idx]` read in those conditions comes after `idx < lightCount`, so it is in bounds and has no effect.

  A written reservoir with M > 0 always has a valid `restirY`, because the first streamed candidate is always selected (`u * wSum < w_i` with `wSum == w_i`). No path consumes the index when M == 0. **Not filed.**
- **`0925f7926` radiance cache**: every candidate arm (fresh, temporal, spatial) evaluates `shadowableLightRadiance` with identical arguments at the current surface. The cached `restirSelectedRadiance` equals the removed recompute.
- **ReSTIR reuse**: rejection on the 25° geometric-normal cone (`TEMPORAL_NORMAL_COS` / `SPATIAL_NORMAL_COS`, `octEncode(fragNormalEffective)` in `pad0`). The surface tag is `inst.surfaceId & RESERVOIR_SURFACE_MASK`. The half-float depth clamp is `min(worldDist, 65504.0)` on both gates.
- **Sky cube**: no direct `skyCube` sample outside the helper. All exterior escape sites route through `exteriorSkyRadianceOr` / `exteriorSkyDiffuseOr`, and all non-portal sites additionally gate on `jitter.w` (see REN-D2-2026-09-27-04). The GI miss (`pathEnvironmentRadiance`) returns finite values, with no NaN/inf path found.
- **Coordinate spaces**:
  - `fragWorldPos = fragWorldPosRel + renderOrigin.xyz` at the top of `main`. Every `dFdx`/`dFdy` consumer reads `fragWorldPosRel`: flat normal, derivative TBN, splat `splatPosdx/dy`, `rtFootprint`, POM via `perturbNormal(…, fragWorldPosRel, …)`.
  - `#3980` still holds: `weatherGroundNoise` uses the integer `byroGcHash1`.
  - No new `sin()`/hash of absolute world coordinates in the delta.
- **Depth**: `be26769b7` replaced `depthLinearize(z, n, f)` with `depthViewSpace(invViewProj, ndcXY, z)`. It is mapping-free and translation-invariant (the w row of `inv(P)` is unaffected by the view translation), so there is no hand-restated convention.
- **Glass**: Frisvad basis (`math_common.glsl`); `triangle_frag_keeps_glass_identity_and_ior_across_rt_lods` passes.
- **`ab255cfd2` viz gating**: no `dbgFlags & DBG_VIZ_*` remains in `triangle.frag`, `water.frag` or the includes; every viz bit reads `vizFlags` (legacy-debug-gated).
- **Early-test variant**: `allows_early_fragment_tests` excludes alpha-blend, alpha-test (`alpha_threshold == 0`), non-zero `material_kind` (fire refraction, effect), decals and wireframe. The remaining `discard`s in `triangle.frag` cannot fire on early-eligible draws. `triangle_early.frag.spv` reproduces from `-DBYRO_OPAQUE_EARLY_TESTS=1`.

### Dim 3

- `GpuLight` 80 B:
  - `gpu_light_is_80_bytes` passes.
  - The 4 GLSL copies are identical and match Rust name, order and type (`gpu_light_glsl_copies_stay_in_lockstep`).
  - The `NoUninit` SAFETY (`descriptors.rs`) is updated.
- `LightHeader` is `#[repr(C)]`, and `previous_to_current` sits at offset 16. The total is 4112, which is 16-aligned, so `lights[]` lands at 4112 in std430 (`history_header_has_exact_std430_offsets`).
- Every light-buffer descriptor range comes from `SceneBuffers::light_buffer_size()`: `compute.rs` (cluster), `caustic.rs`, `post_passes.rs` (volumetrics), `init.rs` and `resize.rs`. No stale hard-coded range exists.
- The `upload_lights` dirty gate hashes the remap as well as the lights (`same_current_lights_can_need_a_different_mapping`). `commit` runs on both the skip arm and the upload arm. The shader reads the remap only for `oldIndex < MAX_LIGHTS`, and slot 1023 is the sentinel.
- The `history_id` producer (`render/lights.rs` `collect_lights`) gives authored lights `[entity, 1, 0, 0]`, the directional `[0, 2, 0, 0]` and procedural lights `[0; 4]`. `World` never reclaims `EntityId`s (`world.rs` `spawn`), so a despawned lamp's id cannot alias a new one. Ambiguous (duplicated) identities are rejected.
- `GpuCamera`'s `sky_tint.w` (sun angular radius) and `exterior_sky_tint.w` (sky-cube ready flag) are documented consistently. The rustdoc, `shader-pipeline.md` rows 272/352 and every `CameraUBO` mirror comment agree. Readers are `triangle.frag` (`skyTint.w`), `bindings.glsl` and `lighting.glsl` (`exteriorSkyTint.w`).
- `BoneBuffer` (set 1 binding 3) moved into `bindings.glsl`, and the layout grants `VERTEX | FRAGMENT` (`bone_palette_is_visible_to_primary_and_secondary_hit_shading`). Reflection tests pass for the triangle and water layouts.
- The instance-SSBO grow path is `grow_instance_ssbos` → `ensure_instance_capacity`. It rewrites scene-set bindings 4 and 18 and retires the old pair through `retired_instance_buffers`.
  - Only `CausticPipeline::rebind_instance_buffer` needs a rebind.
  - Volumetrics (`post_passes.rs:647`) and the ground-cover model tier (`dispatch_skin_and_cluster.rs:546`) re-read `instance_buffers()[frame]` every frame.
  - Resize and init pass the live buffers. This confirms 09-26 premise item 8.
- The grow runs twice per frame: first in `begin_frame_recording`, before `instance_map_cap(instance_capacity(frame))` feeds the TLAS map (#4833), then in `build_and_upload_instances` with `gpu_instances.len() + model_tail`. Both run after the fence wait.
- The capacity constants match the skill: `MAX_INSTANCES = 0x40000`, `INITIAL_INSTANCE_CAPACITY = 0x10000`, `MAX_INDIRECT_DRAWS = MAX_INSTANCES` (eager by design, #4615), `MAX_MATERIALS = 16384`, and `MAX_LIGHTS = RESERVOIR_LIGHT_MASK = 1023`. The generated `#define MAX_LIGHTS 1023u` is what the GLSL header arrays use.
- `upload_materials` asserts `len <= MAX_MATERIALS`, uploads the unique slice length and uses an FxHash dirty gate. `material.rs` is unchanged in the window, and the dedup guards pass.
- `GpuMaterial` stays 432 B, all-scalar with no padding. `gpu_material_size_claims` finds no stale size in docs or `.claude/commands`.
- Neutral values:
  - Tint multiplies only when `TINT_ALPHA_WEIGHT_BIT` is set, on both the raster path (`triangle.frag:1423-1429`) and the secondary hit (`ray_hit.glsl:530`).
  - Detail divides by `max(mat.detailNeutral, 1e-4)` on both paths.
  - The only `detail_neutral: 0.0` literal (`draw.rs` `is_caustic_source_tests`) is a test fixture.
- Generated constants in the delta are wired through `build.rs` → `shader_constants.glsl`, with value pins in `shader_constants.rs`:
  - `VERTEX_BONE_INDICES/WEIGHTS_OFFSET_FLOATS` are derived from `offset_of!`. The build script's private `Vertex` mirror is cross-checked against the crate `Vertex` by the value-pin test (`shader_constants.rs` ~l.965).
  - `TONEMAP_OP_*` (#4584 holds).
  - `MAX_COMPOSITE_SKY_APERTURES`, which `composite.rs` uses as `MAX_SKY_APERTURES`.
- `groundcover_models.comp` now preserves the host-packed `GcModelShape.flags` bits and only substitutes the non-uniform-scale bit. The #4847 fix holds.
- The `DBG_*` mask still tops out at bit 31 (`DBG_VIZ_SELECTED_LIGHT = 0x80000000`), and no new `DBG_` bit was added in the window.

### Dim 4

Frame order and infallibility:
- **Frame order** (`draw_frame`): `sync_and_acquire_frame` → `begin_frame_recording` → `assemble_camera_and_lights` → `dispatch_skin_and_cluster` → `build_and_upload_instances` → `record_groundcover_models` → `record_geometry_pass` → probe `FRAGMENT→HOST` barrier → `copy_depth_to_history` (conditional) → `depth_capture_record_copy` → `record_post_passes` → egui → `screenshot_record_copy` → #4602 flush edge → `end_command_buffer`.
- **Post-pass order**: `record_post_passes` runs svgf → caustic → volumetrics → ssao → composite → bloom → exposure meter → taa → upscale → presentation. It matches doc steps 10–20.
- **Post-pass helpers are infallible**: `record_post_passes` and all ten `record_*_pass` helpers return `()`, and `record_groundcover_models` returns `()` too.
- **Tail `Err` sites**: all three (`end_command_buffer`, `reset_fences`, `queue_submit`) run `rollback_skin_frame_state` and recreate `image_available`. `note_frame_submitted`, `promote_skin_frame_state`, `mark_frame_completed` and `take_submitted_dispatch` run only after `queue_submit` returns `Ok`.

Fences, semaphores and cross-frame state:
- `MAX_FRAMES_IN_FLIGHT == 2` is const-asserted.
- The wait is all-slots: `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)`, pinned by `the_all_slots_wait_argument_is_pinned`.
- `render_finished[img]` is per swapchain image (`render_finished_is_sized_and_indexed_per_swapchain_image`).
- Rider list 9–13 names the model tier and the skin resources.

Model tier (`GroundCoverModelTier::record`):
- The WAR pre-barrier covers `DRAW_INDIRECT|COMPUTE|TRANSFER` reads of the shared slab, counters and draws.
- There is a COMPUTE→COMPUTE barrier between phases, and EMIT → `DRAW_INDIRECT|VERTEX|FRAGMENT|TRANSFER`.
- The stats copy is covered by the #4602 edge.
- The TLAS comes from `ray_query_tlas(frame)`, pinned by `compute_ray_query_passes_take_the_build_gated_tlas`.
- The tier is created only when `indirect_draws_supported()` (`init.rs`).
- It draws through the late opaque pipeline with trackers updated, so it never runs under early tests with alpha-tested cards.

In-frame copies and clears:
- **Dynamic RGBA**: `fence_confirmed_idle_slot == Some(frame)` is `ensure!`d; staging is per-FIF; copies touch a single mip (`can_update_rgba` implies 1-mip RGBA); every copy returns to `SHADER_READ_ONLY`, so an abandoned recording leaves the layout valid; the updates are acknowledged only by `note_frame_submitted`; `release.rs` drops the queued updates.
- **ReSTIR `begin_frame`**: it covers the previous frame's FRAGMENT read of this slot's curr buffer (WAR) and the writes to prev (RAW). `recreate_on_resize` recreates and zeroes the buffers, and scene bindings 16/17 are rewritten.

Pipelines and G-buffer:
- **Early-test pipeline**: same create-info as opaque with only the fragment stage swapped; partial-failure cleanup is present.
- **`allows_early_fragment_tests`** excludes alpha blend, alpha test, material kinds ≠ 0, wireframe and decals, and requires z_test/z_write with z_function 1|3. Every `discard` in `triangle.frag` sits behind one of the excluded conditions.
- `pipeline_early` is rebuilt on surface-format change (`recreate_swapchain_core`) and destroyed in `destroy_render_pass_pipelines`.
- **Write masks**: water enables 0/4/6/7, matching its outputs at locations 0/4/6/7. The ground-cover blade enables 0/2/5/6/7 (mask `R|G|B` on B10G11R11), pinned by `draw_write_masks_match_each_fragment_shaders_outputs`.
- **Formats**: triangle's eight outputs match the `gbuffer.rs` formats (RG16_SNORM normal, RG16F motion, R32_UINT mesh ID, B10G11R11 ×2, R8 ×2).
- **Mesh ID**: the RP-1 `MAX_INSTANCES` overrun keeps its one-shot `log::error!` plus clamp.

Descriptor validation, push constants and dynamic state:
- **`validate_set_layout`** runs in every one of the 20 layout builders: texture_registry, scene_buffer, bloom, caustic, composite, compute, exposure_meter, groundcover (3 contracts), groundcover_bench, groundcover_models, presentation, skin_compute, sky_cube (+ filter and irradiance), ssao, svgf, taa, volumetrics/init, water.
- **Push constants**: the model tier's `GcModelPush` is 64 B in GLSL, matching `size_of::<ModelPush>() == 64`.
- **Timestamps and queries**: geometry-phase timestamps are recorded inside the render pass with host query reset. The pipeline-statistics query is begun and ended in the same subpass on every path, and an unwritten end is gated out by `active_bits`.

Pipeline cache:
- `validate_pipeline_cache_header` checks length, headerSize, version, vendor, device and UUID. A mismatch warns and falls back to an empty cache (12 tests).
- The cache is saved at teardown, and mid-session through `save_pipeline_cache_if_grown` when `created > 0` blend variants were built this frame.

Resize:
- `recreate_swapchain` recreates the swapchain; depth and depth-history (scene binding 15 rewritten); G-buffer; SVGF; reservoirs (16/17); caustic; water-caustic (set 2 rebound, falling back to a sink); bloom; volumetrics (whole pass); composite and egui; TAA and presentation; SSAO and AO binding; and the BLAS budget last.
- Framebuffers are built after `recreate_for_swapchain`.
- The exposure meter and the model tier rewrite their descriptors on every dispatch.
- `fog_cluster_dirty_range` is per-FIF and resets with the volumetrics rebuild.
- `a_surface_format_change_rebuilds_every_main_pass_pipeline` still covers render pass, triangle, water and ground-cover.

Acceleration-structure inputs:
- Build-input barriers use `SHADER_READ`: skin → AS_BUILD in `skinned_blas_refit.rs`, and TLAS instance copy → AS_BUILD in `tlas.rs`.
- `ACCELERATION_STRUCTURE_READ_KHR` is used only for AS→AS serialisation and for AS → shader reads.

### Dim 5

- **Two-phase publish.** `plan_geometry_compaction` still only fills `CompactionPlan.offsets`. Mesh offsets change only in `apply_compaction_plan`, which has three callers: the empty-pool early return, the synchronous pre-atomic call, and `advance_geometry_rebuild` swap-in via `deferred_compaction.take()`. The pools become final in the plan in both the old and new code.
- **In-place aliasing.** `in_place` requires consecutive survivors (slot order) to satisfy `v0.end <= v1.start && i0.end <= i1.start`, i.e. globally ascending and disjoint. By induction the write cursor stays ≤ each source start, so each destination ⊂ [0, src.end) and never reaches a later source. `copy_within` is memmove-safe for the self-overlap. `truncate(v_write)` equals the survivors' total exactly (no off-by-one). The zero-survivor and zero-length-range cases degrade correctly. An out-of-range offset would panic identically in both branches (unchanged behaviour).
- **Readers of the pools between plan and publish.** `geometry_sharing.rs` (`register_scene_geometry_for_sharing_with_fingerprint`, `acquire_matching_geometry_with_fingerprint`) and `geometry_residency.rs` all early-out while `deferred_compaction.is_some()`. `scene_geometry_resident` holds out slots at or past `CompactionPlan.mesh_count`. The chunk copier reads linear ranges that are final after the plan.
- **In-flight frames.** Compaction mutates CPU `Vec`s only. The bound old SSBO is untouched until swap-in and then goes through `deferred_destroy` with `DEFAULT_COUNTDOWN`. Chunk copies write only into the unbound new generation, and bindings 8/9 are re-pointed per frame in `draw_frame`.
- **Invariant source.** `upload_scene_meshes_batched` and `accumulate_global_geometry` append geometry and `meshes.push` in the same order, and the batch rollback `truncate`s without creating slots. Survivors are therefore ascending in production, and the in-place branch is the one that normally runs.
- **VRAM gate (`geometry_rebuild_needs_idle`).**
  - Units are bytes throughout: `projected_bytes` from element counts × sizes, `heapUsage` / `heapBudget` sums.
  - Integer `approaching_oom_line = (b / 5) * 4`, and `saturating_add` is used.
  - `None` (no extension) keeps the exact pre-change 256 MiB rule. A zero budget reading makes the gate idle (conservative).
  - The reading is fresh and unthrottled via `VulkanContext::live_memory_budget`, taken only when no rebuild is in progress.
  - `VK_EXT_memory_budget` is enabled at device creation when present (`device.rs`, `caps.memory_budget_supported`), and the query cannot fail (it is a void call).
  - A failed duplicate allocation now always reclaims (`reclaim_before_rebuild = true`) before the atomic build.
- **Summing DEVICE_LOCAL heaps.** On devices with a small separate DEVICE_LOCAL BAR heap (AMD without ReBAR, 256 MiB), the sum overstates the geometry heap's line by ≤ 0.8 × 256 MiB. That is still below the real heap budget for any heap ≥ ~1 GiB. Approximate but safe; not filed.
- **Chunked-path teardown.** `MeshRegistry::destroy_all` destroys an in-flight job's `new_vertex_buffer` / `new_index_buffer` and releases `geometry_staging_pool`, dropping its `Arc`.
- **New owners since the baseline.**
  - `pipeline_early` is destroyed in `Drop` and in resize (`destroy_render_pass_pipelines`). Partial pipelines are destroyed when `create_graphics_pipelines` fails, and the shader modules unwind.
  - The `fragment_invocation_pools` query pools are destroyed in `GpuPerFrameTimers::destroy`, and a partial create failure destroys the pools already made.
  - `LightHistory` is CPU-only and bounded by `MAX_LIGHTS`.
  - No new `GpuBuffer`, `GpuImage` or `Allocation` owner appeared in the window.
- **`record_dds_upload` unwinding (item 14a).** The create → allocate → bind → view arms each unwind before any command is recorded. The staging guard is returned to the caller after recording.
- **Allocator `Arc` holders.** No new holder of an `Arc<Allocator>` clone was added.

### Dim 6

- **No per-game branch or per-frame classifier on the render path.** There is no `GameKind`/game check in `render/static_meshes.rs`, `render/skinned.rs` or `vulkan/material.rs`. `classify_pbr_keyword`/`resolve_pbr` are called only inside `translate_material` (and the `mat.*` console command).
- **EmissiveSource.** The renderer reads resolved `emissive_color`/`emissive_mult`/`effect_shader_flags` off `Material`; `emissive_source` has no render-side consumer.
- **Bit 31 masking.** `resolveRayHitUV` (`ray_hit.glsl`), `parallaxDisplaceUV` (`material_sampling.glsl`), triangle.frag's gloss (`NORMAL_ALPHA_SPEC_BIT`) and tint (`TINT_ALPHA_WEIGHT_BIT`) samplers and the material-role view all mask before indexing or before the "bound?" test. The CPU writes `PARALLAX_ALPHA_HEIGHT_BIT` at exactly one site (`render/static_meshes.rs`), and `particles.rs` writes 0.
- **Both POM marchers' TBN.** Both use `frameN` flipped to face the viewer and `B = sign·cross(frameN, T)`, so raster and secondary-ray parallax agree on back faces. The skinned secondary-hit tangent uses the bone `mat3` (not the inverse-transpose), which is correct for tangents (`getRayHitTangentFrame`, `b9e961eeb`).
- **Flipbook.** `apply_texture_flip_roles` returns `(normal_has_alpha, height_has_alpha)` from the active frames (#4301/#4528), and both `normal_alpha_spec_binding_applies` and the parallax bit or slot-zeroing (#4260) key on them. `parallax_height_in_alpha` still blocks the double read (#3567).
- **`normal_has_alpha` source.** It is taken from the bound DDS in the one shared producer (`asset_provider/texture.rs` `handle_has_alpha(texture_handles.normal)`), so #4636's dead-NIF-path swap cannot desync it.
- **MSWP.** `resolve_mesh_paths_with_pre_merge` re-runs `merge_external_material` for the swap target over the pre-merge snapshot (#4290), carrying every scalar. Snapshots are empty only for precombines, `.spt` placeholders and the provider-less (non-FO4) path.
- **BGSM merge.**
  - Envmap fill is gated on `base.environment_mapping` in both the merge and `RefrTextureOverlay::fill_from_bgsm` (#4428).
  - The BGEM arm gates envmap on `env_mapping_enabled()`.
  - The chain-local sentinels are intact.
  - Scalars without a sink are recorded in the #2704/#4667 ledger.
- **`.btr` MSN (#4632).**
  - `translate_texture_only_material_with_authored_msn` is followed by `resolve_msn_z_source`.
  - The Skyrim `.btr` `_n.dds` is DXT5 with fully opaque alpha (all 4,096 mip-0 blocks), so the normal-alpha-as-spec binding it also triggers multiplies spec by 1.0.
  - The Skyrim `.bto` object-LOD normals are not model-space (`msn_basis_probe`), so the absence of `resolve_msn_z_source` in `object_lod.rs`/`placement_lod.rs` is inert there. Those LOD paths also attach no `MaterialTextureHandles`.
- **Tangent import.** No delta in `tangent.rs`. #786 "read the bitangent half", Z-up→Y-up on T with N, and `bitangent_sign(n, t, b)` hold on all three paths. `1ab08644a` is a pre-size-only perf change. `perturbNormal` is on by default with the `DBG_BYPASS_NORMAL_MAP` opt-out and the `dot(Tproj,Tproj) < 1e-8 → N` guard.
- **Particles.** `apply_emitter_overlays` is the single overlay boundary, called by both `cell_loader/spawn.rs` `spawn_particle_emitters` and `scene/nif_loader.rs` `spawn_nif_particle_emitters`. `render/particles.rs` reads only `ParticleEmitter` / `TextureHandle`.
- **Ground cover.** Its exemption is recorded in `exal-groundcover.md` and nifal.md §3. The authored-cover model tier spawns through `cell_loader::spawn::authored_cover` → the normal mesh path, so it is inside the guard.
- **Detail neutral (#4422).** It flows `Material.detail_neutral` → `GpuMaterial.detailNeutral`, and both raster and `ray_hit.glsl` divide by it. The `0.5` literals in `water.rs`/`context/mod.rs` are test-only.

### Dim 7

- SVGF history ping-pong: `write_descriptor_sets` binds `indirect_history[prev]` and `moments_history[prev]` as history, and `[f]` as output, with `prev = (f + MFIF − 1) % MFIF`. `mesh_id_views[prev]` and `normal_views[prev]` follow the same scheme. `advance_completed_frames` only bumps dispatched slots.
- Motion vectors: `triangle.frag` `outMotion = (currNDC − prevNDC)·0.5` uses the un-jittered `fragCurrClipPos` (jitter is added to `gl_Position` afterwards in `triangle.vert`). SVGF, TAA and ReSTIR all consume it as `prevUV = uv − motion`. `origin_corrected_prev_view_proj` handles render-origin crossings, and a camera cut uploads `pvp = vp` (zero motion) plus `signal_temporal_discontinuity(8)`.
- Mesh-ID disocclusion: opaque draws write `inst.surfaceId` (= `entity_id + 1`, `build_and_upload_instances.rs`). Blended draws write the sorted index with `MESH_ID_NO_HISTORY_BIT`. `stableMeshIdsMatch` rejects either side carrying the bit. The SVGF temporal pass, à-trous and TAA all early-out or bypass on no-history IDs.
- Firefly clamp: the 3×3 spatial mean + 3σ clamp runs before `if (hasHistory)`, so the no-history path writes the clamped `currInd` (REG-07 / #1481 invariant holds). The only unclamped write is the sky / alpha-blend early-out, which carries ≈0 GI.
- `params.w` decision: `next_svgf_temporal_alpha(recovery, caustic_history_valid)`, `progressive_accumulation = scene_static && !recovering` (#3995). `caustic_history_valid = camera_static && caustic_scene_static` (see REN-D7-2026-09-27-02 for the gap).
- Dispatch coverage: temporal and à-trous use `width/height.div_ceil(WORKGROUP_X/Y)` with in-shader bounds returns. `ATROUS_ITERATIONS = 3`, odd, const-asserted so the final slot is 0, with steps `1 << k`. There is one `cmd_svgf_start/end` bracket around `svgf.dispatch`, which records temporal + all à-trous iterations.
- Composite order (`composite.frag` main):
  - geometry arm: `combined = direct + indirect*albedo + caustic`;
  - sky/translucent-over-sky arm: `sky*(1-coverage) + direct + skyIndirect*skyAlbedo`;
  - then `combined*vol.a + vol.rgb`, then beyond-grid `combined*transmittance + aerial`;
  - then underwater shafts and precipitation, then a ±1/2048 linear dither;
  - linear HDR out; no tone map or exposure.
- Caustics: glass is `texelFetch(causticTex, ivec3(..,layer))` over 3 layers (binding 5, usampler2DArray). Water is `texelFetch(waterCausticTex, ..)` (binding 8), gated by `caustic_flags.x` (#2508). Both are divided by `CAUSTIC_FIXED_SCALE`, capped at 16, multiplied by albedo, and added as their own term (not into denoised indirect).
- SSAO: `ssao.comp` `NUM_SAMPLES = 16`. The sole reader is `triangle.frag` (`aoTexture`, floor 0.20), and it multiplies `indirectLight` only. Composite has no AO binding.
- `auxiliaryAlpha`: `triangle.frag` writes `outRawIndirect` / `outAlbedo` alpha = `isAlphaBlend ? finalAlpha : 1.0`.
- TAA only in taa mode: `init.rs` builds `TaaPipeline` only when `renderer_config.upscaler == UpscalerMode::Taa`, and `resize.rs` builds it only in the switch-to-Taa and rollback arms.
- TAA placement and consumption: TAA resolves `composite.scene_view` post-bloom and post-meter (`record_post_passes` order: svgf → caustic → volumetrics → ssao → composite → bloom → exposure meter → taa → upscale → presentation). `record_upscale_pass` blits TAA's `output_image(frame)` in GENERAL when `taa_resolved`.
- TAA internals: history per FIF in GENERAL; 3×3 YCoCg mean ± 1.5σ, with γ = 0 soft clamp on sky disocclusion; flat α = 0.1; octahedral-normal reject `< 0.85`; luma-weighted blend; NaN history falls back to current.
- `taa_failed` un-jitter: `taa_jitter_gate(taa_present, taa_failed, raw_output_view)` gates the Halton jitter (#1932 / #4513). The FSR arm zeroes jitter via `is_fsr_dispatch_active()`, which includes the raw-output predicate (#3632 jitter half).
- Bloom mip chain: `BLOOM_MIP_COUNT = 5` down / 4 up, `B10G11R11_UFLOAT_PACK32`, half-res seed, `max(1)` per level.
- Bloom filters: the ±0.5-texel bilinear taps land on texel centres, giving an exact 2×2 box (the guard proves it). The soft-knee bright pass applies only when `bright_pass_enable(i)` (level 0).
- Bloom apply and constants: `bloom_apply.comp` does `scene.rgb + bloom*BLOOM_INTENSITY` in place and preserves alpha. `BLOOM_INTENSITY = 0.15` in `shader_constants_data.rs`, matching `shader-pipeline.md`. The only disable path is `record_bloom_pass`'s raw-output early return. Params are uploaded once at construction.
- Composite aperture cull (`e2f99ad55`): `prepare_sky_aperture` projects with `inverse(inv_vp_arr)`, the same matrix `screen_to_world_dir` inverts. It uses absolute `camera_pos` against absolute `center_shape` and subtracts `render_origin` only for the corner projection. Its square bound contains the rectangle (the brute-force ray test proves it). In the shader the reject precedes the rotation.
- #4857 holds: clear-depth `surfaceDistance = +inf`. #4841 (TAA / meter raw-gate asserts) holds.
- No per-frame std `HashMap` in the Dim 7 files. See the cross-dimension lead for one outside them.

### Dim 8

- **Prior fixes hold**:
  - #4773: `record_post_passes` args are now named through `VolumetricsPassInputs`, with `medium_params[3]` ← scale height and `temporal_params[3]` ← coverage.
  - #4775: `TRANSPORT_EXPIRED_DT` writes the empty field.
  - #4776: fixed-point jitter; `view_dir` from the ray.
  - #4781: `fit_froxel_divisor_to_device` applied on init and resize.
  - #4828: the single `volumeProfileKind` clamp to LIGHT_SHAFT; no private `clamp(volume.profile_params.x …)` remains.
  - #4831: `depthViewSpace(invViewProj, …)` / `clip.w` in the caustic gate.
  - #4588 / #4589: the caustic gates.
  - #4728: scroll sign, in both `water.frag` branches and `water.vert` phase A.
- **Froxel grid**: `froxel_extent` derives from the render extent plus `froxel_xy_divisor` / `froxel_z_slices`. Inject dispatches `div_ceil(WORKGROUP_{X,Y,Z}=8)` against `local_size = WORKGROUP_*`, with an `imageSize` bounds early-out. Integrate is 2D `div_ceil` with a Z-march.
- **HG phase**: `medium_henyey_greenstein` clamps `g` to (−0.999, 0.999) (`medium_transport.glsl`). Both lobes route through it.
- **No-emitter interiors**: dust (`INTERIOR_DUST_EXTINCTION_PER_METER`) applies only with local emitters or portal sun. Otherwise `requires_dispatch` is false → `skip_clear_decision` → `record_neutral_frame` clears to (0,0,0,1), which is neutral and non-NaN, and resets transport state.
- **`VOLUMETRIC_OUTPUT_CONSUMED`**: gates the whole `record_volumetrics_pass` body; the pass is skipped, not dispatched-and-ignored.
- **Resize**: `recreate_bloom_and_volumetrics` destroys and recreates the pass after device idle (fresh `TransportFieldState`, dirty ranges, moment latches). 09-23 traced the composite binding-6 rebind, and nothing in the window touched it.
- **Combustion moments**:
  - `sync_and_acquire_frame` (fence wait) → `assemble_camera_and_lights` → `append_combustion_surface_lights` (drain via `latched_drain`, zero, flush) → `record_post_passes` → `dispatch` sets `combustion_moment_dirty[frame] |= combustion_active`.
  - The shader accumulates moments only under `!transportExpired`, so the relaxed dirty latch (#4786) cannot miss a non-zero buffer.
  - The COMPUTE→HOST barrier is at the end of `dispatch`.
  - `COMBUSTION_REACH_CANARY_CULL_RADIUS_BU` (1024) is still applied to the appended lights.
- **`TransportFieldState`**:
  - The empty proof is promoted only in `mark_frame_completed` (submit-gated); `prepare(active)` clears all slots.
  - It is reset in `signal_history_reset`, `record_neutral_frame` and resize.
  - Known-empty frames skip both the reads and the stores of the transport fields, and the slot keeps its submitted expiry write.
  - A failed submit leaves the flag false.
- **Fog cluster upload**: `build_fog_volume_clusters` zeroes every CPU entry count each frame, so uploading `fog_cluster_write_range(current, previous)` clears stale GPU counts. The initial dirty range is (0, COUNT). `write_mapped_at` bounds-checks and flushes the range.
- **`filter_fog_volumes_for_grid`**: `camera_pos` and the fog volume centres are both absolute world. It uses a sphere of `far + radius`, which is conservative for the radial froxel far. Remote-aperture retention is a superset of the cluster build's `sealed_interior && sun_radiates` sweep condition, so it is harmless.
- **`fogRayQualityTier`**: travels in `GpuFogVolumeUpload.count[1]` and is written before the prefix upload, so it is present even with zero volumes.
- **Caustics**:
  - Composite divides `(glass + water) / CAUSTIC_FIXED_SCALE` after promoting to float (#1575).
  - `tune.x` uploads the `CAUSTIC_FIXED_SCALE` symbol (pinned).
  - Source selection is `(flags & INSTANCE_FLAG_CAUSTIC_SOURCE)` (generated, no hex literal).
  - `prev_caustic_scene_key` still folds source placement and light count in `build_and_upload_instances.rs`.
  - `WaterCausticAccum` is cleared by `clear_pre_render_pass` (`clear_general_accumulator`).
  - The caustic term is added beside direct: `combined = direct + indirect * albedo + caustic`, which bypasses SVGF.
- **Composite sky apertures**: `prepare_sky_aperture` projects with `inverse(inv_vp_arr)`, the same matrix the shader's `screen_to_world_dir` uses. Behind-camera and eye-crossing cases fall back to full-screen bounds (pinned by `aperture_screen_bounds_preserve_brute_force_ray_hits`).
- **Water**:
  - `water.frag` writes locations 0/4/6/7, and the blend table enables exactly 0 (`hdr_blend`), 4 (`auxiliary_blend`) and 6/7 (`fsr_mask_max`), with 1/2/3/5 masked.
  - Water draws resolve through `water_instance_slot(wc, instance_map)` and skip on `None`; the per-plane param index stays `water_index`.
  - Fresnel F0 = authored WATR `fresnel` clamped [0.001, 0.20] (default 0.02). RT refract/caustic IOR is the authored `push.timing.w` (#4010).
  - `cmd_set_cull_mode(NONE)` is emitted before the water pass.
  - `sun_direction` is copied into `GpuCamera` every frame (`assemble_camera_and_lights.rs`).
  - Water is excluded from the TLAS (`!draw_cmd.is_water` in `predicates.rs`), so it casts no opaque shadow.

### Dim 9

**Ordering and barriers**
- **Palette before vertices.** `skin_palette.comp` dispatches before `skin_vertices.comp`. `dispatch_skin_and_cluster` records the palette, then `record_skinned_blas_refit`. The palette barrier is COMPUTE/SHADER_WRITE → COMPUTE|VERTEX|FRAGMENT/SHADER_READ over the whole palette buffer, and FRAGMENT is pinned by `palette_publish_barrier_covers_fragment_bone_readers`.
- **Copy barriers.** The bone-world copy barrier is TRANSFER→COMPUTE over `[0, bone_input_upload_bytes)`, the envelope of all sparse regions. The bind-inverse copy barrier is TRANSFER→COMPUTE over `WHOLE_SIZE`.
- **Workgroups.** Both shaders use `local_size_x = SKIN_WORKGROUP_SIZE` (64), with `div_ceil` over the range width (`workgroups_count_only_the_range_width`, `skin_palette_workgroup_size_matches_skin_vertices`).

**Palette dispatch plan (`plan_palette_dispatch`)**
- It clears then fills the persistent scratch, clamps to `bone_count`, sorts, merges in place, and falls back to dense when coverage ≥ the dense range. `a_reused_plan_scratch_keeps_capacity_and_drops_the_previous_plan` pins the reuse.
- **No changed slot outside the plan.** Every writer of palette inputs lands in the plan:
  - pose change → `mark_bone_world_slot_dirty` → the copy region for each frame-in-flight until `promote_bone_world_writes` (submit-gated, #3991);
  - first-sight bind-inverse → `pending_slots` this frame plus `mark_palette_slots_dirty` for the other slot.
- **Slot reuse.** A reused slot drops its `last_pose_hash` in `sweep`, so the new occupant hashes first-sight dirty.
- **Hidden or revealed entities** reallocate (`staged_body_…` test).
- **Slot 0** is seeded pending (`bone_world_slot_states[0] = 0x80`), and bind-inverse slot 0 is seeded identity.
- **Late bind-inverse versus an idle pose.** The upload cap (1366) equals the pool capacity, so a same-frame drain can never be split. A failed upload rolls back the pose commits (`rollback_pending_pose_commits` on `bind_inverse_upload_failed`), so the next frame re-dispatches `skin_vertices` and refits against the landed bind-inverse.

**Skip gate and descriptor caching**
- `should_skip_skin_gpu_refresh` (`> MAX_FRAMES_IN_FLIGHT` clean frames) only engages after both frame slots have refreshed.
- A pending first-sight upload resets the streak.
- `#4829`: `bone_world_buffer_size` is the `GpuBuffer`'s own size (a renderer-lifetime constant), and `PaletteDescriptorKey` includes all three ranges.

**Skinned output buffer and addresses**
- Usage is exactly `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. `VERTEX_BUFFER` is absent, with the rationale comment in `create_slot`.
- `output_address` is cached on `SkinSlot` at create (#3469). Index and scratch addresses are re-queried per refit on purpose.
- MorphSlot delta and weight addresses are cached at create.

**BLAS build and refit**
- **Scopes.** Skin compute → AS_BUILD|FRAGMENT|COMPUTE / SHADER_READ (#1436 input access, #2403, #3582, the last pinned by `every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask`). Before `build_tlas`, AS_WRITE→AS_READ runs unconditionally (#4179). After the TLAS, AS_WRITE → FRAGMENT|COMPUTE / AS_READ runs on both arms.
- **`record_scratch_serialize_barrier`.** The dst mask is `AS_WRITE | AS_READ` (#1790). It is not the WRITE-only regression.
- **Refit matches BUILD.** `validate_refit_counts` and `validate_refit_flags` hold, with `SKINNED_BLAS_FLAGS` shared. Dim 1 verified `built_flags`.
- **Rebuild limit.** `skinned_blas_refit_limit` = `SKINNED_BLAS_REFIT_THRESHOLD` (600) + `entity_id % SKINNED_BLAS_REFIT_JITTER` (60), so `should_rebuild_skinned_blas_after` is correct.
- **Skinned BLAS lifetime.**
  - Skinned BLAS never enter `evict_unused_blas`.
  - `drop_skinned_blas` defers through `pending_destroy_blas` with `DEFAULT_COUNTDOWN`.
  - `rollback_provisional_skinned_blas` drops never-built BLAS on submit failure, and `invalidate_tlas_recording` is wired (`5eb07a4f3`).

**Morph**
- **Shared delta.** It is keyed by `MeshHandle` through a `Weak` upgrade. Mesh handles are never reused (#372), and skinned and morph meshes do not opt into content geometry sharing (`geometry_sharing.rs` module doc), so a cache hit always sees the same mesh's deltas.
  - Hardening note: the cache hit does not re-check `target_count × vertex_count` against the cached delta's size. It is safe only by that handle discipline; not filed.
- **Draw and dispatch gate.** `morph_slot_backs_mesh` guards remaps.
- **Eviction drain** sits outside the `skin_compute`/`accel_manager` guard (#3374, pinned).
- **Destroy.** `release_shared` destroys on the last strong ref even with the cache `Weak` alive (#4838).
- **Cache pruning.** `morph_delta_cache.retain(strong_count != 0)` prunes dead keys.
- **Lifecycle.** A MorphSlot cannot be evicted while its entity lives (see the summary).

**Overflow and dirty-upload agreement**
- **Bone-palette overflow.** `SkinSlotPool::allocate` warns once (`overflow_warned`) and counts (`overflow_attempt_count`). The entity falls back to `bone_offset = 0` (the identity slot, seeded). Nothing truncates silently.
- **Per-slot dirty upload, CPU/GPU agreement.** Slot state bits are promoted only after `queue_submit` succeeds, `discard_bone_world_promotion` runs on every tail `Err`, and a stale latch is cleared on zero-region frames (#3991).

**Hot-path hashing**
- Every `SkinSlotPool` collection is `FxHashMap`/`FxHashSet` (source-pinned).
- The renderer side is Fx too: `skin_slots`, `morph_slots`, `morph_delta_cache`, `failed_skin_slots`, `failed_skin_blas`, `skin_dispatch_seen_scratch`, `skin_built_this_frame_scratch` and `skinned_blas`.
- `render/skinned.rs` `skin_offsets` is `FxHashMap`, and `pose_dirty` is `FxHashSet`.

**Allocation**
- Plan and dispatch scratch are persistent (#1133, #4611).
- The eviction `evictees` / `morph_evictees` use `mem::take` and allocate only on eviction frames.

### Dim 10

- `shadowableLightRadiance` (`include/lighting.glsl`) is the sole `disneyDiffuseSplit` caller (grep over all shaders). It scales `(dd.diffuse + dd.sheen) * PI * (1.0 - metalness)` and multiplies N·L once, via `bethesdaDiffuseLightFactor`. The Disney `dd.diffuse` carries no N·L of its own.
- The Disney gate is `MAT_FLAG_PBR_BSDF` only. `is_pbr` is set only by the BGSM merge (`merge.rs`, unconditional #2700), the Starfield CDB (`cdb.rs`) and Cornell. NIF import leaves it false.
- `disneyDiffuseSplit`'s sheen is `FH * sheen * sheenColor` (not ÷π), with a luminance-normalised tint (#2819).
- `distributionGGXAniso` equals `distributionGGX` at `ax == ay == roughness²` (algebra checked). `deriveAxAy` clamps anisotropy to [0, 1] with the 0.025² floor, and the Gram-Schmidt `Tproj` guard is intact.
- Rim, back-light and soft-lighting wrap evaluate independently inside `shadowableLightRadiance`. Translucency and model-space normals are independent flag tests in `triangle.frag`.
- `collect_lights`: the directional is pinned at slot 0 (`history_id [0,2,0,0]`, `VisibilityMask::FULL`, type 2.0, including interior XCLL). The point suffix is sorted with `sort_unstable_by` on the caller-owned `sort_scratch`, by descending `gi_priority_score`.
- The renderer re-sort in `assemble_camera_and_lights` keeps the leading `type > 1.5` prefix pinned.
- The `upload_lights` overflow warn names the lowest-`gi_priority_score` tail.
- LightHistory, end to end:
  - `remap` maps `ids[(frame-1)%FIF]` to the uploaded `lights[..count]`, and ambiguous or missing identities go to `INVALID`;
  - `triangle.frag` `remapReservoirLight` covers both temporal `rp` and spatial `rn` reads of `reservoirsPrev`, which is bound to the previous FIF slot (`write_reservoir_buffers`);
  - the dirty-hash gate includes the mapping, and `commit` runs on the hash-hit path;
  - entity IDs are never reused.
- All 4 GLSL `LightBuffer` mirrors carry `previousLightToCurrent[MAX_LIGHTS + 1u]` + `uvec4 history_id`, and Rust `GpuLight` is 80 B with `history_id` last. `docs/engine/shader-pipeline.md` §GpuLight and `renderer.md` "Multi-light SSBO" are updated.
- `cluster_cull.comp`: 16×9×24 (`CLUSTER_TILES_X/Y`, `CLUSTER_SLICES_Z`), `local_size_x = THREADS_PER_CLUSTER` (32), dispatched `cmd_dispatch(cmd, 16, 9, 24)` in `ClusterCullPipeline`. The slot is bounds-checked before `sharedIndices` is written. The count is clamped at `MAX_LIGHTS_PER_CLUSTER` (512), and `overflowedClusters` / `droppedLights` / `maxLights` are atomically recorded. The telemetry buffer is `cmd_fill_buffer`-zeroed each dispatch, harvested into `RenderStats.cluster_*`, and gated in the `rt.integrity` health predicate (`cluster_overflowed == 0 && cluster_dropped == 0`).
- The single visibility policy is `VisibilityMask::for_legacy_local_light()` = `FULL`. Every `LightSource::from_legacy_world_units` producer goes through it: `synth_child.rs` ×2, `mesh_instance.rs`, `spawn.rs` `spawn_nif_lights`, and Cornell ×2. No producer hand-picks a narrower mask. The shader removes EFFECT from the opaque mask (`traceShadowTransmittanceDetailed`: `opaqueMask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE`), with a defensive `effectCard` skip.
- Animation decode is strict: `canonical_light_animation_flags` gives FO4/FO76 `FLICKER|PULSE` and Starfield 0. `animate_lights_system` / `flicker_intensity` read only `LightFlicker.animation_flags`, never raw `LightSource.flags`. `shadow_spotlight_bit_never_leaks_into_animation_on_any_game` passes.
- #4859 fix holds: `crates/nif/src/import/walk/lights.rs` passes `outer_spot_angle.to_radians()`, pinned by `walk/tests.rs`.
- The interior portal-sky sun (#4839, `a4a68fa92`) routes `WeatherSkyState::sunlight_color` through `compute_directional_upload`. The room lane stays unlit, pinned by `interior_portal_sky_clouds_are_lit_by_the_exterior_sunlight`.
- The disabled-WTHR / no-sky fallback is neutral: `collect_lights` falls back to `(SUN_INTENSITY_PEAK, 0.0)` with no `SkyParamsRes`, `WeatherSkyState::default().sunlight_color = FB_SUNLIGHT`, the coverage is classification-derived (finite), and `no_cell_lighting_emits_no_directional` passes.

### Dim 11

- **FFI.** Both `unsafe fn` in `crates/fsr3-sys/src/lib.rs` (`Context::create`, `Context::dispatch`) carry `# Safety` sections. Every internal `unsafe {}` has a `// SAFETY:` comment, including the test-only `byro_fsr3_abi_layout` call. `Drop for Context` cites `create`'s idle clause. There is no `unsafe impl Send/Sync`.
- **Frame order.** `record_post_passes` runs `record_composite_pass` → `record_bloom_pass` → `record_exposure_meter_pass` → `record_taa_pass` → `record_upscale_pass` → `record_presentation_pass`. `shader-pipeline.md` steps 17b/19/20 match, and this is now pinned by `shader_pipeline_documents_every_record_pass_helper`.
- **Tone mapping.** Nothing upstream tone-maps: no `aces(`/`tonemap(` call exists outside `presentation.frag`. Presentation does `tonemap(graded * exposure)` with `exposure = texelFetch(exposureTex, …)`, and `tonemap()` dispatches on the generated `TONEMAP_OP_AGX`.
- **Exposure texel.** One per-FIF `ExposureResource` texel (`exposure.image(frame)` → `FrameUpscaler::record` with `pre_exposure: 1.0`, and `exposure_views[frame]` → presentation binding 2) is written by one producer, `ExposureMeterPipeline::dispatch`, with SHADER_READ↔GENERAL barriers covering COMPUTE|FRAGMENT consumers. There is no `NO_EXPOSURE_RESOURCE_FALLBACK`: the slots are cleared to `DEFAULT_EXPOSURE` (0.85), and `latch_exposure_meter_failure` freezes the last value for both consumers alike.
- **Meter raw-view gate.** `record_exposure_meter_pass` has the `render_debug_requires_raw_output` gate (#4591 holds). The FSR path is forced native under raw views (`force_native_debug`), and presentation bypasses exposure on `rawDebug`.
- **Forced dispatch failure.** `BYRO_FSR_FORCE_DISPATCH_FAIL` returns `Err` before the SDK is entered (`force_dispatch_failure`, `env_flag_is_set`). The recovery branch restores depth and blits from `inputs.scene_color_layout` with a GENERAL output (#4592 holds), latches `dispatch_failure`, and `record_upscale_pass` raises `signal_temporal_discontinuity(FSR_DISPATCH_FAILURE_RECOVERY_FRAMES)` via `take_new_dispatch_failure`.
- **Infallible upscale.** `record_upscale_pass` and `FrameUpscaler::record` return `()`. The post-latch no-`?` scan passes.
- **Preset switch.** `step_upscaler_switch` drains before `render_one_frame` (`app_events.rs`). `set_upscaler_mode` early-returns on the same mode, runs `device_wait_idle`, destroys TAA, `recreate_swapchain`, and has a rollback arm plus a TAA rebuild. `recreate_screen_passes` rebuilds volumetrics from config at the new render extent, and `recompute_blas_budget_for_current_state` runs last with the refreshed `sdk_memory_bytes` (#4111 holds). `screen_scaled_reservation_bytes` bills TAA unconditionally and the output-extent `upscale_output_bytes` plus SDK bytes.
- **Jitter.** There is one `GpuCamera.jitter` lane. FSR takes `FsrTemporalState::current().ndc`/`.pixel` (the same SDK sample goes to the projection and the dispatch); TAA takes `taa_jitter` behind `taa_jitter_gate` (raw-view aware, #4513). FSR jitter is gated on `is_fsr_dispatch_active`, and DOF is gated by `fsr_gated_dof` on the same predicate.
- **ABI.** The Rust and C structs are layout-pinned from both sides (see the stale-open issues above).
- **SPIR-V.** All artifacts are byte-fresh (`check-shader-artifacts.sh`).

### Dim 12

- `QUERIES_PER_FRAME = 56` equals 23 named pairs plus 5 `GeometryTimerPhase` pairs. Every `cmd_*_start/_end` writes its own `Q_*` slot, and every `_end` ORs its own `BIT_*`. The stage choices match the doc: TOP_OF_PIPE, or COMPUTE_SHADER for main render, volumetrics and ground-cover models, and BOTTOM_OF_PIPE for the phases.
- The `GpuTimerSnapshot` field lists are 28 `_ms` and 28 `_active`, in the same order, and all are filled in `snapshot_from_bits_with_valid_bits`.
- No double-write per frame:
  - skin palette uses a `skin_palette_timer_started` latch (`dispatch_skin_and_cluster.rs`);
  - the ground-cover bench has exclusive paths (`groundcover_bench.rs`, early `return`);
  - MainBlended is started once via `groundcover_models_drawn`, and MainWater via `water_drawn`;
  - the opaque fragment-invocation begin/end is balanced inside one subpass.
- `read_and_reset` runs after `wait_for_fences` in `sync_and_acquire_frame.rs`, so results are `MAX_FRAMES_IN_FLIGHT` behind. It does a non-WAIT batched read gated by `active_bits`, then a host reset.
- `timestamp_supported == false` returns `Ok(None)` through `caps.gpu_timers_supported()` (timestamps plus hostQueryReset). The draw path has no unwrap; every site is `if let Some(timers)`.
- `pipelineStatisticsQuery` is enabled at device creation under the same `fragment_invocation_query_enabled()` predicate as pool creation (`device.rs:794`). A partial pool-creation failure unwinds.
- `GpuPerFrameTimers::destroy` frees both pool sets; teardown calls it and `destroy_depth_capture_staging` (`teardown.rs`).
- The readers honour `_active`:
  - `fill_skin_coverage_stats` sets all 22 pairs, and zeroes them with `false` when timers are absent;
  - the bench line prints the `gpu_inactive=` token;
  - `gpu_breakdown` prints `n/a`;
  - `metrics_sample_system` maps inactive to `None`, and the debug-ui grid, byro-dbg display and TUI render it as n/a;
  - `fill_upscaler_telemetry` carries `upscale_active`;
  - the ray-budget controller filters `> 0.0`, which is equivalent.
- The post-pass timer coverage guard derives from `record_post_passes`. All 10 passes are bracketed, the exposure meter included (#4618).
- Raw-output gate on every post pass:
  - TAA, exposure meter, bloom and upscale (FSR forced to native, and the TAA-resolved source dropped) are gated in Rust via `render_debug_requires_raw_output`;
  - composite and presentation are gated in-shader through the generated `DBG_VIZ_REQUIRES_RAW_OUTPUT`, with `mode != FINAL`, plus composite's COMPOSITE_TERM/VOLUMETRIC_TERM self-handling;
  - TAA jitter is disabled for raw views (`taa_jitter_gate`).
- `RenderDebugMode::USER_MODES` has 16 entries (Final + 15), dense 0..15, and matches `RENDER_DEBUG_MODE_MAX = 15u`. The out-of-range magenta guards in triangle and composite are pinned against the SPIR-V.
- `request_selected_ray_probe` bounds-checks against `frame_extents.render`, keeps generation non-zero on wrap, and holds one pending request. The request clears only when the armed generation was submitted.
- egui: `loadOp = LOAD`, initial and final layout `PRESENT_SRC_KHR`, its own incoming COLOR_ATTACHMENT_OUTPUT dependency. Framebuffers are recreated on resize, and the whole pass is rebuilt on a format change (`recreate_composite_and_egui`). The `Option<EguiPass>` is taken and destroyed in teardown. Unchanged since the baseline.
- Depth capture: `depth_capture_finish_readback` runs after the fence wait (pinned); the non-D32 format is refused before arming; staging is destroyed on teardown. Unchanged.
- Cornell: live runs built a valid TLAS (31/31) and rendered clean under validation in both FSR and TAA modes.
  - The new rungs (skinned, shared-skin, point/spot, cache-pressure, mirrored) and `--godray-lab` are unit-tested.
  - `mat.*` goes through `mat_set_tests`, including `every_shader_consumed_material_scalar_is_console_reachable`.
  - The Material-literal exemption is documented at `material_translate.rs:2431`.
  - #4552 named parallax defaults are used at both original sites.

## Appendix C — Live validation runs (orchestrator, release build of HEAD 7e9da5dcc, RTX 4070 Ti, BYRO_VALIDATION=1 = core+sync)
- `--cornell --bench-frames 90` (**actually fsr3/native-aa** — a saved `~/.config/byroredux/settings.toml` `render.upscaler = fsr3/native-aa` overrode the CLI default silently; see REN-D11-2026-09-27-01): 0 VUID, 0 SYNC-HAZARD. rt-integrity tlas_eligible=31 emitted=31, missing_*=0, lights_dropped=0, cluster_overflowed=0.
- FNV `GSProspectorSaloonInterior --bench-frames 300` (also native-aa, same cause) (skinned NPCs live): 0 VUID, 0 SYNC-HAZARD. rt-integrity tlas_eligible=893 emitted=893, missing_skinned/rigid/ssbo=0, lights 25/25, cluster_overflowed=0.
- The 09-20 water push-constant range pair ([0,28] vs [0,16]) no longer fires — fixed.
- Both runs: 9 `(SPIR-V Interface)` performance warnings at one vkCreateGraphicsPipelines — vertex outputs at locations 6,7,13,16 (float) and 10,11,12,15,20 (uint) with no fragment input. Matches `groundcover_blade.vert` (`vBladeWidth`@7, `vLodMidWeight`@13, `vCardWeight`@16 …). Valid per spec (discarded writes); LOW / owned by /audit-exterior; it recurs at every session start and dilutes the validation signal.
- Not covered: FSR preset switch, --upscaler taa, exterior grid streaming, resize.
- `--cornell --upscaler taa --bench-frames 90`: 0 VUID, 0 SYNC-HAZARD (same 9 ground-cover perf warnings).
- Lead for Dim 11: an accidental bare launch (no --upscaler/--fsr-quality) logged `Renderer upscaler selection: fsr3/quality` then `Frame extents: render=1280x720, output=1280x720 (fsr3/native-aa)` and `Frame upscaler: fsr3/native-aa`. The default path resolved to native-aa, not Quality — find out why (persisted settings registry override? extent floor?) and whether it is silent.
- **Reruns with the default path pinned** (`--upscaler fsr3 --fsr-quality quality`, render 853x480 -> output 1280x720): Cornell 90 frames and FNV Prospector 300 frames — both 0 VUID, 0 SYNC-HAZARD (only the validation banner + the 9 ground-cover perf warnings); FNV rt-integrity 893/893, missing_*=0, lights 25/25, cluster_overflowed=0. The render-below-output FSR Quality boundary is validated clean at HEAD.
