# ReSTIR Overview — Research Notes

**Source**: Wikipedia article on Spatiotemporal Reservoir Resampling
**Key papers referenced**:
- Talbot et al. 2005 — Resampled Importance Sampling (RIS) foundation
- Bitterli et al. 2020 — ReSTIR DI (direct illumination), the original
- Ouyang et al. 2021 — **ReSTIR GI** (indirect illumination, our target)
- Lin et al. 2022 — GRIS (generalized path tracing framework)

## ReSTIR Family — Which Variant We Need

| Variant | Use Case | Relevance to Us |
|---------|----------|-----------------|
| ReSTIR DI | Direct lighting from many lights | Already solved by clustered shading |
| **ReSTIR GI** | **Indirect illumination (diffuse)** | **Exactly our GI problem** |
| ReGIR | Light sampling in 3D grid | Interesting but more complex |
| Volumetric ReSTIR | Clouds, smoke, fog | Not needed yet |
| GRIS / ReSTIR PT | Full path tracing, specular | Overkill for now |

## Core Architecture

### The Reservoir
Per-pixel data structure containing:
- **1 selected sample** (the "winner" from streaming RIS)
- **Running weight sum** (wₛᵤₘ = Σw(xᵢ) over all candidates seen)
- **Confidence weight** (M = effective sample count)

Key insight: only 1 sample stored regardless of how many candidates
were evaluated. Memory = O(pixels), not O(pixels × candidates).

### Streaming RIS (Weighted Reservoir Sampling)
Process M candidates one at a time:
1. For each new candidate xₘ₊₁, compute weight w(x) = p̂(x)/p(x)
   - p̂ = target PDF (what we want — e.g. proportional to luminance contribution)
   - p = source PDF (what we can cheaply sample from)
2. Accept candidate with probability w(xₘ₊₁) / Σw
3. Update running weight sum

Result: selected sample has distribution approaching p̂ as M→∞.
The 1-sample RIS estimator: ⟨f⟩ = f(xᵣ) · (1/M · Σw) / p̂(xᵣ)

### Per-Frame Pipeline (4 steps)

```
1. Initial Sampling
   - Generate M candidate light/path samples from cheap source PDF
   - Select 1 via streaming RIS using target PDF (luminance-proportional)
   - Test visibility (shadow ray) for selected sample only
   - Store in per-pixel reservoir

2. Temporal Reuse
   - Backproject current pixel to previous frame (motion vectors)
   - Merge previous frame's reservoir into current using RIS
   - MIS weights prevent bias from different target PDFs
   - Cap confidence weight at Q (typically 4-20)

3. Spatial Reuse
   - Choose 1-5 random neighbor pixels within 10-30px radius
   - Merge their reservoirs into current pixel's reservoir
   - MIS weights account for different surface/lighting at neighbors
   - Heuristics: prefer neighbors with similar depth/normal
   - Can repeat multiple times per frame

4. Final Shading
   - Use reservoir's sample + weight to compute pixel color
   - weight × f(sample) is unbiased estimate of correct color
```

## Critical Design Decisions

### Biased vs Unbiased
- **Unbiased**: requires 2+ rays per pixel per reuse step for MIS weights
- **Biased**: skip visibility checks during reuse, accept darkening at edges
- **Biased is often preferred** for real-time: less noise, faster, edge darkening
  is less objectionable than noise
- Cyberpunk 2077 uses biased ReSTIR

### Confidence Weight Capping (Q)
- Without cap: confidence grows exponentially → old samples never replaced
- Q = 4-20 typical (Q=4 for dynamic lighting, Q=20 for static scenes)
- Higher Q = smoother but slower to adapt to lighting changes
- Lower Q = noisier but more responsive

### Temporal Reprojection
- Use motion vectors to find where current pixel was last frame
- If disoccluded (new surface revealed): set confidence = 0, reinitialize
- Unlike TAA: must select a single sample, no interpolation

### Spatial Neighbor Selection
- Random within 10-30px radius (low-discrepancy sequence)
- Heuristic filtering: prefer similar depth + normal (doesn't examine reservoir samples — that would introduce bias)
- 3 neighbors at 10px radius is a good balance (Volumetric ReSTIR paper)

## Noise Characteristics

### Why Our Current Approach Fails
Our raw 1-SPP approach produces the exact problem described in the article:
"each pixel uses the color of a single light source, which produces very
visible noise even though the pixels have approximately the correct brightness"

The article explicitly states:
- **Reservoir output is inherently noisy** — denoising is always needed
- **Color noise** is worse than luminance noise because the single sample
  picks one color, not a blend
- **Spatial correlation** causes blotchiness (few "good" samples reused many times)

### Solutions (in order of complexity)
1. **Temporal accumulation** (exponential moving average) — simplest
2. **Reservoir-based temporal reuse** (proper ReSTIR) — mathematically sound
3. **Spatial reuse** on top of temporal — highest quality
4. **Denoiser** as final pass — always recommended even with ReSTIR

## What Cyberpunk 2077 Does
- Uses ReSTIR for both direct and indirect illumination
- Biased mode (skip some visibility checks for performance)
- Denoiser on output
- One of the first games to ship with ReSTIR (2020+)

## Applicability to ByroRedux

### Immediate (MVP): Temporal Accumulation Buffer
Before implementing full ReSTIR, a simple temporal accumulation buffer
would eliminate the noise problem:
- RGBA16F storage image per frame (ping-pong)
- Each frame: new_value = mix(previous, current_sample, α)
  - α = 0.05 for static camera (20-frame convergence)
  - α = 0.2 for moving camera (5-frame convergence)
  - α = 1.0 for disoccluded pixels (reinitialize)
- Use motion vectors (or screen-space reprojection) for temporal lookup

### Medium-term: ReSTIR GI (Ouyang et al. 2021)
Full reservoir-based GI with:
- Per-pixel reservoir storing: hit position, hit normal, incident radiance
- Temporal reuse with confidence weight capping (Q=10-20)
- 1-3 spatial reuse passes with depth/normal heuristics
- Reconnection shift mapping for diffuse surfaces
- Biased mode acceptable for our use case

### Long-term: GRIS / ReSTIR PT
Full path tracing with specular support, shift mappings, etc.
Not needed until we have complex specular/glass interiors.

## Key References to Read
1. **Ouyang et al. 2021** — "ReSTIR GI: Effective Path Resampling for
   Real-Time Path Tracing" — THE paper for our indirect lighting problem
2. **Bitterli et al. 2020** — original ReSTIR DI paper (foundation concepts)
3. **Lin et al. 2022** — GRIS paper (mathematical framework, shift mappings)
