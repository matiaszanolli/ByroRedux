# Gamebryo 2.3 — Glass / Alpha-Blend Path Investigation

Ground-truth notes extracted from the Gamebryo 2.3 source tree at
`/home/matias/Downloads/Gamebryo_2.3 SRC/Gamebryo_2.3/` and from vanilla
Oblivion meshes under `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

## 1. Alpha accumulator — sort granularity

Source: `CoreLibs/NiMain/NiAlphaAccumulator.cpp` (lines 31–107)

- `RegisterObjectArray` walks `NiVisibleArray` of `NiGeometry*`.
- Any geometry whose `NiAlphaProperty::GetAlphaBlending()` is true AND that
  does not have the no-sort hint set AND whose `GetSortObject()` is true is
  added to `m_kItems` (a linked list).
- Everything else calls `kObject.RenderImmediate(pkRenderer)` straight away.
- `Sort()` computes ONE depth per geometry: `m_pfDepths[i] =
  m_ppkItems[i]->GetWorldBound().GetCenter() * kViewDir` (signed projection of
  the world-bound sphere centre onto the camera world direction). The
  `m_bSortByClosestPoint` variant subtracts the bound radius.
- Sort is strictly per-object (the `NiGeometry` node), never per-triangle.

Implication for Redux: collecting transparent draw calls into a list and
sorting by `worldBound.center · camForward` is the correct, byte-compatible
behaviour. No BSP, no per-triangle split.

## 2. Z-write vs NiAlphaProperty

Source: `CoreLibs/NiDX9Renderer/NiDX9RenderState.cpp`

`ApplyAlpha(const NiAlphaProperty* pkNew)` (lines 411–438) sets exactly four
D3D render states: `D3DRS_ALPHABLENDENABLE`, `D3DRS_SRCBLEND`,
`D3DRS_DESTBLEND`, `D3DRS_ALPHATESTENABLE` (+ `D3DRS_ALPHAFUNC`/`ALPHAREF`
when test is on). It does **not** touch `D3DRS_ZWRITEENABLE`.

`ApplyZBuffer(const NiZBufferProperty* pkNew)` (lines 579–601) is the only
writer of Z state:
```
SetRenderState(D3DRS_ZWRITEENABLE, pkNew->GetZBufferWrite());
```
So Z-write is governed entirely by `NiZBufferProperty`, independent of
whether alpha blending is enabled. Authors flip the `ZBufferWrite` flag on
`NiZBufferProperty` to make glass translucent without depth occlusion — the
renderer never infers it from the alpha property.

## 3. NiStencilProperty cull-mode — one draw, no double-draw

Source: `CoreLibs/NiDX9Renderer/NiDX9RenderState.cpp` lines 344–353, 547–571.

The mapping table is:
```
DRAW_CCW_OR_BOTH (0)  RH -> D3DCULL_CW   LH -> D3DCULL_CCW
DRAW_CCW         (1)  RH -> D3DCULL_CW   LH -> D3DCULL_CCW
DRAW_CW          (2)  RH -> D3DCULL_CCW  LH -> D3DCULL_CW
DRAW_BOTH        (3)  RH -> D3DCULL_NONE LH -> D3DCULL_NONE
```
`ApplyStencil` does exactly one `SetRenderState(D3DRS_CULLMODE, …)` for
whichever mode the property declares, then the geometry is submitted once.
There is **no two-pass back-then-front emulation path** — `DRAW_BOTH` just
disables culling and draws once with `CULL_NONE`.

Redux implication: when `draw_mode == 3`, we need a single pipeline with
`VK_CULL_MODE_NONE`. We currently do not, and should not, issue two draw
calls. For `draw_mode == 0` or `1`, back-face culling is the default.

## 4. Vanilla Oblivion glass mesh — concrete decode

Mesh: `meshes\clutter\magesguild\crystalball01.nif` extracted from
`Oblivion - Meshes.bsa` (31 364 bytes, NIF version 20.0.0.4, user_version 11,
bsver 11, 19 blocks). The transparent glass sphere is the second TriStrips
geometry, hanging off `NiNode "CrystalBall01"`.

Block layout for the glass sub-mesh:

```
[11] NiTriStrips         "CrystalBall01:6"         (shape node)
[12] NiMaterialProperty  "EnvMap2"
[13] NiVertexColorProperty
[14] NiSpecularProperty
[15] NiAlphaProperty
[16] NiTexturingProperty
[17] NiSourceTexture     "textures\clutter\upperclass\GlassPain.dds"
[18] NiTriStripsData
```

Decoded field values (raw bytes verified against offsets):

```
NiMaterialProperty "EnvMap2"
  ambient    = (0.7882, 0.8000, 0.9686)
  diffuse    = (0.4235, 0.4353, 0.6745)   # darker than ambient — stylised tint
  specular   = (0.8667, 0.8667, 0.8667)
  emissive   = (0.0000, 0.0000, 0.0000)
  glossiness = 10.0
  alpha      = 1.0                          # material alpha is full opaque

