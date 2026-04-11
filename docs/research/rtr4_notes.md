# Real-Time Rendering 4th Edition — Reading Roadmap

**Book**: "Real-Time Rendering, Fourth Edition" (2018)
**Authors**: Akenine-Möller, Haines, Hoffman, Pesce, Iwanicki, Hillaire
**Publisher**: CRC Press / Taylor & Francis
**PDF source**: realtimerendering.com (TOC/Preface/Intro/Bibliography only — 1052-page
full book not included)

This is **the** comprehensive real-time rendering reference. The freely-available
TOC PDF gives us the map of what the book covers — we use this to identify which
chapters to consult when working on specific rendering problems.

## Book Structure at a Glance

1052 pages, 24 chapters, organized in logical layers:
- **Foundations** (Ch 2–4): Pipeline, GPU, transforms
- **Basics** (Ch 5–8): Shading, texturing, shadows, color
- **Physical Realism** (Ch 9–11): PBR, local illumination, **global illumination**
- **Effects** (Ch 12–15): Image space, beyond polygons, volumetric, NPR
- **Geometry** (Ch 16–17): Polygonal techniques, curves/surfaces
- **Performance** (Ch 18–20): Pipeline optimization, acceleration, **efficient shading**
- **Specialized** (Ch 21–23): VR/AR, intersection, graphics hardware
- **Future** (Ch 24)

## Chapters Relevant to Our Current Lighting Problems

### Chapter 11 — Global Illumination (pp. 437-512) — **MOST RELEVANT**
Core chapter for our GI work. Sections:
- **11.1 The Rendering Equation** — the math we must respect
- **11.2 General Global Illumination** — overview of techniques
- **11.3 Ambient Occlusion** (pp. 446-465) — SSAO and variants we already have
- **11.4 Directional Occlusion** (pp. 465-472) — what we're missing
- **11.5 Diffuse Global Illumination** (pp. 472-497) — **exactly our problem**
- **11.6 Specular Global Illumination** (pp. 497-509)
- **11.7 Unified Approaches** (pp. 509-512)

**Action**: When implementing ReSTIR GI, cross-reference with 11.5. The chapter
predates ReSTIR GI (2018 book vs 2021 paper) but covers precursors: irradiance
probes, light propagation volumes, voxel cone tracing, path tracing.

### Chapter 7 — Shadows (pp. 223-266)
Our RT shadows work but could be improved. Sections:
- **7.4 Shadow Maps** — what most games use (we use RT instead)
- **7.5 Percentage-Closer Filtering** (PCF)
- **7.6 Percentage-Closer Soft Shadows** (PCSS) — concepts applicable to our stochastic soft shadows
- **7.7 Filtered Shadow Maps** — VSM, EVSM
- **7.8 Volumetric Shadow Techniques**

### Chapter 9 — Physically Based Shading (pp. 293-374) — **FOUNDATIONS**
Our PBR classifier is ad-hoc. This chapter has the rigorous math:
- **9.3 The BRDF** — the foundational equation
- **9.5 Fresnel Reflectance** — we implement Schlick approximation
- **9.7 Microfacet Theory** — our GGX BRDF is from here
- **9.8 BRDF Models for Surface Reflection**
- **9.9 Subsurface Scattering**
- **9.13 Blending and Filtering Materials** — relevant for our classifier

### Chapter 10 — Local Illumination (pp. 375-436)
- **10.1 Area Light Sources** (pp. 377-391) — our window lights should be area lights
  not point lights; this chapter has the proper math
- **10.2 Environment Lighting** — image-based lighting for sky/outdoor
- **10.5 Specular Image-Based Lighting** — reflection probes
- **10.6 Irradiance Environment Mapping** — diffuse IBL (spherical harmonics)

**Key insight for our window lights**: Section 10.1 would tell us how to
properly compute illumination from a polygonal area light (the window rectangle)
rather than approximating it as a point light.

### Chapter 20 — Efficient Shading (pp. 881-914) — **ALREADY USING**
Our clustered shading is from here:
- **20.1 Deferred Shading** — alternative to our forward path
- **20.3 Tiled Shading** — precursor to clustered
- **20.4 Clustered Shading** — **what we use**
- **20.5 Deferred Texturing** — interesting for visibility buffers

