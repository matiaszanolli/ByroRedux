# Glass / Env-Mapped Alpha-Blend — Renderer Audit

Mental-model dump of the current GPU path for alpha-blended + env-mapped
(glass-like) draws. No fixes proposed — facts only.

All paths verified 2026-04-20 against `main`.

## 1. Pipeline selection for alpha-blend draws

`crates/renderer/src/vulkan/context/draw.rs:560-570` maps a DrawCommand to
`PipelineKey::Blended { src, dst, two_sided }`. The blend pipeline is
lazily created in `crates/renderer/src/vulkan/pipeline.rs:372-517`
(`create_blend_pipeline`) and cached per `(src, dst, two_sided)` tuple.

Per-pipeline static state (pipeline.rs:423-470):
- `cull_mode = two_sided ? NONE : BACK` — ONE draw, one cull mode.
- `depth_test_enable = true`, `depth_write_enable = false`,
  `depth_compare_op = LESS`.
- Only HDR attachment 0 blends; G-buffer attachments overwrite.
- `alpha` channel uses `ONE / ZERO` (src alpha writes raw).

Dynamic state reset per-batch (draw.rs:894-911, via VK_EXT extended dynamic
state, 1.3 core):
- `cmd_set_depth_test_enable` ← `batch.z_test`
- `cmd_set_depth_write_enable` ← `batch.z_write`  ← **see §3**
- `cmd_set_depth_compare_op`   ← `batch.z_function`

This means the static "depth_write=false" baked into the blend pipeline is
immediately overridden by the dynamic state emitted for the batch. The
effective z_write is whatever `DrawCommand.z_write` carries.

There is **no separate render pass or subpass for opaque vs blend**.
Opaque first / blend second is enforced purely by `draw_sort_key`
ordering (see §4). Both phases record into the same single subpass.

## 2. Shader — env map / Fresnel / RT / material_kind

File: `crates/renderer/shaders/triangle.frag`

- `envMapIndex` (offset 184) and `envMaskIndex` (offset 188) are declared
  on the `GpuInstance` struct (triangle.frag:77-78) but are **never
  sampled** in the fragment shader. `grep -n envMapIndex triangle.frag`
  returns only the struct declaration. Same story in `triangle.vert` and
  `ui.vert` / `caustic_splat.comp` — struct declared for layout parity,
  slots unused. The cube-map env texture the legacy BSShaderPPLighting
  path carries is never read.
- `materialKind` (offset 156) is declared (triangle.frag:66) but there
  is **no `if (inst.materialKind == X)` branch anywhere in the shader**.
  No Glass / SkinTint / HairTint / EyeEnvmap dispatch exists today.
- Fresnel: `fresnelSchlick(cosTheta, F0)` at triangle.frag:329-330 is
  used by the metal path (848-875), the glass path (736-741, 756-821),
  and the specular integrand (888-891, 993). `F0 = vec3(0.04)` for
  dielectrics, `mix(0.04, albedo, metalness)` for metals.
- Glass reflection path (triangle.frag:756-826) fires TWO ray queries:
  - Reflection ray along `reflect(-V, N)` via `traceReflection()`
    (triangle.frag:254-296, uses `gl_RayFlagsTerminateOnFirstHitEXT`,
    maxDist=3000).
  - Through-ray along `-V` (triangle.frag:777-783, same flags,
    maxDist=2000). On miss → hard-coded sky `vec3(0.6, 0.75, 1.0)`.
  - Mix via `fresnelScalar` (triangle.frag:816), returns early at 825.
- Detection gate (triangle.frag:664-671):
  ```
  isAlphaBlend = (inst.flags & 2u) != 0u;
  isWindow     = isAlphaBlend && texColor.a < 0.5 && texColor.a > 0.02;
  isGlass      = isAlphaBlend && metalness < 0.1 && texColor.a < 0.6 && texColor.a > 0.02;
  ```
  `isWindow` ⊂ `isGlass` for alpha in `(0.02, 0.5)`. `isWindow` branch
  runs first (triangle.frag:673-734) and returns early on portal-escape.
  When the ray hits interior, `hitsInterior=true`, the fragment falls
  through to `isGlass` which ALSO returns early (triangle.frag:825).
  Interior glass that fails the portal test therefore still goes through
  the RT-glass path, never reaching the default Lo+indirect composite.

## 3. Depth write / depth bias for alpha blend

`DrawCommand.z_write` is derived **only** from `NiZBufferProperty`:

- `crates/nif/src/import/material.rs:642-643`: `info.z_write = zbuf.z_write_enabled`.
- `byroredux/src/render.rs:428-430`: threaded straight into DrawCommand
  with default `(true, true, 3)`.

There is **no clamp to `!alpha_blend`**. Gamebryo's runtime
`NiAlphaProperty::Apply()` overrides `D3DRS_ZWRITEENABLE = FALSE` when
alpha_blend is on regardless of NiZBufferProperty authored state; we
don't replicate that rule. A glass mesh authored with
`NiZBufferProperty.z_write_enabled = true` (which is the Oblivion
default) + NiAlphaProperty.blend = true therefore gets:
- Static pipeline state says depth_write=false.
- Dynamic `cmd_set_depth_write_enable(true)` overrides it per-batch.

