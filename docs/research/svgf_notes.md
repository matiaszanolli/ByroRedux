# A-SVGF — Adaptive Spatiotemporal Variance-Guided Filtering

**Paper**: "Gradient Estimation for Real-Time Adaptive Temporal Filtering"
**Authors**: Christoph Schied, Christoph Peters, Carsten Dachsbacher (KIT Germany)
**Venue**: Proc. ACM Comput. Graph. Interact. Tech. (i3D 2018)
**PDF**: `schied.pdf` (gitignored)

**Important**: This paper is **A-SVGF** (adaptive), which is an improvement
on the original **SVGF** (Schied et al. 2017). The underlying SVGF filter is
cited as prior work throughout this paper. To properly implement SVGF you
need BOTH papers:
- **SVGF 2017** (Schied et al.) — the original spatial filter + variance
  estimation + temporal accumulation with fixed α
- **A-SVGF 2018** (this paper) — adaptive α via gradient estimation to kill
  ghosting/lag

## The Problem SVGF Solves

At 1 sample per pixel, stochastic ray tracing output is unusable — it's pure
noise. Real-time renderers need a reconstruction filter that:
1. Denoises single-sample input to near-reference quality
2. Runs in <2ms per frame
3. Stays temporally stable (no flicker, no ghosting)
4. Handles shadows, indirect lighting, global effects

SVGF is **the** standard answer. It's paired with ReSTIR GI and every other
modern real-time RT technique. It is **exactly what we're missing**.

## SVGF (2017) — The Original Filter

The original SVGF has three components:
1. **Temporal accumulation** with fixed α via backprojection (exponential
   moving average of the noisy signal)
2. **Variance estimation** — per-pixel variance used to steer spatial filter
3. **Edge-aware À-trous wavelet spatial filter** — 5 iterations, weights based
   on depth/normal/luminance similarity

Key equation (temporal accumulation):
```
ĉ_i(x) = α · c_i(x) + (1 - α) · ĉ_{i-1}(x_prev)
```
where x_prev is the reprojected pixel from the previous frame.

**Critical insight**: SVGF uses `α = 0.2` (20% new, 80% history). Very
aggressive temporal reuse. That's what kills the noise — but it also causes
ghosting and lag when things change.

## A-SVGF (2018) — The Adaptive Fix

The problem with fixed α: when lighting changes (light turns on/off, object
moves, camera pans), the history is stale. Fixed α means the history has
to decay naturally over ~5-10 frames, causing visible ghosting trails.

**A-SVGF's contribution**: estimate the **temporal gradient** (rate of change
of the shading signal) and use it to make α adaptive per-pixel per-frame.
Regions where the signal changed rapidly get α close to 1 (drop history).
Regions that are stable keep α ~ 0.1 (maximum reuse).

### The Temporal Gradient (§3.1)

Define gradient as:
```
δ_i,j_forward = f_i(G_forward) - f_{i-1}(G_prev)
```

Where:
- `G_prev` = surface sample from previous frame (position, normal, etc.)
- `G_forward` = forward-projected to current frame's screen coordinates
- `f_i` = shading function (takes surface sample + random numbers)

**Key**: use **forward projection** not backprojection. Why? Backprojection
needs to re-shade the previous frame using current-frame state, which
requires keeping all previous-frame scene data around. Forward projection
only needs the previous frame's stored shading samples.

### Stable Stochastic Sampling (§3.2)

The gradient formula has a subtle trap: the `f_i` evaluation uses **new**
random numbers, while `f_{i-1}` used **old** ones. Pure noise in random
numbers would produce a fake gradient.

**Fix**: reuse the same random number seed for the forward-projected sample:
```
ξ_i = ξ_{i-1}  // force same seed
```

This way, if the scene hasn't changed, `f_i(G_forward, ξ) - f_{i-1}(G_prev, ξ) ≈ 0`
regardless of noise. Only actual scene changes produce non-zero gradients.