### Chapter 12 — Image-Space Effects (pp. 513-544)
- **12.1 Image Processing** — for post-effects
- **12.2 Reprojection Techniques** — **needed for ReSTIR temporal reuse**
  Our lack of motion vectors is why our temporal accumulation attempts stumbled.

### Chapter 14 — Volumetric and Translucency Rendering (pp. 589-650)
Not needed yet but relevant future work:
- **14.1 Light Scattering Theory**
- **14.4 Sky Rendering** — proper sky model would replace our hardcoded blue
- **14.5 Translucent Surfaces**

## Chapters Relevant to ECS/Engine-Level Concerns

### Chapter 2 — Graphics Rendering Pipeline (pp. 11-28)
Sanity check for pipeline architecture.

### Chapter 19 — Acceleration Algorithms (pp. 817-880)
- **19.1 Spatial Data Structures** — our BVH/TLAS
- **19.5 Portal Culling** — possibly useful for cell-based games
- **19.6 Detail and Small Triangle Culling**
- **19.9 Level of Detail** — we have none currently
- **19.10 Rendering Large Scenes** — exterior cells

### Chapter 23 — Graphics Hardware (pp. 993-1040)
- **23.11 Ray Tracing Architectures** — understanding what the hardware does
- **23.4 Memory Architecture and Buses** — bandwidth math
- **23.5 Caching and Compression**

## Chapters for Future Features

| Chapter | Topic | Future Use |
|---------|-------|-----------|
| 6.7-6.8 | Bump/Parallax mapping | Improve normal map handling |
| 8 | Light and Color | Tone mapping, exposure control |
| 13 | Beyond Polygons | Particle systems, sprites, voxels |
| 15 | Non-Photorealistic | Stylized rendering options |
| 17 | Curves and Curved Surfaces | Tessellation for LOD |
| 18 | Pipeline Optimization | Profiling workflow |
| 21 | VR/AR | If we ever support VR |
| 22 | Intersection Tests | Picking, collision |

## Bibliography Notes

The bibliography starts at p. 1051 — we can see entries like:
- [1] Aalto — "Experiments with DirectX Raytracing in Remedy's Northlight Engine"
- [47] Andersson & Barré-Brisebois — "Shiny Pixels and Beyond: Real-Time Raytracing at SEED" (2018)
- Bitterli et al. (ReSTIR original) is referenced (would be around page 150-200 of bib)

The authors explicitly say they maintain a reference list at
**realtimerendering.com** — that's where to go for the actual linked PDFs of
cited papers.

## How to Use This Book for Our Project

**When hitting a lighting problem**: Start with Chapter 11 (GI), cross-reference
with 9 (PBR) and 10 (Local Illumination). Most of our current GI struggles
would be addressed by the mental model in these chapters.

**When optimizing draw calls**: Chapters 18-20. We've already absorbed the
clustered shading content (Ch 20) — Ch 18 on pipeline optimization would
help us with profiling-driven work.

**When debugging Vulkan/GPU performance**: Chapter 23 for the hardware model,
Chapter 18 for the methodology.

**When we get to VR**: Chapter 21 exists as a complete guide.

## Reading Priority for Current Work

1. **Chapter 11.1 (Rendering Equation)** — foundation, ~10 pages
2. **Chapter 11.5 (Diffuse GI)** — our current focus, ~25 pages
3. **Chapter 10.1 (Area Lights)** — for fixing window lights properly, ~15 pages
4. **Chapter 12.2 (Reprojection)** — for temporal reuse, ~5 pages
5. **Chapter 9.3, 9.5, 9.7, 9.8 (BRDF/Microfacet)** — rigor for our PBR, ~30 pages

That's ~85 pages of targeted reading to have a solid foundation before
writing more GI code. Much more efficient than iterating on magic numbers.

## Note on Access

The free PDF only contains the TOC, preface, intro (Ch 1), bibliography, and
index — ~158 pages of metadata. The actual chapter content requires purchasing
the book or accessing it through a university/library. We treat this PDF as a
**roadmap to know which chapters to read** rather than the source itself.
