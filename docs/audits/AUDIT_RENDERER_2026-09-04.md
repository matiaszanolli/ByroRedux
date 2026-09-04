# Renderer Audit — 2026-09-04 (scoped: `water-deep`)

**This is a SCOPED run, not a full renderer audit.** It covers only five of
`/audit-renderer`'s dimensions, selected by the `water-deep` audit-suite
preset:

| Dim | Name | Why it is in scope |
|---|---|---|
| 1 | Acceleration Structures (BLAS/TLAS) | water's TLAS exclusion + BLAS/eviction interaction |
| 2 | SSBO/Index plumbing & RT ray queries | `water.frag`'s ray queries and its caustic write |
| 8 | Denoiser & composite | how water's G-buffer masking reaches SVGF + composite reassembly |
| 14 | Caustic splat (#321) | the glass-side writer that shares water's fixed-point basis |
| 15 | Water (M38) + water-side caustics | the subject |

**Dimensions 3–7, 9–13, and 16–23 were NOT examined.** In particular
GPU-struct layout (3), sync/barriers (4), memory/lifecycle (5), NIFAL (6),
material table (7), skinning (9), precision (10), pipeline/render pass (11),
command recording (12), TAA (13), volumetrics/bloom (16), Disney BSDF (17),
sky/weather (18), tangent space (19), telemetry (20), Cornell (21), light
animation (22) and the FSR/presentation chain (23) carry no coverage from
this report.

**Trigger**: `b15b0527` "Refactor water handling and testing in exterior grid
streaming" — a water change that reaches the shader, the pipeline, the
streaming budget and the smoke gates, which is why the preset traces water
through AS / SSBO / composite / caustics rather than reading `water.rs` and
`water.frag` in isolation.

**Verification discipline applied**: findings are anchored on symbols, every
backticked path/symbol was confirmed against the live tree, no
render-pass/pipeline/barrier edit is proposed on reasoning alone (the two
look-affecting findings are explicitly marked as needing a visual A/B and the
streaming one as needing a frame-time measurement), and no bench numbers are
quoted.

**Dedup baseline**: `gh issue list --state all` searched per finding keyword
(`/tmp/audit/renderer/issues.json`, `issues_all.json`); `docs/audits/` scanned
for prior renderer + water reports (most recent full renderer audit:
`AUDIT_RENDERER_2026-08-30.md`; most recent water-specific:
`AUDIT_RENDERER_2026-07-15_DIM15.md`, `AUDIT_WATR_ARBITRATION_2026-08-20.md`).

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 4 |
| LOW | 3 |
| **Total** | **7** |

By dimension: Dim 1 → 1 LOW; Dim 2 → 1 MEDIUM + 1 LOW; Dim 8 → 1 MEDIUM +
1 LOW; Dim 14 → 0 (one cross-reference); Dim 15 → 2 MEDIUM.

Pipeline areas affected: water shading + blend state, composite reassembly,
the water-side caustic write, exterior LOD streaming budget, and two
documentation sites (one in-code, one in the audit skill itself).

### RT Pipeline Assessment

**BLAS/TLAS: clean.** Every Dimension-1 regression guard listed in the skill
still holds — build geometry format/flags, the three build-flag constants,
the VUID-03667 refit flag+count validation, the `instance_map`-derived
`instance_custom_index` with its 24-bit assert, the empty-TLAS frame-0 guard,
the `SHADER_READ`-at-AS-BUILD input barrier, the scratch-serialize barrier's
`WRITE | READ` dst mask, deferred BLAS destruction, and the ordering pins
around `restore_missing_static_blas_for_draws`. The two changes since the
last full audit — `#3666`'s `sort_tlas_instances_by_blas_address` and
`#3669`'s per-entity refit jitter — were both checked against the
BUILD-vs-UPDATE contract and neither can desync it.

**Water × AS is symmetric.** Water meshes upload `rt_enabled = false` (no
BLAS) and water draws are excluded from the TLAS by
`draw_command_eligible_for_tlas`'s `!is_water` term, and `is_water` is set by
`reemit_water_planes` before `restore_missing_static_blas_for_draws` runs —
so water can neither leak a BLAS nor generate a perpetual `missing_rigid_blas`
counter.

**Ray queries: clean.** No `gl_InstanceID` anywhere; every hit resolves
through `rayQueryGetIntersectionInstanceCustomIndexEXT` into the same
`instances[]`/`materials[]` pair; every trace site uses the shared
`ray_origin.glsl` offset with `tMin = 0`; every ray-query group is gated on
`sceneFlags.x`. The thin-glass gate, the interior-ambient miss fallback, the
ReSTIR normal cone + stable surface tag, and the BC1 punch-through guard are
all intact.

**Denoiser: clean.** SVGF's ping-pong, motion-vector convention, mesh-ID
disocclusion, hoisted firefly clamp and `div_ceil` dispatch all hold, as do
composite's caustic decode (integer sampler → float promote → shared
`CAUSTIC_FIXED_SCALE` divide → added to direct, never to the denoised
indirect) and the ACES-lives-in-`presentation.frag` split.

