# Starfield material maps — Phase 0 record (2026-11-26)

For [#7420](https://github.com/matiaszanolli/ByroRedux/issues/7420),
[PRD](../prd/starfield-material-maps.md) §9 Phase 0. Engine base: `9fd45cc65`.
The renderer contract is in [NIFAL](../engine/nifal.md) ("Starfield
material-map role contract").

## Sources, and how far they go

| Source | Pin |
|---|---|
| **Viewer**: [fo76utils/nifskope](https://github.com/fo76utils/nifskope/tree/3797c8939058f34b9b3a2fea6e2259f0e4fbfa48): `res/shaders/stf_default.frag`, `src/gl/renderer.cpp`, `src/gl/glproperty.cpp`, `src/gl/gltexloaders.cpp`, `lib/libfo76utils/src/` | commit `3797c893` |
| **xEdit**: `Core/wbDefinitionsSF1.pas`, branch `dev-4.1.6` of TES5Edit/TES5Edit | commit `7aeb22b6` |
| Installed Starfield archives, through the census below | — |

The viewer is a third-party preview tool, not Bethesda documentation. Its
shader and its database reader are one project, so they are **one** source,
not two. It draws no shadows and no screen-space occlusion. "Verified" below
means "this source says so, and the installed data does not contradict it".

## Verify items

| Item | Result | What the source says | Evidence |
|---|---|---|---|
| SMM-SRC-003 channel and encoding | Verified | All four maps are read from the red channel, in the DDS file's own colour space. Of the chosen maps found in the archives, nearly all are `BC4_UNORM` (single channel, linear), a few `R8_UNORM`. The other exceptions are BC1, nearly all `BC1_UNORM_SRGB`, many of them colour textures reused as data maps; those decode as sRGB | `stf_default.frag:418-424`, `:505-540`; `gltexloaders.cpp` loads the DDS format as stored; format table below |
| SMM-SRC-004 replacement value | Verified | A replacement is a float colour clamped to 0–1. Slots 2–5 read its red value as linear. Every replacement on those slots in the census is grey, so the choice of channel changes nothing | `renderer.cpp:463`, `:489` (mode 1); `glproperty.cpp:898-906`; census |
| SMM-SRC-005 texture and replacement together | Verified | The texture wins. The replacement is used when the path is empty or the texture does not bind | `glproperty.cpp:909-950` |
| SMM-SRC-007 empty slot | Verified; **no change** | Roughness 0, metalness 0, occlusion 1, opacity 1. The requirement keeps today's fallback unless the owner adopts another. Roughness 0 would make every such surface a mirror | `stf_default.frag:380`, `:452`; `material.cpp:9` |
| SMM-SRC-009 `TXST` slots | Verified | TX08 metal, TX09 rough, TX17 AO, TX19 opacity, the numbers NIFAL already records | `wbDefinitionsSF1.pas`, `RegisterRecordDef(TXST, …)` |
| SMM-OPA-004 cutouts and threshold | Verified | A non-effect material is alpha-tested (`alpha > threshold`) when `HasOpacity` is true and the threshold is above zero. The reader starts the threshold at one third. So materials with no threshold on their chain are cutouts at one third (the engine does not test them today); an authored zero means no test | `renderer.cpp:594`; `material.cpp:31`; `stf_default.frag:600` |
| SMM-OPA-005 layer number | Verified | `MATERIAL_LAYER_n` is the layer whose `LayerID` component index is `n`. In the data, 7 materials with `HasOpacity` name layer 1. 5 have that layer; in 2 of them only layer 1 has an opacity texture. The other 2 have no layer 1 and stay Absent | `matcomps.cpp:337`, `:1234`; census |
| SMM-OPA-006 slot 2 without the flag | Verified; **conflicts for effect materials** | Non-effect materials ignore slot 2 without the flag, as the requirement says. Effect materials (effect settings and a route other than `Deferred`) use slot 2 whatever the flag says. No census material has both the flag and the effect route | `renderer.cpp:216`; `stf_default.frag:507-540` |
| PRD §6 row 2, original occlusion use | **Not settled**, filed as [#7424](https://github.com/matiaszanolli/ByroRedux/issues/7424) | No Creation Kit documentation was found (one web search; the local wiki dumps have no Starfield wiki). The viewer multiplies occlusion into direct and ambient diffuse and derives a specular occlusion for all specular. As a preview with no shadows, it does not show what the game does | `stf_default.frag:665-672` |

One more difference to record in PRD §6: when a texture does not bind, the
viewer falls back to the replacement. SMM-SRC-011 shades it as Absent.

## Lighting terms occlusion may darken (SMM-AO-001)

All in `crates/renderer/shaders/triangle.frag`, at `9fd45cc65`. The PRD names
three; **there are four**.

| # | Term | Lines | SSAO today |
|---|---|---|---|
| 1 | Untraced share of exterior sky diffuse | 5153-5154 | multiplied |
| 2 | Skyrim directional ambient cube | 5162-5166 | multiplied |
| 3 | Flat ambient (cell ambient and metal fill) | 2971-2995, 5168-5170 | multiplied, floor `AMBIENT_AO_FLOOR` |
| 4 | **Probe share of environment reflections** (missing from the PRD) | 3216-3345 | inside the specular-occlusion formula, SSAO floored at 0.2. On exteriors within reflection reach a traced ray replaces it, so occlusion must not apply there |

The owner agreed on 2026-11-26 to add term 4 and to leave out the next
group. Ambient additions that have neither SSAO nor a ray today: the effect-lit cell
ambient (near line 1243), the face-on glass reflection colour (near 2346) and
the glass rim ambient (near 5223). They are special paths, not surface
lighting. Excluded because a ray resolved them: the GI bounce, the traced
share of reflections, refraction-hit ambient, and sky colours used after a
ray misses or escapes.

For Phase 3: `ao` is already floored at 0.2 where it is sampled (line 5087),
so the `DBG_VIZ_AO` view shows the floored value, although its comment says
otherwise.

## Owner decisions (2026-11-26)

- SMM-OPA-006 stands. Effect materials stay without opacity, recorded as a
  deviation (PRD §6).
- SMM-SRC-011 stands against the viewer's fallback to the replacement.
- Occlusion covers the four terms above and not the three special paths.

## Engine checks (PRD §7.2)

Release build of `9fd45cc65` plus the uncommitted census view (no engine
behaviour changed), under Xvfb, one engine at a time, with a memory guard.

```bash
cargo run --release -- --game starfield --studio --mesh '<mesh>' \
    [--loose-root <fixture>] [--rt-test-lod-scale 0.000001] --bench-frames 120 --bench-hold
```

| Check | Result |
|---|---|
| The viewer launch runs the database merge | Yes. The log shows the database discovered and indexed; `mat.dump` lists colour and normal with source `mat`, and no texture is missing |
| A loose material override loads | Yes, with `--loose-root <fixture>`. `--game starfield` adds `--esm`, so without that flag the game's `Data` loose files are layered. The flag replaces them. `mat.dump` showed the fixture's colour path and, for the opacity fixture, `test=true threshold=0.5` |
| The loose file merges over its database parent | **No.** The loose material replaces the database lookup, so roles it does not author (normal, for one) are absent. Fixtures are self-contained, so this does not block them |
| A wall can be made a mirror with `mat.set` | Yes, with two conditions. At the viewer's default framing every pixel is at the reflection ray's distance cut-off (`render.debug rt_lod`), so a metal wall shows only the ambient fallback; `--rt-test-lod-scale 0.000001` restores the ray. That flag also extends bounce-light rays, so both sides of an A/B need it. The back wall hides the object's reflection behind the object; use a side wall. A pose that shows a chosen sample in the mirror is **not yet verified** |
| Debug view output transfer | sRGB. A surface painted `0.08` (the non-participant grey, `render.debug water_term`) reads 80/255. Encode an expected value with the sRGB curve, then compare within 2/255 |
| Fixture generator | `scripts/starfield-material-fixture.py` (`opacity`, `pbr`, `ao`, `ao-open`). The opacity and colour textures loaded in the engine; the other three kinds' maps are not read by the engine yet |

### Acceptance meshes

| Scenario | Before-and-after captures | Fixture pass rule | Peak resident memory, GiB |
|---|---|---|---|
| A4 | `meshes\landscape\grass\grasssage01.nif` (`GrassSagebrush01.mat`, threshold 0.5, 88.9 % of the opacity map's top level at or below it). Today it draws as solid cards | `meshes\setdressing\found\foundsquaretable_01.nif` (`FoundSquareTable_01.mat`) | 1.10; 1.23 |
| A5 | `meshes\setdressing\gagarinstatue01\gagarinstatue01.nif` | the table | 1.03 to 1.23; 1.23 |
| A6 | `meshes\setdressing\humanbones\ribcage.nif` | the ribcage (the fixture value is uniform) | 1.10 |

The table has a large flat upright panel on which the 4x4 blocks show with
clean edges, which suits samples on the visible surface. It has three shapes
(its LOD levels are all drawn). **Whether any floor point gives a readable
shadow through an open block is not verified.** The panel is upright under a
ceiling light, and the studio light is wide enough that a 15-unit block may
wash out in the penumbra. Phase 1 must confirm A4's shadow samples before it
relies on them.

The sagebrush is a poor fixture mesh: its cards are tilted and overlap.

## Census

Reproduce with the commands in
`crates/sfmaterial/examples/sf_material_census.rs`. The path list is every
material path named by the models in `Meshes01`, `Meshes02` and
`MeshesPatch` (10,380; a run with no model limit gave the same set). The
texture archives are `Textures01`–`11`, then `TexturesPatch01` and `02`.
Peak memory was 280 MB. `lookup` gave byte-identical results for all 10,380
paths before and after the refactor that added the per-layer view.

Added bytes are for the whole population, not for any one scene.

### Population

| Measure | Materials |
|---|---:|
| Paths named | 10380 |
| Resolved in the base database | 10158 |
| With a texture set named by the root object | 0 |
| With 0 numbered layer(s) | 23 |
| With 1 numbered layer(s) | 7218 |
| With 2 numbered layer(s) | 1464 |
| With 3 numbered layer(s) | 715 |
| With 4 numbered layer(s) | 728 |
| With 5 numbered layer(s) | 2 |
| With 6 numbered layer(s) | 8 |

### Roughness, metalness and occlusion: the SMM-SRC-010 layer

| Property | Texture only | Replacement only | Both | Neither |
|---|---:|---:|---:|---:|
| roughness | 7201 | 804 | 1547 | 606 |
| metalness | 2220 | 1993 | 182 | 5763 |
| ambient occlusion | 3282 | 89 | 5 | 6782 |

### Layers (SMM-OBS-005)

| Property | More than one layer supplies it | SMM-SRC-010 layer is not the first | `lookup` takes texture and replacement from two layers |
|---|---:|---:|---:|
| opacity | 111 | 105 | 57 |
| roughness | 1979 | 44 | 816 |
| metalness | 307 | 116 | 182 |
| ambient occlusion | 1245 | 93 | 162 |

### Replacement colours on the SMM-SRC-010 layer (supports SMM-SRC-004)

| Property | Grey (r = g = b) | Not grey | Commonest red values (value × count) |
|---|---:|---:|---|
| opacity | 375 | 0 | 0 × 116, 128 × 54, 204 × 25, 175 × 11, 78 × 10, 230 × 10 |
| roughness | 2351 | 0 | 20 × 327, 128 × 221, 255 × 185, 35 × 111, 0 × 93, 34 × 92 |
| metalness | 2175 | 0 | 255 × 1265, 128 × 134, 94 × 56, 0 × 55, 230 × 23, 235 × 21 |
| ambient occlusion | 94 | 0 | 128 × 31, 233 × 23, 142 × 9, 0 × 5, 215 × 5, 102 × 2 |

### Opacity with `HasOpacity` true (SMM-OPA-004, -005)

| Measure | Materials |
|---|---:|
| `HasOpacity` true | 2273 |
| …of which effect materials | 0 |
| Threshold above zero | 865 |
| Threshold authored as zero or less | 1385 |
| No threshold on the chain | 23 |
| Named layer is not layer 0 | 7 |
| …of which the index has a layer with that number | 5 |
| Named layer unresolved | 2 |
| Layer 0 with only a root-named set | 0 |
| `OpacitySourceLayer` = (not authored) | 2266 |
| `OpacitySourceLayer` = MATERIAL_LAYER_1 | 7 |

| Slot 2 on the named layer | Texture only | Replacement only | Both | Neither |
|---|---:|---:|---:|---:|
| `HasOpacity` true, layer resolved | 1921 | 17 | 39 | 294 |

### Slot-2 data without `HasOpacity` true (SMM-OPA-006)

| Measure | Materials |
|---|---:|
| Effect materials | 434 |
| Not effect materials | 546 |
| `HasOpacity` authored false | 13 |
| `HasOpacity` not on the chain | 967 |

### Texture formats of the chosen textures (SMM-SRC-003)

| Property | Format | Textures |
|---|---|---:|
| opacity | (not found) | 5 |
| opacity | BC1_UNORM_SRGB | 17 |
| opacity | BC4_UNORM | 1331 |
| roughness | (not found) | 18 |
| roughness | BC1_UNORM_SRGB | 10 |
| roughness | BC4_UNORM | 3694 |
| roughness | R8_UNORM | 2 |
| metalness | (not found) | 9 |
| metalness | BC1_UNORM | 1 |
| metalness | BC1_UNORM_SRGB | 11 |
| metalness | BC4_UNORM | 1784 |
| ambient occlusion | (not found) | 11 |
| ambient occlusion | BC4_UNORM | 2733 |
| ambient occlusion | R8_UNORM | 2 |

### Added texture bytes (PRD §8)

Distinct textures each property would bind that no material in the population binds as colour, normal or emissive today. Sizes are unpacked bytes from the record headers.

| Property | Added textures | Not found | All levels, MiB | Floor, MiB: multi-chunk records | Floor, MiB: one-chunk records, at most | One-chunk records |
|---|---:|---:|---:|---:|---:|---:|
| opacity | 1340 | 4 | 1214.4 | 57.3 | 0.3 | 7 |
| roughness | 3708 | 17 | 5447.2 | 156.4 | 2.3 | 18 |
| metalness | 1798 | 8 | 2496.6 | 74.0 | 0.0 | 34 |
| ambient occlusion | 2745 | 10 | 4665.3 | 116.2 | 2.3 | 12 |
| all four, each texture once | 9561 | 39 | 13799.5 | 402.6 | 5.0 | 71 |