NiAlphaProperty
  flags  = 0x00ED
    bit 0  : blend_enable = 1
    bits 1-4 : src_blend  = 6 (SRC_ALPHA)
    bits 5-8 : dst_blend  = 7 (INV_SRC_ALPHA)
    bit 9  : test_enable  = 0
    bits 10-12 : test_mode = 0 (ALWAYS) — irrelevant, disabled
    bit 13 : no_sort      = 0            # DO sort
  threshold = 0

NiTexturingProperty
  apply_mode = 3 (MODULATE)
  num_tex    = 7
  slot[0] base : NiSourceTexture -> "textures\clutter\upperclass\GlassPain.dds"
                 clamp=3 (WRAP_S_WRAP_T), filter=2 (TRILERP), uvset=0
  slot[1..6] : all absent (has_tex=0)
```

What is NOT present on this glass material:
- no `NiStencilProperty` — so draw_mode defaults to `DRAW_CCW_OR_BOTH`,
  which renders as CULL_CW (back-face cull, single-sided) per §3.
- no `BSShaderPPLightingProperty` — this is a pure NiTexturingProperty
  material. Oblivion ships both paths; the ancient/clutter `crystalball01`
  uses the legacy fixed-function-shader material, not PP lighting.
- no `NiZBufferProperty` — so the state comes from whatever was inherited
  along the property stack (default is write=true, test=LESS). The
  transparency is carried entirely by the DDS alpha channel.

Authoring convention summary for Oblivion "pure" glass:
- material alpha 1.0; all alpha comes from the DDS texture
- NiAlphaProperty 0x00ED (classic SRC_ALPHA / INV_SRC_ALPHA, sorted, no test)
- single-sided (no stencil property)
- normal MODULATE texture application; no env/gloss/glow slots
- same shape mesh is also the visual bound that feeds the sort depth
  computation in §1

## 5. Files referenced

- `/home/matias/Downloads/Gamebryo_2.3 SRC/Gamebryo_2.3/CoreLibs/NiMain/NiAlphaAccumulator.cpp`
- `/home/matias/Downloads/Gamebryo_2.3 SRC/Gamebryo_2.3/CoreLibs/NiMain/NiBackToFrontAccumulator.cpp`
- `/home/matias/Downloads/Gamebryo_2.3 SRC/Gamebryo_2.3/CoreLibs/NiDX9Renderer/NiDX9RenderState.cpp`
  (`ApplyAlpha` L411, `ApplyStencil` L547, `ApplyZBuffer` L579, cull mapping L344)
- Extracted mesh: `/tmp/crystalball01.nif` (from `Oblivion - Meshes.bsa`)
- Tracer: `/mnt/data/src/gamebyro-redux/crates/nif/examples/trace_block.rs`
- Ad-hoc single-file extractor: `/mnt/data/src/gamebyro-redux/crates/bsa/examples/bsa_extract_one.rs`
