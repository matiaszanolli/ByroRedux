**HEAD**: `f97775ca8` · **Baseline**: `AUDIT_RENDERER_2026-09-20.md` (HEAD `052891f22`; delta = `052891f22..f97775ca8`, 34 renderer-path commits) · **Audited**: Dims 1–8, 10–12 · **Unchanged since baseline (skimmed)**: Dim 9 (skinning — no commits on its Paths; guards spot-checked)

# Renderer Audit — 2026-09-21

One leg of `/audit-suite --preset comprehensive`. Single auditor, dimensions run
sequentially (no sub-agents, per the suite orchestrator), guard-first. No engine
launches were planned (the runtime audit owns them) — see *Process notes* for one
accidental launch. Evidence sources beyond code reading: the full renderer lib suite
(1139 pass / 1 device-ignored), the HEAD-built `byroredux` test harness for bin-crate
guards, `byroredux-core --features inspect` lighting/material guards,
`scripts/check-shader-artifacts.sh` + `spirv-dis`, and three read-only real-data
censuses run from scratch tools outside the repo (FNV/FO3/Oblivion mesh BSAs; every
DX10 BA2 of Starfield/FO76/FO4; FNV/FO3/Oblivion texture BSAs).

The baseline did **not** cover the three newest commits (`c5663fe39` auto-exposure
meter + AgX, `d54382415` Stage-1 wiring + W2.10 flip + depth-bias slopes,
`f97775ca8` facing-ratio/ReSTIR views + shadow-mask policy); they are audited here.