Net effect for interior Oblivion glass: **depth writes are ON for
alpha-blended draws**, which breaks back-to-front compositing and lets
z-fighting manifest between co-planar / near-planar glass faces.

Decal depth bias (draw.rs:884-890): `-4.0` slope bias when `is_decal`.
Not applied to glass.

## 4. Sort order for alpha-blend

`byroredux/src/render.rs:109-131` — `draw_sort_key`:

- alpha_blend branch returns tuple with slot 3 = `!cmd.sort_depth`
  (bitwise NOT of the sortable u32 encoding) — **back-to-front**.
- `sort_depth` is built from `clip.w` at render.rs:443-446:
  ```
  let pos = model_mat.col(3); // translation column (origin, world-space)
  let clip = vp_mat * pos;
  let sort_depth = f32_sortable_u32(clip.w);
  ```
  This is **camera-space linear depth of the mesh origin only**.
  The entire glass mesh shares one depth value regardless of its
  triangle-by-triangle extent. For face-to-face glass (bottle front vs
  back pane, a chandelier with glass bells inside glass globes), the
  draws collide on one sort key and order is stable-sort-unspecified
  within that run.
- Sort field order is `(alpha_flag, is_decal, two_sided, !depth,
  depth_state, texture, mesh)`. Alpha-blend draws therefore cluster by
  (decal, two_sided) FIRST, THEN sort back-to-front within the cluster.
  A single mesh with two_sided=true and two_sided=false sub-parts
  cannot interleave by depth — the two_sided pane is either wholly
  before or wholly after the non-two_sided pane.

## 5. Two-sided handling — ONE draw with CULL_NONE

`PipelineKey::Blended { two_sided: true }` produces a single pipeline
with `CullModeFlags::NONE` (pipeline.rs:423-427). The draw assembler
emits ONE `cmd_draw_indexed` for the mesh (draw.rs:937-944 indirect
path, identical for non-indirect).

There is **no two-draw fallback** that would render back faces first
(CULL_FRONT) then front faces (CULL_BACK) for proper back-to-front
ordering within one mesh. On a two-sided glass pane with z_write=true
(per §3), front + back triangles rasterize in index order and the first
to win the depth test blocks the other. Combined with the depth-write
bug, this is the textbook recipe for cross-hatch moiré on glass that
covers both faces in one mesh.

## 6. What's available that we're not using

- **TLAS + ray queries are already live** for shadows (triangle.frag:
  ~1089), GI (~1137), glass reflection (766), glass through-ray (777),
  window portal (694). The RT-glass code path exists and runs today.
- The per-pixel cost of the existing glass path on a 4070 Ti is
  effectively free vs. the raster cost of the whole scene; adding one
  more refraction ray per transparent fragment is well within budget.
- A **true refraction ray** with `refract(V, N, ior)` would replace the
  current `-V` through-ray (line 781), which is straight-through (no
  eta). That's the fix for "behind the glass" sampling, not the moiré.
- Descriptor sets: env cubemap would need a new bindless slot or a
  dedicated combined-image-sampler binding — `envMapIndex` already
  exists in the GpuInstance struct (unused), so no layout change needed
  if we read it.
- The real cause of cross-hatch moiré on glass is NOT missing shader
  logic — it's §3 (wrong z_write) + §5 (no front/back split).

## 7. MSAA / TAA interaction

- **MSAA is off everywhere** — `rasterization_samples(TYPE_1)` on every
  pipeline (pipeline.rs:241, 439, 589; plus all compute outputs).
- **TAA is on** when `TaaPipeline::new()` succeeds (context/mod.rs:794-
  806). Halton(2,3) sub-pixel jitter in NDC is baked into the camera
  UBO at draw.rs:310-320. The jitter shifts the PROJECTION for both
  opaque AND blend draws — glass fragments move ±0.5 px per frame in
  sync with the world.
- TAA resolve (draw.rs:1058-1062) runs on the HDR attachment AFTER the
  whole render pass ends. It reads `prev_view_proj` motion vectors +
  does a YCoCg neighborhood clamp. It does **not** treat alpha-blend
  layers separately — it sees the final composited HDR.
- Motion vectors for glass come from the SAME motion attachment the
  opaque geometry writes to. If glass writes depth (§3) it ALSO owns
  the motion vector for that pixel on the frame it rasterizes, but
  the opaque surface behind the glass wrote its own motion vector
  earlier in the same pass; whichever won the depth test owns the
  motion. With z_write=true + two-sided single-draw, that winner
  flips between glass-front and glass-back across frames based on
  triangle-draw order → **history reproject picks the wrong source
  pixel on alternating frames**.

This is the strongest candidate for "cross-hatch moiré that appears
to move" — TAA history thrashing driven by z-fighting between two
co-planar glass faces that both think they own the pixel.

## File pointers

- `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/context/draw.rs` (pipeline select + dynamic depth)
- `/mnt/data/src/gamebyro-redux/crates/renderer/src/vulkan/pipeline.rs` (blend pipeline creation)
- `/mnt/data/src/gamebyro-redux/crates/renderer/shaders/triangle.frag` (glass shading)
- `/mnt/data/src/gamebyro-redux/byroredux/src/render.rs` (draw_sort_key, DrawCommand assembly)
- `/mnt/data/src/gamebyro-redux/crates/nif/src/import/material.rs` (z_write extraction from NiZBufferProperty)
