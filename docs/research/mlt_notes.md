# Metropolis Light Transport (MLT)

**Paper**: "Metropolis Light Transport"
**Authors**: Eric Veach, Leonidas J. Guibas (Stanford)
**Venue**: SIGGRAPH 1997
**PDF**: `metro.pdf` (gitignored)

**⚠ Relevance note**: This is a **foundational offline rendering paper** from
1997. It is **not applicable to real-time rendering** or our current ReSTIR/SVGF
research thread. Including notes for historical/conceptual context only.

## What It Is

MLT was the first application of the **Metropolis-Hastings algorithm** (from
computational physics, 1953) to light transport in graphics. It's an unbiased
Monte Carlo method that uses **Markov Chain Monte Carlo (MCMC)** to efficiently
sample difficult light transport paths.

The key idea: instead of generating independent paths, generate a **random
walk** through path space where each new path is a small **mutation** of the
previous one. Paths that contribute more to the image are visited more often
(proportional to their contribution).

## The Core Algorithm

```
x̄ ← InitialPath()
image ← { zeros }
for i ← 1 to N:
    ȳ ← Mutate(x̄)              // propose mutation
    a ← AcceptProb(ȳ | x̄)      // Metropolis-Hastings ratio
    if Random() < a:
        x̄ ← ȳ                  // accept
    RecordSample(image, x̄)     // accumulate to pixel
return image
```

**The Metropolis-Hastings acceptance probability**:
```
a(ȳ | x̄) = min(1, f(ȳ) · T(x̄|ȳ) / (f(x̄) · T(ȳ|x̄)))
```

Where:
- `f` is the image contribution function (path → contribution to image)
- `T(ȳ|x̄)` is the tentative transition probability (how likely the mutation was)

The magical property: after many iterations, paths are sampled **proportionally
to their contribution `f`**, regardless of the starting path.

## Mutation Strategies (§5.3)

MLT's power comes from designing good mutations. The paper proposes three:

### 1. Bidirectional Mutations (§5.3.1)
Replace a subpath of the current path with a new subpath. Can change path
length (add/remove vertices), can change which light source is used. The
foundation mutation — ensures ergodicity so the walk can't get stuck in
a subregion of path space.

### 2. Perturbations (§5.3.2)
**Small** mutations that keep the overall path structure but slightly
move vertices. Used when the current path is in a high-contribution region
(caustic, small hole, etc.) — bidirectional mutations would usually jump
*out* of these regions, but perturbations stay nearby to exploit the find.

Two variants:
- **Lens perturbations**: regenerate from the lens side, good for diffuse
  surfaces
- **Caustic perturbations**: regenerate from the light side, good for
  specular caustics (L-S-S-D-E paths)

### 3. Lens Subpath Mutations (§5.3.3)
Modify only the lens edge to redistribute samples across the image plane
without changing the rest of the path. Used to address the "balls in bins"
problem where stratification is lost.

## Why It Was Important (1997)

1. **First MCMC-based renderer** — opened a whole branch of graphics
   research (Kelemen MLT, ERPT, Multiplexed MLT, Gradient-Domain MLT, etc.)

2. **Handles difficult scenes** — caustics, bright indirect lighting,
   small geometric holes, glossy surfaces. Scenarios where unidirectional
   path tracing fails catastrophically.

3. **Path integral formulation** — the mathematical framing of light
   transport as an integral over **path space** (rather than solving
   the rendering equation iteratively) became the foundation for BDPT,
   VCM, ReSTIR PT, and most modern offline renderers.

4. **Unbiased** — guaranteed to converge to the correct answer (unlike
   biased techniques which may have systematic errors).

## Why It's NOT Applicable to Real-Time Rendering

### The fundamental mismatch

MLT is a **progressive** algorithm. It works by accumulating millions of
samples over seconds or minutes, where the Markov chain has time to
explore the path space. At 1 sample per pixel per frame at 60 FPS,
there's simply not enough iterations for the chain to converge.

Specifically:
1. **Correlation between consecutive samples**: Metropolis samples are
   strongly correlated (each is a small mutation of the previous). This
   is fine for offline accumulation but creates visible streak artifacts
   if each sample is a different frame.

2. **Rejection stalls**: a rejected mutation means the same path is recorded
   twice. At low sample counts, this shows as repeated pixel hits that look
   like noise clumps.

3. **Start-up bias**: the chain needs to "warm up" to converge to the
   stationary distribution. At real-time rates, you're always in the warm-up
   phase.

4. **No temporal coherence between frames**: MLT has no concept of reusing
   information from the previous frame like ReSTIR does with reservoirs
   or SVGF does with temporal accumulation.

5. **Global state**: Metropolis maintains a single current path in memory
   and mutates it. This doesn't parallelize well to thousands of pixels
   computed simultaneously on a GPU.

### Gradient-Domain MLT and follow-ups

