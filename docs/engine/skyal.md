# SKYAL — Sky Abstraction Layer

**Status: PARTIAL (2026-09-13).** The overdrive fix, the shared sky
include, and the sky cubemap — baked every frame and consumed by the
ray-traced miss and bounded-path-escape paths — have landed. Still
specified-but-unbuilt: prefiltered mips for rough reflections, the
irradiance projection for diffuse ambient, and the volumetric cloud
layer. See §3 for exactly which steps are done.

Sibling of [NIFAL](nifal.md), [EXAL](exal.md), [PHYSAL](physal.md),
[WATAL](watal.md), [CHARAL](charal.md). SKYAL sits **downstream of EXAL**:
EXAL already translates per-game WTHR/CLMT into the canonical
`SkyParamsRes`, so everything here is game-agnostic by construction. Do
not add a per-game branch to any of it — that is what the EXAL boundary
is for (see `feedback_format_translation`).

---

## 1. Why this layer exists

### 1.1 The measured defect

Skyrim SE, Tamriel 5,-24, weather `SkyrimCloudy`, deterministic capture
(`--camera-pos 5000,10500,-49000 --camera-forward 0.2,0.15,-1.0
--bench-frames 180 --screenshot`), reproducible run-to-run to a mean of
0.078/255.

| Quantity | Authored (WTHR) | Rendered (pre-fix) |
|---|---|---|
| zenith RGB | `0.161, 0.380, 0.459` | `0.572, 0.822, 0.914` |
| zenith R/B (hue) | 0.35 | 0.81 |
| zenith→horizon gradient | large | ~4% |

The sky was a flat near-white wash. Decomposition through the
`composite_term` debug view (pre-bloom, pre-tonemap, linear) isolated it:

* pre-bloom `(0.335, 0.477, 0.539)` — **matches the analytic prediction**
  `mix(horizon, zenith, sqrt(sin(elev)))` for that view direction, i.e.
  the sky assembly was correct;
* post-bloom `(0.572, 0.822, 0.914)` — a flat **1.71×** on all three
  channels.

Bloom had no bright-pass anywhere in its chain. Over a large uniform
region a bloom pyramid's "local blurred average" converges to that
region's own value, so `scene += bloom * BLOOM_INTENSITY` degenerates
from a highlight glow into a **pure gain** — and the sky is the largest
uniform bright region an exterior frame has. ACES then compressed the
result into its shoulder, which is what destroyed the gradient and the
hue. The sky was not too bright; it was being multiplied into the part of
the curve that cannot represent it.

**Fixed** (`62a09fd9`) by a Karis/Jimenez soft knee at the pyramid seed.
Same capture after: `(0.424, 0.550, 0.591)`, a 1.01× bloom gain. See
`BLOOM_THRESHOLD` in `shader_constants_data.rs` for the constants and the
deliberate emissive trade-off.

### 1.2 What is still wrong

Fixing the gain does not make the sky good, because the sky *model* is a
two-colour vertical lerp:

* **No atmosphere.** `mix(horizon, zenith, sqrt(t))` cannot produce a
  real sky. There is no Rayleigh/Mie separation, so no sun-relative
  azimuthal variation at all: the sky is identical 180° away from the sun
  as toward it.
* **Clouds are UV-projected planes.** Up to four WTHR cloud textures are
  projected onto an infinite plane (`dir.xz / dir.y`). Near the horizon
  this stretches into visible streaks and needs a `smoothstep(0, 0.12,
  elevation)` fade to hide the singularity. Clouds have no depth, no
  self-shadowing, and no parallax.
* **Reflections and GI never see the sky.** `raytrace.glsl`'s miss
  returns `skyTint * 0.5 + sceneFlags.yzw * 0.5` — a single flat colour.
  Every sky reflection and every sky-lit indirect bounce in the engine is
  currently a uniform blob, regardless of where the sun is or what the
  clouds are doing.

---

## 2. Architecture

