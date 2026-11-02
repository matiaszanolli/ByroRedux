# Subsurface Scattering — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (BGSM translucency suite re-attributed to FO76, SLSF2 lobes to
Skyrim only, the Beyond Skyrim `_sk` citation corrected, the dead Burley
link replaced, the Cornell oracle driver and its tolerances corrected at
HEAD, the actor-skin denominator defined, ROADMAP-row consequences named in
Q3 and Q4, sibling PRDs linked). Revision 1 was the first draft of the M56
promise: one canonical subsurface class decided at the translation boundary,
a Burley normalized-diffusion profile for skin and authored organics, eye
cornea refraction, and an RT-traced reference variant. Statements about how
an original engine, file format, tool or paper behaves cite external
sources (nif.xml, nifly, the Bethesda Material Editor source, the CS wiki,
the Beyond Skyrim wiki, the published papers); a claim that could not be
verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: What it means for an actor's skin, an eye, or an authored
translucent organic surface to "scatter light under the surface" on
ByroRedux, for the titles whose actors the engine already spawns (M41):
how each game's material data becomes one canonical subsurface class in the
translate step, which canonical response each class gets in the renderer,
how the response is measured, and what it costs. Out of scope: hair and fur
(M57, [hair-shading](hair-shading.md)), decal and micro-detail layering
(M59, [material-decal-layering](material-decal-layering.md)), wet-skin states
(M61, [wet-surface-accumulation](wet-surface-accumulation.md)), participating
media (M55, [volumetric-lighting](volumetric-lighting.md)), and any per-game
look matching. Fallout 76 is parse-only and has no actors to shade.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Per-game material signals → canonical `Material` (the translate step) | [NIFAL §3](../engine/nifal.md#3-materials--the-reference-realisation), [NIFAL shader flags](../engine/nifal.md#shader-flags--texture-sets--effect-shaders--converged-surveyed-2026-06-02), [Material Abstraction §3](../engine/material-abstraction.md#3-the-canonical-contract-target-state) |
| `GpuMaterial` layout and the shader-struct contract | [Shader Pipeline § GpuMaterial](../engine/shader-pipeline.md#gpumaterial--432-bytes-ssbo-set-1-binding-13) |
| G-buffer attachments a screen-space pass reads | [Shader Pipeline § G-Buffer Layout](../engine/shader-pipeline.md#g-buffer-layout) |
| Visibility vocabulary, transport, ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract) |
| Deterministic Cornell captures and A/B workflow | [Renderer Evaluation](../engine/renderer-evaluation.md#current-scope-and-next-cases) |
| Diffusion pass, eye refraction, RT reference variant | *None yet* — Phase 0 writes `docs/engine/subsurface-scattering.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `SSS-` prefix.

---

## 1. Product thesis

M41 made actors spawn with skeleton, body, FaceGen head and outfit. They
are lit by the same canonical response as a wooden table, plus the
Bethesda soft / rim / back-light lobes where the content authors them. The
ROADMAP row puts the gap plainly: "M41's NPCs become visibly human only
once skin gets SSS — flat Lambert reads as plastic"
([ROADMAP § Milestone tiers](../../ROADMAP.md#milestone-tiers), Tier 8).
The feature is for the player looking at a face at conversation distance,
and for the screenshot or reference capture of that face.

None of the seven titles authors a diffusion profile. What they author is
*intent*: a shader type, a flag, a material-name convention or a BGSM bool
that says "this is skin", "this is an eye", "this is a thick translucent
object". That intent is spelled differently by every game, and some games
spell it nowhere a parser reads today (§3.2).

The corollary that shapes every requirement below: **subsurface is a
canonical material class decided once, in the translate step, from the
game's own authored intent; the renderer applies one physical profile per
class and never learns which game the class came from.** It follows that the
engine will not invent skin where no authored signal says skin, will not
tune a per-game "look", and will not grow a per-game branch in any shader to
get there. A surface with no authored signal keeps today's response.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Subsurface class** | The canonical classification a material receives in the translate step (§3.1): one of `None`, `Skin`, `Eye`, `Organic`. A new field on the canonical `Material`; the name of the Rust type is the Phase 0 design's to choose. |
| **Authored signal** | A field the source format carries that declares material intent (shader type, shader flag, BGSM bool, material-property name, CDB component). The only input to the subsurface class (§3.2). |
| **Burley profile** | The normalized-diffusion reflectance profile: `R(r) = (e^(−r/d) + e^(−r/3d)) / (8πdr)`, a sum of two exponentials that integrates to one over the plane, so the result is scaled by the diffuse albedo; Christensen relates the shape parameter `d` to the physical mean free path ([Burley, SIGGRAPH 2015 course notes](https://blog.selfshadow.com/publications/s2015-shading-course/burley/s2015_pbs_disney_bsdf_notes.pdf), eqs. 5–6, citing Christensen & Burley, *Approximate Reflectance Profiles for Efficient Subsurface Scattering*, Pixar Technical Memo 15-04, 2015). ROADMAP's M56 row names it the preferred model. |
| **Realization** | How the Burley profile is evaluated at runtime: a screen-space convolution ([Golubev 2018](https://www.advances.realtimerendering.com/s2018/Efficient%20screen%20space%20subsurface%20scattering%20Siggraph%202018.pdf)), or ray-traced disc probes against the receiver's own geometry (the RT reference variant). Decision Q1. |
| **Mean free path (MFP)** | The per-channel distance parameter of the Burley profile, in canonical `Meters` ([Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract), item 1). |
| **Bethesda lobes** | The existing direct-light response terms keyed on `MAT_FLAG_SOFT_LIGHTING`, `MAT_FLAG_RIM_LIGHTING`, `MAT_FLAG_BACK_LIGHTING` and `MAT_FLAG_TRANSLUCENCY` (§3.3 rows 1–4). They are the content's own fake-SSS. |
| **Reflection half / transmission half** | The split `shadowableLightRadiance` returns (#5192): the transmission half is non-zero only where the light is behind the shading plane, and is shadowed by the own-instance-skipping trace (#5249). |
| **Cornea refraction** | Offsetting the iris sample along the view ray refracted at a cornea surface, so the iris appears behind a transparent dome. The published real-time source is **Phase 0: verify**: the candidate is Jimenez & von der Pahlen, *Next-Generation Character Rendering*, GDC 2013 ([session page](https://gdcvault.com/play/1018270/Next-Generation-Character)), whose session description names "realistic eye shading" but not the refraction model; Phase 0 settles it from the talk's slides or another published eye model. |
| **Actor skin shape** | The census denominator: a shape in a mesh that the game's records name as an actor's head or body model For Skyrim the race's `WNAM` skin (an ARMO record) and its default `HEAD` head parts (HDPT records) name them (UESP, *Skyrim Mod:Mod File Format/RACE*, local dump `uesp-wiki/Skyrim Mod/Mod File Format/RACE.wiki`). The record list for Oblivion, FO3 / FNV, FO4 and Starfield is **Phase 0: verify** (xEdit definitions per title). |
| **RT reference variant** | The ray-traced realization of the Burley profile, selectable at runtime, used as the oracle for the default realization and as the higher-quality path for close-up captures (§5.5). |
| **Census** | An example binary that walks a game's mesh and material archives and counts each authored signal and each resulting class, without a renderer (precedent: `crates/nif/examples/emissive_census.rs`). |

---

## 3. Core model

### 3.1 The subsurface classes

| Class | Meaning | Canonical response | Reported as (`mat.dump`) |
|---|---|---|---|
| `None` | No authored subsurface intent | Today's response, unchanged (SSS-DIF-004) | `subsurface: none` |
| `Skin` | Authored skin: head, body, hands of an actor | Burley profile with the canonical skin MFP (Q6), plus the existing transmission half | `subsurface: skin (<signal>)` |
| `Eye` | Authored eyeball material | Cornea refraction of the iris (§5.4) over today's eye reflection | `subsurface: eye (<signal>)` |
| `Organic` | Authored thick translucent object that is not skin (BGSM v ≥ 8 translucency *thick object*, Starfield `UseSSS`). Both producers are outside the verified in-scope set today: the BGSM v ≥ 8 suite is Fallout 76's layout (§3.2 S12) and `UseSSS` is unverified (S15). | Burley profile with the authored subsurface colour as the scatter tint | `subsurface: organic (<signal>)` |

Thin translucent sheets (leaves, paper) keep the existing translucency lobe
and are class `None` for diffusion purposes; diffusion width is meaningless
on a single-sided card. `<signal>` names the authored signal row of §3.2
that produced the class, so a wrong class is traceable to one row.

### 3.2 Authored signals per game (the translation table)

This is the hard part of the milestone: the inventory below is the state at
HEAD on 2026-11-01. Each row is one translate-step arm. The census (SSS-CLS-008)
replaces the hand-written Today column in Phase 0.

| # | Game | Authored signal | External source | Class | Today |
|---|---|---|---|---|---|
| S1 | Oblivion | `NiMaterialProperty` name `Skin` | CS wiki, *Working With Nifs 301: Properties Breakdown* (local dump `cs-uesp-wiki/(main)/Working With Nifs 301 : Properties Breakdown.wiki`, § MaterialProperty: "Skin - The skin shader") | `Skin` | Not captured: `MaterialInfo` (`crates/nif/src/import/material/mod.rs`) has no material-property name field |
| S2 | Oblivion | `NiMaterialProperty` name `Right Eye` | Same page ("Right Eye - Eyeball shader"); whether a left-eye name exists is **Phase 0: verify** (census of material-property names in `Oblivion - Meshes.bsa`) | `Eye` | Not captured, as S1 |
| S3 | FO3 / FNV | `BSShaderPPLightingProperty` shader type `SHADER_SKIN` = 14 | nif.xml `BSShaderType` (`/mnt/data/src/reference/nifxml/nif.xml`); nifly `BSShaderProperty::IsSkinTinted` returns `shaderType == SHADER_SKIN` (`nifly/src/Shaders.cpp`) | `Skin` | Captured on `MaterialInfo::legacy_shader_type` (`crates/nif/src/import/material/mod.rs`, #2320) and set in `legacy_properties.rs`; **not** forwarded to `ImportedMaterial` (`crates/nif/src/import/types/material.rs` has no such field) |
| S4 | FO3 / FNV | `BSShaderFlags` bit 10 `FaceGen` | nif.xml `BSShaderFlags` (`F3SF1`) | `Skin` (head) | No constant: `fo3nv_f1` in `crates/nif/src/shader_flags.rs` defines no FaceGen bit |
| S5 | FO3 / FNV | `BSShaderFlags` bit 17 `Eye_Environment_Mapping` | nif.xml `BSShaderFlags` | `Eye` | Read for env-map authorship only (`fo3nv_f1::EYE_ENVIRONMENT_MAPPING`, `legacy_properties.rs`) |
| S6 | Skyrim LE / SE | `BSLightingShaderType` 4 Face Tint, 5 Skin Tint | nif.xml `BSLightingShaderType`; Beyond Skyrim, [*NetImmerse Format/Texture Slots*](https://wiki.beyondskyrim.org/wiki/Arcane_University:NetImmerse_Format/Texture_Slots) | `Skin` | Reaches `Material::material_kind` and `source_shader_type` (`translate_material_with_provenance`, `byroredux/src/material_translate.rs`); consumed only as the skin-tint colour multiply (§3.3 row 6) |
| S7 | Skyrim LE / SE | `BSLightingShaderType` 16 Eye Envmap | nif.xml (`Eye Cubemap Scale`, left/right reflection centres) | `Eye` | Consumed as the iris-centred cube reflection (§3.3 row 8) |
| S8 | Skyrim LE / SE | Slot 2 `_sk` map under Skin Tint | Beyond Skyrim, *Arcane University:NetImmerse Format/Texture Slots* ([page](https://wiki.beyondskyrim.org/wiki/Arcane_University:NetImmerse_Format/Texture_Slots)), slot 2: skin tint textures for the Skin Tint shader type, `_sk` suffix, BC1, requiring shader type Skin Tint and `SLSF1_FaceGen_RGB_Tint`; the effect "simulates the diffusion of light inside a translucent medium". The page states the effect, not the encoding: how the map's RGB maps onto a scatter colour or MFP is **Phase 0: verify** (a capture of a vanilla Skyrim face against the map, or a published shader description) | Skin scatter tint input | Inert on vanilla: bound only through `TINT_ALPHA_WEIGHT_BIT` (`byroredux/src/render/static_meshes.rs:1063-1073`, #4423); the #4423 audit decoded all nine vanilla `_sk` maps as alpha-less DXT1 ([nifal.md § shader flags](../engine/nifal.md#shader-flags--texture-sets--effect-shaders--converged-surveyed-2026-06-02)) |
| S9 | Skyrim LE / SE | SLSF2 bits 25 / 26 / 27 Soft / Rim / Back Lighting; `Lighting Effect 1/2` | nif.xml `SkyrimShaderPropertyFlags2` (`versions="#SKY# #SSE#"`); nif.xml documents `Lighting Effect 1/2` with a question mark ("Controls strength for envmap/backlight/rim/softlight lighting effect?") — the exact meaning is **Phase 0: verify**. FO4 has neither: `Fallout4ShaderPropertyFlags2` bits 25–27 are `Alpha_Test`, `Gradient_Remap`, `VATS_Target_Draw_All`, and `Lighting Effect 1/2` carry `vercond="#NI_BS_LT_FO4#"`; FO4 authors the lobes in BGSM (S13) and in the `#BS_FO4_2#` scalars `Subsurface Rolloff`, `Rimlight Power`, `Backlight Power` | Not a class signal; Bethesda lobes | Consumed (§3.3 rows 1–3); the NIF flags are read for the Skyrim slot layout only (`crates/nif/src/import/material/dedicated_shader.rs:196-211`) |
| S10 | FO4 | `F4SF1` bit 10 `Face`, bit 21 `Skin_Tint`; shader types 4 / 5 | nif.xml `Fallout4ShaderPropertyFlags1`, `BSLightingShaderType` (`versions="#SKY_AND_LATER#"`) | `Skin` | Flags: constants `fo4_slsf1::FACE`, `fo4_slsf1::SKIN_TINT` defined in `crates/nif/src/shader_flags.rs:190,204`, no reader outside that file. Shader types: passed through unchanged for the FO4 layout (`canonical_shader_type`, `crates/nif/src/import/material/slot_role.rs:174-189`), so they reach `Material::material_kind` as in S6 |
| S11 | FO4 | BGSM `Facegen`, `Skin Tint` bools | Material Editor source (`/mnt/data/src/reference/Material-Editor/Main.cs:1201-1202`: tooltips "Toggle facegen rendering", "Toggle skin tint rendering") | `Skin` | Decoded (`BgsmFile::facegen`, `BgsmFile::skin_tint`, `crates/bgsm/src/bgsm.rs:131-132`); no consumer outside the crate |
| S12 | FO76 (BGSM v ≥ 8) | BGSM v ≥ 8 translucency suite (subsurface colour, transmissive scale, turbulence, thick object, mix albedo) | Material Editor `Main.cs:848-856` (shown only for `Version >= 8`); its default versions are `DefaultVersionFO4 = 2` and `DefaultVersionFO76 = 21` (`Main.cs:42-43`), so the suite is Fallout 76's, not vanilla FO4's; nif.xml `BSSPTranslucencyParams` (`versions="#F76#"`). The *thick object* tooltip is "Object on which the material is applied is thick (or a billboard if not). Used for correct shadowing." (`Main.cs:1158`): reading it as the `Organic` class is this PRD's choice, not the tool's statement (§6 row 6) | `Organic` when *thick object*, else `None` with the translucency lobe | Forwarded for any BGSM that carries it (`forward_bgsm_phase1_flags`, `byroredux/src/asset_provider/material/mod.rs:90`); consumed by the translucency lobe (§3.3 row 4) |
| S13 | FO4 (BGSM v < 8; vanilla v2) | BGSM `Subsurface Lighting` + rolloff | Material Editor `Main.cs:834-845` (shown only for `Version < 8`), tooltip "Toggle subsurface lighting effect." (`Main.cs:1155`) | Not a class signal; maps onto the soft-lighting wrap | Forwarded as `soft_lighting` + `subsurface_rolloff` (`forward_bgsm_rim_subsurface`, `byroredux/src/asset_provider/material/mod.rs:132`) |
| S14 | FO76 | `BSShaderType155` 3 Face Tint, 4 Skin Tint, 12 Eye Envmap; CRC32 flags `FACE`, `SKIN_TINT` | nif.xml `BSShaderType155`, `BSShaderCRC32` | `Skin` / `Eye` | Type remap exists (`slot_role.rs`); no actors spawn (§7.1) |
| S15 | Starfield | CDB `TranslucencySettingsComponent.UseSSS` | No external source in the reference set; its meaning and the location of a subsurface colour are **Phase 0: verify** (CDB field census plus a capture of a Starfield face) | `Organic` or `Skin` (per verify) | Decoded (`crates/sfmaterial/src/index.rs`, `use_sss`); parked at the boundary (`merge.rs`, test `use_sss_is_parked_until_a_colour_source_exists`, #5196) |

### 3.3 What already ships (inventory)

The renderer already carries Bethesda's own fake-SSS lobes. M56 builds on
them; it does not replace the content's authored terms wholesale. The
inventory below is the state at HEAD on 2026-11-01.

| # | Mechanism | Today |
|---|---|---|
| 1 | Soft-lighting wrap | `bethesdaDiffuseLightFactor` (`crates/renderer/shaders/include/lighting.glsl`): wrap width from `subsurfaceRolloff` or `lightingEffect1`, masked by the slot-2 lighting mask |
| 2 | Rim lobe | `bethesdaRimFactor` (same file) |
| 3 | Back-light lobe | `bethesdaBackFactor` (same file), slot-7 back-light map, transmission half |
| 4 | BGSM translucency lobe | `MAT_FLAG_TRANSLUCENCY` block in `shadowableLightRadiance` (same file), thick-object / mix-albedo variants, transmission half shadowed by `traceLightTransmittanceSkippingInstance` (#5249) |
| 5 | Disney Hanrahan–Krueger fake subsurface | `disneyDiffuseSplit` (`include/pbr.glsl`), gated on `MAT_FLAG_PBR_BSDF`; `subsurface` is hard-set to `0.0` for all content in `translate_material_with_provenance` and reachable only through `mat.set <id> subsurface` (`byroredux/src/commands/scene.rs`) |
| 6 | Skin Tint colour | `materialKind == 5u` albedo multiply in `crates/renderer/shaders/triangle.frag` |
| 7 | `_sk` tint map | Inert on vanilla (§3.2 S8) |
| 8 | Eye reflection | `isEyeEnvmap` branch in `triangle.frag` (#5961): authored cube sampled from the nearest reflection centre; no refraction |
| 9 | Caustic splat | `caustic_splat.comp` (#321); sources are alpha-blended refractive glass and MultiLayerParallax only (`is_caustic_source` / `is_refractive_glass`, `crates/renderer/src/vulkan/context/frame_params.rs`) |
| 10 | Diffusion profile (screen-space or RT) | None |
| 11 | `GpuMaterial` | 432 B, pinned by `gpu_material_size_is_432_bytes` (`crates/renderer/src/vulkan/material_tests.rs`) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **SSS-G-001 — Skin is classified from authored intent.** Every actor
  skin shape (§2) the census finds in a supported game resolves to
  class `Skin` through a §3.2 signal, counted by the skin classification
  coverage metric (§8).
- **SSS-G-002 — Skin diffuses light.** `Skin` and `Organic` surfaces show a
  Burley-profile diffusion measured against an analytic oracle in a
  deterministic Cornell rung (§7.2).
- **SSS-G-003 — Eyes refract.** `Eye` surfaces show the iris displaced by
  cornea refraction, measured by a view-angle probe (§7.2).
- **SSS-G-004 — No per-game renderer branch.** Zero shader or render-data
  code reads a game, a source shader type or a source format to decide
  subsurface behaviour; checked by a source-shape test (SSS-CLS-007).
- **SSS-G-005 — Bounded cost.** The default realization's GPU time on the
  bench-of-record actor scenes is published and stays inside the budget
  Phase 0 agrees (SSS-PERF-001).

### 4.2 Non-goals

- **SSS-NG-001.** Per-game look matching. The ROADMAP anti-scope rejects
  per-pixel parity with Bethesda's look
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)); no
  per-game MFP, tint or strength is tuned to resemble a vanilla screenshot.
- **SSS-NG-002.** Hair and fur shading — M57, [hair-shading](hair-shading.md).
- **SSS-NG-003.** Classification from texture-path or mesh-path keywords on
  titles that author no signal, unless decision Q3 says otherwise. The
  no-guessing rule applies: no authored signal, no class.
- **SSS-NG-004.** A per-instance or `DrawCommand` subsurface field. R1's
  promise holds: material slot extensions go to `GpuMaterial` only, as M59
  already states ([ROADMAP § Milestone tiers](../../ROADMAP.md#milestone-tiers);
  [material-decal-layering](material-decal-layering.md)).
- **SSS-NG-005.** Fallout 76 actor shading: the title is parse-only (§7.1).
- **SSS-NG-006.** A new "cinematic mode" host contract inside M56 unless
  decision Q5 chooses it. At HEAD on 2026-11-01 no rendering cinematic mode
  exists: `byroredux/src/systems/cinematic.rs` holds the sinks for
  *scripted* cinematic requests (Papyrus `PlayIdle`, scene packages), not a
  render setting, and the M58 PRD's
  [Q6](reference-post-process.md#10-decisions-required) recommends that no
  such mode be created.

---

## 5. Requirements by area

### 5.1 Classification at the translation boundary (`SSS-CLS`)

- **SSS-CLS-001.** The canonical `Material` MUST carry exactly one
  subsurface class (§3.1), set in the translate step
  (`translate_material_with_provenance` or a function it calls). Today no
  such field exists.
- **SSS-CLS-002.** Every §3.2 row whose class column names a class MUST map
  to that class through one translate-step arm, and the arm MUST record the
  row number as the class's provenance, reported by `mat.dump` (SSS-DX-001).
- **SSS-CLS-003.** The FO3 / FNV `SHADER_SKIN` signal (S3) MUST reach
  `ImportedMaterial`. Today it stops at `MaterialInfo::legacy_shader_type`.
- **SSS-CLS-004.** The Oblivion `NiMaterialProperty` name (S1, S2) MUST be
  captured on `MaterialInfo` and forwarded to `ImportedMaterial` for the
  classifier; matching MUST follow the names the Phase 0 census confirms.
  Today the name is not captured.
- **SSS-CLS-005.** The FO4 BGSM `Facegen` / `Skin Tint` bools (S11) and the
  `F4SF1` `Face` / `Skin_Tint` bits (S10) MUST each be able to produce
  `Skin`, BGSM taking precedence over the NIF bits under the existing
  child-first merge order. Today neither reaches `ImportedMaterial`.
- **SSS-CLS-006.** A material with no §3.2 signal MUST classify `None`. No
  keyword, path or name heuristic MAY produce a class unless decision Q3
  admits one, and then only behind its own provenance tag.
- **SSS-CLS-007.** Shaders under `crates/renderer/shaders/` and code under
  `byroredux/src/render/` MUST NOT read `source_shader_type`, a game
  identifier or a source-format flag to select subsurface behaviour; a new
  source-shape test in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` MUST
  assert it, extended in the same commit as any classifier refactor.
- **SSS-CLS-008.** A census example (new, `subsurface_signal_census`; it
  needs the ESM's race and head-part records for the actor-skin denominator
  and the BGSM / CDB merge, so it cannot live in `crates/nif` alone — the
  design places it) MUST print, per game, a table of material counts per
  §3.2 signal row and per resulting class, followed by the list of actor
  skin shapes (§2) classified `None`.
- **SSS-CLS-009.** The Skyrim `_sk` map (S8) MUST stay as inert as it is
  today (#4423's `TINT_ALPHA_WEIGHT_BIT` gate unchanged) until Phase 0
  settles how its RGB encodes the scatter tint; the source already says it is
  a subsurface input, not an albedo multiplier.
- **SSS-CLS-010.** The Starfield `UseSSS` arm (S15) MUST stay parked until
  Phase 0 identifies its colour source; when it lands it MUST set the class,
  the colour and the scale together, as `merge.rs`'s #5196 note requires.

### 5.2 Diffusion (`SSS-DIF`)

- **SSS-DIF-001.** `Skin` and `Organic` surfaces MUST be shaded with the
  Burley profile (§2) in the realization decision Q1 selects. Today no
  diffusion is evaluated.
- **SSS-DIF-002.** The diffusion MUST be energy-normalized: on the Cornell
  SSS rung (SSS-DX-003) the total diffuse reflectance of a lit slab with
  diffusion on MUST equal the reflectance with diffusion off within the
  tolerance the Phase 0 design states (the profile integrates to one, §2).
- **SSS-DIF-003.** The skin MFP MUST be a canonical constant set in `Meters`
  and derived from a cited measurement (decision Q6); no per-game or
  per-race value MAY exist.
- **SSS-DIF-004.** With M56 built in, the existing hardware-gated Cornell
  oracle tests (`byroredux/tests/cornell_rt_oracle.rs`, rungs L0–L5 and
  their skinned variants) and the golden-frame test
  (`byroredux/tests/golden_frames.rs`, at its `PIXEL_TOLERANCE`,
  `MAX_DIFF_PCT` and `MAX_CHANNEL_DELTA` constants) MUST pass without a
  baseline re-bless; no scene in either carries a subsurface signal. At HEAD
  on 2026-11-01 the FNV real-content probe (`scripts/renderer-eval-fnv.sh`)
  and `scripts/renderer-eval.sh` record PNGs and hashes with no pass rule
  ([renderer-evaluation.md](../engine/renderer-evaluation.md#current-scope-and-next-cases):
  "PNG hashes establish artifact identity; they are not pass/fail
  criteria"), so they are review captures here, not gates.
- **SSS-DIF-005.** Diffusion MUST apply to direct and indirect diffuse
  lighting alike and MUST NOT blur specular. Today direct diffuse and
  specular share the HDR colour attachment and indirect is albedo-demodulated
  in its own attachment ([G-Buffer Layout](../engine/shader-pipeline.md#g-buffer-layout));
  how the default realization separates them is the Phase 0 design's.
- **SSS-DIF-006.** Diffusion MUST NOT cross object boundaries: a `Skin`
  pixel MUST NOT gather lighting from a non-subsurface surface behind or
  beside it, checked on the Cornell rung with an adjacent class-`None`
  occluder.
- **SSS-DIF-007.** On the Cornell SSS rung, the 64-frame capture SHOULD
  differ from the 32-frame capture by no more than the golden-frame test's
  thresholds (`golden_frames.rs` constants named in SSS-DIF-004), under TAA
  and under FSR 3.1 Quality alike.

### 5.3 Transmission and the Bethesda lobes (`SSS-TRN`)

- **SSS-TRN-001.** The transmission half (back-light lobe, translucency
  lobe, wrap excess) MUST keep its own-instance-skipping visibility (#5249)
  for every class.
- **SSS-TRN-002.** On a `Skin` material with `MAT_FLAG_SOFT_LIGHTING` set,
  the reflection half of `bethesdaDiffuseLightFactor` MUST follow decision
  Q2; under the recommendation it MUST equal the unwrapped front term
  (`max(N·L, 0)` times the horizon clamp), checked on the Cornell SSS rung
  with the soft-lighting flag on and off. Today the wrap is the only
  subsurface-like term skin receives (`include/lighting.glsl:115-137`).
- **SSS-TRN-003.** The rim and back-light lobes MUST stay evaluated from the
  authored flags and scalars as today for every class.
- **SSS-TRN-004.** The Disney `subsurface` weight MUST stay `0.0` for
  translated content: diffusion replaces the Hanrahan–Krueger approximation
  for the classes that have a signal, and the field stays a Cornell-harness
  control.

### 5.4 Eyes (`SSS-EYE`)

- **SSS-EYE-001.** An `Eye` surface MUST offset its iris sample along the
  view ray refracted at a cornea model whose published source Phase 0
  settles (§2, *Cornea refraction*). Today eyes reflect their authored cube
  and do not refract (`isEyeEnvmap`, `crates/renderer/shaders/triangle.frag:3078`).
- **SSS-EYE-002.** The iris / sclera separation MUST come from an authored
  map; which map (the eye environment mask the reflection branch already
  reads, or another slot) is **Phase 0: verify** per game.
- **SSS-EYE-003.** The cornea index of refraction and iris depth MUST be
  canonical constants with a cited anatomical or published source, chosen in
  Phase 0.
- **SSS-EYE-004.** The existing iris-centred cube reflection (#5961) MUST
  keep its output on the sclera unchanged.
- **SSS-EYE-005.** The eye caustic MUST follow decision Q4. If it reuses the
  caustic splat, `is_refractive_glass` MUST remain the single classifier both
  its consumers read (its doc states that rule), and the eye MUST enter it
  through a canonical material signal, not a source-format check.
- **SSS-EYE-006.** The census (SSS-CLS-008) MUST report, per game, whether
  the eye meshes ship a separate cornea shape over the eyeball; that is
  **Phase 0: verify**, and SSS-EYE-005's route depends on it.

### 5.5 RT reference variant (`SSS-RT`)

- **SSS-RT-001.** A ray-traced realization of the same Burley profile MUST
  exist, selectable at runtime through a console command (name chosen in the
  design), sampling the receiver's own instance with ray queries.
- **SSS-RT-002.** The RT variant's rays MUST be claimed through the
  `AdaptiveRayBudget` tiers ([Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract),
  item 6), never outside it.
- **SSS-RT-003.** On the Cornell SSS rung the default realization's image
  MUST match the RT variant's converged image within the tolerance the
  Phase 0 design states; this is the default realization's oracle.
- **SSS-RT-004.** The RT variant MAY become the default for close-up actors
  only through decision Q5.

### 5.6 Observability (`SSS-DX`)

- **SSS-DX-001.** `mat.dump <entity>` MUST report the subsurface class and
  its §3.2 provenance row for every material of the entity.
- **SSS-DX-002.** `render.debug` MUST gain one view (name chosen in the
  design) that shows the subsurface contribution alone, zero on class-`None`
  pixels.
- **SSS-DX-003.** The Cornell oracle MUST gain one SSS rung (a new
  `CornellOracleRung` variant in `byroredux/src/cornell/oracle.rs`,
  selected through `--cornell-oracle`) with a lit `Skin` slab, an adjacent
  class-`None` occluder and an `Eye` probe, gated by a new ignored test in
  `byroredux/tests/cornell_rt_oracle.rs`. At HEAD on 2026-11-01 the oracle
  rungs run only through that test; `scripts/renderer-eval.sh` launches the
  `--cornell` showcase (`scripts/renderer-eval.sh:92,98`), not the oracle.
- **SSS-DX-004.** `scripts/lighting-game-matrix.py` SHOULD capture its
  paired NPC images with the SSS debug view as well as the final view, so
  the per-game acceptance (§7.2) needs no new harness.

### 5.7 Performance and layout (`SSS-PERF`)

- **SSS-PERF-001.** The default realization's GPU time MUST be reported as
  its own GPU timer and published for the bench-of-record actor scenes
  ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)); the budget it
  must stay under is agreed in Phase 0.
- **SSS-PERF-002.** With no `Skin` or `Organic` pixel on screen, the default
  realization SHOULD cost no more than its dispatch overhead, measured on a
  bench-of-record scene without actors.
- **SSS-PERF-003.** Any new per-material field MUST go into `GpuMaterial`;
  `GpuInstance` and `DrawCommand` MUST NOT grow (SSS-NG-004). A size change
  MUST update `gpu_material_size_is_432_bytes` and the `bindings.glsl`
  mirror in the same commit.
- **SSS-PERF-004.** Any new G-buffer attachment MUST be stated in the Phase 0
  design with its VRAM cost at the default render resolution, against the
  [memory budget](../engine/memory-budget.md).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Skin is lit by the game's skin shader, a forward-renderer approximation (Oblivion "Skin" material, FO3/FNV `SHADER_SKIN`, Skyrim/FO4 Skin/Face Tint) — sources in §3.2 | One physical Burley profile per class (SSS-DIF-001) | Per-game skin shader programs; skin-shader ENB-style presets | Faces look different from vanilla → accepted by SSS-NG-001; a broken profile is caught by SSS-DIF-002 and SSS-RT-003 |
| 2 | Soft-lighting wrap is skin's subsurface term (Skyrim: nif.xml SLSF2 bit 25 `Soft_Lighting`; FO4: Material Editor "Subsurface Lighting", BGSM v < 8) | On `Skin`, wrap is replaced by diffusion if Q2 so decides (SSS-TRN-002) | The wrap width on skin | Double-counted or lost terminator softness → SSS-TRN-002 with Q2; the Cornell rung measures both |
| 3 | Eyes reflect a cube and do not refract (nif.xml `Eye Cubemap Scale`, reflection centres) | Iris refracts behind a cornea (SSS-EYE-001) | Nothing authored | Iris drift at grazing angles on meshes whose UVs assume no refraction → SSS-EYE-002, SSS-EYE-006 |
| 4 | The `_sk` map drives Skyrim's skin-tint diffusion effect (Beyond Skyrim, *NetImmerse Format/Texture Slots*, slot 2) | Stays inert until its encoding is verified (SSS-CLS-009) | — | Skyrim skin lacks its authored scatter tint until Phase 0 closes S8 → SSS-CLS-009 |
| 5 | Surfaces with no skin signal get no skin shading | Same: no keyword classification (SSS-CLS-006) | — | Wax, fruit, organics on Oblivion–FO4 stay `None`, which narrows the ROADMAP row's "wax / fruit / soft organic" promise → decision Q3 |
| 6 | BGSM *thick object* tells the original shader how to shadow translucency (Material Editor tooltip, §3.2 S12) | Redux also reads it as class `Organic` (S12) | — | A thick translucent non-organic (ice, glass-like props) gains diffusion → the census lists every S12 hit (SSS-CLS-008) before the arm ships; no in-scope vanilla title authors it (S12) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)
and the strength of each title's authored signal. Every per-game difference
is a translate-step arm (§3.2); the renderer has none.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. Actors spawn (`byroredux/src/npc_spawn/`); the signal is a material-property *name* (S1, S2) that needs a new capture and a census first |
| Fallout 3 | yes | Wave 1, with FNV: same shader property and the same signal rows (S3–S5) |
| Fallout: New Vegas | yes | Wave 1. Engine reference title; `SHADER_SKIN` is already parsed (S3) |
| Skyrim (LE / SE / AE) | yes | Wave 1. Shader types already reach the canonical `Material` (S6, S7); LE and SE share the NIF signals |
| Fallout 4 | yes | Wave 2. Signals split between NIF flags and shader types (S10) and BGSM v2 (S11, S13); needs the BGSM precedence rule (SSS-CLS-005) |
| Fallout 76 | no | Parse-only: no cell loads, so no actor is ever shaded ([compatibility matrix](../../ROADMAP.md#compatibility-matrix); [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail), Tier 3); the S12 and S14 arms MAY be written with the FO4 arms but carry no acceptance |
| Starfield | yes | Wave 3. Actors spawn on the face-bone rig (`humanoid_skeleton_path`, `byroredux/src/npc_spawn/skeleton_paths.rs`); `UseSSS` needs its colour source verified first (S15) |

### 7.2 Acceptance

Every scenario runs headless. Game data is the user's local install; nothing
is checked in.

| # | Scenario | Driver | Data | Pass rule |
|---|---|---|---|---|
| A1 | Translate arms | New `cargo test -p byroredux --bin byroredux` cases in `material_translate.rs`, one per §3.2 class-bearing row (e.g. `subsurface_class_from_fo3_shader_skin`, `subsurface_class_from_skyrim_skin_tint`, `subsurface_class_from_bgsm_facegen`) | None (synthetic `ImportedMaterial`) | Each row yields its class and provenance; a signal-free material yields `None` |
| A2 | No renderer branch | The SSS-CLS-007 source-shape test (new) | None | No subsurface selection reads a source shader type, game or source flag |
| A3 | Layout | `gpu_material_size_is_432_bytes` (renamed with the size if it changes) and `gpu_material_field_offsets_match_shader_contract` | None | Pass |
| A4 | Census | `subsurface_signal_census` (new) per game | Each in-scope game's ESM plus its mesh and material archives | Every actor skin shape (§2) classified; any `None` among them is listed with its file and missing signal, and the list is empty at the wave's exit |
| A5 | Diffusion oracle | New SSS rung test in `byroredux/tests/cornell_rt_oracle.rs` (`cargo test --release -p byroredux --test cornell_rt_oracle -- --ignored`, RT-capable runner) | None (Cornell is self-contained) | SSS-DIF-002, SSS-DIF-006 and SSS-DIF-007 tolerances met (Phase 2); SSS-RT-003 added once the RT variant exists (Phase 4) |
| A6 | Neutral regression | Existing `cornell_rt_oracle` tests and `golden_frames` test, both `--ignored` on an RT-capable runner; `scripts/renderer-eval-fnv.sh` as a review capture | FNV install for the review capture only | SSS-DIF-004 holds (both tests pass, no re-bless) |
| A7 | Real actors | `scripts/lighting-game-matrix.py` per game (its `SCENES` table), then on the held engine: `byro-dbg` → `mat.dump <actor head entity>` and the SSS `render.debug` view | Each in-scope game's install | `mat.dump` reports `skin` / `eye` with the expected §3.2 row; the SSS view is non-zero on actor skin and zero on the room |
| A8 | Eye refraction | Cornell SSS rung `Eye` probe at two view angles, in the A5 test | None | Iris sample offset matches the cornea model's analytic offset within the design's tolerance |
| A9 | Cost | Bench-of-record runs per [How to run a bench](../../ROADMAP.md#how-to-run-a-bench), same-machine control, pinned camera | Skyrim SE `WhiterunBanneredMare`, FNV `GSProspectorSaloonInterior` | SSS-PERF-001 budget met; SSS-PERF-002 on a control without actors |

---

## 8. Metrics

One home for the numbers: the M56 row of ROADMAP's Tier 8 table, and the
bench-of-record for cost. This section defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| Skin classification coverage | actor skin shapes (§2) classified `Skin` ÷ actor skin shapes found by the census | per game |
| Signal provenance | materials classified through each §3.2 row | per game, per row |
| Diffusion oracle error | default-realization vs RT-variant image error on the SSS rung, in the design's metric | per renderer revision |
| Energy ratio | slab diffuse reflectance with diffusion ÷ without | per renderer revision |
| SSS GPU time | the default realization's GPU timer, median over the bench window | per bench-of-record scene |

---

## 9. Delivery sequence

M56's milestone dependencies, M41 (actors spawn) and R1 (`MaterialTable`,
[risk-reducers](../../ROADMAP.md#risk-reducers-r1r7-2026-04-22)), are met at
HEAD on 2026-11-01. Phases are ordered by what they unblock: classification
first, because every later phase measures on classified actors.

**Phase 0 — Census, verify list and design (no engine code).** Write
`docs/engine/subsurface-scattering.md` (realization, attachments, profile
constants, eye model, RT variant). Write the census example
(SSS-CLS-008; a tool, not engine code) and run it over every in-scope game.
Close every **Phase 0: verify** item: the actor-skin record list per game
(§2), Oblivion eye names (S2), the `_sk` map's encoding (S8),
`Lighting Effect 1/2` (S9), Starfield `UseSSS` and its colour (S15), the
cornea model's published source (§2, SSS-EYE-001), the eye iris mask
(SSS-EYE-002), cornea geometry (SSS-EYE-006), the cornea constants
(SSS-EYE-003). Resolve Q1–Q6 and agree the SSS-PERF-001 budget. Exit:
design checked in; census table per game checked in with zero unexplained
actor-skin shapes in its `None` list; every verify item closed or filed as
an issue.

**Phase 1 — Canonical class, Wave 1 (FNV, FO3, Skyrim).** The class field,
the S3–S7 arms (S8 stays inert per SSS-CLS-009), `mat.dump` reporting, the
no-branch test (SSS-CLS-001–003, SSS-CLS-006, SSS-CLS-007, SSS-DX-001). No
visual change. Exit: A1–A4 pass for the Wave 1 games; skin classification
coverage published.

**Phase 2 — Diffusion.** The default realization, the SSS debug view, the
Cornell rung, the Q2 wrap rule (SSS-DIF-*, SSS-TRN-*, SSS-DX-002–004,
SSS-PERF-001–004). Exit: A5, A6, A7 (Wave 1) and A9 pass; SSS GPU time
published in the bench-of-record.

**Phase 3 — Eyes.** Cornea refraction and the Q4 caustic route
(SSS-EYE-*). Exit: A8 passes; A7 reports `eye` for every Wave 1 actor's
eyes.

**Phase 4 — RT reference variant.** SSS-RT-001–003, and SSS-RT-004 if Q5
chooses it. Exit: diffusion oracle error published and inside the design
tolerance.

Phase 2's default realization is checked against an analytic expectation
(SSS-DIF-002, SSS-DIF-006) until Phase 4 lands; SSS-RT-003 closes the
oracle loop afterwards.

**Phase 5 — Wave 2 and Wave 3.** Oblivion (SSS-CLS-004), FO4
(SSS-CLS-005), Starfield (SSS-CLS-010), each title's arms followed by its
A4 and A7 runs. Exit: skin classification coverage and A7 pass published for
Oblivion, FO4 and Starfield.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option. The
model itself is not reopened: ROADMAP's M56 row already prefers the Burley
profile.

1. **Q1 — Default realization of the Burley profile.** Recommended:
   screen-space Burley over diffuse lighting with depth-aware sampling
   ([Golubev 2018](https://www.advances.realtimerendering.com/s2018/Efficient%20screen%20space%20subsurface%20scattering%20Siggraph%202018.pdf)),
   because it serves direct and indirect diffuse in one pass at a cost
   independent of light count. Alternative A: ray-traced disc probes as the
   default — consequence: per-pixel rays on every skin pixel competing for
   the `AdaptiveRayBudget`, but no G-buffer change. Alternative B:
   separable SSS ([Jimenez et al. 2015](https://graphics.unizar.es/papers/Separable-Subsurface-Scattering.pdf))
   — consequence: cheaper, but not the Burley profile ROADMAP prefers, so the
   M56 row changes. The unscheduled draft
   [RENDERING-PLAN.md](../../RENDERING-PLAN.md) (Stage 4) orders the work
   the same way (screen-space first, a traced variant as the follow-up); it
   decides nothing and is cited only as consistent.
2. **Q2 — Soft-lighting wrap on `Skin`.** Recommended: diffusion replaces
   the wrap on `Skin` (both model light leaving the surface away from where
   it entered; keeping both double-counts). Alternative A: keep the wrap
   stacked under diffusion — consequence: authored terminator width
   preserved, but skin brightens past the energy check of SSS-DIF-002.
   Alternative B: drop the wrap but feed its authored width
   (`lightingEffect1` / `subsurfaceRolloff`) into the profile as a per-material
   scale on the canonical MFP, as RENDERING-PLAN.md Stage 4 sketches ("the
   existing `subsurface_rolloff` / `SLSF2_Soft_Lighting` flags become
   inputs") — consequence: authored variation survives, but the mapping
   from a wrap width to a distance has no source and would be tuned, which
   SSS-NG-001 and SSS-DIF-003 exclude unless Phase 0 finds one.
3. **Q3 — Organics with no authored signal (wax, fruit, foliage on
   Oblivion–FO4).** Recommended: authored signals only; these stay `None`.
   Consequence: the ROADMAP M56 row's "skin / wax / fruit / soft organic
   materials" narrows to skin and eyes on every in-scope title, because the
   only `Organic` producers are FO76's BGSM v ≥ 8 suite (out of scope) and
   Starfield's unverified `UseSSS` (§3.1); approving this recommendation
   changes the row's promise (README T1) and the row is edited to match.
   Alternative: a path-keyword classifier with its own provenance tag, on
   the precedent of `classify_pbr_keyword`
   (`crates/core/src/ecs/components/material.rs:1209`) — consequence: the
   row's promise holds, at the price of a heuristic the no-guessing policy
   then has to bound with a census.
4. **Q4 — Eye caustic.** Recommended: reuse `caustic_splat.comp` only if
   Phase 0 finds separate cornea geometry (SSS-EYE-006); otherwise ship
   refraction without a caustic. Consequence: if no title ships cornea
   geometry, the ROADMAP row's "Eyes get cornea refraction + caustic"
   loses its caustic and the row is edited to match. Alternative: an
   analytic iris-caustic term inside the eye branch — consequence: a second
   caustic mechanism, which the ROADMAP row's "reused, not duplicated" rules
   out, so the row changes the other way.
5. **Q5 — The RT variant's consumer.** Recommended: ship it as a
   runtime-selectable reference (SSS-RT-001) and leave "cinematic mode" to
   the milestones that own it: M51
   ([path-tracing-reference-mode](path-tracing-reference-mode.md)) names a
   photo / cinematic user, and M58
   ([reference-post-process](reference-post-process.md#10-decisions-required),
   its Q6) recommends that no separate cinematic mode exist. Alternative A:
   add a cinematic toggle in M56 — consequence: a new CLI / settings
   contract (README T2) owned by a rendering milestone, contradicting M58's
   Q6 recommendation. Alternative B: drop the RT variant — consequence: the
   default realization loses its oracle (SSS-RT-003) and the ROADMAP row's
   "optional RT-traced SSS variant" is withdrawn.
6. **Q6 — Skin profile parameters.** Recommended: one canonical skin MFP
   per channel from published measured skin data (candidate: Jensen,
   Marschner, Levoy & Hanrahan, *A Practical Model for Subsurface Light
   Transport*, SIGGRAPH 2001, [doi:10.1145/383259.383319](https://doi.org/10.1145/383259.383319),
   whose `skin1` measurement the Burley course notes fit with the
   normalized-diffusion profile, Fig. 8), with variation coming from albedo
   and authored tint maps. Alternative: per-race profiles — consequence:
   needs data no game authors, so it would be tuning, which SSS-NG-001
   excludes.

---

## 11. Document map

| Need | Go to |
|---|---|
| The translate step and its converged material contract | [nifal.md §3](../engine/nifal.md#3-materials--the-reference-realisation) |
| The `_sk` parking note and per-game flag vocabularies | [nifal.md § shader flags](../engine/nifal.md#shader-flags--texture-sets--effect-shaders--converged-surveyed-2026-06-02) |
| Canonical material history and the no-per-game-shader rule | [material-abstraction.md](../engine/material-abstraction.md#3-the-canonical-contract-target-state) |
| `GpuMaterial` fields and the struct-sync contract | [shader-pipeline.md](../engine/shader-pipeline.md#gpumaterial--432-bytes-ssbo-set-1-binding-13) |
| G-buffer attachments | [shader-pipeline.md](../engine/shader-pipeline.md#g-buffer-layout) |
| Units, visibility, ray budget | [physical-lighting-backbone.md](../engine/physical-lighting-backbone.md#runtime-contract) |
| Cornell captures and the A/B workflow | [renderer-evaluation.md](../engine/renderer-evaluation.md#current-scope-and-next-cases) |
| Cornell oracle and golden-frame gates | `byroredux/tests/cornell_rt_oracle.rs`, `byroredux/tests/golden_frames.rs` |
| Unscheduled research order for skin (draft) | [RENDERING-PLAN.md](../../RENDERING-PLAN.md) Stage 4 |
| Sibling promises: hair, layering, wet skin, path tracing, post-process | [hair-shading](hair-shading.md), [material-decal-layering](material-decal-layering.md), [wet-surface-accumulation](wet-surface-accumulation.md), [path-tracing-reference-mode](path-tracing-reference-mode.md), [reference-post-process](reference-post-process.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| VRAM ceilings | [memory-budget.md](../engine/memory-budget.md) |
| Smoke-test index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state, bench-of-record and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: BGSM translucency suite re-attributed to FO76 (S12), SLSF2 lobes Skyrim-only (S9), FO4 shader-type path (S10), Beyond Skyrim `_sk` citation corrected (S8), dead Burley link replaced, eye-refraction source made a verify item, Cornell oracle driver and golden-frame tolerances corrected (SSS-DIF-004/007, SSS-DX-003), actor-skin denominator defined, ROADMAP-row consequences in Q3/Q4/Q5, Q2 alternative B, sibling PRDs linked | claude: revise |
