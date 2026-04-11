# SVGF (2017) — Original Spatiotemporal Variance-Guided Filter

**Paper**: "Spatiotemporal Variance-Guided Filtering: Real-Time Reconstruction
for Path-Traced Global Illumination"
**Authors**: Schied, Kaplanyan, Wyman, Patney, Chaitanya, Burgess, Liu,
Dachsbacher, Lefohn, Salvi (NVIDIA + Karlsruhe + U.Montreal)
**Venue**: High Performance Graphics (HPG) 2017
**PDF**: `svgf_2017.pdf` (gitignored)

This is the **original SVGF paper**. The 2018 A-SVGF paper (`schied.pdf`)
is an improvement that replaces the fixed α with a gradient-adaptive α.
This paper contains all the foundational details: variance estimation math,
À-trous filter weights, temporal accumulation rules, edge-stopping functions.

## Results at a Glance
- **10 ms at 1080p** on Titan X Pascal (2017 hardware)
- **200 Mpixels/sec** reconstruction throughput
- **1 SPP input → near-reference quality** (Figure 1)
- **10× better temporal stability** vs prior work
- **5-47% better SSIM** vs best prior denoisers

## Core Architecture (Figure 2 + 3)

```
Path Tracer (1 SPP)
    │
    ├─ Direct Illumination ──────┐
    │                            │   Demodulate albedo from both
    │                            ├──▶ (split high-freq texture
    │                            │    from light transport)
    └─ Indirect Illumination ────┘
                                 │
                     ┌───────────▼──────────┐
                     │  Reconstruction      │
                     │  Filter (both)       │
                     │                      │
                     │  1. Temporal accum   │
                     │  2. Variance estim   │
                     │  3. À-trous filter   │
                     └───────────┬──────────┘
                                 │
                         Modulate albedo back
                                 │
                           Tone Mapping
                                 │
                           TAA
                                 │
                           Output
```