```
EXAL (per-game WTHR/CLMT)  →  SkyParamsRes  →  SkyDome  →  sky_radiance()
                                                              │
                                            ┌─────────────────┴──────────────┐
                                            │                                │
                                   background pixel                   sky cubemap bake
                                   (composite.frag)                   (sky_cube.comp)
                                                                             │
                                                    ┌────────────────────────┼─────────────┐
                                                    │                        │             │
                                              RT miss / GI          rough reflections   ambient
                                             (triangle.frag)         (prefiltered mips)   (SH)
```

### 2.1 Shared sky — LANDED (`b7abdaa5`)

`shaders/include/sky.glsl` holds the one implementation, parameterised by
a `SkyDome` value rather than a uniform block, because the consumers do
not share a descriptor set. Verified behaviour-identical to the inlined
original: mean 0.0122/255 final, 0.0262 pre-bloom, against a 0.0777
noise floor.

`sky_dome.rs` guards the seam's one silent failure mode: GLSL neither
zero-initialises a local struct nor warns about a field a builder forgot,
so a missed field is undefined data, not a diagnostic.

### 2.2 Sky cubemap — NOT YET IMPLEMENTED

**Why a cubemap rather than calling `sky_radiance` from the RT path.**
Three independent reasons, in increasing order of weight:

1. Cost: one evaluation per texel (6 × 128² ≈ 98k) instead of per ray.
2. Prefiltering: mip level selects roughness for glossy reflections;
   there is no other way to get that from an analytic sky.
3. **It is what makes volumetric clouds affordable at all.** A raymarched
   cloud layer cannot be evaluated per RT ray or per background pixel.
   Baked once per frame into a cubemap, it can.

There is also a hard plumbing reason: `triangle.frag` binds the bindless
arrays at **set 0**, `composite.frag` at **set 1**. `include/sky.glsl`
declares `textures[]` at set 1, so it cannot simply be included into
`triangle.frag`. A cubemap is one `samplerCube` sample with no sky code
and no set conflict.

**Shape.**

* `R16G16B16A16_SFLOAT` cube, 6 faces, 128² (tunable), `CUBE_COMPATIBLE`,
  with a mip chain for roughness prefiltering.
* Two views: `TYPE_2D_ARRAY` (storage, for the bake) and `CUBE`
  (sampled). `GpuImageDesc` needs a `flags` field and a `color_cube`
  constructor — extend it, do not add a second image path.
* `sky_cube.comp`: dispatch `(N/8, N/8, 6)`, one invocation per texel,
  `imageStore(skyFaces, ivec3(x, y, face), vec4(sky_radiance(dome, dir), 1))`.
