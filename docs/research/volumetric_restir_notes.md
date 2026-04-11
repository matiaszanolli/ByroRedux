# Volumetric ReSTIR — Research Notes

**Paper**: "Fast Volume Rendering with Spatiotemporal Reservoir Resampling"
**Authors**: Daqi Lin (U. Utah), Chris Wyman (NVIDIA), Cem Yuksel (U. Utah)
**Venue**: ACM TOG / SIGGRAPH Asia 2021
**PDF**: `volumetric_restir.pdf`

## Core Contribution

Extends ReSTIR (originally for direct illumination on surfaces) to volumetric
path tracing with multiple scattering and volumetric emission. Achieves
interactive performance (10–90ms) at 1 SPP by reusing samples across
pixels (spatial) and frames (temporal).

## ReSTIR Fundamentals (from §2.4–2.5)

### Resampled Importance Sampling (RIS)

Given a function f(x) to integrate, RIS generates M candidate samples
from a cheap source PDF p(x), then selects one sample proportional to
a target PDF p̂(x) that better matches f(x):

- Generate M candidates x₁...xₘ from source p
- Select sample xᵣ with probability proportional to w(xᵢ) = p̂(xᵢ)/p(xᵢ)
- The 1-sample RIS estimator:
  ⟨f⟩ = f(xᵣ) · (1/M · Σw(xᵢ)) / p̂(xᵣ)

**Key insight**: As M→∞, the distribution of xᵣ approaches p̂. RIS is
effective when p̂ closely matches f and generating/evaluating candidates
from p and w(xᵢ) are cheap.

### Reservoir Sampling (Weighted)

RIS can be done in streaming fashion via weighted reservoir sampling
(Chao 1982): only the selected sample and a running weight sum are stored.
Many candidates M can be considered without storing all M samples.

Each new candidate xₘ₊₁ is selected with probability:
  p(xₘ₊₁ | xᵣ ∪ {xₘ₊₁}) = w(xₘ₊₁) / Σⱼw(xⱼ)

### Spatiotemporal Reuse

**Temporal**: Pass the selected sample xᵣ forward for reuse next frame.
The effective candidate count M grows exponentially across frames.
A temporal limiting factor Q caps M to prevent unbounded influence
of stale samples (typical Q = 4–20).

**Spatial**: After per-pixel reservoir construction, combine reservoirs
from neighboring pixels using RIS. Correction factor w_{q→q'} accounts
for different target PDFs between pixels:
  w_{q→q'} = (p̂_q(x_q') / p̂_q'(x_q')) · w_q'^sum

Uses 3 random neighbors within 10-pixel radius (low-discrepancy sequence).

**Combined**: Temporal → spatial reuse. Quality progression (Fig. 4):
  - RIS only (no reuse): 17ms — noisy
  - Temporal only: 32ms — much cleaner
  - Spatial only: 42ms — cleaner still
  - Spatiotemporal: 45ms — best quality

## Key Design Decisions Relevant to Our GI

### 1. Reservoir Data Structure
Stores per-pixel: selected sample + running weight sum.
Minimal memory — just 1 sample persisted, not M candidates.

### 2. Temporal Reprojection
Uses motion vectors to find where the current pixel was last frame.
For volumes, they use velocity resampling (stochastic motion vector
selection). For surfaces, standard reprojection works.

### 3. MIS Weighting
Stochastic MIS (Bitterli et al. 2020) is O(1) but introduces noise.
Deterministic Talbot MIS (Talbot 2005) is O(N) but cleaner.
For volumes, Talbot MIS works better despite higher cost (Fig. 7).

### 4. Transmittance Optimization
Computing transmittance T is expensive (ray marching).
They use a piecewise-constant volume approximation for cheap
candidate generation, then analytical T only for the final selected sample.
Coarser approximations (Mip 1–2) give most of the benefit (Fig. 12).

### 5. Temporal Limiting Factor Q
Q controls how much history to accumulate:
- Q=1: temporal samples get low weight → noisy
- Q=4: good balance (their default)
- Q=20: more accumulated → smoother, but stale under lighting changes
Larger Q under complex illumination reduces fireflies (Fig. 14).

## Performance Numbers (RTX 3090, 1920×1080)

| Scene | Their Method | Baseline (equal time) |
|-------|-------------|----------------------|
| Bunny Cloud (1 bounce) | 42ms, MSE 0.0026 | 42ms, MSE 0.0096 |
| Plume (1 bounce) | 13ms, MSE 0.0015 | 13ms, MSE 0.0053 |
| Explosion (2 bounces) | 38ms, MSE 0.0032 | 42ms, MSE 0.0070 |
| Bistro (1 bounce) | 47ms, MSE 0.0158 | 45ms, MSE 0.0189 |

~2–3× MSE reduction at equal render time. Converges to reference
in 1–10 seconds of accumulation.

## Applicability to ByroRedux

### Directly applicable:
- **Reservoir data structure** for our GI: store 1 sample per pixel
  (hit position, albedo, indirect radiance) in a storage image
- **Temporal reuse**: reproject last frame's GI sample via motion vectors
  (or simpler: screen-space reprojection since our scenes are mostly static)
- **Spatial reuse**: combine neighboring pixel reservoirs to reduce noise
- **Temporal limiting factor Q**: prevents ghosting during camera motion

### Not directly applicable:
- Volumetric path generation (we don't have participating media yet)
- Transmittance estimation (no volumes)
- Multiple scattering (our GI is 1-bounce surface-to-surface)

### Architecture for our case:
Instead of raw 1-SPP noise → clamp → hope, we should:
1. Store GI sample in a per-pixel reservoir (R16G16B16A16 storage image)
2. Temporal reuse: blend current sample with reprojected history (α=0.05–0.1)
3. Spatial reuse: 3–5 neighbor samples within 10px, weighted by normal/depth similarity
4. Use the accumulated reservoir value as the indirect term

This is essentially what NVIDIA's ReSTIR GI does for surface indirect
lighting (Ouyang et al. 2021, referenced in §1 of this paper).