**Critical design choice**: Filter **direct** and **indirect** illumination
**separately**. This is because:
- Direct lighting has sharp shadow edges (don't over-blur)
- Indirect lighting is low-frequency noisy (blur aggressively)
- Same filter parameters work for both because of variance guidance

## Inputs Required

**G-Buffer** (from rasterization of primary visibility — no ray tracing):
- Depth (clip-space)
- World-space normal
- Mesh ID (for disocclusion detection)
- Screen-space motion vectors
- Diffuse albedo (for demodulation)

**Path tracer output** (1 SPP):
- Direct color
- Indirect color

**History buffers** (from previous frame):
- Temporally integrated color (direct + indirect)
- First and second luminance moments (for variance)
- Previous frame depth, normal, mesh ID (for consistency tests)

## The Three Core Passes

### Pass 1: Temporal Accumulation (§4.1)

```
C_i = α · Raw_i + (1 - α) · C_{i-1}^backproj
```

**α = 0.2** — 20% new sample, 80% history. This is the authoritative value
from the paper. Same for moments:

```
μ1_i = α · L_i + (1 - α) · μ1_{i-1}
μ2_i = α · L_i² + (1 - α) · μ2_{i-1}
```

Where L_i is the luminance of the current frame's raw sample.

**Backprojection rules**:
1. Backproject via motion vectors → find previous frame pixel
2. Use **2×2 bilinear** tap for sub-pixel accuracy
3. For each tap: test depth + normal + mesh ID consistency
4. Discard inconsistent taps, redistribute their weight to consistent ones
5. If all 2×2 taps fail → try 3×3 (thin geometry like foliage)
6. If 3×3 also fails → **disocclusion**, reset: `C_i = Raw_i`, moments = current sample

### Pass 2: Variance Estimation (§4.2)

**Temporal variance** (primary path):
```
σ² = max(0, μ2_i - μ1_i²)
```

This is the standard `E[X²] - E[X]²` formula, computed on the temporally
accumulated luminance moments.

**Spatial fallback** (when history < 4 frames):
- Use a 7×7 bilateral filter weighted by depth + normal
- Computes variance within that neighborhood
- Lower quality than temporal but works from frame 1

The filter **switches** between temporal and spatial variance based on
history age. This is critical — a disoccluded region can't use temporal
variance because its moments are fresh.

### Pass 3: Edge-Avoiding À-Trous Wavelet Filter (§4.3)

This is from Dammertz et al. 2010 — edge-aware wavelet reconstruction
with 5 iterations of increasing footprint.

**Iteration kernel**: 5×5 cross-bilateral
```
h = (1/16) * [1, 1, 3, 1, 1]  (1D weights, applied separably)
```

**Filter equation** (iteration i+1):
```
c_{i+1}(p) = Σ_q h(q) · w(p,q) · c_i(q) / Σ_q h(q) · w(p,q)
```

**Critical**: Variance is also filtered each iteration, with squared weights:
```
Var(c_{i+1})(p) = Σ_q h²(q) · w²(p,q) · Var(c_i)(q) / (Σ_q h(q) · w(p,q))²
```

Then the **luminance edge-stopping function uses this updated variance**
in the next iteration. So each iteration tightens the filter as variance
shrinks.

**À-trous trick** (Figure 4): Between iterations, tap positions spread
apart with zero-padding. Iteration spread: 1, 2, 4, 8, 16 pixels.
5 iterations → **effective 65×65 filter footprint** at constant 5×5 per-iter cost.

**Output of iteration 0** (first filter pass) is stored as the **color
history** for next frame's temporal accumulation. NOT the final filtered
output — just iteration 0. Why? Balances temporal stability with spatial
bias. Taking iteration 5 would over-blur the history.

### Edge-Stopping Functions (§4.4)

```
w(p, q) = w_z · w_n · w_l
```

**Depth weight** — accounts for oblique surfaces via screen-space depth gradient:
```
w_z = exp(-|z(p) - z(q)| / (σ_z · |∇z(p) · (p-q)| + ε))
```

With `σ_z = 1`. The `∇z(p) · (p-q)` term projects the gradient onto the
pixel-space offset — so a slanted surface doesn't accidentally fail the
depth test.

**Normal weight** — cosine term raised to a high power:
```
w_n = max(0, dot(n(p), n(q)))^σ_n
```

With `σ_n = 128`. Very sharp falloff — even 7° normal difference halves
the weight.

**Luminance weight** — Gaussian on the luminance difference, normalized
by the **local standard deviation**:
```
w_l = exp(-|l(p) - l(q)| / (σ_l · sqrt(g3x3(Var(l(p))))) + ε)
```

With `σ_l = 4`. Critical detail: pre-filter the variance with a **3×3
Gaussian** (denoted `g3x3`) before using it as the denominator — otherwise
noisy variance estimates destabilize the filter (Figure 5).

The variance-normalized luminance weight is why it's called "variance-guided":
- High variance region → wider luminance tolerance → more aggressive filtering
- Low variance region (already converged) → narrow tolerance → preserves detail

**All three σ values (σ_z=1, σ_n=128, σ_l=4) work on all tested scenes.
The paper explicitly says NOT to expose them to users.**

## Algorithm Summary (Pseudocode)

```
// Per frame, per pixel:

// 1. Backproject to previous frame
prev_coord = motion_vector[p]
if (prev_coord out of bounds) disocclusion = true

// 2. Consistency test (2×2 bilinear, fallback 3×3)
for each tap in neighborhood:
    if (depth_consistent && normal_consistent && mesh_id_match)
        accept tap, accumulate weight
if no valid taps: disocclusion = true

// 3. Temporal accumulation
if (disocclusion):
    C_i = Raw_i
    moments = (L_i, L_i²)
    history_age = 1
else:
    C_i = 0.2 · Raw_i + 0.8 · C_{prev}
    μ1 = 0.2 · L_i + 0.8 · μ1_{prev}
    μ2 = 0.2 · L_i² + 0.8 · μ2_{prev}
    history_age += 1

// 4. Variance estimation
if (history_age >= 4):
    variance = max(0, μ2 - μ1²)  // temporal
else:
    variance = spatial_fallback_7x7(C_i)  // spatial

// 5. À-trous wavelet filter (5 iterations)
for i in 0..5:
    for each pixel p:
        numerator = 0
        denominator = 0
        for each tap q in 5×5 (spread by 2^i):
            w = w_z(p,q) · w_n(p,q) · w_l(p,q, variance)
            numerator += h(q) · w · C[q]
            denominator += h(q) · w
        C_filtered[p] = numerator / denominator
        variance[p] = filter variance the same way (squared weights)
    if (i == 0): save as color_history for next frame

// 6. Post processing
output = modulate_albedo(C_filtered) → tone_map → TAA
```

## Why It's Called "Variance-Guided"

Spatial filters like cross-bilateral need to know HOW AGGRESSIVELY to filter.
Too much → over-blur. Too little → residual noise.

SVGF solves this by using the **per-pixel variance estimate** to automatically
tune the luminance weight. Noisy regions (high variance) get wide filter
kernels; converged regions (low variance) keep their detail.

The variance itself is estimated from temporal accumulation of luminance
moments — essentially "how much does this pixel's luminance fluctuate over
time." High fluctuation = noisy = needs more filtering.

## Performance Details (Titan X Pascal, 2017)

**SanMiguel courtyard, 1280×720**:
- Average frame cost: **4.4 ms**
- Variation: 4.1–5.8 ms (15% max)
- Spikes: disocclusion-heavy frames, foliage
- Worst case: application start, camera cuts (~50% slower)

**1920×1080**: ~10 ms.

**Breakdown** (from Figure 8):
- Temporal filter: ~1 ms
- Variance estimation: ~1 ms (spikes on disocclusion)
- À-trous wavelet: ~2 ms
- TAA (post-process): ~1 ms

## Limitations (§6)

1. **Chrominance over-blur**: luminance-only variance tracking can't distinguish
   color noise. Fixable by tracking RGB variance separately (3× memory).

2. **Detached shadows in fast motion**: temporal accumulation doesn't know
   about changed illumination from moving geometry.

3. **Extremely low-light noise instability**: variance estimation unreliable
   when sample density → 0.

4. **Specular reflection blur**: noisy specular reflections (from poorly-sampled
   indirect) get blurred away.

5. **Motion blur on sharp features**: backprojection introduces non-zero
   variance even for static features, causing over-blur under motion.

6. **Incompatible with stochastic primary rays**: depth-of-field, stochastic
   transparency, etc. — primary rays must be noise-free (G-buffer from raster).

7. **Aliased sub-pixel geometry**: edge-stopping functions fail, preventing
   filtering.

**Most of these are fixed in the 2018 A-SVGF paper via adaptive α.**

## Implementation Notes for ByroRedux

### What this paper adds over A-SVGF notes

1. **Exact σ values**: σ_z=1, σ_n=128, σ_l=4 — use these.
2. **Wavelet kernel**: 5×5 with weights `h = (1/16) * [1, 1, 3, 1, 1]` (separable)
3. **5 iterations** = 65×65 effective filter footprint
4. **Variance pre-filter**: 3×3 Gaussian on variance before edge-stopping
5. **Color history = iteration 0 output**, NOT iteration 5
6. **Temporal α = 0.2** (was 0.1 for A-SVGF)
7. **Separate direct/indirect filtering** — not a single color buffer
8. **Albedo demodulation** — filter untextured light, re-apply texture after

### Minimum viable SVGF implementation for us

**Prerequisites** (must happen first):
1. Motion vectors: render target R16G16_SFLOAT, `prev_pos - curr_pos` in
   clip space. Needs previous viewProj matrix uniform.
2. G-buffer: at minimum depth + normal + mesh ID. Our current forward pass
   writes depth; we need to add a normal attachment and mesh ID attachment.
3. Indirect light separation: instead of accumulating `Lo + indirect`, write
   `indirect` to its own render target. Direct lighting stays in the main
   forward pass.
4. Albedo render target: write texture-modulated albedo separately so
   demodulation is just a division.

**SVGF pipeline** (new passes):

```
Pass A: Forward pass (existing, modified)
  - Outputs: final color, depth, normal, mesh ID, motion vector, albedo,
    raw_indirect (new), moments (new: L, L²)

Pass B: Temporal accumulation (compute shader)
  - Inputs: raw_indirect_i, color_history_{i-1}, moments_history_{i-1},
    G-buffer curr/prev (for consistency tests)
  - Outputs: accumulated_indirect_i, accumulated_moments_i

Pass C: Variance estimation (compute shader)
  - Inputs: accumulated_moments_i, history_age_i, G-buffer
  - Outputs: variance_i (temporal if history ≥ 4, spatial fallback otherwise)

Pass D: À-trous filter × 5 iterations (compute shader, ping-pong)
  - Inputs: accumulated_indirect_i, variance_i, G-buffer
  - Outputs (per iteration): filtered indirect, updated variance
  - Iteration 0 output → save as color_history for next frame
  - Iteration 5 output → final filtered indirect

Pass E: Composite (fragment or compute)
  - final = direct + filtered_indirect
  - Apply tone mapping
  - (Optional) TAA pass
```

**Rough cost estimate** (RTX 4070 Ti vs Titan X, ~5× faster):
- Temporal accumulation: ~0.3 ms
- Variance estimation: ~0.3 ms  
- 5× À-trous: ~1.5 ms total
- **Total SVGF overhead: ~2-3 ms** at 1080p

### Binding budget

Currently using bindings 0-9 on set 1. SVGF would need:
- Binding 10: indirect light storage (RGBA16F)
- Binding 11: color history (RGBA16F)
- Binding 12: moments buffer (RG32F)
- Binding 13: variance (R32F)
- Binding 14: motion vectors (RG16F)

That's 5 new bindings — we're over the 12-binding minimum. Options:
- Move to multiple descriptor sets
- Use a single "SVGF resources" descriptor set (set 2)
- Combine into fewer, larger textures

### The Big Picture

Our current GI implementation is:
```
GI ray → accumulate directly into forward pass output
```

What it needs to be (SVGF architecture):
```
GI ray → raw indirect buffer
            ↓
         temporal accumulation (α=0.2 + consistency tests)
            ↓
         variance estimation (temporal + spatial fallback)
            ↓
         5× À-trous wavelet filter (variance-guided edge-stopping)
            ↓
         filtered indirect
            ↓
         composite: direct + filtered_indirect → tone map → output
```

This is non-trivial infrastructure but it's **the standard real-time RT
denoising pipeline**. Every modern RT renderer does some variant of this.

### What This Means for Our Roadmap

The previous plan (window lights + 1-bounce GI) skipped the denoising
infrastructure entirely. To do it properly we need to add:

1. **Motion vectors** — 1-2 hours
2. **G-buffer additions** (normal, mesh ID, raw indirect, moments) — 2-3 hours
3. **Temporal accumulation pass** — 2-3 hours
4. **Variance estimation pass** — 2-3 hours
5. **À-trous wavelet filter (5 iterations)** — 3-4 hours
6. **Composite with demodulated albedo** — 1 hour
7. **Optional: TAA pass** — 2 hours

Call it **~15-20 hours** for a full SVGF implementation. Then GI would be
rock-solid and we could experiment with ReSTIR GI on top.

Alternative: skip SVGF for now, implement just temporal accumulation with
motion vectors (steps 1-3) for ~6-8 hours. Would get us 60-70% of the way
there without the full filter pipeline. Then add variance + À-trous later
if needed.