Store **one RNG seed per pixel** in the G-buffer. That's all that's needed.

### Sparse Gradient Sampling (§3.3)

Computing the gradient for every pixel is expensive. Instead:
- Divide screen into **3×3 strata**
- Per stratum, pick ONE pixel from the previous frame to reproject
- Forward-project it, re-shade with the same RNG seed
- Compute gradient at that one location
- Leave the other 8 pixels unchanged (normal shading with new random samples)

**Result**: only ~11% of shading budget is repurposed for gradient samples.
Rest goes to normal stochastic sampling.

Why not 2×2? Too many gradient samples destroy low-discrepancy properties
of the random numbers, hurting the stochastic estimator. Why not 4×4?
Gradient resolution becomes too coarse, small bright regions get missed.
**3×3 is the sweet spot**.

### Gradient Reconstruction (§3.4)

Sparse gradient samples must be filtered to a dense gradient image. Use
an **edge-aware À-trous wavelet** (same as spatial filter in SVGF) with
edge-stopping functions based on:
- `w_z` — depth difference (normalized by screen-space depth derivative)
- `w_n` — normal similarity: `max(0, <n_p, n_q>)^σ_n`
- `w_l` — luminance similarity (normalized by luminance variance)

5 À-trous iterations. For each iteration `k`, the filter taps spread apart
(1, 2, 4, 8, 16 pixels) giving an effective filter radius of ~48 pixels at
5 iterations. This is the key to efficient wide-area filtering.

### Computing Adaptive α (§3.5)

```
λ(p) = min(1, |δ̂_i(p)| / Δ̂_i(p))      // normalized gradient
α_i(p) = (1 - λ(p)) · α + λ(p)            // interpolate α toward 1
```

Where:
- `δ̂` = reconstructed (denoised) gradient
- `Δ̂` = reconstructed max(current, previous) luminance (normalization)
- `α` = global base accumulation factor (e.g., 0.1)

When gradient is small (stable): `α_i ≈ α` (e.g., 0.1, heavy reuse)
When gradient is large (changed): `α_i → 1` (drop history entirely)

Use max over 3×3 neighborhood of α_i to account for reconstruction error.

## SVGF Spatial Filter (from 2017 paper, summarized)

After temporal accumulation, apply edge-aware À-trous wavelet:
- 5 iterations
- Weights: `w = w_z · w_n · w_l` (same as gradient filter above)
- **Luminance weight normalized by variance**: regions with high variance
  (still noisy) get wider filter; low-variance (converged) regions keep
  their detail
- **Variance estimation**: either temporal (use history samples) or
  spatial fallback (when history unavailable due to disocclusion)

**This is why it's called "Variance-Guided" Filtering** — the spatial filter
bandwidth is controlled by the per-pixel variance estimate.

## Performance (Titan Xp, 1080p)

- Full A-SVGF pipeline: ~5 ms total
- Gradient estimation + reconstruction: <1 ms
- Temporal filter: <2 ms
- Spatial filter: ~2 ms

A-SVGF is slightly slower than SVGF because dropping history more often
forces use of the (more expensive) spatial variance fallback.

## Error Reduction Results

| Scene | SVGF RMSE | A-SVGF RMSE | Improvement |
|-------|-----------|-------------|-------------|
| Pillars (moving shadows) | 0.018 | 0.015 | 15% |
| Sponza (moving light) | 0.075 | 0.023 | **70%** |
| GlossySponza | 0.131 | 0.051 | **61%** |
| Dungeon (dynamic) | 0.100 | 0.054 | **46%** |

The big wins are in **dynamic scenes**. For static scenes SVGF is already
good; A-SVGF mostly helps when things change.

## Applicability to ByroRedux

### What we need to implement SVGF (2017 — original)