* Bound at **scene set 1 / binding 20**, not into the `cubemaps[]`
  bindless array. The bindless route was rejected: `TextureRegistry`
  entries own their `Texture`, so an externally-owned per-frame view would
  need a new ownership kind threaded through the destroy path *and* would
  have to respect the fence-gated descriptor-write protocol (#92 / #2715).
  Binding 20 follows the `aoTexture` / `depthHistoryTex` precedent — a
  renderer-owned per-frame image written into the scene set once at init —
  which is simpler and has no ownership ambiguity.
* `GpuCamera::exterior_sky_tint`'s previously-reserved `w` lane carries a
  **ready flag**. The bake is optional (VRAM pressure) and binding 20 is
  PARTIALLY_BOUND, so consumers must gate on the flag rather than on the
  binding existing.
* The bake needs **two** descriptor sets: its own, plus the bindless array
  at set 1, because `include/sky.glsl` samples the WTHR cloud layers and
  the CLMT sun sprite by index. The bindless layout's `stageFlags` also
  had to gain `COMPUTE`. Both are accepted by the shader compiler and
  rejected at pipeline creation
  (VUID-VkComputePipelineCreateInfo-layout-07988) — validation-layer-only
  failures, invisible to `cargo test`. Both were hit for real.

**Face mapping** — derived from the Vulkan spec §16.5.4 cube-map face
selection table by inverting it (`|ma| = 1`, `u = 2s-1 = sc`,
`v = 2t-1 = tc`), not guessed:

| face | major axis | direction from (u, v) |
|---|---|---|
| 0 | +X | `( 1, -v, -u)` |
| 1 | -X | `(-1, -v,  u)` |
| 2 | +Y | `( u,  1,  v)` |
| 3 | -Y | `( u, -1, -v)` |
| 4 | +Z | `( u, -v,  1)` |
| 5 | -Z | `(-u, -v, -1)` |

**Consumers, in the order they should be wired:**

1. `raytrace.glsl` miss — replaces the flat `skyTint * 0.5 + ambient *
   0.5`. Largest visible win; reflections and sky GI become real.
2. `lighting.glsl`'s bounded-path escape — same substitution.
3. Rough reflections via prefiltered mips.
4. Diffuse ambient via an irradiance projection, replacing the
   hand-rolled `sceneFlags.yzw` term.

The **background stays analytic**. A 128² face cannot represent a sharp
sun disc, and the background pixel can afford the full evaluation. Once
volumetric clouds exist, the background samples the cubemap for the cloud
layer and keeps the disc analytic on top.

### 2.3 Volumetric clouds — NOT YET IMPLEMENTED

Replaces the UV-projected cloud planes. Marched into the cubemap
(§2.2), never per-pixel.

Open research questions — **resolve against a citable reference before
implementing, per the no-guessing policy**:

* Density field. Perlin–Worley, Schneider & Vos, "The Real-Time
  Volumetric Cloudscapes of Horizon: Zero Dawn", SIGGRAPH 2015. Needs a
  3D noise texture generated at init (or baked to disk).
* Phase function. Henyey–Greenstein, or the dual-lobe HG that talk uses.
* Scattering. Beer–Powder from the same source.
* Coverage/type driven from WTHR — the canonical mapping from
  `WTHR` classification flags (`WTHR_CLOUDY`/`RAINY`/`SNOW`) and the
  per-layer cloud alphas to coverage/density is **undetermined** and must
  be measured against real data, the way the WATR `DATA` offsets were.

The four authored WTHR cloud-texture layers do not disappear: they remain
the per-game authored signal EXAL hands over. Whether they drive the
volumetric coverage field or continue to render as a distinct high-cirrus
deck above the volumetric layer is an open design question.

---

## 3. Sequencing

| Step | State |
|---|---|
| Bloom bright-pass (the overdrive fix) | **DONE** `62a09fd9` |
| Shared `include/sky.glsl` + `SkyDome` guard | **DONE** `b7abdaa5` |
| `GpuImageDesc` cube support | **DONE** |
| Cubemap resource + `sky_cube.comp` bake | **DONE** |
| Construct + dispatch in `VulkanContext` / `draw_frame` | **DONE** — baked every frame before the geometry pass |
| Bind into scene set 1 / binding 20 + ready flag | **DONE** — a dedicated binding rather than the bindless array; see below |
| RT miss + bounded-path escape consume it | **DONE** |
| Prefiltered mips for rough reflections | TODO |
| Irradiance projection for ambient | TODO |
| Volumetric cloud march into the cube | TODO |
| WTHR → coverage/density mapping (needs data) | TODO |

---

## 4. Harness

The measurements above are reproducible, and any change to this layer
should be checked the same way — `cargo test` cannot see any of it.

```bash
./target/release/byroredux --game skyrim_se --new-game \
  --camera-pos 5000,10500,-49000 --camera-forward 0.2,0.15,-1.0 \
  --bench-frames 180 --screenshot out.png
```

Framing is deliberate: above the terrain, so the frame is pure sky with
the below-horizon `SKY_LOWER` band visible for reference. Add
`--render-debug-mode composite_term` for the pre-bloom, pre-tonemap,
linear scene — that view is what separates a sky-assembly change from a
tone-mapping or bloom change.

Two traps, both hit while producing the numbers above:

* **Screenshots are sRGB-encoded.** The swapchain is `B8G8R8A8_SRGB`, so
  PNG values must be sRGB-decoded before being compared against authored
  linear colours.
* **A recompiled `.spv` does not reliably trigger a `cargo` rebuild.**
  `include_bytes!` dependency tracking did not pick up the regenerated
  SPIR-V, so captures silently ran a stale binary and produced a
  ~6.9/255 mean "regression" that did not exist. Always
  `touch crates/renderer/src/lib.rs` after `glslangValidator` and before
  `cargo build`, and establish the run-to-run noise floor (two captures
  of the same binary) before trusting any delta.
