# Penumbra-Aware Shadow Filter — Design

**Status**: PROPOSED (2026-11-02). Written in M-LIGHT Phase 0 (#6596) to
satisfy [RLT-SHD-003](../prd/reference-lighting.md#52-soft-shadows-and-penumbrae-rlt-shd);
implemented in Phase 3 (#6603), after that phase's entry baselines
(A2–A5, A14) are published, so the change is measured against them.
Requirement IDs use the `PAF-` prefix.

**Clean-room rule.** This design is built from two published descriptions
only: Fernando's *Percentage-Closer Soft Shadows* (NVIDIA, SIGGRAPH 2005
Sketches; [paper and supplement](https://developer.download.nvidia.com/shaderlibrary/docs/shadow_PCSS.pdf))
and the public README of NVIDIA's NRD ([github.com/NVIDIA-RTX/NRD](https://github.com/NVIDIA-RTX/NRD)).
Nobody implementing it opens NRD's shader sources, and the NRD SDK is not
a dependency.

---

## 1. What the sources say

| Source | Statement used here |
|---|---|
| Fernando 2005, §3 | PCSS shades each pixel in three steps: **blocker search** ("average the depths that are closer to the light source than to the point being shaded … The size of the search region depends on the light size and the receiver's distance from the light source"), **penumbra estimation** ("Using a parallel planes approximation … *w*<sub>Penumbra</sub> = (*d*<sub>Receiver</sub> − *d*<sub>Blocker</sub>) · *w*<sub>Light</sub> / *d*<sub>Blocker</sub>"), and **filtering** ("a typical PCF step … using a kernel size proportional to the penumbra estimate from Step 2"). |
| Fernando 2005, supplement | The estimate "assum[es] that the blocker, receiver, and light source are parallel. This assumption works very well in practice and could be relaxed as an improvement." |
| NRD README, SIGMA | SIGMA is a "per-light shadow-only denoiser" for "an infinite light source (sun, moon) or a local light source (omni, spot)". Its occluder input is per pixel: "`NoL <= 0` - 0", "`NoL > 0, hit` - hit distance", "`NoL > 0, miss` - >= NRD_FP16_MAX", with the visibility ray "cast from the point of interest to a light source". It works "better with 'blue' noise". |

The README does not say how SIGMA turns the hit distance into a filter
size; that part below is derived from Fernando's estimate, not taken from
NRD.

## 2. Today (at HEAD on 2026-11-02)

`shadow_filter.comp` runs a 5×5 B3 à-trous chain over the ReSTIR shadow
ratio (`accum`), two iterations at strides 1 and 2, so every pixel's reach
is a fixed 6 pixels regardless of how wide its penumbra is
(`ITERATIONS = 2`, `crates/renderer/src/vulkan/shadow_filter.rs`). Edge
stops are the geometric normal, view-ray distance against the local slope,
the ratio difference, and the mesh-ID ownership test (#6249). A confidence
factor `SHADOW_FILTER_CONFIDENT_HISTORY / histLen` fades the correction on
converged pixels. The last iteration writes U · (filtered − raw), which
`composite.frag` adds to direct light. Two consequences:

- a wide penumbra (large source, tall blocker) is filtered over a fraction
  of its width, so its noise survives when the ratio EMA runs at its
  motion tail;
- a narrow penumbra (a physical-radius sun, Q3) is blurred across up to
  6 pixels even where its true width is one or two, which A2 measures as
  width error.

## 3. Model

### 3.1 Penumbra width per pixel

For the light a pixel's reservoir selected, the shadow ray runs from the
receiver to the light and reports the occluder's hit distance *t* (the
quantity NRD's SIGMA takes). In Fernando's terms *d*<sub>Receiver</sub> −
*d*<sub>Blocker</sub> = *t* and *d*<sub>Blocker</sub> = *d*<sub>L</sub> − *t*,
where *d*<sub>L</sub> is the receiver-to-light distance. With *w*<sub>Light</sub>
the source's diameter, the world-space penumbra width is:

| Light | Width *w* | Inputs at HEAD |
|---|---|---|
| Local (point, spot) | *t* · 2*r* ⁄ max(*d*<sub>L</sub> − *t*, ε) | *r* = `GpuLight.params.y` (floor 1.0 unit) |
| Directional | *t* · 2 tan θ | θ = the sun cone carried in `skyTint.w` (0.020 rad at HEAD; the exterior value Q3 sets) |

Behind a straight edge this is the full (0–100 %) penumbra; the PRD's A2
fits the 10–90 % width of the measured visibility, about 0.69 of it for a
disk source (RLT-SHD-001), so the filter radius below is sized from *w*
and the gate compares widths, never this estimate.

- **PAF-001.** `triangle.frag`'s ReSTIR finalize MUST compute *w* for the
  selected light from the mean hit distance of the shadow samples that
  were occluded this frame, and 0 when none was (lit) or when N·L ≤ 0. It
  MUST store *w* as an f16 in the high half of the pixel's
  `directUnshadowed[].y`, which is written as `packHalf2x16(vec2(U.b,
  0.0))` today and so carries no data there (PRD V3). The `Reservoir`
  struct does not grow.
- **PAF-002.** An umbra pixel (every sample occluded) keeps its *w*: the
  estimate is what tells its neighbours how far the transition reaches.

### 3.2 Screen-space radius

The pixel footprint at view distance *z* is *f* = 2*z* tan(fov<sub>y</sub> ⁄ 2) ⁄ *H*
for a render height of *H* pixels. The kernel radius in pixels is
*R* = *w* ⁄ (2*f*): the filter reaches half a penumbra to either side of
the pixel, so a pixel at the penumbra's centre spans all of it.

- **PAF-003.** *R* MUST be clamped to [*R*<sub>min</sub>, *R*<sub>max</sub>],
  with *R*<sub>min</sub> = 1 so a sharp edge is still denoised across its
  own pixel, and *R*<sub>max</sub> equal to the chain's reach at the
  iteration count in use (§3.4).
- **PAF-004.** The projection is isotropic: a receiver seen at a grazing
  angle compresses its penumbra on screen along one axis, and the filter
  over-reaches along it. The distance edge stop already rejects most of
  that over-reach on a grazing floor; an anisotropic kernel is a later
  improvement, recorded here, not built.

### 3.3 Blocker search (Fernando's step 1, in screen space)

One frame's *w* is noisy: at tier 1 a pixel traces one shadow sample
(T4), so in the penumbra about half the pixels see no occluder and report
0. PCSS solves the same problem by averaging blocker depths over a search
region.

- **PAF-005.** A new first pass MUST average *w* over the valid,
  surface-owning (#6249) neighbours whose *w* > 0, within a fixed window
  of 9 × 9 taps at stride 2, weighted by the existing normal and distance
  edge stops; the result *w̄* replaces *w* in §3.2. A pixel with no
  neighbour reporting *w* > 0 is outside every penumbra and gets
  *R* = *R*<sub>min</sub>.
- **PAF-006.** The blocker search MUST read only this frame's data. A
  temporally accumulated *w* would need the 40-byte reservoir (PRD V3,
  +16.6 MB at 1080p) and is out of scope unless A3 / A14 show the
  one-frame search too noisy.

### 3.4 Filtering

- **PAF-007.** The à-trous chain MUST run strides 1, 2, 4 and 8 (reach
  2 + 4 + 8 + 16 = 30 pixels) at ray quality tier 3, and the iteration
  count per tier MUST be a field of `GpuRayBudget` so the adaptive budget
  can shed the extra iterations (RLT-PERF-002); at the 2-iteration floor
  the filter's reach is today's.
- **PAF-008.** Each tap at screen offset *δ* MUST be weighted, beside the
  existing stops, by a Gaussian exp(−|*δ*|² ⁄ 2*σ*²) with *σ* = *R* ⁄ 2,
  and an iteration whose stride exceeds *R* MUST pass the pixel's value
  through unchanged, so a narrow penumbra stops early and a wide one uses
  the whole chain. That is Fernando's "kernel size proportional to the
  penumbra estimate", applied to an edge-aware chain instead of PCF.
- **PAF-009.** The ratio edge stop and the confidence fade stay as they
  are; the change adds a reach, it does not loosen what the filter may mix.

## 4. Cost and observability

- **PAF-010.** The blocker search and the extra iterations MUST run inside
  the existing `shadow_filter` GPU bracket (it sits inside `svgf` on the
  `bench:` line), and the phase's A13 MUST show the total within the Q8
  ceiling (PRD RLT-PERF-004). An estimate before measuring: two extra
  5 × 5 iterations and one 9 × 9 search at render resolution are about
  2.2× today's tap count.
- **PAF-011.** A debug view MUST show *R* in pixels (blue = *R*<sub>min</sub>,
  red = *R*<sub>max</sub>), listed in the `RenderDebugMode` catalog, so a
  wrong blocker distance is visible before it is a width error.

## 5. Validation

| Check | Gate |
|---|---|
| Width matches the reference on the local-light and sun penumbra rungs | A2 (RLT-SHD-001) |
| No light leaks into the umbra band or shadow into the lit band; the thin rod's shadow survives | A4 (RLT-SHD-004) — the main risk of a wider reach |
| No ghosting under camera motion or a moving blocker | A3, A14 (RLT-SHD-002, RLT-SHD-010) |
| Contact bands not lifted | A5 (RLT-CON-002) |
| Settled tier and pass cost | A13 (RLT-PERF-004, RLT-PERF-005) |

A unit test MUST pin the width formula against the two closed forms of
§3.1 on CPU (`PAF-001`), and a shader-contract test MUST pin that the
f16 lane it writes is the one `shadow_filter.comp` reads.

## 6. Open points

| # | Question | Settled by |
|---|---|---|
| P1 | Whether one frame's blocker search is stable enough under motion | A3 / A14 at Phase 3; if not, PAF-006's 40-byte reservoir |
| P2 | Whether the isotropic kernel over-blurs grazing floors measurably | A2's profile lines on the floor rung |
| P3 | Behaviour where two lights' penumbrae overlap in one cluster (one ratio per cluster, #5020) | A4 on a two-light variant if the single-light rungs pass |