1. **G-buffer / Visibility buffer** — with depth, normal, albedo, and RNG seed
2. **Motion vectors** (or forward projection) — for reprojection
3. **Temporal accumulation pass**:
   - Storage image (RGBA16F), ping-pong
   - Read reprojected previous frame, blend with current at α=0.2
4. **Variance estimation**:
   - Track first+second moments of luminance over time
   - Compute variance per pixel (or spatial fallback for disoccluded)
5. **À-trous wavelet spatial filter**:
   - 5 iterations, increasing tap spread
   - Edge-stopping on depth/normal/luminance
   - Runs as compute or fragment pass after temporal

**That's ~4-5 ms of pipeline work** but gives us reference-quality denoised
RT output.

### What A-SVGF adds on top (if we want dynamic lighting quality)

6. **Per-pixel RNG seed** stored in visibility buffer
7. **Sparse gradient sampling**:
   - 3×3 strata, pick 1 pixel per stratum from previous frame
   - Forward-project, re-shade with old seed
   - Compute gradient at that location
8. **Gradient reconstruction** via same À-trous filter
9. **Adaptive α computation** per pixel

### Prerequisites We Don't Have Yet

Before we can implement ANY version of SVGF, we need:
1. **Motion vectors** — no temporal technique works without these
2. **G-buffer or visibility buffer** — we're currently pure forward
3. **Separated direct/indirect lighting** — SVGF filters the noisy indirect
   term, NOT the rasterized direct lighting

### Recommended Implementation Order

1. **Motion vectors**: compute in vertex shader, write to R16G16_SFLOAT
   render target. Needs the previous frame's viewProj matrix.
2. **Separate indirect pass**: move the GI ray + accumulation into a dedicated
   R11G11B10F storage image. The fragment shader writes both final color AND
   the raw indirect sample separately.
3. **Simple temporal accumulation** first: fixed α=0.2 EMA on the indirect
   buffer. This alone would eliminate ~70% of our current noise.
4. **Variance estimation**: track moments for variance-guided filter
5. **À-trous spatial filter**: 5 iterations with edge-stopping
6. **A-SVGF adaptive α** (optional, only if ghosting is visible)

Steps 1-3 alone would be a massive visual improvement and are a prerequisite
for either ReSTIR GI or SVGF. Steps 4-5 complete basic SVGF. Step 6 upgrades
to A-SVGF for dynamic scenes.

## Key Insights for Our Current Problem

1. **Our flashing GI is exactly the noise SVGF is designed to solve**. The
   paper's 1-SPP input looks identical to what we're outputting raw.

2. **Fixed α=0.2 is the baseline** — not 0.05 as I suggested earlier. The
   SVGF paper explicitly uses α=0.2 and α=0.1 for A-SVGF. These are the
   numbers that work in practice.

3. **Temporal accumulation ALONE doesn't work**. You need the spatial filter
   after it. That's the whole point of "variance-guided": the variance
   estimate tells the spatial filter where to be aggressive.

4. **Indirect lighting MUST be separated from direct lighting** before
   filtering. You can't blur the direct-lit table geometry or you lose
   all detail. SVGF only denoises the noisy indirect signal.

5. **Motion vectors are the key missing piece**. Every temporal technique
   (SVGF, A-SVGF, ReSTIR GI, ReSTIR DI, TAA) requires them. We should
   implement motion vectors **first**, then any of those techniques can
   layer on top.

6. **Per-pixel RNG seed storage** is cheap and enables A-SVGF's gradient
   trick. Worth doing from the start.

## Reading the 2017 SVGF Paper Next

This paper references the 2017 SVGF paper extensively but does NOT contain
its algorithmic details (temporal accumulation formula, variance estimation,
À-trous weights are only sketched). To implement SVGF we also need:

**"Spatiotemporal Variance-Guided Filtering: Real-Time Reconstruction for
Path-Traced Global Illumination"** — Schied et al. 2017, HPG proceedings.
That paper has the full variance estimation math and the spatial filter
details.
