# ReSTIR GI — Implementation Notes

**Paper**: "ReSTIR GI: Path Resampling for Real-Time Path Tracing"
**Authors**: Y. Ouyang, S. Liu, M. Kettunen, M. Pharr, J. Pantaleoni (NVIDIA)
**Venue**: High Performance Graphics 2021 / Computer Graphics Forum 40(8)
**PDF**: `restir_gi.pdf` (gitignored)

This is the foundational paper for real-time RT indirect lighting. It's
what Cyberpunk 2077 and Unreal Engine 4 ship. **This is exactly what we
need to implement for our GI problem.**

## Results (RTX 3090, 1080p)
- **9.3× to 166× MSE reduction** vs. naive path tracing at equal time
- 8–18 ms total cost per frame (including GI + reuse)
- 1 sample per pixel, works with standard spatio-temporal denoisers

## The Core Algorithm (Four Passes)

### Data Structures

**Sample struct** (48 bytes compressed):
```
struct Sample {
    float3 xv, nv;      // Visible point position + normal (on rasterized surface)
    float3 xs, ns;      // Sample point position + normal (hit by ray from xv)
    float3 Lo;          // Outgoing radiance at xs toward xv (f16 in storage)
    uint   random;      // RNG seed for sample validation
}
```

**Reservoir struct**:
```
struct Reservoir {
    Sample z;       // Currently selected sample
    float  w;       // Running weight sum (w = w + w_new during update)
    int    M;       // Count of candidates considered
    float  W;       // Final RIS weight: W = w / (M * p̂(z))
}
```

**Three screen-space buffers** (per frame, double-buffered for temporal reuse):
1. Initial sample buffer
2. Temporal reservoir buffer
3. Spatial reservoir buffer

### Weighted Reservoir Sampling (WRS)

```glsl
void reservoirUpdate(inout Reservoir r, Sample sNew, float wNew) {
    r.w += wNew;
    r.M += 1;
    if (random() < wNew / r.w) {
        r.z = sNew;
    }
}

void reservoirMerge(inout Reservoir r, Reservoir other, float p_hat) {
    int M0 = r.M;
    reservoirUpdate(r, other.z, p_hat * other.W * other.M);
    r.M = M0 + other.M;
}
```

### Pass 1: Initial Sampling

For each pixel q with visible point xv:
1. Sample random direction ωi from source PDF pq (uniform hemisphere is best)
2. Trace ray → find sample point xs
3. At xs, compute outgoing radiance L̂o toward xv via path tracing
   (next-event estimation + BSDF sampling for multi-bounce)
4. Store `Sample{xv, nv, xs, ns, L̂o}` in initial buffer

**Key finding**: Uniform hemisphere sampling beats cosine-weighted for
grazing-angle light (e.g. sunlight through windows).

### Pass 2: Temporal Resampling

```glsl
for each pixel q {
    Sample S = initialBuffer[q];
    // Reproject pixel using motion vectors (or screen-space identity if static)
    Reservoir R = temporalBuffer_prev[reprojected(q)];

    float w = p_hat(S) / p_source(S);  // RIS weight
    reservoirUpdate(R, S, w);
    R.W = R.w / (R.M * p_hat(R.z));

    temporalBuffer[q] = R;
}
```

**Target function** (simple, works well):
```
p̂(sample) = luminance(sample.Lo)  // Just use outgoing radiance brightness
```

They also tested the more physically accurate `Lo * BSDF * cos(θ)` but the
simpler target function is more robust for spatial reuse because it preserves
samples useful at other pixels.

### Pass 3: Spatial Resampling

```glsl
for each pixel q {
    Reservoir Rs = spatialBuffer[q];
    for s = 1 to maxIterations {
        pixel qn = randomNeighbor(q);  // within search radius

        // Geometric similarity test: skip if normals differ >25° or
        // normalized depth differs >0.05
        if (!similar(q, qn)) continue;

        Reservoir Rn = temporalBuffer[qn];

        // CRITICAL: Jacobian correction for different visible-point geometry
        float jacobian = computeJacobian(q, qn, Rn.z);
        float p_hat_adjusted = p_hat(Rn.z) / jacobian;

        // Optional visibility ray (unbiased mode)
        if (unbiased && !visible(xv_q, Rn.z.xs)) p_hat_adjusted = 0;

        reservoirMerge(Rs, Rn, p_hat_adjusted);
    }
    Rs.W = Rs.w / (M_sum * p_hat(Rs.z));
    spatialBuffer[q] = Rs;
}
```

**Jacobian determinant** for reusing sample from pixel qn at pixel q:
```
|J_qn→q| = (|cos(φr2)| / |cos(φq2)|) · (||x1q - x2q||² / ||x1r - x2q||²)
```

Where:
- x1 = visible point (q or r = current or source)
- x2 = sample point
- φ2 = angle between view direction and normal at sample point

**Figure 7 in the paper shows this is essential** — without the Jacobian,
you get "lighting discontinuities on the floor and overestimated lighting
at the base of the wall."

### Pass 4: Shading

```glsl
float3 indirect = bsdf(xv, ωo, Rs.z.xs - xv) *
                  Rs.z.Lo *
                  max(dot(nv, normalize(Rs.z.xs - xv)), 0) *
                  Rs.W;
```

This is the RIS estimator: `L = f(z) * W(z)` where f is the integrand
and W is the reservoir weight.

## Critical Implementation Details

### Clamping M (Confidence Weight Cap)
- Temporal reservoir M capped at **30**
- Spatial reservoir M capped at **500**
- Higher = smoother but slower lighting updates

