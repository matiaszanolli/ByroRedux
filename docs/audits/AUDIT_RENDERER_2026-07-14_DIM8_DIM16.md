# Renderer Audit — Dimensions 8 (Denoiser & Composite) + 16 (Volumetrics & Bloom)

- **Date**: 2026-07-14
- **Command**: `/audit-renderer 8 16` → `--focus 8,16 --depth deep`
- **Branch**: main
- **Method**: Orchestrator + 2 dimension agents (renderer-specialist), adversarial per-finding disproof, symbol-anchored verification against the live tree. Reference docs (`docs/engine/shader-pipeline.md`, `docs/engine/memory-budget.md`) treated as authoritative — a code/doc divergence is itself a finding. Dedup baseline: `gh issue list` (30 open) + prior `docs/audits/` reports (most recent same-numbered: `AUDIT_RENDERER_2026-05-21_DIM8.md`, `AUDIT_RENDERER_2026-05-26_DIM16.md`). This pass targets **drift since those audits**, because both dimensions saw substantial change afterward.

**Drift under audit since the last DIM8/DIM16 reports (May):**
- **Dim 8** — the Session-49 **RT denoiser overhaul** (`6b061120`: multi-scatter, à-trous GI wavelet pass `svgf_atrous.comp`, ReSTIR-DI shadow reservoirs) plus `48906670` (firefly-clamp hoist), `b7d1215c` (spatial firefly rejection), `c7ca4864`/`f983b71a` (progressive 1/N + camera-static accumulation), `6c844744` (composite camera-relative `screen_to_world_dir`), `507945d8` (composite loadOp sync), `e4d574dc` (#1894/#1895 docstring corrections).
- **Dim 16** — `68d9c43b` (#1937/#1939 sun-direction sign), `2db2d900` (volumetrics UBO block-size pin), `36f66493` (camera-relative cascade), `6e851403` (TLAS latch reset per frame), `d8ccbc9d` (froxel memory doc), `d11704da`/`6ada7a57` (bloom DC-gain + barrier folding).

---

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 3 | REN-D8-01, V-DIM16-01, B-DIM16-03 |
| INFO | 3 | REN-D8-02, REN-D8-03, V-DIM16-02 |

**No behavioural (CRITICAL/HIGH/MEDIUM) defects in either dimension.** Every
structural invariant and every commit delta from the two post-May change waves
verifies as **holding**. The six findings are all documentation-rot or a
missing-test-guard — the shader/host code is correct throughout.

Both dimensions came through their large recent refactors clean: the à-trous GI
pass integrates without read-write aliasing or caustic feedback into denoised
indirect; the sun-direction sign fix (#1937/#1939) is correctly applied in the
volumetric inject shadow ray; the volumetrics UBO block-size pin and per-frame
TLAS-latch reset are in place; and the #931 bloom pre-barrier removal was NOT
reintroduced by the #6ada7a57 barrier-folding.

No release-blocking issues. No speculative Vulkan changes proposed. No
needs-RenderDoc items.

---

## RT Pipeline Assessment

**Denoiser / composite (Dim 8)** — the temporal→à-trous→composite chain is
correct. History ping-pong reads `indirect_history[prev]` / `moments_history[prev]`
and writes slot `f`; first-frame safety routes through `should_force_history_reset`
→ `params.z` so garbage prev taps are never blended. The new à-trous pass
(`svgf_atrous.comp`, `ATROUS_ITERATIONS = 5`, odd → final slot 0) never
read-write-aliases within a dispatch (`src_pp(k) != dst_pp(k)`, temporal feedback
image read only by iteration 0), and each iteration is ordered by a COMPUTE→COMPUTE
barrier. Motion-vector convention (`outMotion = (currNDC − prevNDC)*0.5` ⇒
`prevUV = uv − motion`) matches `triangle.frag` exactly; disocclusion rejection
uses masked mesh-ID equality AND a normal cone. The firefly clamp (#48906670) sits
ahead of `if (hasHistory)` so it also clamps the no-history path. Caustics ride a
separate accumulator and are added to **direct only** at composite — never fed back
into the SVGF-denoised indirect (double-count guard holds); the #1575
float-before-add u32-wrap guard is intact. Composite `screen_to_world_dir`
subtracts camera position in the same render-origin-relative space as the relative
`inv_view_proj`, so ray reconstruction is correct (#6c844744).

**Volumetrics (Dim 16)** — inject casts a single `TerminateOnFirstHit` opaque
shadow ray per froxel (plus one bounded GLASS ray for the interior window/ceiling
disambiguation), with `ray_dir = normalize(sun_dir.xyz)` toward the sun (#1937
fix, consistent with `composite.frag` and `water.frag`). Integrate multiplies
transmittance across the walk; HG `g` is clamped to (−0.999, 0.999) with a
denominator floor (no NaN). The `VOLUMETRIC_OUTPUT_CONSUMED` gate (#928) skips
inject+integrate entirely when composite drops the sample, and the composite apply
is pinned to the const by a reflect test. Froxel world-position reconstruction adds
`render_origin` to the relative `inv_view_proj` result (#36f66493) — no
absolute/relative mixing. The per-frame TLAS/lights latch reset (#6e851403) means a
stale latch cannot skip the shadow ray.

## GPU-Struct & Memory Assessment

**Volumetrics froxel memory** matches `memory-budget.md`: `160·90·128·8 B = 14.06
MiB` per volume, 2 volumes × 2 FIF ≈ 56 MiB fixed (grid is resolution-independent,
not recreated on resize). The `#2db2d900` UBO block-size pin
(`volumetrics_ubo_sizes_match_host_structs_in_every_shader`) reflects both
`VolumetricsParams` and `IntegrationParams` sizes from the shipped `.spv` and
`assert_eq!`s against the host structs. **Bloom** uses `B10G11R11_UFLOAT_PACK32` on
every mip (no R16G16B16A16 mid-chain), per-FIF mip chains, fence-gated cross-frame
WAR with only post-barriers in the down/up loops (the #931 pre-barrier removal was
not reintroduced by #6ada7a57). SVGF/composite per-FIF images are recreated and all
descriptor sets (incl. à-trous) rewritten on resize, with `frames_since_creation`
zeroed to force a post-resize history reset. No leaks, no lifecycle findings.

---

## Findings

All findings are documentation-rot or a missing test guard. None affects rendering.

### LOW

#### REN-D8-01 — SVGF module-doc binding-6 format stale (`rgba16f` vs `r11f_g11f_b10f`)
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/svgf.rs` module docstring "Descriptor set (binding layout)" table, binding-6 (`outIndirect`) row
- **Status**: OPEN (docstring rot)
- **Description**: The doc table lists binding 6 as `rgba16f`. The shader declares
  it `r11f_g11f_b10f` and the backing image uses `INDIRECT_HIST_FORMAT =
  B10G11R11_UFLOAT_PACK32`. Only binding 7 (`outMoments`, `MOMENTS_HIST_FORMAT =
  R16G16B16A16_SFLOAT`) is actually `rgba16f`. The `e4d574dc` docstring sweep
  (#1894/#1895) missed this row.
- **Evidence**: `svgf_temporal.comp` — `layout(set=0, binding=6, r11f_g11f_b10f) uniform writeonly image2D outIndirect;`; `svgf.rs` — `const INDIRECT_HIST_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;` feeds binding 6.
- **Impact**: None at runtime. Could mislead a maintainer into widening the indirect history image to 8 B/px, undoing the memory the #275 note deliberately halved.
- **Suggested Fix**: Change the binding-6 cell to `image2D (r11f_g11f_b10f / B10G11R11, storage)`; leave binding 7 as `rgba16f`.

#### V-DIM16-01 — `VolumetricsParams::sun_dir` Rust doc asserts the pre-#1937 (wrong) convention
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`, `struct VolumetricsParams`, `sun_dir` field doc comment
- **Status**: OPEN (doc-rot)
- **Description**: The field doc reads *"from sun toward ground"* — the OLD
  convention that `68d9c43b` (#1937/#1939) explicitly corrected. The shader it
  feeds (`volumetrics_inject.comp`) now documents the same field as *direction TO
  the sun … matches GpuLight.direction_angle / GpuCamera.sun_direction*, and the
  host passes `sky_params.sun_direction` (live value `[-0.4, 0.8, -0.45]`, +Y in
  Y-up = pointing up toward the sun). The Rust doc contradicts both the shader and
  the data. The fix commit corrected the three shader comments but not this struct doc.
- **Evidence**: `68d9c43b` touched only `triangle.frag`, `volumetrics_inject.comp`, and their `.spv`. Default `SkyParams.sun_direction = [-0.4, 0.8, -0.45]` (`context/mod.rs`). Shader: `light_in = -normalize(sun_dir.xyz)`; `ray_dir = -light_in` = toward sun — correct.
- **Impact**: None functionally. Risk is a future editor trusting the stale comment and re-negating the sign (exactly the #1937 bug).
- **Suggested Fix**: Update to *"xyz = direction TO the sun (world space, unit; matches GpuCamera.sun_direction / GpuLight.direction_angle, #1937)"*.

#### B-DIM16-03 — bloom_upsample DC-gain note overstates the aggregate ceiling (~8× vs actual ~5×)
- **Dimension**: Bloom
- **Location**: `crates/renderer/shaders/bloom_upsample.comp`, DC-gain note (#1275)
- **Status**: OPEN (doc accuracy)
- **Description**: The note (from `d11704da`) states *"accumulates up to ~8× peak
  at up[0]"*. The **mechanism** it describes matches the code exactly (`upsampled`
  unit-gain 4×0.25, `same` unit-gain, summed with no renormalisation), but the
  aggregate figure is wrong for the shipped `BLOOM_MIP_COUNT = 5` pyramid: the DC
  ceiling is **linear, not geometric** — seed `down[4]=V`; `up[3]=2V`, `up[2]=3V`,
  `up[1]=4V`, `up[0]=5V` → ~5×, not ~8×.
- **Evidence**: Down mips are independent box-downsamples (`down[i]=V` for DC input); `bloom.rs::BloomFrame::new` seeds `up[3]` from `down[4]` and each `up[i]` from `up[i+1]` + `down[i]`; upsample of a constant field = `4×0.25 = V`.
- **Impact**: None on rendering — `BLOOM_INTENSITY = 0.15` is empirically tuned, not derived from the multiplier. Concern is a future reader re-deriving from "~8×".
- **Suggested Fix**: Reword to *"accumulates linearly to ~5× peak at up[0] for the 5-mip pyramid (seed + 4 unit-gain additions)"*.

### INFO

#### REN-D8-02 — Firefly-hoist invariant (REG-07) not covered by a regression test
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` firefly-clamp block (immediately before `if (hasHistory)`)
- **Status**: OBSERVATION
- **Description**: Commit `48906670` hoisted the spatial firefly clamp ahead of the
  `hasHistory` branch so it also clamps the no-history/disocclusion path. Correct in
  the current source, but protected only by an in-shader comment (`INVARIANT
  (REG-07 / #1639, #1481)`), not a test — unlike the sibling TAA α-floor invariant,
  which has a source-scanning unit test
  (`taa.rs::taa_comp_floors_alpha_for_moving_pixels_under_parked_camera`). A future
  edit re-scoping the clamp inside `hasHistory` would compile clean and pass `cargo test`.
- **Impact**: None today; a latent regression-guard gap.
- **Suggested Fix**: Add a `#[test]` in `svgf.rs` that `include_str!`s `svgf_temporal.comp` and asserts the firefly-clamp site precedes the `if (hasHistory)` token (mirroring the TAA test).

#### REN-D8-03 — `shader-pipeline.md` composite row omits the caustic term
- **Dimension**: Denoiser/Composite (cross-cutting doc)
- **Location**: `docs/engine/shader-pipeline.md`, Shader Files table, `composite.frag` row
- **Status**: OBSERVATION
- **Description**: The row lists "direct + SVGF-denoised indirect, ACES tone-map,
  bloom add, volumetric froxel sample, underwater FX" but omits the two caustic
  accumulators (`causticTex` + `waterCausticTex`) summed into the direct term — a
  real composite responsibility.
- **Evidence**: `composite.frag` — `vec3 combined = direct + indirect * albedo + caustic;` with `caustic` from bindings 5 (`causticTex`) + 8 (`waterCausticTex`).
- **Impact**: Doc incompleteness only.
- **Suggested Fix**: Append "+ dual caustic accumulator (glass/water)" to the composite row.

#### V-DIM16-02 — `shader-pipeline.md` GpuCamera `sun_direction` row says "from sun", stale post-#1937
- **Dimension**: Volumetrics (cross-cutting doc)
- **Location**: `docs/engine/shader-pipeline.md`, GpuCamera table, `sun_direction` (offset 288) row
- **Status**: OPEN (doc-rot)
- **Description**: The row reads *"xyz = direction **from** sun"*. Per #1937 the
  host and every consumer upload it as the direction **TO** the sun;
  `include/bindings.glsl` and `water.frag` both document `sunDirection` as
  "direction TO the sun (light-incoming, matches GpuLight.direction_angle)". The
  doc is stale. (Same root cause as V-DIM16-01, different location — worth fixing
  together.)
- **Evidence**: `bindings.glsl` `vec4 sunDirection;`; `water.frag` "xyz = world-space direction TO the sun", `sunDir = normalize(sunDirection.xyz);`.
- **Impact**: Documentation only.
- **Suggested Fix**: Change the row to "xyz = direction **to** sun (unit)".

---

## Coverage Record — invariants verified to HOLD

### Dimension 8 — Denoiser & composite
1. **SVGF history ping-pong** — read `[prev]`, write `[f]`; first-frame reset via
   `should_force_history_reset` → `params.z`; α ∈ (0,1] clamped; à-trous has no
   read-write alias / double-read within a dispatch; `ATROUS_ITERATIONS = 5` (odd)
   → composite reads the spatially-filtered slot 0, not temporal history.
2. **Motion vectors** — `(currNDC−prevNDC)*0.5 == currUV−prevUV`; consumed as
   `prevUV = uv − motion`; disocclusion = masked mesh-ID equality AND normal cone.
3. **Dispatch coverage** — temporal + à-trous both `div_ceil(8)` against
   `WORKGROUP_X/Y = 8`, both bounds-guarded.
4. **Firefly clamp (#48906670)** — precedes `if (hasHistory)`; no-history branch
   writes the clamped value; spatial rejection (b7d1215c) present.
5. **Composite reassembly + tone-map order** — `direct + indirect*albedo + caustic`
   → `+ volumetric` → `+= bloom*BLOOM_INTENSITY` (all pre-ACES) → `aces()` →
   underwater mix; `direct` is TAA-resolved HDR when TAA present; caustic never in
   SVGF indirect (double-count guard); #1575 float-before-add intact.
6. **Composite camera-relative (#6c844744)** — `screen_to_world_dir` subtracts
   `camera_pos`, both operands in render-origin-relative space; singular-matrix guard.
7. **Output** — composite render pass single color attachment = swapchain format,
   `final_layout = PRESENT_SRC_KHR`, EXTERNAL→0 dependency orders the DONT_CARE
   loadOp after the layout transition (#507945d8).
- Submission order matches `shader-pipeline.md`; `next_svgf_temporal_alpha`
  recovery + progressive 1/N (#c7ca4864) + camera-static detection (#f983b71a) all
  unit-tested and same-space; `validate_set_layout` reflection guards SPIR-V drift.

### Dimension 16 — Volumetrics & bloom
- **V1** grid `160×90×128` ↔ inject `local_size (8,8,8)` ↔ `div_ceil` dispatch,
  bounds-guarded; integrate 2D `20×12`, marches Z internally.
- **V2** per-FIF `lighting_volumes`/`integrated_volumes`, fence-gated WAR;
  resize rewrites composite bindings 6 (volumetric) + 7 (bloom) (#905).
- **V3** inject `TerminateOnFirstHit`, `ray_dir` toward sun (#1937); integrate
  multiplies transmittance; HG `g` clamped (−0.999, 0.999) with denom floor.
- **V4** `VOLUMETRIC_OUTPUT_CONSUMED` gate (#928) skips both dispatches when
  unconsumed; composite apply pinned by `composite_frag_spv_matches_recompiled_branch_count`;
  interior/no-sun → neutral non-NaN.
- **V5** `volumetrics_ubo_sizes_match_host_structs_in_every_shader` pin (#2db2d900);
  per-frame TLAS/lights latch reset in `dispatch()` (#6e851403).
- **Froxel memory** 14.06 MiB/volume, ~56 MiB total — matches `memory-budget.md`.
- **Render-origin (#36f66493)** `froxel_to_world` adds `render_origin` to the
  relative `inv_view_proj`; no absolute/relative mixing.
- **V6** 5 down + 4 up mips, `B10G11R11_UFLOAT` throughout, 4-tap bilinear down
  (sum 1.0), additive up (no clamp); DC-gain mechanism correct (figure loose,
  B-DIM16-03).
- **V7** per-FIF chains, fence-gated WAR, only post-barriers — #931 pre-barrier
  removal not reintroduced by #6ada7a57.
- **V8** bloom added pre-ACES; `BLOOM_INTENSITY` from generated header; source is
  the raw pre-TAA HDR attachment (#1166), NOT TAA output; no runtime bloom-disable
  toggle exists (init hard-fails if absent, #1081/#1276) → disable-short-circuit is
  N/A, not a gap.

---

## Prioritized Fix Order

All items are documentation/test hygiene — no correctness work. Suggested order:

1. **V-DIM16-01 + V-DIM16-02** (LOW + INFO) — fix the two stale sun-direction docs
   together (Rust struct doc + `shader-pipeline.md` row). Same root cause; the
   stale "from sun" wording is the exact trap that produced the #1937 sign bug, so
   closing it prevents a recurrence.
2. **REN-D8-01** (LOW) — correct the SVGF binding-6 format in the module docstring.
3. **B-DIM16-03** (LOW) — reword the bloom DC-gain aggregate figure (~8× → ~5×).
4. **REN-D8-02** (INFO) — add the firefly-hoist source-scanning test (mirrors the
   TAA α-floor test).
5. **REN-D8-03** (INFO) — append the caustic term to the `shader-pipeline.md`
   composite row.

## Needs-RenderDoc

None. No sync/barrier finding in either dimension required capture-based
verification; the barrier structures were read for consistency only and found
consistent. (A future concern about interior two-pass shadow-ray cost or godray
banding would need RenderDoc, but neither is a finding here.)