Later work (Kelemen 2002, Cline 2005, Jakob 2012, Kaplanyan 2014,
Hachisuka 2014, Lehtinen 2013 gradient-domain) improved MLT's parallelism
and convergence. These are still **offline** techniques — used in
production renderers (Mitsuba, PBRT) but not real-time.

## Conceptual Connections to Our Current Work

Despite being non-applicable, MLT has some interesting conceptual
overlaps with ReSTIR:

### 1. Both are about finding "important" paths efficiently

- **MLT**: Markov chain naturally concentrates on high-contribution paths
- **ReSTIR**: reservoir resampling concentrates on high-weight samples

### 2. Both reuse information across samples

- **MLT**: consecutive samples share most of the path (one mutation)
- **ReSTIR**: reservoirs share samples across pixels (spatial) and
  frames (temporal)

### 3. Both handle the "small important region" problem

- **MLT**: perturbations exploit the current high-contribution region
- **ReSTIR**: spatial reuse propagates a good sample to neighboring pixels

The deep insight: both algorithms replace expensive global search
with cheap local exploration once a good path is found. MLT does it
temporally (next iteration). ReSTIR does it spatially (neighboring pixels
this frame) and temporally (same pixel next frame).

### ReSTIR PT (Lin 2022) is the generalization

The "ReSTIR PT" paper (Lin, Kettunen, Yuksel 2022, a.k.a. "Generalized
Resampled Importance Sampling") brings MLT-style path space operations
into the ReSTIR framework. It defines **shift mappings** between paths
that are exactly analogous to MLT's mutations:

| MLT (1997)                 | ReSTIR PT (2022)         |
|----------------------------|--------------------------|
| Bidirectional mutation     | Random replay shift      |
| Lens perturbation          | Reconnection shift       |
| Caustic perturbation       | Half-vector copy shift   |

Reading MLT first makes the ReSTIR PT paper's concepts much easier to
understand. The 25-year gap between them shows how long foundational ideas
take to translate into real-time techniques.

## What We Can Learn From It

Even without implementing MLT, these concepts matter for our work:

### 1. Path space thinking
Light transport as integration over paths (not solving the rendering
equation). This is the framing used by ReSTIR GI, BDPT, and every
modern advanced renderer. Worth internalizing.

### 2. The importance of exploration vs exploitation
When you find a path that contributes a lot, you should **stay nearby**
to extract more value from it before moving on. This is exactly what
ReSTIR's temporal reuse does at the pixel level.

### 3. Ergodicity constraints
If your sampling strategy can't reach every path, your image will
be biased. MLT's bidirectional mutations exist specifically to prevent
the walk from getting stuck. Our ReSTIR implementation needs analogous
properties (e.g., always including some random new candidates) to
avoid getting locked into bad samples.

### 4. The acceptance probability structure
MLT's Metropolis-Hastings ratio `f(ȳ)/f(x̄) · T(x̄|ȳ)/T(ȳ|x̄)` is
the mathematical foundation of "how to correctly reuse samples."
ReSTIR's reservoir merging is a descendant of this idea — the weight
`p̂_q(r.y) · r.W · r.M` plays a similar role.

## Honest Recommendation

**Don't implement MLT**. It's a 27-year-old offline algorithm and there
are better modern techniques (ReSTIR, SVGF, path guiding) for every real-time
use case. However:

1. **Read §4 (path integral formulation)** — the mathematical framing is
   still standard and you'll encounter it in every modern paper
2. **Understand the acceptance probability concept** — it's the root of
   how sample reuse can be made unbiased
3. **If you ever want to work on offline rendering** (e.g., a reference
   renderer for validating our real-time output), Metropolis methods
   are still relevant

For our current real-time GI problem, this paper is **background
context**, not actionable. The priority order remains:
1. Motion vectors + G-buffer (blocking everything)
2. SVGF denoising (fixes our noisy GI)
3. ReSTIR GI (principled indirect lighting)

## The Pattern I'm Noticing

The last few papers (NGLOD, MLT) have been tangential to our active
research thread (real-time GI denoising). I want to flag this in case
it's unintentional. If the goal is **building a comprehensive rendering
library**, MLT and NGLOD belong in it. If the goal is **fixing the
flashing GI right now**, neither helps directly.

Papers that would **directly advance** the current thread:
- Ouyang et al. 2021 — ReSTIR GI (already have notes)
- Bitterli et al. 2020 — ReSTIR DI (already have notes)
- Schied et al. 2017 — SVGF (already have notes)
- Schied et al. 2018 — A-SVGF (already have notes)
- **Mara et al. 2017** — "An Efficient Denoising Algorithm for GI"
- **Heitz et al. 2018** — "Combining Analytic DI and Stochastic Shadows"
- **Lin et al. 2022** — ReSTIR PT / GRIS (the generalized ReSTIR framework)
- **Motion vectors / TAA implementation references** — our biggest blocker

Let me know which direction you want to go — more foundational background,
or focused on unblocking the GI implementation.
