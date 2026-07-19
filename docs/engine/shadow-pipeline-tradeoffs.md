# Shadow Pipeline — Alpha-Era Trade-offs

This document inventories four load-bearing constants and architectural
decisions in the shadow / direct-lighting pipeline that work *together*
to produce the current visual result. Each looks defensible in isolation
but collectively they represent unmeasured trade-offs whose continued
validity is conditional on milestones that have not yet landed.

For each entry: **what** the decision is, **why** it works today, the
**milestone** whose completion makes it obsolete or forces re-validation,
and the **verification** that has not yet been performed.

The intent is to give audit agents an anchor. If any of these constants
drift from the conditions documented here, the trade-off chain may have
broken in a way that the test suite does not catch.

---

## 1. `RESERVOIR_W_CLAMP = 64.0`

**Where:** [`crates/renderer/shaders/triangle.frag:2225`](../../crates/renderer/shaders/triangle.frag#L2225)

**What:** Caps the WRS unbiasing weight `W = resWSum / (K · w_sel)` per
reservoir before multiplying by the shadowed-radiance subtraction.
Without the cap, a reservoir that selects a dim light in a cluster
dominated by a hero light produces a per-pixel firefly of multiple
orders of magnitude.

**Justifying condition (today):** Empirically tuned to "the ratio of a
dim fill light to a hero light" per the inline comment. Introduced in
commit `327a9787` ("Shadow pipeline overhaul…") without an A/B test or
dedicated benchmark. The clamp is known *in theory* to produce a
systematic under-estimate of dim-light contribution in mixed clusters —
WRS remains unbiased in *selection*, but the truncated `W` truncates the
radiance integral. The bias magnitude is not measured.

**Invalidated by:** Full ReSTIR-DI with temporal + spatial reservoir
reuse. Once `M_effective` per pixel is large enough that the WRS
estimator converges with low intra-frame variance, the clamp becomes
either unnecessary or visibly biasing and should be removed.

**Verification owed:** Candle-heavy interior cell rendered with clamp on
vs off, integrating luminance over surfaces near dim sources vs near the
directional light, with skylight on and off. Heatmap of
`log(W_uncapped / W_capped)` per pixel localizes where the clamp is
biting. Acceptance: luminance integral delta <5% justifies the clamp;
>15% means the trade-off needs an explicit milestone gate.

---

## 2. TAA variance clamp `γ = 1.25`

**Where:** [`crates/renderer/shaders/taa.comp:186`](../../crates/renderer/shaders/taa.comp#L186)

**What:** The neighborhood variance clamp on YCoCg history sampling
uses `γ = 1.25` to widen the valid-history bounding box. Canonical TAA
uses `γ ≈ 1.0` (strict bounds) for ghosting prevention; `γ = 1.25`
deliberately admits more variance.

**Justifying condition (today):** The inline comment notes "penumbra
edges higher per-frame variance" — the direct shadow path produces
stochastic shadow rays per reservoir *without* a dedicated denoiser, so
frame-to-frame variance in soft-shadow penumbras exceeds what a strict
clamp accepts as valid history. The wider clamp lets the TAA
accumulator act as a de facto temporal-reuse layer for direct lighting.
This is parameter tuning compensating for an absent architectural piece.

**Invalidated by:** ReSTIR-DI temporal reservoir reuse. Once direct
lighting has its own temporal accumulation path, the per-frame variance
reaching TAA drops, and `γ` should be re-tuned (likely closer to 1.0).

**Verification owed:** A/B `γ = 1.25` vs `γ = 1.0` with the camera
panning over a candle-lit interior. If `γ = 1.0` produces visible
penumbra erosion, the current value is doing real work; if both look
identical, the wider clamp is admitting ghosting unnecessarily.

---

## 3. ReSTIR-DI replaced the transient `M = 8` estimator

**Where:** [`crates/renderer/shaders/triangle.frag:2221`](../../crates/renderer/shaders/triangle.frag#L2221)

**Historical state:** Each fragment formerly ran 8 independent WRS
reservoirs over the cluster's light list and discarded them at the end of the
shader. Phase 19 later raised the legacy path to 16 reservoirs.

**Current state:** M37.3 is complete. The default shader streams one
current-frame reservoir, combines temporal history, then samples a disk of
previous-frame neighbouring reservoirs in the same fragment invocation. The
spatial candidates are re-evaluated at the current surface, normal-rejected,
and the selected sample is visibility-tested again. Screen-sized reservoir
SSBOs at scene-set bindings 16/17 persist history across frames.

**Remaining trade-off:** Spatial reuse reads fenced previous-frame neighbours
rather than a dedicated current-frame resample pass. This avoids an extra
dispatch and read-after-write hazard but makes the estimator a deliberate
single-pass approximation whose bias and temporal behavior must be measured.
MIS calibration over the combined temporal and spatial samples is where the
new system can break subtly.

**Verification owed:** The harder verification is whether the fused estimator
converges cleanly and whether its temporal-only and spatial-only components
each improve variance without introducing persistent bias or ghosting. Run
the renderer-evaluation suite's four matched ReSTIR captures before changing
reservoir clamps, history caps, neighbour selection, or visibility-ray count.
Reference:
Bitterli et al., "Spatiotemporal Reservoir Resampling for Real-Time Ray
Tracing with Dynamic Direct Lighting" (SIGGRAPH 2020) plus errata.

---

## 4. 24-bit masked frame counter as RT noise seed

**Where:** [`crates/renderer/src/vulkan/context/draw.rs:497-513`](../../crates/renderer/src/vulkan/context/draw.rs#L497-L513)

**What:** `GpuCamera.position.w` is loaded with
`(frame_counter & 0xFFFFFF) as f32`. The shader reads this as
`cameraPos.w` and uses it as the seed for `interleavedGradientNoise`
in shadow ray jitter, reflection rays, GI hemisphere sampling, and
reservoir streaming offsets.

**Justifying condition (today):** f32 mantissa stops resolving ±1
increments above 2^24. Without the mask, consecutive frames beyond 2^24
would map to the same `cameraPos.w` and the RT noise pattern would
freeze, causing fireflies and banding to reappear after ~3.2 days of
continuous uptime at 60 FPS. Issue #1161 documents the bug; the mask
wraps the counter to the bottom 24 bits, accepting that the noise
pattern repeats every ~3.2 days (TAA accumulation is expected to absorb
the wraparound discontinuity, but this has not been exercised).

**Invalidated by:** Nothing structural — this is a permanent f32-
precision workaround. The concern is operational: long QA sessions,
speedruns, or soak tests could approach 2^24 frames and the wraparound
itself becomes a stability test that has never been run.

**Verification owed:** A `debug_assert!` at `frame_counter > (1 << 23)`
(half the budget, ~1.6 days at 60 FPS) so testing surfaces the regime
before it ships. Currently no such warning exists.

---

## Audit Hook

The four items above each name a specific constant or location. The
`audit-renderer` and `audit-incremental` skills can verify:

1. `RESERVOIR_W_CLAMP` remains at `64.0` **and** the justifying comment
   remains attached. If the constant changes without an accompanying
   A/B benchmark commit, flag it.
2. TAA `γ = 1.25` remains tied to the absence of a direct-lighting
   denoiser. If a ReSTIR-DI temporal pass lands, this value should be
   re-tuned and the corresponding milestone closed.
3. `DBG_DISABLE_SPATIAL` and `DBG_DISABLE_TEMPORAL` continue to isolate the
   two reuse dimensions without selecting the compile-time-gated legacy WRS
   path. Changes to neighbor count/radius, history caps, or visibility-ray
   count require renderer-evaluation captures.
4. The 24-bit mask on `frame_counter` remains in place. If a
   `frame_counter as f32` cast ever appears without the mask, flag it
   as a regression of #1161.

A change to any of these four constants without an accompanying update
to *this document* is itself a flag — the trade-off chain assumes each
piece compensates for the others, so a unilateral edit means either the
compensation is no longer needed (good — close the loop here) or someone
edited a load-bearing constant without realizing what it was holding up.

## Real-content finding: volumetric coordinates are Gamebryo units

The 2026-07-19 Prospector Saloon probe found that the volumetric pipeline's
documented 200-metre range was passed directly alongside unscaled Bethesda
world positions. `VOLUME_FAR = 200` therefore covered only about 2.86 metres
and the `0.0035 / metre` density was integrated as though every world unit
were a metre. The range is now 14,000 world units and density is divided by
70 units/metre, preserving the intended 200-metre optical depth while making
froxel positions, TLAS queries, integration, and composite sampling share one
coordinate scale.

The same probe established that resolved indirect contains substantial
authored ambient, but the isolated stochastic GI bounce is nearly black apart
from sparse high-energy samples around window/cutout geometry. The next GI
work should target hit-surface light evaluation and firefly rejection rather
than increasing SVGF strength or exposure.
