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

**Corrected in Phase 1 (#7421).** The first run counted an authored empty
texture path as a texture. The `fo76utils` viewer treats it as none and uses
the replacement (`glproperty.cpp` `getSFTexture`), and `CdbSlotSet::texture`
now does too. The tables below are the corrected run. What moved: the
presence splits (opacity was 1,921 / 17 / 39 / 294, roughness 7,201 / 804 /
1,547 / 606, metalness 2,220 / 1,993 / 182 / 5,763, occlusion 3,282 / 89 / 5 /
6,782), the layer counts, slot 2 without the flag (was 434 effect and 546
other) and, by 0.7 MiB, occlusion's added bytes.

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
| roughness | 7169 | 960 | 1391 | 638 |
| metalness | 2148 | 2021 | 158 | 5831 |
| ambient occlusion | 3220 | 89 | 5 | 6844 |

### Layers (SMM-OBS-005)

| Property | More than one layer supplies it | SMM-SRC-010 layer is not the first | `lookup` takes texture and replacement from two layers |
|---|---:|---:|---:|
| opacity | 109 | 108 | 54 |
| roughness | 1898 | 45 | 848 |
| metalness | 297 | 120 | 179 |
| ambient occlusion | 908 | 97 | 162 |

### Replacement colours on the SMM-SRC-010 layer (supports SMM-SRC-004)

| Property | Grey (r = g = b) | Not grey | Commonest red values (value × count) |
|---|---:|---:|---|
| opacity | 378 | 0 | 0 × 116, 128 × 54, 204 × 28, 175 × 11, 78 × 10, 230 × 10 |
| roughness | 2351 | 0 | 20 × 327, 128 × 221, 255 × 185, 35 × 111, 0 × 93, 34 × 92 |
| metalness | 2179 | 0 | 255 × 1266, 128 × 134, 94 × 56, 0 × 55, 230 × 23, 235 × 21 |
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
| `HasOpacity` true, layer resolved | 1912 | 19 | 37 | 303 |

### Slot-2 data without `HasOpacity` true (SMM-OPA-006)

| Measure | Materials |
|---|---:|
| Effect materials | 432 |
| Not effect materials | 494 |
| `HasOpacity` authored false | 1 |
| `HasOpacity` not on the chain | 925 |

### Texture formats of the chosen textures (SMM-SRC-003)

| Property | Format | Textures |
|---|---|---:|
| opacity | (not found) | 4 |
| opacity | BC1_UNORM_SRGB | 17 |
| opacity | BC4_UNORM | 1331 |
| roughness | (not found) | 17 |
| roughness | BC1_UNORM_SRGB | 10 |
| roughness | BC4_UNORM | 3694 |
| roughness | R8_UNORM | 2 |
| metalness | (not found) | 8 |
| metalness | BC1_UNORM | 1 |
| metalness | BC1_UNORM_SRGB | 11 |
| metalness | BC4_UNORM | 1784 |
| ambient occlusion | (not found) | 10 |
| ambient occlusion | BC4_UNORM | 2734 |
| ambient occlusion | R8_UNORM | 2 |

### Added texture bytes (PRD §8)

Distinct textures each property would bind that no material in the population binds as colour, normal or emissive today. Sizes are unpacked bytes from the record headers.

| Property | Added textures | Not found | All levels, MiB | Floor, MiB: multi-chunk records | Floor, MiB: one-chunk records, at most | One-chunk records |
|---|---:|---:|---:|---:|---:|---:|
| opacity | 1340 | 4 | 1214.4 | 57.3 | 0.3 | 7 |
| roughness | 3708 | 17 | 5447.2 | 156.4 | 2.3 | 18 |
| metalness | 1798 | 8 | 2496.6 | 74.0 | 0.0 | 34 |
| ambient occlusion | 2746 | 10 | 4666.0 | 116.2 | 2.3 | 12 |
| all four, each texture once | 9562 | 39 | 13800.1 | 402.6 | 5.0 | 71 |

## Phase 1 acceptance (opacity, #7421)

Run on 2026-11-26 against the installed game, in the Studio viewer.

**Reach** (`starfield_material_property_reach`, SMM-OBS-006). It counts
database records on imported shapes; the census above counts path strings,
and some records are named by more than one spelling.

| Measure | Value |
|---|---:|
| Models parsed | 68,459 |
| Records on imported shapes that resolve | 10,073 |
| Records authoring opacity data (144,995 shapes) | 2,875 |
| …reaching on every shape, as Map | 1,934 |
| …reaching on every shape, as Constant | 19 |
| …held: `HasOpacity` not true (SMM-OPA-006) | 920 |
| …held: the named layer does not exist (SMM-OPA-005) | 1 |
| …held: the named layer authors no opacity (SMM-OPA-005) | 1 |
| …with no recorded rule | 0 |

**A4, fixture** (`scripts/starfield-material-acceptance.py a4`): every check
passed. `mat.dump` named the fixture as Map; the 14 lid readings and 6
tabletop readings held the exact value for their block's state in both
patterns. Direct light on the floor, mean of five captures, 0 to 255:

| Floor point | Behind an open block | Behind a closed block | Two identical runs differ by |
|---|---:|---:|---:|
| centre | 253.1 | 128.8 | 0.3 |
| left | 231.7 | 198.1 | 0.1 |
| right | 212.9 | 180.8 | 0.0 |

The floor shadow comes from the flat tabletop behind the upright lid. The
side points stay well short of full shadow behind a closed block, because
light also arrives through neighbouring openings and from the fill light, so
the script requires a gap of 20 rather than a lit and a dark level. Single
captures within one run spread by up to 3.1, hence the mean.

**A4, vanilla captures** of `grasssage01.nif`, same pose, engine at
`1326f9b13` and with Phase 1. Before: solid rectangular cards showing the
colour texture's off-leaf texels. After: stems and leaves, with a floor
shadow of the same outline. The cutout test moved from `>=` to `>` (func 6
to 4) and `mat.dump` names the sagebrush opacity map. The captures show game
content and are not checked in.

**Blended materials** (SMM-OPA-003). Five records on 29 shapes blend and
have an opacity map (signs and labels): three on 25 shapes without a cutout,
two on 4 shapes with one. A temporary count in the reach test, not kept, gave
these. The lit path took a blended surface's alpha from the material alone, so
opacity only discarded texels near zero. That rule now leaves a surface with a
bound opacity map alone. Checked on a loose `AlphaBlend` fixture on the table:
before the change a quarter-opaque block drew solid; after it the blocks blend,
and half-opaque blocks read 229 to 237 (final view, luminance) between solid
(143 to 187) and fully open (243 to 246). Traced rays still treat a blended
non-glass surface as blocking wherever its coverage is at least 1/255, so a
half-opaque surface casts a full shadow.

**Missing texture** (SMM-SRC-011, -OBS-002): with the fixture's opacity file
deleted the table drew solid, as an opaque one does, and `tex.missing` listed
the file as `slot=opacity, src=mat`.

**Loose materials.** Reading the alpha settings along the layer graph stops a
LOD material's settings reaching the main material, which is where
`portablegreenhouse02.mat` had been getting its cutout. Its parent,
`portablegreenhouse01`, authors `HasOpacity` in the database, so the loose
reader now takes the alpha settings a file leaves unauthored from its database
parent. Five installed screen materials on `genericglow01_base`
(`requisitionkiosk_splashscreen_*`) inherit `HasOpacity` the same way. They
author no opacity map, and the top mip of each of their `BC3_SRGB` colour
textures has alpha 255 throughout, so their cutout removes nothing at that
level. None of the six was captured in the viewer.

**Added resident bytes** on `grasssage01.nif`, streaming off: 2.7 MiB
(10.9 MiB resident against 8.3 MiB at `1326f9b13`). No GPU time delta was
measured; Phase 1's exit does not ask for one.

## Phase 2 acceptance (roughness and metalness, #7422)

Run on 2026-11-26 against the installed game, in the Studio viewer.

**Reach** (`starfield_material_property_reach`, SMM-OBS-006), over the same
68,459 models and 10,073 resolving records as Phase 1. Opacity is unchanged.

| Measure | Roughness | Metalness |
|---|---:|---:|
| Records authoring data | 9,447 | 4,291 |
| …on imported shapes | 272,131 | 183,436 |
| …reaching on every shape, as Map | 8,500 | 2,285 |
| …reaching on every shape, as Constant | 947 | 2,006 |
| …with no recorded rule | 0 | 0 |

Neither property has a holding rule: a record that authors one always has a
first supplying layer (SMM-SRC-010).

**What a mesh arrives with.** The NIF importer's legacy classifier can fill
roughness and metalness from names and shader scalars. The merge does not treat
such a guess as an authored Override. A temporary program, not kept, imported
the same models: all 339,858 shapes naming a material arrived with neither
value.

**A5, fixture** (`scripts/starfield-material-acceptance.py a5`): every check
passed. `mat.dump` named both fixtures as Map. For each property, 15 readings
on the visible surface and 12 in the mirror held their block's value within
2/255. The lid's texture coordinates mirror about its middle, both ways, so
these cover fixture columns 0 to 2 and rows 0 and 1: six pairs. The mirror is
the room's left wall at the script's pose, which this run fixes. With the wall
left matte, all 12 mirror points read one value, the wall's own.

What the mirror reading shows: the debug view sends a ray along the reflection
direction and applies `applyRayHitPbrMaps` at the closest covered hit, the
function the path loop calls for its hits. The final frame's sharp reflection
ray shades its hit without roughness or metalness, before this phase and after
it, so no final-frame mirror pixel depends on either map. That the path loop
takes the maps before it shades or bounces is checked by source shape
(`every_pbr_reader_takes_the_maps`), not by a capture. The view also traces
whatever `rt_lod` says; the run passed `--rt-test-lod-scale` as PRD §7.2 asks.

**A4** was run again on the final shaders and passed.

**A5, vanilla captures** of `gagarinstatue01.nif`, same pose, engine at
`7377b6b7d` and with Phase 2. `mat.dump` names the statue's own roughness and
metalness maps. Over a crop around the statue, 14.8 % of pixels differ by more
than 8 of 255 in luminance (mean difference 4.46); a patch of back wall differs
by 0.10 on average. By eye the arms and legs lose some broad bright highlights.
The captures show game content and are not checked in.

**Missing texture** (SMM-SRC-011, -OBS-002): with both of the `pbr` fixture's
data maps deleted, `mat.dump` showed the two roles as placeholders,
`tex.missing` listed each file under `slot=roughness` and `slot=metalness`
with `src=mat`, and the 15 lid points read the fallbacks, roughness 0.85 and
metalness 0.

**Added resident bytes** on `gagarinstatue01.nif`, streaming off: 1.3 MiB for
both maps (3.6 MiB resident against 2.3 MiB at `7377b6b7d`).

**GPU time delta** (`scripts/starfield-material-acceptance.py gpu`): main
render on the table at the default pose, the `pbr` fixture with its maps
unbound, one bound, or both. Five rounds of each, 1280x720, pooling the
engine's once-a-second timing samples. Unbound surfaces shade the neutral
scalars.

| Maps bound | Samples | Median, ms | Quartiles, ms | Median minus unbound, ms |
|---|---:|---:|---:|---:|
| none | 113 | 4.00 | 2.80 to 4.60 | |
| roughness | 111 | 3.90 | 2.80 to 4.80 | -0.10 |
| metalness | 109 | 4.00 | 2.80 to 4.90 | 0.00 |
| both | 111 | 3.90 | 2.80 to 4.50 | -0.10 |

**No cost is resolved by this measurement, and it cannot resolve a small one.**
Single samples run from 0.6 to 9.1 ms under identical settings: the scene
loads the GPU lightly, its clocks were not held, and the table covers a small
part of the frame. The bench line's single value per run swung by up to 2.9 ms
between identical runs, so the script does not use it. A cost on a full scene
is not measured.

**Not checked.** A placed `TXST` override of TX08 or TX09 in the viewer (a
parser test covers the slots). The maps under texture streaming (A8 is a unit
test). The installed loose `.mat` files, which now resolve both properties from
their own layers and previously forwarded neither.
