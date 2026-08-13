# Issues 2807, 2810, 2809, 2811

All filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md`. Domain: renderer
(`byroredux-renderer`), plus one pure-docs file.

## #2807 — REN-D9-2026-08-12-05: shader-pipeline.md doc errors

(a) Compute table says `skin_vertices.comp` deforms "positions **/ normals**";
it has been position-only since #2170 (`SKIN_OUTPUT_STRIDE_FLOATS = 3`), and the
shader body itself says so — the doc contradicts the code's own explanation of a
live behavioural gap.
(b) `MAX_TOTAL_BONES` factorised as 144 x 1364 = 196,416 != 196,608, omitting the
reserved identity slot 0.

Location: `docs/engine/shader-pipeline.md`.

Severity: low. Labels: documentation, renderer.

## #2810 — REN-D17-08: anisotropic GGX contracts have no regression guard

The #1250 isotropic-degeneracy contract and the #1254 anisotropic clamp have
zero automated guards, unlike every sibling invariant in this dimension (#2243,
#2244, #2472, #1190 all have string-mirror tests with negative assertions).
`grep -rn "distributionGGXAniso\|deriveAxAy" --include=*.rs` returns nothing.
Both contracts verified algebraically to hold today; exposure is purely
regression, in a lobe with no CPU producer, so a break wouldn't be caught by
eyeball either.

Location: `crates/renderer/shaders/include/pbr.glsl` (`distributionGGXAniso`,
`deriveAxAy`).

Severity: low. Labels: bug, renderer, tech-debt.

## #2809 — REN-D16-04: froxel temporal clamp uses history volume; emissive asymmetry

(1) The 3x3 clamp statistics are gathered by sampling `previousFroxel` — the
history volume itself — not the current frame. Unlike `taa.comp` (moments from
`uCurrHdr`) and `svgf_temporal.comp` (firefly statistic from
`currIndirectTex`), clamping a value to a neighbourhood that *includes that
value* removes single-froxel spatial spikes but bounds nothing about
disagreement with the current frame; a spatially smooth but temporally stale
history passes untouched.

(2) The only genuine current-vs-history rejections are the density term
(`exp(-temporal_params.y * relativeDensityDelta)`, `DEFAULT_DENSITY_REJECTION =
4.0`) and `emissionAgreement = exp(-2.5 * relativeRadianceDelta *
emissionFraction)`. The second is multiplied by `emissionFraction`, derived from
the **current** sample alone. On the trailing edge — a froxel a flame has just
left — `emissionFraction -> 0`, so `emissionAgreement -> 1` AND
`mix(steadyWeight, emissiveWeight, emissionFraction)` selects `steadyWeight =
0.92` (~12-frame decay) rather than `emissiveWeight = 0.75` (~3.5 frames). The
emissive time constant is asymmetric: fast on, slow off.

Location: `crates/renderer/shaders/volumetrics_inject.comp` — the
`params.prev_camera_pos.w > 0.5` / `reprojectHistory` block; constants in
`crates/renderer/src/vulkan/volumetrics.rs`.

**Partially self-disproved (issue states for honesty)**: when the departing
volume also dominated that froxel's extinction, `relativeDensityDelta ~= 1` and
`exp(-4) ~= 0.018` collapses `historyWeight`, suppressing the trail. Surviving
case: a froxel where the ambient/global medium dominates `sigma_t` — fogged
exteriors, or a bright but optically thin flame.

**Non-findings established, do NOT re-derive**: `historyWeight` is
`clamp(_, 0, 0.98)` times two `exp(-x) <= 1` factors and `mix(current, history,
w<1)` is a contraction with fixed point `current` → no runaway accumulation;
magnitudes far below RGBA16F 65504 ceiling; previous-slot index is the other FIF
slot, barriered, so no slot reads its own in-flight write; `history_valid`
starts false and gates the block via `prev_camera_pos.w`.

Suggested fix: gather clamp statistics from the current frame's computed
`inscatter`/`extinction` neighbourhood (needs shared-memory or second-pass
restructure), OR — much cheaper — derive `emissionFraction` from `max(current
emission luma, reprojected-history emission content)` so the trailing edge
inherits the same short time constant as the leading edge.

Severity: medium. Labels: bug, renderer, vulkan.

## #2811 — REN-D17-09: presets have unverifiable citation + phantom fallback role

(a) The module pins its values to `knightcrawler25/GLSL-PathTracer`, which the
user-memory note *reference_glsl_pathtracer.md* records as cloned to
`/mnt/data/src/reference/` — **it is not there**, so the Dim-17 checklist item
"Disney preset constructors match documented values (cross-ref GLSL-PathTracer)"
is not executable offline. Same for the four `pbr.glsl` doc references into
`disney.glsl` line ranges.
(b) The doc claims the presets are the "fallback when authored BGSM is absent";
**no such fallback exists** — `translate_material` never consults `presets`, and
the only hits outside `material.rs` are its own tests. A documented fallback role
no code implements is an invitation to wire it in and bypass the NIFAL single
boundary.

Location: `crates/renderer/src/vulkan/material.rs` (`pub mod presets`).

Severity: low. Labels: documentation, renderer.