**Totals (NEW + regression): 0 CRITICAL · 1 HIGH · 3 MEDIUM · 12 LOW** (16 findings;
1 is a regression of closed #3322; 0 matched to existing open issues).

## Executive Summary

The baseline's 33 issues (#4510–#4542) all landed in the window and the fixes spot-checked
here hold (water push-constant block, DDS cap — re-verified against every vanilla DX10 BA2:
0 textures above 8192 —, staging-pool sizing, TAA raw-view jitter gate, doc sweeps).
The new work is where the risk is:

- **`f97775ca8` turned legacy FX cards into shadow blockers (HIGH).** Removing the
  `alpha_blend → EFFECT` divert rests on the premise that non-occluders are separated
  by material kind (glass/effect/fire). That is true for Skyrim+ but false for FO3/FNV
  (`BSShaderNoLightingProperty` → kind 102) and Oblivion (kind 0). Real-data census:
  4,699 FNV / 2,119 FO3 / 1,011 Oblivion drawn, blended, non-glass FX meshes (mist
  planes, ground fog, light beams, glow shells) now sit in opaque shadow buckets, where
  the direct path treats alpha ≥ 1/255 as covered and the volumetric/caustic paths
  trace them as solid quads.
- **The shipped `composite.frag.spv` is stale again (MEDIUM, regression of #3322)**:
  both debug views `f97775ca8` added for the light-leak hunt render full-frame magenta,
  and the new `DBG_VIZ_AO` oracle gets fog/caustics composited over it. CI's
  `shader-artifacts` job has been red on main for six commits.
- **AgX is wired incorrectly (MEDIUM, opt-in)**: a linear [0,1] clamp before `log2`
  (the reference clamps in log space) flattens every value ≥ ~1.0 to 0.59
  display-linear, so AgX can never reach white, and black goes through `log2(0)` → NaN.
  The Rust mirror and its tests copy the deviation, so they cannot catch it.
- **`c0b740ce7` silently disabled the NIFAL boundary's core copy-fidelity test
  (MEDIUM)** — the new test was inserted between `#[test]` and the old fn.

The Stage-1 exposure meter's synchronisation traces clean (details in Dim 4). Its
remaining issues are LOW: the adaptation rate is half the documented one, and docs and
the ledger fell behind the pass move for the third audit running.

## RT Pipeline Assessment (AS, SSBO indexing, ray-query safety, denoiser)

- **AS (D1): structurally sound, policy regression.** `instance_custom_index` still comes
  from the shared `instance_map` (24-bit assert at the truncation site). OPAQUE geometry,
  sort-by-BLAS-address and the census partition are unchanged, and all 128 acceleration
  tests plus the TLAS-barrier and BLAS-recovery pins pass. The shadow-mask change is the
  HIGH above. A premise correction: `TRIANGLE_FACING_CULL_DISABLE` is inert, because no
  ray query sets a facing-cull ray flag. Every ray already sees both faces, which is also
  why single-sided walls block from behind.
- **Ray queries (D2): PASS.** The RT gate, scale-aware origins, absolute/relative space
  split and MSN flip (#3922, measured) all hold, and `triangle.frag.spv` is byte-fresh.
  Two LOW defects in the new debug views:
  - the RESTIR_LIGHT block re-binds the legacy-WRS `else` in evaluation builds;
  - FACING_RATIO paints every two-sided back face red.
- **Denoiser/resolve (D7): PASS.** Order is now composite → bloom → exposure meter → TAA →
  upscale, and it is pinned. Exposure changes cannot perturb TAA/SVGF history (both work on
  pre-exposure linear HDR; FSR gets the same per-FIF texel with `pre_exposure = 1.0`).
  `docs/engine/renderer.md`'s frame flow still describes four retired pipeline states (LOW).
- **Media/water (D8): PASS with two LOW gate-precision findings.**
  - The glass caustic depth gate's NDC slop grows about d²/near: 23 % of d at 300 BU.
  - The water caustic visibility ray lets effect cards occlude.

## GPU-Struct & Memory Assessment

- **Structs (D3): PASS.** `vulkan/scene_buffer/gpu_types.rs` and `shaders/include/bindings.glsl` are untouched. The new
  `MeterParams` UBO and the reworked `PresentationPushConstants` (`tonemap_op`@16) match
  their GLSL blocks, checked field by field. Two LOW test gaps:
  - `MeterParams` is not in the SPIR-V UBO block-size table;
  - the tonemap operator id is a hand-written `1u` in GLSL.
- **Memory/lifecycle (D5): PASS.** The meter's UBOs and the per-FIF exposure images are
  destroyed before the allocator `Arc::try_unwrap`; the three load-bearing teardown
  orderings are still exactly three (pinned by #4527's test); the meter's
  construction-error path cleans every partial handle. The exposure resource is now
  hard-fail at init (the soft `NO_EXPOSURE_RESOURCE_FALLBACK` path is gone — skill premise
  stale).
- **Skinning (D9): unchanged.** `d689e0c04` (#4399) made the #3231 morph path reachable
  for the first time (loose-NIF/NPC route). LRU stamping protects live slots, and it is
  ledgered. It has not been validated live.

## Findings

IDs: `REN-D<dim>-2026-09-21-NN`.

### HIGH

#### REN-D1-2026-09-21-01: removing the blend→EFFECT divert makes FO3/FNV/Oblivion blended FX cards (mist, ground fog, light beams, glow shells) binary shadow blockers
- **Severity**: HIGH. Rendering correctness on the default path in three games (decision tree:
  "rendering correctness → at least HIGH"). The per-cell magnitude is not captured (see Needs-RenderDoc).
- **Dimension**: AS Correctness (instance mask) · **Status**: NEW (introduced by `f97775ca8`, 2026-09-21)
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` `shadow_mask_for_instance`
  (blend arm removed; `_alpha_blend` unused). Consumers: `shaders/include/shadow_transport.glsl`
  `traceShadowTransmittanceDetailed`, `shaders/include/ray_hit.glsl` `rayHitHasCoverage`,
  `shaders/include/shadow_common.glsl` `traceShadowBinary` (volumetrics_inject / caustic_splat),
  and `shaders/volumetrics_inject.comp` `combustionPathBlocked` (`VISIBILITY_MASK_SOLID`).
- **Description**: From 2026-08-09 (`5798e4672`) until this commit, every non-actor
  alpha-blended instance was routed to `VISIBILITY_LAYER_EFFECT`, which is outside
  `ALL_OPAQUE`/`SOLID`. Now it keeps its render layer's opaque bucket. The comment's
  premise, "True non-occluders are separated by MATERIAL KIND above (effect shader, fire
  refraction, refractive glass)", holds for Skyrim+ (effect shaders are kind 101). It does
  not hold for:
  - FO3/FNV, whose effect family is `BSShaderNoLightingProperty` → kind 102
    (`crates/nif/src/import/material/legacy_properties.rs`, `nolighting_sets_material_kind_to_102`);
  - Oblivion, whose FX cards are kind 0.

  TLAS membership (`render/static_meshes.rs` `tlas_exclusion`) excludes only distant LOD,
  `IsDecalMesh` and fire refraction, so these cards are in the TLAS.
- **Evidence**:
  - **Census.** A read-only scratch tool imported every NIF in the vanilla `Meshes` BSAs
    and counted drawn meshes that are alpha-blended, not glass-keyword, not kind
    100/101/103, and not an `IsFxMesh` never-drawn texture. Result: **FNV 4,699** (kind 102:
    3,777), **FO3 2,119** (kind 102: 1,613), **Oblivion 1,011** (kind 0). Samples, nearly all
    additive `SRC_ALPHA/ONE` or soft-alpha cards:
    - FNV `effects\nv\hchambergroundfog.nif`, `effects\ambient\fxmistlow01.nif`,
      `architecture\strip\lucky38lights.nif`, `ultraluxdome_glowsbackside.nif`;
    - FO3 `clutter\fakefog01.nif`, `effects\ppurityfx\ppurityfxtankfog01.nif`;
    - Oblivion `dungeons\misc\fx\fxlightbeam01.nif`, `oblivion\environment\fxoblivionlightbeam01.nif`,
      `oblivion\gate\flashglow01.nif`.
  - **Coverage.** `rayHitHasCoverage` counts a blended non-glass hit as covered at
    alpha ≥ 1/255. Alpha is forced to 1.0 without `DIFFUSE_ALPHA`, so soft fog/beam textures
    are effectively solid quads. `traceShadowBinary` has no coverage test at all.
  - **Lights affected.** FO3/FNV ESM lights are all `FULL` (zero-authoring correction), as
    are NIF lights and the sun/XCLL. Oblivion's unflagged lights use the conservative mask,
    which gained `STATIC_PROP` in `d54382415`.
- **Impact**:
  - Ground-fog and mist planes shadow the floor beneath them from every overhead light.
  - Light-beam and glow shells block the lights they depict and cast hard quad shadows.
  - Volumetric in-scatter is cut off at each card.
  - Smoke/fire transport stops at mist and beam cards (the comment "effect cards do neither" is now false).
  - `rt.masks` cannot show any of this, because blended cards are now counted inside the opaque buckets.
- **Related**: #3305 (open, actor ground shadows — different direction); `84bbc44ed` (blended-actor
  policy); REN-D1-2026-09-21-02 (the facing-cull premise the commit's investigation relies on).
- **Suggested Fix**: Keep the Skyrim ice/door-panel intent, but add the legacy authored
  "not solid" signals to the non-occluder arm: kind 102 (NoLighting) and additive blends
  (`dst_blend == ONE`). Better, carry an explicit canonical non-occluder bit across the NIFAL
  boundary. Add an FNV/Oblivion fixture to `shadow_mask_bucket_selection_is_pinned` and a
  blended-in-opaque counter to `rt.masks`.

### MEDIUM

#### REN-D12-2026-09-21-01: `composite.frag.spv` is stale on two generated constants — both new debug views render full-frame magenta and the `DBG_VIZ_AO` oracle is contaminated
- **Severity**: MEDIUM (debug surface, same grade as #3322) · **Dimension**: Debug/Telemetry · **Status**: Regression of #3322
- **Location**: `crates/renderer/shaders/composite.frag.spv` vs `crates/renderer/shaders/composite.frag`
  (the `debugMode > RENDER_DEBUG_MODE_MAX` guard and `DBG_VIZ_REQUIRES_RAW_OUTPUT`), fed by the generated
  `shaders/include/shader_constants.glsl`.
- **Evidence**:
  - `scripts/check-shader-artifacts.sh` (the CI `shader-artifacts` job; glslang 11:16.2.0 matched) reports
    `DRIFT crates/renderer/shaders/composite.frag.spv`, the only drifting artifact of 35.
  - `spirv-dis` of the committed and freshly compiled binaries differs in exactly three instructions:
    - `OpUGreaterThan %bool %debugMode %uint_13` (the source says 15);
    - the mask `OpConstant 2215116800`, which should be `2215117312` (the latter adds `DBG_VIZ_AO` 0x200);
    - the `OpBitwiseAnd` that uses that mask.
  - History: the last recompile was `0e12f1f10`, whose message warns about exactly this trap.
    `09d9bc6f8` changed the mask and `f97775ca8` bumped `RENDER_DEBUG_MODE_MAX`; both recompiled only
    ssao/triangle.
- **Impact**:
  - `render.debug facing` / `restir` (modes 14/15) trip composite's out-of-range guard, which paints magenta.
    The Rust `render_debug_requires_raw_output` routes them raw past bloom, TAA, FSR and presentation, so the
    magenta frame reaches the screen. The two views added for the single-sided-wall leak hunt are unusable.
  - Legacy `DBG_VIZ_AO` is treated as non-raw by composite, which adds caustics (into direct), fog, volumetric
    transmittance and sky over the raw AO image. The oracle is clean only in fog-free, caustic-free scenes
    (the Cornell box it was tuned on).
  - CI is red on main since 2026-09-20 22:29, which masks any further SPIR-V drift.
- **Related**: #3120 (triangle sibling, pinned); #3322 (the composite fix was a recompile with no sibling pin); REN-D2-2026-09-21-01/02.
- **Suggested Fix**: Recompile `composite.frag.spv`. Extend
  `triangle_frag_spv_debug_mode_guard_matches_render_debug_mode_max`'s `max_u_greater_than_rhs_constant`
  pin to `composite.frag.spv`, and add an `OpConstant == DBG_VIZ_RAW_OUTPUT_ANY_MASK` presence pin for every
  shader using `DBG_VIZ_REQUIRES_RAW_OUTPUT`.

#### REN-D11-2026-09-21-01: AgX clamps linear input to [0,1] before `log2` — highlights ≥ ~1.0 flatten to 0.59 display-linear, white is unreachable, black takes a `log2(0)` → NaN path
- **Severity**: MEDIUM (opt-in feature visibly wrong; ACES default unaffected) · **Dimension**: FSR/Presentation · **Status**: NEW (`c5663fe39`)
- **Location**: `crates/renderer/shaders/presentation.frag` `agx()`; mirror `crates/renderer/src/tonemap.rs` `agx()` + its tests.
- **Description**:
  - The shader does `val = AGX_MAT * val; val = clamp(val, 0.0, 1.0); val = log2(val); …`.
  - Minimal AgX (Wrensch 2023) instead does `val = clamp(log2(val), min_ev, max_ev)`; three.js's port also
    clamps after the log encode.
  - As a result the `max_ev = 4.026` headroom (about 4 stops above 1.0) is never used.
  - An exactly-zero channel (black, or negative input clamped to 0) gives `log2(0) = -inf`. The contrast
    polynomial then evaluates to inf − inf = NaN, which the outset matrix keeps. The only rescue is
    `max(NaN, 0.0)`, which lowers to SPIR-V `FMax`, whose NaN result is undefined. The image-health counter
    runs before tone mapping and cannot see this.
- **Evidence**: Evaluated with the shader's own constants on a grey ramp (repo / reference output):

  | Input | Repo | Reference |
  |---|---|---|
  | 0.5 | 0.425 | 0.425 |
  | 1.0 | 0.590 | 0.590 |
  | 1.5 | 0.590 | 0.683 |
  | 2.0 | 0.590 | 0.743 |
  | 4.0 | 0.590 | 0.861 |
  | 16 | 0.590 | 0.995 |

  The Rust mirror copies the clamp, and `saturated_channels_are_monotonic`'s comment calls the clip
  "reference behaviour". Rust's `f32::max(NaN, 0.0)` returns 0, so `outputs_are_bounded_and_black_maps_to_black`
  cannot see the GLSL NaN path. The tests are circular.
- **Impact**: With `--tonemap agx` / `tonemap agx`, every bright surface, light and sky clips to a flat ~79 %
  sRGB grey. That defeats the highlight roll-off Stage 1 chose AgX for. Black-pixel output depends on the driver.
- **Suggested Fix**: Use the reference log-space clamp with a `max(val, 1e-10)` floor before `log2`, mirror it in
  Rust, and add a test that the grey ramp keeps rising past 1.0 and reaches ≥ 0.99 at 16.

#### REN-D6-2026-09-21-01: `c0b740ce7` silently disabled `translate_material_copies_every_canonical_field` — the NIFAL boundary's core copy-fidelity regression test
- **Severity**: MEDIUM. This is a test gap, but on the HIGH-floor single boundary (all-game blast radius); the code is currently correct.
- **Dimension**: NIFAL Material · **Status**: NEW (owner `/audit-nifal` Dim 1; found here — the 2026-09-21 NIFAL report predates the commit)
- **Location**: `byroredux/src/material_translate.rs`, `mod canonical_completeness_harness`.
- **Evidence**:
  - The new `an_msn_named_normal_slot_declares_model_space_normals` (doc + `#[test]`) sits between the old
    `#[test]` and `fn translate_material_copies_every_canonical_field()`.
  - `--list` on both HEAD-built `byroredux` test harnesses (2353 tests) shows
    `an_msn_named_normal_slot_declares_model_space_normals` twice (it ran twice: "2 passed") and no
    `translate_material_copies_every_canonical_field`.
  - `every_source_derived_material_field_is_pinned_by_a_test` scans the test module's source text, so it stays
    green while the assertions it counts never execute.
- **Impact**: The #2214/#3462 contract ("fails on a deliberately reintroduced boundary drop") is void for every
  field only that test covers. This includes the `water_shader_flags`/`is_water_shader` WATAL seam and the
  emissive, specular, diffuse, ambient, UV, alpha, env-map and vertex-colour copies.
- **Suggested Fix**: Move the orphaned attribute and doc block back onto the fn and drop the duplicate
  attribute. Make the meta-pin assert that the fn is `#[test]`-annotated, or have it call the fn.

### LOW

| ID | Dimension | Finding (one line) |
|---|---|---|
| REN-D1-2026-09-21-02 | AS Correctness | `TRIANGLE_FACING_CULL_DISABLE` is inert. No ray query sets a facing-cull ray flag (every query is `gl_RayFlagsOpaqueEXT [| TerminateOnFirstHit]`), so #416's premise and `acceleration/tlas.rs`'s "RT honors two_sided / ~2× ray cost" comment are false. Correct the docs and the skill premise; do NOT add cull flags (that would open leaks through single-sided room shells). |
| REN-D1-2026-09-21-03 | AS Correctness | `context/telemetry.rs` `fill_shadow_mask_census` #4518 comment still says the divert "returns `AlphaBlend` only for non-Actors" and that the field removal is "tracked separately". The cause no longer exists and the field is already gone. |
| REN-D2-2026-09-21-01 | Ray Queries | `triangle.frag`: the RESTIR_LIGHT `if (viewRestirLight) {…return;}` sits between the `if (useRestir) {…}` finalize and `#if ENABLE_LEGACY_WRS else {`. In the documented evaluation build (`ENABLE_LEGACY_WRS == 1`), legacy pass 2 now runs whenever the view is off, including ReSTIR pixels, so direct light is double-counted. |
| REN-D2-2026-09-21-02 | Debug/Telemetry | FACING_RATIO uses `geometricNormal` oriented to the un-flipped vertex normal (only `N` gets the `!gl_FrontFacing` flip), so every two-sided back face (foliage, banners, cloth) reads "inverted-normal red", although `offsetRayOriginForDirection` makes those leak-free. |
| REN-D3-2026-09-21-01 | GPU-Struct | `presentation.frag` `tonemap()` compares `params.tonemapOp == 1u`, a hand-written literal that is not generated from `tonemap.rs` `TONEMAP_OP_*` and not pinned GLSL-side (only the Rust value = 1 is). |
| REN-D3-2026-09-21-02 | GPU-Struct | `MeterParams` ↔ `exposure_meter.comp` `Params` is missing from `reflect.rs` `every_remaining_uniform_block_size_matches_its_host_struct`. `meter_params_are_two_vec4s` pins only the Rust size. |
| REN-D4-2026-09-21-01 | Pipeline/RenderPass | `shader-pipeline.md`'s submission order has no exposure-meter step between bloom (17) and TAA (18), and step 20 still says "exposure + ACES". `memory-budget.md` still describes one exposure producer (now 2 slots + 2 UBOs + pipeline, tiny). Third consecutive audit where the doc did not move with the pass. |
| REN-D7-2026-09-21-01 | Denoiser/Composite | `docs/engine/renderer.md` describes four retired states: composite "with ACES"; volumetrics "×0.0 until Phase 2"; TAA before bloom/composite and composite `+ bloom`; `rebind_hdr_views` / `fall_back_to_raw_hdr`; and "exposure + ACES" presentation. These are incomplete sweeps of #3573/#3608/#4524. |
| REN-D8-2026-09-21-01 | Caustics | Glass-caustic occlusion gate (`0418ac768`): `depthSlop = 1e-4 + ndc.z*5e-3` in conventional NDC grows ~d²/near. On BU content (near 5) it misses occluders within 4 % of d at 50 BU, 9 % at 100 BU, 23 % at 300 BU; on Cornell (near 0.1) it is already 34 % at 10 units, so the commit's check could only show no over-rejection. The motivating "bottle in front of its pool" is not rejected beyond ~50–100 BU. Use linearized depth with a relative tolerance. |
| REN-D8-2026-09-21-02 | Water | The #4545 water caustic visibility ray traces mask `0xFF` with opaque traversal. Effect-layer spray/mist meshes and whole alpha-card foliage quads therefore count as occluders, and caustics vanish under them although the camera sees the bed. Use `VISIBILITY_MASK_SOLID`. |
| REN-D11-2026-09-21-02 | FSR/Presentation | Auto-exposure blends from THIS per-FIF slot (written 2 frames earlier) with a one-frame `adaptation_alpha(dt)`. The effective time constant is 2τ (0.2 s documented → 0.4 s) and exposure updates at half the frame rate. The module doc's "simply lags" wording is wrong; this is a rate change. Opt-in (`--auto-exposure`). |
| REN-D11-2026-09-21-03 | FSR/Presentation | `record_exposure_meter_pass` lacks the `render_debug_requires_raw_output` gate that bloom, TAA, FSR and presentation carry. In auto mode the meter adapts to debug imagery, so leaving a view re-adapts from a debug-driven exposure. |

## Prioritized Fix Order

1. **REN-D12-01**: recompile `composite.frag.spv` and add the composite SPIR-V pins. This is one
   command, restores CI's `shader-artifacts` gate, and un-breaks the leak-hunt views REN-D1-01's
   verification will need.
2. **REN-D1-01**: restore non-occluder status for kind 102/additive FX cards, with a fixture and an
   `rt.masks` counter. Then A/B one cell per game (FNV Lucky 38 / FO3 Project Purity / Oblivion
   Ayleid beam dungeon) plus the Bleak Falls Barrow case that motivated `f97775ca8`.
3. **REN-D6-01**: re-attach `#[test]` (one-line move) and harden the meta-pin.
4. **REN-D11-01**: fix the AgX log-space clamp and its mirror/tests before any Stage-1 oracle L6 is minted
   against it.
5. **REN-D2-01 / REN-D11-02 / REN-D11-03 / REN-D8-01**: small, local correctness fixes.
6. **Doc/test-gap sweep**: REN-D4-01, REN-D7-01, REN-D1-02, REN-D1-03, REN-D3-01, REN-D3-02,
   REN-D2-02, REN-D8-02.

## Needs-RenderDoc / live validation

(No engine runs were available to this audit; the runtime audit runs serially after the suite.)

- **REN-D1-01 magnitude.** A/B pre/post `f97775ca8` on FNV/FO3/Oblivion FX-card cells and on Bleak Falls
  Barrow; `rt.masks` + screenshots.
- **W2.10 flip (`d54382415`).** Conservative lights now trace `STATIC_PROP`. A light whose ESM position
  sits inside a Clutter-layer fixture is now occluded by its housing unless the hit is an emissive shell
  within `emitterRadius`. The skill named Prospector (FNV) / BanneredMare (Skyrim) captures; the flip
  cites Markarth SilverBloodInn + Cornell L2/L5 instead.
- **Exposure meter barriers under `BYRO_VALIDATION=1`**: default FSR path, `--upscaler taa`, a raw debug
  view (bloom skipped), and `BYRO_FSR_FORCE_DISPATCH_FAIL=1`. FSR consuming a per-frame-varying
  exposure texel under `--auto-exposure`.
- **#4399's newly reachable GPU morph dispatch** on an NPC-head route.
- Carried from the baseline: #3305 A/B; the >1 GiB BLAS-ceiling degradation; live FSR boundary layouts
  and mask content; FP32 FSR permutation (untested).

## Stale skill premises (for the next `/audit-renderer` sync)

1. **Phase 1 / guards.** Make `scripts/check-shader-artifacts.sh` a standing first step. It alone caught
   REN-D12-01, which every source-shape test passes. Note that composite.frag has no SPIR-V constant pin
   (#3322 recurred).
2. **Dim 1.**
   - `TRIANGLE_FACING_CULL_DISABLE` is inert (no ray query requests facing culling). Replace "a blanket
     enable is the regression (~2× ray cost)" with that fact.
   - The mask bullet should record `f97775ca8`'s policy (blended non-actors keep their layer bucket) and
     REN-D1-01's legacy-kind gap.
3. **Dim 6.**
   - `bgem_uses_thin_glass_behavior` is a production fn in `asset_provider/material/mod.rs`, not a test.
   - `resolve_pbr_is_idempotent` / `glass_behavior_preserves_authored_map_overlay` live in `byroredux-core`
     (run with `--features inspect`), not the bin crate.
   - Bin-crate guards can be run from the HEAD-built harness in `target/debug/deps` **only after
     confirming it is a libtest binary** (see Process notes).
4. **Dim 10.** The "Open, device-gated: should the conservative row add `STATIC_PROP`" premise is resolved
   in code (`d54382415`). Replace it with the Needs-A/B item above.
5. **Dim 11.**
   - `NO_EXPOSURE_RESOURCE_FALLBACK` no longer exists: exposure is now hard-fail at init, with per-FIF slots
     cleared to `DEFAULT_EXPOSURE`.
   - "Presentation owns exposure + ACES (`aces(graded * params.exposure)`)" is now
     `tonemap(graded * exposure)`, with `exposureTex` and an ACES|AgX switch.
   - Add the exposure meter (`exposure_meter.rs`, `exposure_meter.comp`), `tonemap.rs`, `ExposureTuning`
     and the `exposure`/`tonemap` commands to Paths and the checklist.
   - Frame order is composite → bloom → exposure meter → TAA → upscale → presentation (Dim 4's frame-shape
     line too).
6. **Dim 12.** `RenderDebugMode::USER_MODES` is now 16 (Final + 15), not "13 user modes". The exposure
   meter has no GPU-timer bracket.
7. **Dim 3.** Add the SPIR-V UBO block-size table (`every_remaining_uniform_block_size_matches_its_host_struct`)
   to the guard line and require new UBO mirrors to join it.
8. **Doc-moves-with-pass rule** (added after the 2026-09-20 audit) was not followed by the Stage-1 commits
   (REN-D4-01, REN-D7-01). Consider a test that `shader-pipeline.md` names every `record_*_pass` called in
   `record_post_passes`.

## Guard posture

- **Renderer lib.** 1139 pass / 1 ignored (device-only `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`).
  Every renderer-crate guard the skill names exists, is not `#[ignore]`d and passes: acceleration 128,
  `shader_contract` 107, scene_buffer 158, `vulkan::material` 41, taa 19, bloom 18, water 74, volumetric 48,
  caustic 43, exposure 7, presentation 8, tonemap 7, upscaler/fsr 29, skin 77, gpu_timers 10, render_debug 6,
  egui 6.
- **Bin crate.** Run from the HEAD-built harness (built 18:12, after every window commit touching bin code
  except `f97775ca8`'s `mat.dump` line):
  - `static_blas_recovery_runs_between_frames_not_in_the_render_driver`
  - `every_exterior_spawner_inserts_a_boundary_material`
  - `directional_source_contract_tests` 7
  - `shadow_spotlight_bit_never_leaks_into_animation_on_any_game`
  - `bone_palette_overflow` 4
  - `skin_dispatch_ran_rollback_scope_tests`
  - `gi_light_priority_tests` 6
  - `every_source_derived_material_field_is_pinned_by_a_test` — vacuous (REN-D6-01)
- **Core** (`--features inspect`): `resolve_pbr_is_idempotent`, `glass_behavior_preserves_authored_map_overlay`,
  `lighting::` + `render_layer` 26.
- **SPIR-V.** 34/35 byte-match; `composite.frag.spv` drifts (REN-D12-01).
- **Source-shape limits noted.** `shaders_pin_the_mirror_constants` (AgX constants present, algorithm unpinned);
  `caustic_deposits_are_gated_on_camera_visibility` / `glass_caustic_deposits_are_gated_on_landing_pixel_visibility`
  (gate present, efficacy unpinned).

## Process notes

- **Dimensions were run sequentially** by one auditor per the suite constraint. Scratch notes for all 12 dims
  are in `/tmp/audit/renderer/dim_{1..12}.md`. Phase-4 cleanup of that directory is left to the suite
  orchestrator.
- **Real-data censuses** used scratch Rust tools in the session scratchpad (path deps on `byroredux-nif` /
  `byroredux-bsa` / `byroredux-core`, target dir `/mnt/data/tmp/renaudit/target`) plus a Python BA2 DX10
  header scan. Nothing was written into the repo besides this report.
- **Deviation: the engine binary was launched by mistake.** While locating a bin-crate test harness, the
  audit invoked `target/debug/deps/byroredux-98e91fabef5cd6a8 --list` twice. That file is a hard link of the
  engine executable, not a libtest binary. Each invocation was killed by a 60 s `timeout`, produced no output,
  and left no process behind. It did run the engine in parallel with the other audits, against the suite's
  no-engine-launch rule.
- **Bisect hole.** `c5663fe39`/`d54382415` declare `exposure_meter.rs`, whose `include_bytes!` target
  `exposure_meter.comp.spv` was first committed in `f97775ca8`, so `d54382415` does not build. This is
  historical and nothing can be fixed at HEAD.

Publish with: `/audit-publish docs/audits/AUDIT_RENDERER_2026-09-21.md` (domain `renderer`; add
`shaders` for REN-D12-01/D11-01/D2-*/D8-*, `vulkan` for REN-D1-*, `nifal` + `test-gap` for REN-D6-01,
`doc-rot` for REN-D4-01/D7-01/D1-02/D1-03; game labels `game:fnv game:fo3 game:oblivion` on REN-D1-01).
