# Renderer Audit — Dimensions 9 + 10 (RT Ray Queries, Denoiser & Composite)

**Date**: 2026-05-17
**Scope**: `--focus 9,10 --depth deep`. RT ray-query shader sites
(`triangle.frag`), SVGF temporal denoiser (`svgf.rs` + `svgf_temporal.comp`),
composite pipeline (`composite.rs` + `composite.frag`).
**Methodology**: Per `.claude/commands/_audit-common.md`. Dedup baseline:
`gh issue list --limit 200` (200 most-recent issues) + prior renderer
audits 2026-04-02 → 2026-05-16. ID indices picked to avoid collisions
with `REN-D9-NEW-01..06` and `REN-D10-NEW-01..09` already filed.

## Executive Summary

6 findings total across the two dimensions:

| Severity | Count | Notes                                                                                                             |
|----------|-------|-------------------------------------------------------------------------------------------------------------------|
| CRITICAL | 0     |                                                                                                                   |
| HIGH     | 0     |                                                                                                                   |
| MEDIUM   | 2     | Both `dim_10`. Two compounding build-time bugs that block shader recompile-from-source for 7 of the 16 shaders.   |
| LOW      | 3     | One Dim-9 cosmetic doc-rot, one Dim-10 SVGF bit-mask inconsistency, one Dim-10 deprecated-Vulkan-flag cleanup.    |
| INFO     | 1     | Long-session (>3 day) frame-counter precision degradation in the RT noise seed.                                   |

**Pipeline areas affected**: SVGF temporal denoiser (1 LOW), composite
render-pass dependency (1 LOW), shader build pipeline (2 MEDIUM), RT
ray-query documentation hygiene (1 LOW), camera-UBO frame counter (1 INFO).

The 2 MEDIUM findings are **build-blockers** for shader iteration:
seven shaders use `#include "include/shader_constants.glsl"` without
declaring `GL_GOOGLE_include_directive`, AND `composite.frag` redeclares
`BLOOM_INTENSITY` as a `const float` after the `#include` `#define`s
it. Shipped `.spv` files mask both issues; the documented build command
in `composite.frag`'s own header (`glslangValidator -V -I crates/renderer/shaders
composite.frag -o composite.frag.spv`) fails twice over.

The 3 LOW findings are non-blocking cleanups but each has a clear minimal
fix.

## RT Pipeline Assessment

All five `rayQueryEXT` sites in `triangle.frag` were re-verified end-to-end:

- **`traceReflection`** (`:447-510`) — invoked from glass IOR
  (`:1692`) and metal/glossy reflection (`:2108`). Both call sites
  apply `+ N_bias * <0.05 | 0.1>` origin bias matching the helper's
  `tMin = 0.05`. Miss fallback is `skyTint.xyz * 0.5 + sceneFlags.yzw *
  0.5` (half-sky / half-cell-ambient). Hit path bary-interpolates UVs
  via `getHitUV` from the global vertex SSBO and routes through
  `materials[hit.materialId]` for the per-material UV transform.
- **Window-portal ray** (`:1543-1556`) — fires along `-N` from
  `fragWorldPos - N * 0.15` (start outside the pane); 2000-unit tMax.
  Intentional bias-against-N_bias is documented at `:1532-1541`
  ("do NOT replace `N` here with `N_bias`"). #421 / #821 pinned.
- **IOR refraction passthru loop** (`:1773-1829`) — `REFRACT_PASSTHRU_BUDGET
  = 2` skips self-texture / fallback-texture hits then commits at the
  terminating iteration. `GLASS_RAY_BUDGET = 8192` claimed via atomic
  with `GLASS_RAY_COST = 4` (worst case: 1 reflection + 3 refraction).
  Frisvad basis for roughness spread at `:1716` resolves the
  cross-product singularity at normal incidence (#820). Miss fallback
  is cell-ambient (`bb53fd5`).
- **Cluster reservoir shadow ray** (`:2470-2487`) — WRS streaming
  (`NUM_RESERVOIRS = 8`) over the cluster's lights; pass-2 shadow rays
  cast for sampled reservoirs with `gl_RayFlagsTerminateOnFirstHitEXT
  | gl_RayFlagsOpaqueEXT`. Point/spot disk-jitter on a plane
  perpendicular to L; directional sun-cone jitter via Frisvad basis
  + `sunAngularRadius` from `skyTint.w` (#1023). Sentinel-init at
  `:2192-2196` correctly gates pass-2 when `rtEnabled == false`.
- **GI bounce ray** (`:2549-2603`) — cosine-weighted hemisphere
  sample around the GEOMETRIC normal (not the bump-perturbed one,
  to prevent Quonset-hut groove false-occlusion); `tMin = 0.05`
  matching the `N_bias * 0.1` origin offset; `tMax = 6000.0` matching
  the `smoothstep(4000, 6000)` fade-end. Miss fallback is
  `sceneFlags.yzw * 0.5` (cell ambient), faded by `giFade`.

The RT ray-query path is in a healthy state. Dim 9 surfaces nothing
load-bearing — `REN-D9-NEW-07` (doc-comment line-number drift) is
purely cosmetic and `REN-D9-NEW-08` (long-session frame-counter precision)
fires only past ~3 days uninterrupted play.

## Rasterization Assessment

SVGF temporal denoiser + composite reassembly are mechanically sound:
ping-pong slot pairing (`prev = (f + 1) % MFIF`), per-FIF
`frames_since_creation` counter with first-frame reset gate, bilinear
4-tap reconstruction with mesh-ID + normal-cone consistency, NaN/Inf
defence-in-depth, weighted-average history age for fast disocclusion
recovery. Composite reassembly is `direct + indirect * albedo + caustic`
(direct already bakes albedo via BRDF; indirect is lighting-only per
#268; caustic is `albedo * causticLum` from the R32_UINT atomic
accumulator). ACES is applied AFTER reassembly. Volumetric attenuation
uses Frostbite §5.3 form (`combined * vol.a + vol.rgb`) with a host-side
`VOLUMETRIC_OUTPUT_CONSUMED` gate to skip when the integrate pass isn't
running. Display-space aerial-perspective fog is the exterior fallback
when volumetrics is off.

The two MEDIUM findings are upstream of any of this — they block
recompile, not runtime.

## Findings

### MEDIUM

#### REN-D10-NEW-10: 7 shaders use `#include` without declaring `GL_GOOGLE_include_directive` — recompile-from-source fails
- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite (anchored on `composite.frag`; the
  same defect affects 6 sibling shaders)
