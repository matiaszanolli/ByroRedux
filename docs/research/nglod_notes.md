# Neural Geometric Level of Detail (NGLOD)

**Paper**: "Neural Geometric Level of Detail: Real-time Rendering with Implicit 3D Shapes"
**Authors**: Takikawa, Litalien, Yin, Kreis, Loop, Nowrouzezahrai, Jacobson,
McGuire, Fidler (NVIDIA + Toronto + McGill + Vector Institute)
**Venue**: CVPR 2021
**PDF**: `nglod.pdf` (gitignored)
**Project**: nv-tlabs.github.io/nglod

**⚠ Relevance note**: This paper is **not directly relevant** to our current
lighting/GI problem. NGLOD is about neural representations for 3D geometry
(implicit surfaces via SDFs), not about lighting or shading. Including this
note for completeness and for possible future use in LOD systems.

## What It Is

NGLOD encodes a 3D shape as a neural signed distance function (SDF) using:
- A **sparse voxel octree (SVO)** spanning the shape's bounding box
- **Feature vectors** (32-dimensional) stored at each voxel corner
- A **small MLP** (1 hidden layer, 128 units) that decodes interpolated
  features into signed distance

The SVO provides **multi-level LOD naturally**: deeper levels = finer detail.
Rendering uses sphere tracing with adaptive ray stepping through the octree.

## Core Results

- **100× faster than DeepSDF** at equal quality
- **500× faster than NeRF**, 50× faster than NSVF
- ~90 ms/frame at 1080p (2020 hardware) for complex models
- Better geometric reconstruction than prior neural SDF methods
- **Smooth LOD interpolation** — distances at adjacent LODs can be blended
  linearly (works for SDFs, doesn't work for meshes/point clouds)
- **Decoder weights generalize** across shapes — only the SVO feature
  volume needs to be swapped per model

### Memory (from Figure 1)
- LOD 2: ~8 KB per shape
- LOD 3: ~19 KB
- LOD 4: ~56 KB
- LOD 5: ~210 KB
- LOD 6: ~900 KB

Compare to a typical mesh at similar quality: 1-10 MB.

## Technical Approach

### Architecture (Figure 3)

```
Query point x → traverse SVO to find voxels at LOD L
              → trilinearly interpolate corner features at each level
              → sum features across levels (L1 + L2 + ... + LL)
              → concatenate with x → feed to MLP fθ_L
              → output signed distance d̂
```

Each LOD level has its own MLP decoder θ_L. The summation across levels
means gradients flow through all levels during training, giving every
LOD a meaningful contribution.

### Training
Joint loss across all LODs:
```
J = Σ_L E[(fθ_L(x, z_L) - d_ground_truth)²]
```

Optimized jointly over MLP parameters θ and feature volume Z.
Sample points drawn from uniform, surface, and near-surface distributions.

### LOD Blending
Smooth transitions:
```
d̂_L̃ = (1 - α) · d̂_L + α · d̂_{L+1}
```
where α is the fractional part of the desired continuous LOD. Only works
for SDFs because distances interpolate linearly; meshes can't do this.

### Sparse Ray-Octree Traversal (Algorithm 1)

The clever rendering trick. Breadth-first parallel traversal:
1. Start with all rays paired with the root voxel
2. Per level: check which ray-voxel pairs intersect (`DECIDE` kernel)
3. Use parallel exclusive sum to compact and subdivide lists
4. At the target LOD, you have a depth-ordered list of intersected voxels
5. Sphere trace within voxels; skip empty space between voxels via AABB

This avoids scanning all voxels — only processes ray-voxel pairs that
could possibly intersect, level by level.

## Why This Could Be Interesting for ByroRedux (Eventually)

### Not Current Priorities
This does **not** fix our current lighting/GI problems. Our immediate
needs are:
1. Motion vectors + G-buffer
2. SVGF denoising
3. ReSTIR GI
4. Temporal accumulation

NGLOD is orthogonal to all of the above.

### Potential Future Applications

**1. Mesh LOD for cell-loaded content**
Bethesda cells load hundreds of meshes. Distant meshes currently render at
full detail. A neural SDF representation with LOD could compress distant
meshes dramatically and reduce vertex count (~1000× for distant objects).

**But**: requires per-mesh neural training (offline), we don't have a GPU
compute pipeline for it, and it doesn't interact with our existing
vertex/index buffer + BLAS pipeline.

**2. Compressed static geometry**
Large static meshes (terrain, architecture) could be represented as
neural SDFs instead of vertex data. 1-10 MB mesh → ~900 KB neural SDF
at comparable quality.

**But**: we'd need an SDF ray-tracing path alongside our triangle path,
which is a significant renderer extension.

**3. Seamless LOD for open-world exteriors**
Traditional mesh LOD requires popping between discrete levels or expensive
geomorphing. NGLOD's linear distance interpolation gives free smooth LOD.

**But**: we're not targeting open-world exteriors yet.

### Why It's Probably NOT Worth It for Us

1. **Training overhead**: each mesh needs ~100 epochs of GPU training
   (minutes per shape). We have thousands of meshes.

2. **Ray tracing pipeline change**: neural SDFs use sphere tracing, not
   triangle intersection. Our TLAS/BLAS infrastructure wouldn't apply.
   We'd need a second rendering path.

3. **Quality vs effort**: modern mesh LOD (Nanite-style) gives similar
   results without neural networks. Not clearly a win.

4. **The paper is from 2021**; there's been significant progress since
   (Instant NGP, 3D Gaussian Splatting, etc.). If we ever want neural
   representations, we'd want to look at newer work.

## What We Can Learn From It

Even if we never implement this, the paper has some useful concepts:

1. **Sparse voxel octrees for ray queries**: the traversal algorithm
   (Algorithm 1) is a good parallel pattern for any octree-based
   data structure on GPU.

2. **Parallel scan kernels (EXCLUSIVE_SUM, DECIDE, SUBDIVIDE)**: general
   primitives for GPU tree operations. We may need similar patterns if
   we ever build spatial acceleration structures on-GPU.

3. **Feature vectors + small MLP pattern**: a general approach for
   neural representations where you want both compression and quality.
   Store learned features in a spatial structure, decode with a shallow
   network. This pattern shows up in Instant NGP, neural radiance caches,
   and many others.

## Honest Assessment

This paper was shared but is tangential to our current thread. Our
immediate rendering problems are **all** about lighting, not geometry:
- Noisy GI (needs SVGF/denoising)
- No temporal reuse (needs motion vectors + G-buffer)
- Poor direct lighting sampling (needs ReSTIR DI)
- Missing area lights (needs proper polygon light math)

If the research goal is still "fix the GI quality," this paper doesn't
contribute. If the goal is shifting toward **geometry compression and
LOD** (a different problem entirely), then NGLOD is a reasonable starting
point — but also worth noting it's 4 years old and the field has moved
on significantly.

**Recommendation**: Park this for now. If we ever do tackle LOD, revisit
along with Nanite-style virtual geometry and Instant NGP as alternatives.
For now, continue the lighting research thread.