### GPU-Struct & Memory Assessment

Out of scope for this run (Dimensions 3 and 5 were not examined). Two
incidental observations recorded while tracing water: the water-caustic
accumulator's per-FIF `R32_UINT` / `array_layers(1)` footprint matches
`docs/engine/memory-budget.md`'s 8 B/px water half, and `CAUSTIC_FORMAT` /
`CAUSTIC_COLOR_LAYERS` match its 24 B/px glass half.

---

## Findings — MEDIUM

### REN-WD-D2-01: the water caustic splat bounds-checks against `screen.xy`, not the accumulator image, so the 1×1 placeholder-sink fallback still depends on out-of-bounds image-atomic behaviour
- **Severity**: MEDIUM
- **Dimension**: SSBO/Indexing (water-side caustic write)
- **Location**: `crates/renderer/shaders/water.frag` (the `causticSize` /
  `imageAtomicAdd(waterCausticAccum, q, fixedVal)` block at the end of
  `main()`); fallback binding in `crates/renderer/src/vulkan/context/resize.rs`
  (`placeholder_caustic_sink`)
- **Status**: NEW (partial regression of the intent of CLOSED #2784)
- **Description**: #2784 replaced the float `uv01 <= 1.0` guard with an
  integer pixel bound and its comment states the change means "the splat no
  longer depends on" Vulkan's out-of-range-write discard rule, explicitly
  naming the "1x1 `placeholder_caustic_sink` fallback" as the case that used
  to rely on it. But the bound it introduced is `ivec2 causticSize =
  ivec2(screen.xy)` — `GpuCamera.screen` is the **render extent**
  (`assemble_camera_and_lights.rs` uploads
  `self.frame_extents.render.{width,height}`), not the size of the image
  currently bound at set 2 binding 0. When `WaterCausticAccum::new` or
  `recreate_on_resize` fails, `resize.rs` deliberately binds the 1×1
  `placeholder_caustic_sink` view instead, and every water fragment then
  passes the render-extent bound and issues `imageAtomicAdd` at coordinates
  far outside a 1×1 image. Unlike a plain image store (which Vulkan defines
  as discarded when out of range), an image *atomic* out of range is not
  covered by that guarantee.
- **Evidence**: `water.frag` — `ivec2 causticSize = ivec2(screen.xy); ... if
  (all(greaterThanEqual(pixel, ivec2(0))) && all(lessThan(pixel, causticSize)))`,
  then the 5×5 loop re-tests against the same `causticSize` before
  `imageAtomicAdd(waterCausticAccum, q, fixedVal)`. `resize.rs`'s #2142 block
  binds `placeholder_caustic_sink` (`vec![p.view; MAX_FRAMES_IN_FLIGHT]`) on
  both accumulator-failure arms. `caustic_splat.comp` has the same shape
  (`ivec2 size = ivec2(causticScreen.xy)`), so the glass writer inherits the
  same assumption for its own sink path.
- **Impact**: Only reachable on the degraded path (accumulator allocation or
  layout transition failed at init/resize — i.e. under the VRAM pressure the
  fallback exists to survive). Blast radius is one atomic per water fragment
  per frame at undefined coordinates. Also a correctness-of-documentation
  problem: the comment asserts an independence the code does not have, which
  is exactly the premise-rot class the No-Guessing rules target.
- **Related**: #2784 (the integer-bound change), #2142 (the sink fallback),
  #1210/#1255 (water caustic phases).
- **Suggested Fix**: Bound on `imageSize(waterCausticAccum)` instead of
  `screen.xy` in `water.frag` (and mirror it in `caustic_splat.comp` with
  `imageSize(causticAccum).xy`); keep `screen.xy` only for the
  world→screen projection. This is a two-line shader change plus a `.spv`
  recompile, pinnable by the existing `water.rs` source-assertion test style.

### REN-WD-D8-01: water preserves the receiver's demodulated GI un-attenuated, and `b15b0527` raised water's coverage without touching that path
- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite (water interaction)
- **Location**: `crates/renderer/src/vulkan/water.rs` (`build_pipeline`'s
  `attachments` array — `masked_off` on colour attachments 1–5),
  `crates/renderer/shaders/composite.frag` (the geometry arm's
  `combined = direct + indirect * albedo + caustic`),
  `crates/renderer/shaders/water.frag` (the `reflectedCoverage` alpha block)
- **Status**: NEW
- **Description**: The water pipeline hard-masks raw-indirect (4) and albedo
  (5) to `color_write_mask = empty()`, so a water fragment leaves the
  **opaque receiver's** demodulated GI and albedo in the G-buffer untouched.
  Composite then adds `indirect * albedo` at full strength on top of a
  `direct` value that already has the water surface alpha-blended into it.
  For low-alpha water that is roughly the intended "see the bed's GI through
  the water", but it is not attenuated by the water column at all — and for
  high-alpha water (waterfalls, lava, and now any grazing-angle fragment) the
  covered surface's GI is still added at 100 %.
  The alpha-blend pipeline has a purpose-built alternative for exactly this:
  `create_blend_pipeline`'s non-`preserve_opaque_gbuffer` arm uses
  `auxiliary_blend` (a coverage blend) on attachments 4 and 5, so an ordinary
  transparent attenuates the receiver's indirect by its own coverage. Water
  has no such variant — it is permanently on the `preserve` shape that was
  introduced for refractive glass. `b15b0527` (`reflectedCoverage = 1.0 -
  (1.0 - baseAlpha) * (1.0 - fresnel)`) raises water's output alpha toward
  1.0 at grazing angles, which increases how much of `direct` the water owns
  while `indirect * albedo` stays at 100 % — so the mismatch got larger with
  that commit without the composite side being revisited.
- **Evidence**: `water.rs` — `let attachments = [hdr_blend, masked_off,
  masked_off, masked_off, masked_off, masked_off, fsr_mask_max,
  fsr_mask_max];` with the comment "Attachments 1..=5 are write-masked off:
  water never updates the G-buffer … so SVGF and motion-vector reprojection
  see only the opaque pass behind the water" — a rationale about *denoiser
  stability*, which does not address composite's reassembly.
  `pipeline.rs::create_blend_pipeline` — `auxiliary_blend, // 4 raw_indirect
  (coverage blend)` / `auxiliary_blend, // 5 albedo (coverage blend)` in the
  non-preserve arm.
- **Impact**: Visual only, exterior/water scenes. Lake and river beds read
  brighter than they should through water, and opaque water (waterfall, lava)
  is contaminated by the GI of whatever it covers. Grazing-angle water
  regions are the worst case post-`b15b0527`. No crash / corruption risk.
- **Related**: #2745 (why refractive glass preserves 3 but not 4/5), #883f57cd
  (the aux-MRT alpha lanes), `b15b0527` (the coverage change).
- **Suggested Fix**: Give the water pipeline the same `auxiliary_blend`
  treatment on attachments 4 and 5 that the ordinary blend pipeline uses, so
  the receiver's demodulated GI is attenuated by the water's own coverage —
  then verify against `docs/smoke-tests/m-exteriors.sh`'s new above/below
  waterline captures. Needs a visual A/B (waterline capture or RenderDoc), not
  a `cargo test`, to confirm the magnitude.

### REN-WD-D15-01: water's authored opacity still discards most of the RT-resolved refraction + absorption in favour of the un-attenuated raster backdrop
- **Severity**: MEDIUM
- **Dimension**: Water
- **Location**: `crates/renderer/shaders/water.frag` (the `── Alpha ──`
  block: `reflectedCoverage` / `float alpha = ...`), consumed by
  `crates/renderer/src/vulkan/water.rs`'s `hdr_blend`
  (`SRC_ALPHA / ONE_MINUS_SRC_ALPHA`)
- **Status**: NEW (residual of the `b15b0527` fix)
- **Description**: `b15b0527` correctly identified that Fresnel was being
  applied twice — once inside `surfaceColor = mix(refrColor, reflColor *
  tint, fresnel)` and again through a low output alpha — and folded the
  reflected share into the coverage
  (`reflectedCoverage = 1.0 - (1.0 - baseAlpha) * (1.0 - fresnel)`). The
  same double-application still exists on the **refraction** half and was
  not addressed. `refrColor` is a fully RT-resolved trace of whatever is
  under the water, already attenuated by `absorbWaterColumn`'s Beer-Lambert
  term. But the surface is then alpha-blended over a framebuffer that
  already contains the *un-attenuated* raster of that same geometry. With a
  legacy ANAM near 0.2 and a face-on view (`fresnel ≈ 0.02`), `alpha ≈ 0.22`
  — so ~78 % of the visible pixel is the raw lake bed and only ~22 % carries
  the authored fog/absorption ramp. The authored `fog_near`/`fog_far`,
  Starfield extinction and pigment-concentration work in
  `absorbWaterColumn` are proportionally discarded.
- **Evidence**: `water.frag` — `refrColor = absorbWaterColumn(hitColor, ...)`
  → `surfaceColor = mix(refrColor, reflColor * push.tint_reflect.w, fresnel)`
  → `outColor = vec4(surfaceColor, alpha)`; `water.rs::build_pipeline`
  `hdr_blend` uses `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`, so the destination is
  the already-rasterized opaque bed. Nothing in the shader consumes
  `refrHit` to decide whether the framebuffer path is still needed.
- **Impact**: Visual only. Vanilla water reads clearer / less tinted than
  authored on every game whose WATR ANAM is low; the deep-colour and
  extinction tuning done in #3224 / #3270 / the Starfield absorption work is
  under-expressed. Bodies whose ANAM is near 1.0 (the 0.88 default and
  waterfalls) are unaffected.
- **Related**: `b15b0527`, #2785 (`fog_near` ramp), #3224, #3270.
- **Suggested Fix**: When the RT refraction resolved (`refrHit` true and
  `sceneFlags.x >= 0.5`), treat the surface as fully covering
  (`alpha → max(alpha, 1.0)` for the transmission share) and keep the
  authored ANAM path only as the no-RT / miss fallback — i.e. let the
  authored opacity select *how much of the framebuffer* substitutes for a
  refraction the engine could not trace, rather than competing with one it
  did. Verify against the new above/below-waterline captures in
  `docs/smoke-tests/m-exteriors.sh`; this is a look change and needs a
  visual A/B, not a unit test.

### REN-WD-D15-02: the boundary-crossing LOD reconcile (including LOD water) is now unbounded and deadline-free
- **Severity**: MEDIUM
- **Dimension**: Water (LOD water / streaming interaction)
- **Location**: `byroredux/src/streaming_helpers.rs`
  (`lod_reconcile_budget_for_frame`'s `grid_changed` arm and
  `reconcile_lod_rings`'s `make_budget`), `byroredux/src/app_step.rs`
  (the `(!grid_changed).then_some(streaming_deadline)` argument)
- **Status**: NEW
- **Description**: `b15b0527` changed the exterior boundary-crossing frame
  from doing **zero** LOD reconcile work (`Some(0)`) to doing **unbounded**
  work: `lod_reconcile_budget_for_frame` now returns `Some(usize::MAX)` when
  `grid_changed`, and `app_step.rs` simultaneously passes `None` for the
  wall-clock deadline on exactly those frames. `make_budget`'s
  `(usize::MAX, _) => LodWorkBudget::unlimited()` arm then drops both the
  per-provider attempt cap and the deadline for terrain, object and
  placement LOD (LOD water planes ride the same reconcile). The stated
  rationale — presentation-atomic handoff, no budget-shaped empty strip — is
  sound, but the mechanism removes both bounds at the single frame with the
  most newly-exposed LOD footprint.
  This is the same shape as #3540, where an unbounded per-frame recovery
  batch (`restore_missing_static_blas_for_draws`) put Starfield's
  `citycydoniamainlevel` on one frame for over ten minutes and was fixed by
  adding `plan_static_blas_restore` + `MAX_STATIC_BLAS_RESTORES_PER_FRAME`.
- **Evidence**: `streaming_helpers.rs` —
  `} else if grid_changed { ... Some(usize::MAX) }` and
  `let make_budget = || match (max_attempts_per_provider, deadline) {
  (usize::MAX, _) => LodWorkBudget::unlimited(), ... }`; `app_step.rs` —
  `(!grid_changed).then_some(streaming_deadline)`. The `make_budget` comment
  still describes the old contract ("`usize::MAX` remains the deterministic
  full-radius bootstrap contract"), which is no longer the only caller.
- **Impact**: A hitch proportional to the newly-exposed LOD ring on every
  exterior cell-boundary crossing, on the largest worldspaces, with no
  ceiling. Correctness is unaffected. Whether it is observable depends on
  the ring size and provider cost — this needs a bench/frame-time
  measurement on a large exterior (`--grid` traversal) rather than
  reasoning.
- **Related**: #3540 (the precedent bound), #2376 / EX-07 (the deadline
  contract this bypasses), `b15b0527`.
- **Suggested Fix**: Keep the atomic-handoff intent but bound it — e.g. a
  boundary-specific cap analogous to `MAX_STATIC_BLAS_RESTORES_PER_FRAME`,
  or keep the deadline and only lift the per-provider attempt cap — and
  update `make_budget`'s comment, which still claims `usize::MAX` is
  bootstrap-only.

---

## Findings — LOW

### REN-WD-D1-01: `STATIC_BLAS_FLAGS` doc and `build_blas_batched`'s eviction comment still name the deleted single-shot `build_blas` site
- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/constants.rs`
  (`STATIC_BLAS_FLAGS` docstring), `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (pre-batch eviction comment inside `build_blas_batched`)
- **Status**: NEW (residual site of CLOSED #2914)
- **Description**: #2914 deleted the never-called single-shot `build_blas` /
  `build_blas_for_mesh` pair and updated `docs/engine/memory-budget.md`, but
  two in-code doc comments were not updated. `STATIC_BLAS_FLAGS` documents
  "the static-BLAS BUILD call sites in `blas_static.rs` (`build_blas`
  single-shot plus `build_blas_batched` per-mesh size-query and per-mesh
  record)" — three sites where two exist. `build_blas_batched`'s pre-batch
  eviction comment opens "#2692 — as at the single-shot site above", pointing
  at a site that no longer exists anywhere in the file.
- **Evidence**: `grep -rn "fn build_blas" crates/renderer/src/` returns only
  `blas_static.rs::build_blas_batched` and its `context/resources.rs` wrapper;
  `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs` records
  "#2914 deleted the third — the never-called single-shot".
- **Impact**: Documentation only. The concrete cost is auditor time: the
  VUID-03801 "all static-BLAS sites must share the flag constant" invariant is
  stated against a site count that no longer matches the code, which is
  exactly the class of drift that produced the false Dimension-1 premise
  #3576.
- **Related**: #2914, #1892, #3576.
- **Suggested Fix**: Rewrite both comments to name only
  `build_blas_batched`'s size-query and record sites.

### REN-WD-D2-02: `audit-renderer/SKILL.md`'s Dimension 2 checklist still describes the removed shader-side `GLASS_RAY_BUDGET` admission gate
- **Severity**: LOW
- **Dimension**: SSBO/Indexing (audit-skill doc-rot)
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 2,
  glass/IOR bullet); live code in `crates/renderer/shaders/triangle.frag`,
  `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`,
  `crates/renderer/src/shader_constants.rs`
- **Status**: NEW
- **Description**: The checklist says "`GLASS_RAY_BUDGET` (from
  `shader_constants.glsl`) cap wired; the budget `atomicAdd` overshoots
  unconditionally by design (#1438)". In the live shader the cap is **not**
  wired: `GLASS_RAY_BUDGET` is emitted into `shader_constants.glsl` but has no
  consumer in any `.frag`/`.comp`/`.vert`. Admission is now
  `qualityTier`-based (`budgetTier = min(rayBudget.qualityTier, 3u)`), the
  `atomicAdd(rayBudget.rayBudgetCount, glassRayCost)` is telemetry only, and
  `crates/renderer/src/shader_constants.rs` carries a *negative* assertion
  (`!src.contains("old + glassRayCost <= rayBudget.glassRayLimit")`) pinning
  the old gate's absence. `ray_budget.rs` documents `glass_ray_limit` as
  "retained in the ABI for telemetry".
- **Evidence**: `grep -rn GLASS_RAY_BUDGET crates/renderer` → definition in
  `shader_constants_data.rs`, emission in `shader_constants.rs`, derivation of
  the per-tier `glass_ray_limit` in `ray_budget.rs`, and the `#define` in
  `include/shader_constants.glsl` — no shader reads it.
- **Impact**: Audit-methodology only. An auditor following the checklist looks
  for a gate that was deliberately deleted and can file a false "the cap
  regressed" finding, or miss that the real limiter is now `qualityTier`.
- **Related**: #1438, #2686 (`glass_ray_limit_tiers_derive_from_*`).
- **Suggested Fix**: Reword the bullet to "the glass ray budget is
  `qualityTier`-gated; `GLASS_RAY_BUDGET` only derives the per-tier
  `glass_ray_limit` telemetry value — verify the negative pin in
  `shader_constants.rs` still holds".

### REN-WD-D8-02: the water pipeline hand-rolls the HDR alpha (coverage) lane instead of using `coverage_alpha_factors`, so water overwrites accumulated transparent coverage
- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/water.rs` (`hdr_blend` in
  `build_pipeline`), `crates/renderer/src/vulkan/pipeline.rs`
  (`coverage_alpha_factors`), `crates/renderer/shaders/composite.frag`
  (the `is_sky` arm's `float coverage = clamp(direct4.a, 0.0, 1.0);`)
- **Status**: NEW
- **Description**: `coverage_alpha_factors` exists so the HDR attachment's
  alpha channel behaves as an **accumulated** coverage lane for
  sky-silhouetted transparents (#2466): for a classic
  `ONE_MINUS_SRC_ALPHA` blend it returns `(ONE, ONE_MINUS_SRC_ALPHA)`, the
  over-operator. The water pipeline does not call it — it hardcodes
  `.src_alpha_blend_factor(ONE).dst_alpha_blend_factor(ZERO)`, which
  **replaces** the destination coverage with the water fragment's own alpha.
  When water draws over an already-blended transparent that has nothing
  opaque behind it (ocean/LOD water at the horizon behind spray, fog cards or
  particles), composite's sky arm then computes `compute_sky(dir) * (1 -
  waterAlpha)` and re-admits sky the earlier layer had already covered.
- **Evidence**: `water.rs` `hdr_blend` builder chain vs
  `pipeline.rs::coverage_alpha_factors`, whose doc comment states the lane's
  purpose and whose only callers are the opaque/blend pipeline paths.
- **Impact**: A brightness/haze seam where water overlaps another transparent
  against open sky. Single-layer water over sky is unaffected (replace and
  accumulate agree when the destination coverage is 0). Visual only.
- **Related**: #2466, #2920.
- **Suggested Fix**: Route water's HDR attachment alpha factors through
  `coverage_alpha_factors(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)` so all
  transparent writers share one coverage convention.

---

## Prioritized Fix Order

Correctness → safety → optimization, within this scoped slice:

1. **REN-WD-D2-01** (MEDIUM, safety) — bound the water caustic
   `imageAtomicAdd` on `imageSize(waterCausticAccum)` instead of
   `GpuCamera.screen`. Smallest change, removes a dependence on
   out-of-bounds image-atomic behaviour on the degraded path, and makes the
   #2784 comment true. Mirror it in `caustic_splat.comp`.
2. **REN-WD-D15-01** (MEDIUM, look) — stop letting the authored ANAM opacity
   compete with an RT refraction the engine already resolved; gate the
   framebuffer-transmission share on `refrHit`. This is the change that makes
   the authored fog/extinction work visible, and it composes with (1).
3. **REN-WD-D8-01** (MEDIUM, look) — give water the `auxiliary_blend`
   coverage treatment on G-buffer attachments 4/5 so the receiver's
   demodulated GI is attenuated by water coverage. Do this **after** (2), as
   the two interact: (2) raises water's effective coverage further.
4. **REN-WD-D15-02** (MEDIUM, performance) — re-bound the boundary-crossing
   LOD reconcile. Measure first: this is a deliberate trade in `b15b0527`, so
   the fix is only warranted if the hitch is real on a large exterior.
5. **REN-WD-D8-02** (LOW) — route water's HDR alpha lane through
   `coverage_alpha_factors` so all transparent writers share one coverage
   convention.
6. **REN-WD-D1-01** (LOW) and **REN-WD-D2-02** (LOW) — the two documentation
   corrections. Cheap, and both are the premise-rot class that has already
   produced false findings in this subsystem (#3576).

## Needs visual / measured verification (not `cargo test`-visible)

| Finding | What is needed |
|---|---|
| REN-WD-D15-01 | Above/below-waterline capture A/B via `docs/smoke-tests/m-exteriors.sh` on a low-ANAM water body (FNV / Oblivion medians) |
| REN-WD-D8-01 | Same capture set; look for lake/river-bed brightness through water and GI contamination on an opaque waterfall |
| REN-WD-D8-02 | A scene with water overlapping another transparent against open sky (ocean horizon behind spray) |
| REN-WD-D15-02 | Frame-time trace across an exterior cell-boundary crossing on a large worldspace (`--grid` traversal) |

No barrier / render-pass / pipeline-dependency change is proposed anywhere in
this report. REN-WD-D2-01 and REN-WD-D8-01/02 are blend-state and shader-math
changes whose effects are inspectable in a capture; none of them alters a
synchronization scope.

## Notes on prior-audit premises checked and found still valid

- The Dimension-1 recovery path (`restore_missing_static_blas_for_draws`) and
  its ordering pins are intact — do not re-file #3576's already-corrected
  premise.
- The `--grid` false-eviction half of #1793 remains open by design (gated
  behind `static_blas_bytes > budget`, unreachable on the dev card); it was
  not re-reported.
- `caustic_splat.comp`'s "water-side caustic is the water shader's
  responsibility (M38)" comment matches a live, non-stub `water.frag`
  implementation.