- **Location**:
  - `crates/renderer/shaders/composite.frag:1-7` (missing `#extension`)
  - `crates/renderer/shaders/triangle.frag` — same defect
  - `crates/renderer/shaders/bloom_downsample.comp` — same
  - `crates/renderer/shaders/bloom_upsample.comp` — same
  - `crates/renderer/shaders/cluster_cull.comp` — same
  - `crates/renderer/shaders/volumetrics_inject.comp` — same
  - `crates/renderer/shaders/volumetrics_integrate.comp` — same
  - Reference (correctly declared): `triangle.vert:2-3`,
    `skin_vertices.comp`, `taa.comp`
- **Status**: NEW
- **Description**: After Session 35–37's `shader_constants.glsl`
  consolidation (#1038 / #1042 / #1119), seven shaders now start with
  `#include "include/shader_constants.glsl"`. But unlike
  `triangle.vert` (and other shaders consolidated in the same wave),
  these seven are missing the corresponding
  `#extension GL_GOOGLE_include_directive : require` declaration.
  `glslangValidator -V composite.frag` errors out at the `#include`
  with `'#include' : required extension not requested`. The shipped
  `.spv` predates the consolidation and is bundled via `include_bytes!`,
  so the engine runs fine — but iteration on any of these shaders
  requires the developer to first patch the missing extension line,
  breaking the documented "Compile with: `glslangValidator -V -I crates/renderer/shaders
  composite.frag -o composite.frag.spv`" workflow that's literally in
  `composite.frag`'s own header at line 6.
- **Evidence**:
  ```bash
  $ for f in *.frag *.vert *.comp; do
      if grep -q "#include" "$f" && ! grep -q "GL_GOOGLE_include_directive" "$f"; then
        echo "MISSING: $f"
      fi
    done
  MISSING: composite.frag
  MISSING: triangle.frag
  MISSING: bloom_downsample.comp
  MISSING: bloom_upsample.comp
  MISSING: cluster_cull.comp
  MISSING: volumetrics_inject.comp
  MISSING: volumetrics_integrate.comp

  $ cd crates/renderer/shaders && glslangValidator -V composite.frag -o /tmp/test.spv
  ERROR: composite.frag:7: '#include' : required extension not requested
  ```
- **Impact**: Latent build-breakage. Anyone iterating on any of the
  7 shaders has to first realize the missing extension is the cause
  of the cryptic glslang error before they can adjust constants.
  Tuning bloom intensity, fog math, sky colour, cluster bounds,
  volumetrics step count are all in-scope for active development;
  the .spv-shipped workflow masks this until the day someone needs
  to change a shader.
- **Related**: #1038 (shader constants drift-detect tests),
  #1119 (DBG_* bit catalog mirror); both are Rust↔GLSL drift catches
  but neither verifies the GLSL actually compiles. The Rust-side
  `validate_set_layout` SPIR-V reflection (`reflect.rs`) reads the
  pre-compiled `.spv`, so it doesn't fire either.
- **Suggested Fix**: Add `#extension GL_GOOGLE_include_directive : require`
  immediately after the `#version` line in each of the 7 affected
  shaders. Then optionally extend `build.rs` (or add a CI lane) to
  recompile every shader and fail if any source can't be regenerated
  byte-identically to the shipped `.spv` — that catches both this
  defect and any future const-redeclaration issue (see REN-D10-NEW-11).

#### REN-D10-NEW-11: `composite.frag` redeclares `BLOOM_INTENSITY` after `#include` defines it — recompile-from-source fails (compounds with NEW-10)
- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/composite.frag:94`
  (`const float BLOOM_INTENSITY = 0.15;`) shadowing
  `crates/renderer/shaders/include/shader_constants.glsl:44`
  (`#define BLOOM_INTENSITY 0.15`)
- **Status**: NEW (escalation of LOW-cosmetic REN-D6-NEW-01)
- **Description**: REN-D6-NEW-01 was filed as cosmetic
  (`BLOOM_INTENSITY duplicated as both #define ... and const float in
  composite.frag`). Re-examined under the recompile-from-source angle,
  this is a build-blocker: after `#include "include/shader_constants.glsl"`
  at line 7 expands the `#define BLOOM_INTENSITY 0.15` into scope, line 94's
  `const float BLOOM_INTENSITY = 0.15;` is textually substituted to
  `const float 0.15 = 0.15;` — a GLSL syntax error. The `.spv` shipped
  was compiled before `BLOOM_INTENSITY` moved into the shared header,
  so the runtime works; the recompile path is dead. Stacks with
  REN-D10-NEW-10 (the extension-declaration issue) — neither alone
  produces a useful rebuild today.
- **Evidence**:
  - `shader_constants.glsl:44` — `#define BLOOM_INTENSITY 0.15`
  - `composite.frag:7` — `#include "include/shader_constants.glsl"`
  - `composite.frag:94` — `const float BLOOM_INTENSITY = 0.15;`
  - `composite.frag:435` — `combined += bloom * BLOOM_INTENSITY;` (single consumer)
  - No `#undef BLOOM_INTENSITY` between line 7 and line 94.
  - Rust mirror: `src/shader_constants_data.rs:44` —
    `pub const BLOOM_INTENSITY: f32 = 0.15;`, exposed via
    `bloom.rs:92` (`pub const DEFAULT_BLOOM_INTENSITY: f32 = crate::shader_constants::BLOOM_INTENSITY;`).
- **Impact**: Same workflow trap as NEW-10. Iterating on bloom intensity
  via shader recompile is broken until the developer drops the const.
  Combined with the Rust-side const + .glsl `#define` + const-in-frag,
  the source of truth is fragmented across 3 places — the doc-comment
  at `composite.frag:91` ("Pinned in lockstep with `bloom::DEFAULT_BLOOM_INTENSITY`
  ... update both at once") is now inaccurate (it's 3, not 2).
- **Related**: REN-D6-NEW-01 (filed cosmetic 2026-05-16), #1038 (drift
  test landed for the `composite_frag_caustic_fixed_scale_matches_rust_const`
  pattern — extend to `BLOOM_INTENSITY` here).
- **Suggested Fix**: Drop the `const float BLOOM_INTENSITY = 0.15;` at
  `composite.frag:94` (and the doc-comment 83-93 explaining the value —
  move to `src/shader_constants_data.rs`). The `#include`-d `#define`
  is the single source of truth. Then add a
  `composite_frag_bloom_intensity_matches_rust_const` drift test modelled
  on the `caustic_fixed_scale` pattern so any future re-divergence trips
  a unit-test failure rather than a silent .spv-vs-source skew.

### LOW

#### REN-D9-NEW-07: Stale doc-comment line numbers in `traceReflection`
- **Severity**: LOW
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:449-455`
- **Status**: NEW
- **Description**: The `tMin = 0.05` rationale comment cites caller bias
  values `0.05 and 0.1` at lines `1633 and 2049`. Post-Session-34 split
  the actual sites are at `1692` (glass IOR reflection ray) and `2108`
  (metal/glossy reflection ray). The bias values are still correct (0.05
  and 0.1); only the line-number anchors are stale.
- **Evidence**:
  ```glsl
  // tMin = 0.05 matches the N_bias offset every caller already applies
  // to `origin` (callers at lines 1633 and 2049 use bias 0.05 and 0.1
  // respectively) and the convention every other ray-query site in
  // this shader uses (1486, 1702, 2408, 2484). Pre-#1017 this was 0.01
  ```
  Actual call sites (verified via grep):
  - `triangle.frag:1692` — `traceReflection(fragWorldPos + N_bias * 0.05, R, 3000.0);` (glass IOR)
  - `triangle.frag:2108` — `traceReflection(fragWorldPos + N_bias * 0.1, jitteredR, 5000.0);` (metal reflection)
  The "other ray-query sites" anchors (1486, 1702, 2408, 2484) are also
  drifted vs the current 1543 / 1774 / 2470 / 2549 set.
- **Impact**: Documentation rot. Anyone tracing `tMin` invariants from
  this comment lands in unrelated code; the audit-skill path-validate
  gate (#1114 / TD7-050) catches backticked paths but not free-text
  line numbers, so this kind of drift persists between Session 34's
  large-module split and the next time someone touches the comment.
- **Related**: Session 34 split sweep (HISTORY.md), #1114 path-validate gate.
- **Suggested Fix**: Update the line-number anchors to the current sites:
  callers `1692` / `2108`; sibling ray-query origins `1543` (window portal),
  `1774` (refraction loop), `2470` (cluster shadow), `2549` (GI bounce).
  Or — since these will drift again — replace with grep-friendly anchor
  comments like `// see windowRQ at the isWindow/glassIORAllowed split`.

#### REN-D10-NEW-12: SVGF nearest-tap fallback compares `nearID == currID` without `ALPHA_BLEND_NO_HISTORY` mask
- **Severity**: LOW
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp:217`
- **Status**: NEW
- **Description**: The bilinear consistency loop at lines 140-192
  masks bit 31 (the `ALPHA_BLEND_NO_HISTORY` marker added by #904 /
  #992) on both `prevID` and `currID` before comparing:
  ```glsl
  if ((prevID & 0x7FFFFFFFu) != (currID & 0x7FFFFFFFu)) continue;
  ```
  The sub-pixel-motion fallback below it at line 217 uses unmasked
  equality:
  ```glsl
  if (nearID == currID && dot(currN, nearN) >= 0.9) {
  ```
  The early-out at line 97 already guarantees `currID`'s bit 31 is
  unset (returns before this code for sky / alpha-blend), so the only
  way the comparison falsely fails is when `nearID` has bit 31 set
  (previous frame was alpha-blend at this pixel) and the underlying
  31-bit instance ID happens to match. Same-instance opaque ↔
  alpha-blend transitions are rare but real (glass props with
  stage-controlled opacity, character cloaks moving between
  alpha-tested and alpha-blended draw paths during animation phases).
- **Evidence**: Compare `svgf_temporal.comp:151` (masked) with `:217`
  (unmasked). Both gates serve the same "same surface" check.
- **Impact**: Niche. The fallback only fires when:
  (a) all 4 bilinear taps were rejected by the SH-5 normal-cone check
      (`dot(currN, prevN) < 0.9`), AND
  (b) motion magnitude < 1.5 px, AND
  (c) the same-instance opaque/alpha-blend transition is in progress
      at this sub-pixel.
  Under those conditions, a single-frame disocclusion sparkle on the
  transitioning fragment instead of the smooth fallback the masking
  was designed to give.
- **Related**: #904 / #992 (alpha-blend bit-31 encoding), #1131
  (sub-pixel motion fallback added in REN-D10-NEW-01 → also masked
  inconsistency).
- **Suggested Fix**: Mirror the bilinear loop's mask:
  ```glsl
  if ((nearID & 0x7FFFFFFFu) == (currID & 0x7FFFFFFFu) && dot(currN, nearN) >= 0.9) {
  ```
  Pure 1-line change. No behavioural change in the dominant path
  (currID's bit 31 is always 0 by the early-out, so masking currID
  is a no-op; masking nearID is the actual fix).

#### REN-D10-NEW-13: Composite outgoing render-pass dependency uses deprecated `BOTTOM_OF_PIPE`
- **Severity**: LOW
- **Dimension**: Denoiser & Composite
- **Location**: `crates/renderer/src/vulkan/composite.rs:448`
- **Status**: NEW
- **Description**: The outgoing subpass dependency
  (`composite_dep_out`, subpass 0 → SUBPASS_EXTERNAL) sets
  `dst_stage_mask(BOTTOM_OF_PIPE)` with `dst_access_mask(empty)`. This
  is the legacy "release ownership / no further synchronization
  required" idiom. Vulkan 1.3 deprecated `BOTTOM_OF_PIPE` and
  `TOP_OF_PIPE` in favour of `vk::PipelineStageFlags::NONE`. The dual
  closeouts already happened on the SRC side (#949 / #1100 / #1121
  / #1122 migrated 8+ `TOP_OF_PIPE` source masks to `NONE`); this site
  is the matching DST-side leftover. SVGF's own
  `initialize_layouts` at `svgf.rs:753` already uses `NONE` for the
  same scenario — composite is the odd one out.
- **Evidence**:
  ```rust
  let composite_dep_out = vk::SubpassDependency::default()
      .src_subpass(0)
      .dst_subpass(vk::SUBPASS_EXTERNAL)
      .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
      .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
      .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)  // ← deprecated
      .dst_access_mask(vk::AccessFlags::empty());
  ```
- **Impact**: None at runtime — every IHV driver still accepts
  `BOTTOM_OF_PIPE` and treats it equivalently to `NONE` under the
  Vulkan 1.3 spec's compatibility section. Pure mechanical cleanup
  / validation-cleanliness sweep.
- **Related**: #1121 / `a49eb945` (six `TOP_OF_PIPE` → `NONE` migrations
  on the SRC side), #1122 (TLAS count invariant test + sibling sites).
- **Suggested Fix**: Migrate to `vk::PipelineStageFlags::NONE`. One-line
  change, matches the sibling `initialize_layouts` site in SVGF / SSAO /
  caustic.

### INFO

#### REN-D9-NEW-08: Frame-counter `u32 → f32` cast loses 1-frame precision after ~16.7M frames
- **Severity**: INFO
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:465`,
  `crates/renderer/shaders/triangle.frag:342` (`interleavedGradientNoise`)
- **Status**: NEW
- **Description**: `cameraPos.w` is assembled as `self.frame_counter as f32`
  where `frame_counter: u32`. Past `2^24 = 16_777_216` frames (~3.2 days
  at 60 FPS, ~6.5 days at 30 FPS) the IEEE-754 single-precision rounding
  step exceeds 1.0, so consecutive frames map to the same `cameraPos.w`
  value. Every IGN call seeds noise from this float, so per-pixel
  temporal patterns freeze. The reservoir streaming (line 2369), shadow
  jitter (2407-2408), glass refraction roughness spread (1703-1706),
  metal reflection cone sample (2102-2103), and GI hemisphere sample
  (2513-2514) all degrade in lockstep — the soft penumbras and
  TAA-friendly accumulated smoothing stop refreshing.
- **Evidence**:
  - `context/draw.rs:465` — `self.frame_counter as f32,`
  - `triangle.frag:342` — `float interleavedGradientNoise(vec2 fragCoord, float frameCount)`
  - `f32` mantissa precision drops to ±1 around `2^24`
  - The GI path at line 2512 uses `floor(frameCount * 0.25)` so its
    effective freeze threshold is ~67M frames (~12.8 days at 60 FPS),
    but shadow / reflection / refraction sites use `frameCount` directly.
- **Impact**: Visible only in long uninterrupted play sessions (multi-day
  exterior cell loops). At typical gameplay duration this is unreachable.
  Bound by `frame_counter: u32` wrap at `2^32` frames (~2.3 years) so even
  the worst case is observable rather than crash-y.
- **Related**: REN-D7-NEW-07 (already-fixed: resize resets `frame_counter
  = 0`, so cell transitions during a long session may unintentionally
  reset the precision drift — different concern).
- **Suggested Fix**: Either (a) upload `(frame_counter & 0xFFFFFFu) as f32`
  to wrap noise seed at the precision boundary, or (b) accept as-is and
  document the limit alongside the existing `frame_counter` doc-comment
  at `context/mod.rs:724`. No urgency — typical play sessions are
  << 3 days.

## Prioritized Fix Order

Correctness first, then safety, then optimization / cosmetic:

1. **REN-D10-NEW-10** (MEDIUM, build): Add the missing
   `#extension GL_GOOGLE_include_directive : require` to the 7 affected
   shaders. Restores recompile-from-source. ~7-line patch.

2. **REN-D10-NEW-11** (MEDIUM, build): Drop the `const float BLOOM_INTENSITY
   = 0.15;` redeclaration in `composite.frag:94`. Add a Rust-side drift
   test mirroring `composite_frag_caustic_fixed_scale_matches_rust_const`.
   Depends on NEW-10 to be useful.

3. **REN-D10-NEW-12** (LOW, denoiser): Mask the SVGF nearest-tap
   fallback's `nearID == currID` with `& 0x7FFFFFFFu`. 1-line change.

4. **REN-D10-NEW-13** (LOW, sync): Migrate composite's outgoing
   render-pass `dst_stage_mask` from `BOTTOM_OF_PIPE` to `NONE`. 1-line.

5. **REN-D9-NEW-07** (LOW, docs): Update stale line-number anchors in
   `traceReflection`'s doc-comment, or replace with grep-friendly symbolic
   anchors.

6. **REN-D9-NEW-08** (INFO, longevity): Either wrap `frame_counter`
   modulo `0xFFFFFF` at upload time, or document the 3-day limit. No
   urgency.

## What's NOT a bug

Detailed "verified-good" lists for both dimensions are in the per-dim
intermediate reports. Key checks that passed:

- All five RT ray-query sites (window portal, glass IOR refraction,
  cluster shadow, GI bounce, helper `traceReflection`) use consistent
  `gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT`, identical
  `topLevelAS` binding, `N_bias`-aligned origin offsets, and `tMin = 0.05`
  matching the convention from #1017.
- Frisvad basis is used wherever a direction needs an orthonormal frame
  (refraction roughness spread, cluster shadow jitter, GI hemisphere)
  — no `cross(N, world-up)` degeneracies remain.
- WRS reservoir streaming + sentinel-initialization correctly gates
  pass-2 shadow rays when `rtEnabled == false` without any explicit
  `rtEnabled` check on pass 2.
- SVGF ping-pong (`prev = (f + 1) % MFIF`), per-FIF `frames_since_creation`
  counter, NaN/Inf history-tap drop, weighted-average history age, and
  first-frame `should_force_history_reset` gate are all correct.
- Composite reassembly formula `direct + indirect * albedo + caustic`
  is consistent with the `triangle.frag` outRawIndirect contract (#268),
  ACES is applied after reassembly, and SSAO is correctly applied to
  indirect-lighting components only (in `triangle.frag`, not in
  `composite.frag`).
- Caustic accumulator is sampled correctly as `usampler2D` R32_UINT
  with the NEAREST sampler (`composite.rs:362-374`), divided by
  `CAUSTIC_FIXED_SCALE = 65536.0` (from `shader_constants.glsl`), and
  added as a separate term to `combined` (not run through SVGF).
- Composite render pass uses `format = swapchain_format`,
  `final_layout = PRESENT_SRC_KHR`, `load_op = DONT_CARE`,
  `store_op = STORE`, and the incoming subpass dependency correctly
  covers both `COLOR_ATTACHMENT_OUTPUT` and `COMPUTE_SHADER` producer
  paths (#963 / REN-D10-NEW-06).

## Out of scope

The following surfaced during the audit but belong to other dimensions
and are deferred:

- `BLOOM_INTENSITY` cross-source drift detection — proposed in NEW-11's
  fix; full coverage of all Rust↔GLSL constants is Dim 3 (Pipeline State).
- TAA history per-FIF counter sharing (REN-D12-NEW-01) — Dim 11.
- Caustic compute pass (atomic accumulation, barriers) — Dim 13.
- Bloom pyramid math (5-down + 4-up, B10G11R11_UFLOAT) — Dim 19.
- Volumetric froxel integrate pass — Dim 18.
- `sun_dir.xyz` host-side normalization contract in `compute_sky` —
  Dim 15 (Sky/Weather).
- Vertex layout / UV offset constants — Dim 3 / Dim 12.
- `GpuInstance` vs `GpuMaterial` `textureIndex` duplication — Dim 14.
- TLAS build-time validity / `built_primitive_count` — Dim 8.