### Sample Validation (Fixing Temporal Bias)
Every 6 frames, re-trace the rays using stored random numbers to
re-compute outgoing radiance. If new value is outside tolerance, clear
the reservoir. This handles lighting changes and moving occluders.

The random numbers MUST be the same as when the sample was generated,
which is why Sample struct stores `random`.

### Spatial Reuse Strategy (Bias Reduction)
- Spatial reservoirs should reuse **temporal** reservoirs from neighbors,
  NOT other spatial reservoirs (avoids cascading bias)
- **Exception**: when M is low (newly disoccluded), allow spatial→spatial
  reuse to speed up convergence

### Adaptive Search Radius
- Start at 10% of image resolution
- Halve on failure (no valid neighbor found)
- Minimum: 3 pixels
- Keep unchanged after successful reuse

### maxIterations (Spatial Samples)
- 9 iterations when M < maxM/2 (high noise)
- 3 iterations when M ≥ maxM/2 (stable)

### Double Buffering
Both temporal and spatial reservoirs need double buffers (ping-pong)
to avoid read/write races when neighbors access each other's reservoirs.
This introduces **1-frame lag** in indirect lighting, acceptable at high FPS.

### Multi-Bounce Optimization
Multi-bounce paths are expensive. Trick:
- Divide screen into 64×32 tiles
- Apply Russian roulette at tile level (25% of tiles get multi-bounce)
- Tiles failing RR use single-bounce + reweight by RR probability
- Expected value remains correct, cost drops dramatically

### Memory Budget (1080p)
- 475 MB total for reservoir buffers
- 570 MB/frame bandwidth for reads
- 285 MB/frame bandwidth for writes

## Key Simplifications They Made
- **Lambertian assumption at sample points**: the radiance `Lo` is stored
  as if scattered uniformly. Accurate for diffuse surfaces, error grows
  with specular surfaces. Visible points can still be glossy.
- **No directional variation of Lo**: stored as a single RGB value,
  not a lobe. Cheap, works well for diffuse interiors.

## Biased vs Unbiased

### Biased (recommended for real-time):
- Skip visibility check during spatial reuse (line 12-13 of Algorithm 4)
- Skip visibility ray for sample validation
- Result: slight darkening in shadow areas, but cheaper and less noisy

### Unbiased:
- Trace visibility ray during spatial reuse
- Must also trace ray for Jacobian-corrected visibility
- 15-20% slower but mathematically correct

**Cyberpunk and UE4 both ship biased ReSTIR GI.**

## Limitations (Section 6.1)

1. **Disocclusion noise**: newly-visible pixels (fast camera motion)
   have no history, produce noise until spatial reuse fills in
2. **Specular surfaces**: Lambertian Lo assumption breaks, spatial reuse
   less effective because specular lobes are narrow
3. **Correlated output**: consecutive frames share samples; some denoisers
   (SVGF) assume independent noise and can produce artifacts
4. **Budget**: still too expensive for sub-30fps GPUs with <2ms budget

## What This Means for ByroRedux

### Required Changes (in order)

**Phase A: Minimal Temporal Reuse (MVP)**
1. Add R16G16B16A16F storage image (binding 10) — double buffered
2. In fragment shader: blend current indirect with previous frame's value
3. α = 0.05 (20-frame convergence), α = 1.0 on disocclusion
4. **This alone would fix the current flashing problem**

**Phase B: Reservoir Infrastructure**
1. Add Sample + Reservoir structs to GLSL
2. Initial sampling pass → initial buffer (can stay in fragment shader)
3. Temporal reservoir buffer (binding 10) — double buffered
4. Temporal reuse pass: can be inlined in fragment shader for MVP

**Phase C: Spatial Reuse**
1. Spatial reservoir buffer (binding 11) — double buffered
2. Separate compute pass or second fragment pass for spatial reuse
3. Geometric similarity test (depth + normal)
4. Jacobian computation for bias correction

**Phase D: Sample Validation**
1. Every N frames, re-trace rays with stored random seeds
2. Clear reservoirs with stale radiance

### Descriptor Budget
Current: 10 bindings on set 1 (0-9)
Vulkan minimum: 12 per set
Available: 2 bindings (10, 11) — exactly what we need for reservoir buffers

### Performance Budget (RTX 4070 Ti, our target)
The paper shows 8-18ms on RTX 3090 at 1080p. Our 4070 Ti should be
similar. At 60fps (16.7ms budget) this is tight but feasible if we
budget ~8ms for ReSTIR GI and keep everything else lean.

### What NOT to Do
- **Don't** try to match the paper's multi-bounce capability initially.
  1-bounce is enough for our use case (interior ambient lighting).
- **Don't** implement the full biased/unbiased toggle. Ship biased only.
- **Don't** implement full spatial reuse on first iteration. Temporal
  reuse alone would already be a massive improvement.

### The Big Realization
Our current GI problem isn't about tuning intensity or noise hold factors.
It's that we're missing **all four passes** of the ReSTIR GI algorithm
and trying to substitute raw 1-SPP sampling. The paper explicitly shows
that raw 1-SPP sampling is fundamentally noisy (Figure 1 left) and that
it takes the full reservoir-based pipeline to produce clean results.

The "right" incremental path:
1. **Temporal accumulation only** (α-blend in a storage image) — 1-2 hours
   work, 80% of the visual improvement
2. **Full reservoir temporal reuse** — proper ReSTIR GI temporal pass
3. **Spatial reuse** — the last 20% of quality, highest complexity
