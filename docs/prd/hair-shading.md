# Hair and Fur Shading — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (HEAD corrections to the raster blend rule, ray flags, FSR
mask and Skyrim/FO4 hair spawn path; the missing FO4 `F4SF1` `Hair` bit;
corrected record-signal citations; split multi-observable requirements;
alignment with the M56 subsurface class; sibling links). Nothing below is
built as a result of this document yet. Statements about how the
original games author hair, about file formats and about the published
hair models cite external sources (niftools `nif.xml`, UESP and the
Creation Kit / GECK wikis, the Material Editor source, the papers in
§11.1); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: M57 in [ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon).
The engine shades the hair that the seven target games already ship (alpha
cards and shells skinned to actors' heads) with a fibre scattering model
instead of the surface GGX lobe, and makes hair cast and receive ray-traced
shadows with fractional coverage. In: classifying hair once at the
translation boundary from data each game authors, a three-lobe fibre BSDF
for direct light, the strand direction it needs, stochastic transparency
on shadow rays through hair, and the observability to measure all of it.
Out: generating strand geometry, hair simulation, matching the original
games' hair look, keyword guessing, skin and eyes (M56), anisotropic
specular on non-hair surfaces (M59), and Fallout 76 (§7.1). Hair shadows
are an RT-path promise; on the rasterized fallback hair keeps today's
shadowing.

**Related**: this document states the *promise*; the designs below own the
*how*. The fibre BSDF itself has no design yet: Phase 0 writes
`docs/engine/hair-shading.md` (§9) and registers it in the engine index.

| Mechanism | Owning design |
|---|---|
| Canonical `Material`, translate-once rule, no per-game renderer branch | [NIFAL](../engine/nifal.md#1-the-three-tier-model), [NIFAL § Materials](../engine/nifal.md#3-materials--the-reference-realisation) |
| Convergence history of the material classifier (glass precedent) | [Canonical Material Abstraction](../engine/material-abstraction.md) |
| `GpuMaterial` layout, `material_kind`, `material_flags` | [Shader Pipeline § GpuMaterial](../engine/shader-pipeline.md#gpumaterial--432-bytes-ssbo-set-1-binding-13) |
| Existing Disney diffuse, sheen and anisotropic GGX lobes | [Vulkan Renderer § Disney BSDF](../engine/renderer.md#disney-bsdf) |
| Shadow visibility buckets, alpha-aware blockers, ray allocation | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract) |
| RT noise seed, TAA variance clamp | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| TAA history rules, FSR reactive and composition masks | [Vulkan Renderer § TAA](../engine/renderer.md#taa-m375), [FSR 3.1 plan §1.4](../engine/fsr3-upscaler-integration-plan.md#14-fsr-input-contracts) |
| Deterministic captures, Cornell oracle, A/B workflow | [Renderer Evaluation](../engine/renderer-evaluation.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |
| Sibling canonical material class (skin, eye) and its `mat.dump` provenance format | [Subsurface Scattering PRD § SSS-CLS](subsurface-scattering.md#51-classification-at-the-translation-boundary-sss-cls) (M56) |
| Anisotropic GGX on non-hair content; the hair/anisotropy boundary | [Material & Decal Layering PRD § MDL-ANI](material-decal-layering.md#56-anisotropic-specular-mdl-ani) (M59, its Q5) |
| Offline reference renderer for the fibre model | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) (M51) |
| Fibre BSDF, strand frame, stochastic shadow coverage | `docs/engine/hair-shading.md` — written in Phase 0 |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `HAIR-` prefix.

---

## 1. Product thesis

The owner's framing, from the ROADMAP row: Bethesda-vintage hair plate
meshes look like clay under standard PBR, and correct hair shading is the
difference between "T-pose mannequin" and "T-pose person". M41 spawns NPCs
with skeleton, body, FaceGen head and outfit
([ROADMAP § Status](../../ROADMAP.md#status)), and both spawn arms mount
their hair at HEAD on 2026-11-01: the runtime-FaceGen arm loads the `HAIR`
record's model for Oblivion, FO3 and FNV (`RuntimePhase::Hair` in
`byroredux/src/npc_spawn/resumable/runtime.rs`), and the pre-baked arm for
Skyrim and later loads the per-NPC FaceGen head mesh, falling back to the
`HDPT` head-part meshes when it is missing (`PrebakedPhase::HeadParts`,
`byroredux/src/npc_spawn/resumable/prebaked.rs`, #5408); every dialogue close-up
now puts a hair card in the middle of the frame, lit by a surface model
that was built for walls and bottles.

Hair is not a surface. A fibre reflects a white highlight shifted toward
the root, transmits a coloured glow when back-lit, and returns a second,
coloured highlight from its far wall: the R, TT and TRT lobes of
Marschner et al. 2003, the factorisation every later model (d'Eon et al.
2011, Chiang et al. 2016, Karis 2016) keeps. None of the three is
reachable by a GGX lobe, isotropic or anisotropic. A card of hair is also
not an opaque blocker: light passes through its gaps and its fringe, and a
binary shadow test turns a hair silhouette into a hard cut-out.

The feature is for the player looking at an NPC, and for the renderer's
RT-first promise ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)):
hair shadows are traced, not mapped.

The corollary that shapes every requirement: **hair is a translated class,
never a guess.** A surface is shaded as fibre only when its own game data
says it is hair (a shader type, a shader flag, a material flag, or the hair
record it was loaded through); the fibre model is fed only what that data
already carries (colour, tint, roughness, tangent); and the renderer reads
one canonical class with no knowledge of which game it came from. After
this section a reader can say what the engine will not do: invent strands,
simulate hair, match the original hair shader's look, or call a texture
"hair" because its path says so.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Hair class** | The canonical, game-agnostic classification of a translated material as fibre. Set once in the translate step (§5.1); read by the renderer. Fur is the same class on a creature. |
| **Hair signal** | An *authored signal* in the sense of [subsurface-scattering.md §2](subsurface-scattering.md#2-terminology) that marks a mesh or material as hair: a row of §3.1 whose family is not "not a signal". A hair signal is the only input that sets the hair class. |
| **Signal source** | Which family set the hair class on a material: `shader` (NIF shader type or flag), `material` (BGSM flag), `record` (loaded through a hair `HDPT` or `HAIR` record), or `none`; reported together with the §3.1 row number (`S1`, `S6`, …) as the class's provenance, the same convention as SSS-CLS-002. |
| **Hair card** | An authored polygon strip or shell carrying a hair texture with alpha coverage; what Bethesda games ship instead of strands. The ROADMAP's "hair plate". |
| **Fibre BSDF** | The scattering function evaluated for hair-class surfaces, factored into lobes. |
| **R / TT / TRT** | The three lobes of the Marschner factorisation: reflection off the cuticle; transmission through the fibre; transmission, internal reflection, transmission (Marschner et al. 2003; Karis 2016, slides 7-12). |
| **Longitudinal / azimuthal** | The two factors `M` and `N` of each lobe: scattering along the fibre axis and around it (Chiang et al. 2016, §3.1, Eq. 2). |
| **Strand tangent** | The per-vertex unit direction of the fibre axis on a hair card; the fibre BSDF's frame. |
| **Coverage** | The alpha a hair card resolves at a ray hit (texture alpha × material alpha × vertex alpha, decal-composited), as `rayHitHasCoverage` (`include/ray_hit.glsl`) computes it before its binary test (P5). |
| **Stochastic transparency** | Accepting a ray-surface hit with probability equal to its coverage, so the expectation over samples equals the fractional transmittance (Enderton et al. 2010). |
| **Fibre fallback** | A hair-class fragment that cannot form a strand frame (degenerate tangent) and is shaded by the surface lobe instead. Counted, never silent. |
| **Wave** | A group of games delivered together (§7.1). |

---

## 3. Core model

### 3.1 Hair signals per game

Every hair signal the original data authors, cited, with what the engine
does with it today. The inventory below is the state at HEAD on 2026-11-01;
the Phase 0 census (§9) replaces the "verify" cells with counts and moves
the table into `docs/engine/hair-shading.md`.

| # | Game | Hair signal (external source) | Family | Today |
|---|---|---|---|---|
| S1 | Skyrim LE / SE, FO4 | `BSLightingShaderProperty` shader type 6, "Hair Tint", carrying `Hair Tint Color` (nif.xml `BSLightingShaderType`, `BSLightingShaderProperty`) | shader | Forwarded as `material_kind` 6 by `normalize_shader_type` (`crates/nif/src/import/material/dedicated_shader.rs`); the `materialKind == 6u` branch in `triangle.frag` multiplies albedo by the tint and nothing else. The surface is lit by the same GGX lobe as every other surface. |
| S2 | Fallout 76 | `BSShaderType155` value 5, "Hair Tint"; `BSShaderCRC32` flag `HAIRTINT` (nif.xml) | shader | Type normalised to kind 6 (`fo76_hair_tint_remaps_material_kind_to_skyrim_constant`, `crates/nif/src/import/material/shader_type_data_tests.rs`); the CRC32 constant is defined (`bs_shader_crc32::HAIRTINT`, `crates/nif/src/shader_flags.rs`) with no reader. FO76 is out of scope (§7.1). |
| S3 | Skyrim LE / SE | `SkyrimShaderPropertyFlags1` bit 18 `Hair_Soft_Lighting`, "keeps from going too bright under lights (hair shader only)" (nif.xml) | shader | Not decoded: `skyrim_slsf1` in `crates/nif/src/shader_flags.rs` defines no constant for bit 18. |
| S4 | Skyrim LE / SE, FO4 | `SLSF2` / `F4SF2` bit 21 `Anisotropic_Lighting` (nif.xml; Skyrim's entry is annotated "Hair only?") | shader | Constants defined (`skyrim_slsf2::ANISOTROPIC_LIGHTING`, `fo4_slsf2::ANISOTROPIC_LIGHTING`); not consumed. `translate_material` writes `anisotropic: 0.0` (`byroredux/src/material_translate.rs`, #3613). |
| S5 | Fallout 4 | BGSM `bHair` + `cHairTintColor`; `bAnisoLighting` (Material Editor: fields in `MaterialLib/BGSM.cs`; tooltips "Toggle hair rendering." and "Toggle anisotropic lighting." in `Main.cs`) | material | Parsed as `BgsmFile::hair`, `hair_tint_color`, `aniso_lighting` (`crates/bgsm/src/bgsm.rs`). No consumer: `hair` is referenced nowhere outside the crate, and `aniso_lighting` is listed in the decoded-but-unsunk ledger in `byroredux/src/asset_provider/material/merge.rs` (#2704). **Phase 0: verify** whether FO4 hair NIFs also author S1, so the two signals agree. |
| S6 | Fallout 3, FNV | `BSShaderPPLightingProperty` flags-1 bit 18 `Hair` (nif.xml `BSShaderFlags`) | shader | Surfaced as `ImportedMaterial::fallout_hair` (#6130; `fo3nv_hair_bit_is_surfaced_on_the_material`). Its only consumer is `bake_fallout_hair_tint` (`byroredux/src/npc_spawn/resumable/runtime.rs`), which turns the tint mask into vertex colour at spawn; it does not reach the canonical `Material`. |
| S7 | Skyrim, FO4 | `HDPT` `PNAM` part type: 3 Hair, 4 Facial Hair, 6 Eyebrows (UESP `Skyrim Mod:Mod File Format/HDPT`; the SKSE function page CK wiki `GetType - HeadPart` gives the same values as Hair, Beard, Brows). FO4: the FO4 CK wiki `HeadPart` says only "a facial feature such as hair, a beard, or a scar"; **Phase 0: verify** FO4's `PNAM` numbering against xEdit's `wbDefinitionsFO4.pas`. | record | Decoded as `HdptRecord::part_type` (`crates/plugin/src/esm/records/misc/character.rs`, #5408). Used only to assemble the head on the pre-baked arm's FaceGen-miss fallback (`prebaked.rs`, `HEAD_PART_TYPE_FACE`); on the common path the hair arrives inside the per-NPC FaceGen head mesh, where no record is attached to the hair shape. Not used for material classification. |
| S8 | Oblivion, FO3, FNV | The `HAIR` record's model (`MODL`): UESP `Oblivion Mod:Mod File Format/HAIR` (`MelModel()`); GECK wiki `Hair` ("Art file: Filename of 3D model (.NIF) used for hair") | record | Parsed as `HairRecord`; the runtime arm resolves the FaceGen recipe's hair form through `index.hair` and loads its `model_path` in `RuntimePhase::Hair` (`runtime.rs`). Not used for material classification. |
| S9 | Oblivion | `HairShaderProperty`, a base-only `BSShaderProperty` subclass (nif.xml, `versions="#BETHESDA#"`) | shader | Parsed as `BSShaderPropertyBaseOnly`; only `env_map_scale` is read (`apply_base_only_shader_property`, `crates/nif/src/import/material/legacy_properties.rs`). **Phase 0: verify** which games' vanilla hair meshes author it; nif.xml does not say. |
| S10 | Starfield | **Phase 0: verify** — no hair signal is known in the `.mat` / CDB material data the engine reads. | — | No hair classification. |
| S11 | Fallout 4 | `Fallout4ShaderPropertyFlags1` bit 18 `Hair` (nif.xml `F4SF1`) | shader | Constant defined (`fo4_slsf1::HAIR`, `crates/nif/src/shader_flags.rs`); no reader. **Phase 0: verify** whether vanilla FO4 hair NIFs set it, and whether it agrees with S1 and S5. |
| — | all | A texture path containing `hair` | not a signal (keyword) | `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs`) returns roughness 0.6, metalness 0 for it. This sets PBR scalars only and is not a hair signal under this PRD (HAIR-CLS-006). |
| — | Skyrim | Dismember partition `SBP_31_HAIR` / `SBP_131_HAIR` / `SBP_141_LONGHAIR` (nif.xml `BSDismemberBodyPartType`) | not a signal (partition) | A visibility slot, not a material: nif.xml lists slot 31 as also "Far(Dragon), Mask2(Dragonpriest)" and slot 131 "use on hoods", so it does not identify a hair surface. |

### 3.2 Rendering path at HEAD

The paths a hair card takes today, at HEAD on 2026-11-01. Each row is what
a requirement in §5 changes, or deliberately keeps.

| # | Path | Today | Requirement |
|---|---|---|---|
| P1 | Direct specular | Isotropic GGX. The anisotropic GGX lobe (`distributionGGXAniso` in `include/pbr.glsl`, branch in `include/lighting.glsl`) runs only when `mat.anisotropic > 0`, which no translated content sets (`material_translate.rs`, #3613); reachable by `mat.set` only. | HAIR-BSDF |
| P2 | Strand frame | `Vertex` carries a tangent; legacy meshes take it from the `NiBinaryExtraData` "Tangent space" blob or synthesise it from UVs (`crates/nif/src/import/mesh/tangent.rs`, nifly `CalcTangentSpace`). Which UV axis follows the strand on a hair card is not known. | HAIR-TAN |
| P3 | Raster coverage | `alphaComparePass` discard for alpha-tested materials (`triangle.frag`). For alpha-blended fragments the blend output depends on the kind: `coverageOnlyBlend` (`triangle.frag`, the `finalAlpha` block before the glass branch) replaces texture alpha with `materialAlpha` for every blended non-glass, non-decal material except kind 6. Skyrim and FO4 HairTint therefore blend by texture alpha, while FO3, FNV and Oblivion hair (no kind 6) blend at material alpha, so their texture alpha acts only through the test. | HAIR-TMP-001 |
| P4 | Shadow rays | Every BLAS is built `GeometryFlagsKHR::OPAQUE` (`blas_static.rs`, `blas_skinned.rs`) and every shadow-transport ray query uses `gl_RayFlagsOpaqueEXT` (`include/shadow_transport.glsl`; the one `gl_RayFlagsNoOpaqueEXT` query in the tree is the volumetric window-glass portal test in `volumetrics_inject.comp`). Alpha is handled by re-tracing past uncovered hits up to `MAX_ALPHA_SKIP_LAYERS` (8) layers in `traceShadowTransmittanceDetailed` (`include/shadow_transport.glsl`); the opaque walk then ends and the segment proceeds as unoccluded by opaque geometry. | HAIR-SHD-003 |
| P5 | Shadow coverage | `rayHitHasCoverage` (`include/ray_hit.glsl`) is binary: an alpha-tested texel blocks or not; an alpha-blended, non-glass texel blocks fully at alpha ≥ 1/255. A hair fringe at alpha 0.3 is a full blocker. | HAIR-SHD-001 |
| P6 | Shadow bucket | Blended actor draws keep `VISIBILITY_LAYER_DYNAMIC_ACTOR` (`shadow_mask_for_instance`, `crates/renderer/src/vulkan/acceleration/predicates.rs`), so hair stays a shadow caster. | HAIR-SHD-005 keeps it |
| P7 | Transmission lobe visibility | The transmission-lobe shadow ray skips the receiver's own instance (#5249, `MAX_TRANSMISSION_SELF_SKIPS`, `shadow_transport.glsl`). | HAIR-SHD-006 |
| P8 | Upscaler masks | Alpha-blended fragments write `min(finalAlpha, 0.9)` to the FSR reactive mask (`outFsrReactive` at the end of `triangle.frag`), where `finalAlpha` is the P3 blend output: texture-alpha coverage for kind 6, `materialAlpha` for other blended hair. Fragments of the opaque pipeline (alpha-tested cutouts) write 0. TAA rejects history on mesh-ID mismatch ([§ TAA](../engine/renderer.md#taa-m375)). | HAIR-TMP-002, HAIR-TMP-003 |

### 3.3 Classification outcomes

Every material ends the translate step in exactly one outcome, reported by
`mat.dump` (HAIR-OBS-001). The outcome is independent of, and reported
beside, the subsurface class of
[subsurface-scattering.md §3.1](subsurface-scattering.md#31-the-subsurface-classes)
(HAIR-CLS-010):

| Outcome | Meaning | Shaded by |
|---|---|---|
| `fibre` | Hair class set from a signal of §3.1 | Fibre BSDF |
| `fibre-fallback` | Hair class set, but the fragment has no usable strand tangent | Surface lobe; counted (HAIR-TAN-003) |
| `surface` | No hair signal | Unchanged surface path |

---

## 4. Goals and non-goals

### 4.1 Goals

- **HAIR-G-001 — Every authored hair surface is fibre.** In each in-scope
  game of the current wave, the share of hair-signal meshes in the Phase 0
  census that translate to the hair class is 100 %, and no mesh without a
  signal does.
- **HAIR-G-002 — Hair has the three lobes.** On the Cornell hair probe and
  on the Wave 1 close-up captures, a back-lit hair card shows a TT
  contribution and a front-lit card shows separated R and TRT highlights;
  each is measurable by toggling it off.
- **HAIR-G-003 — Hair shadows are fractional.** Shadow transmittance
  through a stack of hair cards converges to the analytic product of their
  coverages.
- **HAIR-G-004 — No new shimmer.** Hair-pixel temporal noise under TAA and
  FSR on a still camera is no worse than the Phase 0 baseline.
- **HAIR-G-005 — Bounded cost.** The GPU cost delta of the feature on the
  Wave 1 close-up bench is published and within the budget Phase 0 sets.

### 4.2 Non-goals

- **HAIR-NG-001.** Strand geometry, grooms or card-to-strand conversion.
  The games ship cards; the keep-list rule "parse data, don't emulate
  engine" applies, and no hair data exists to build strands from.
- **HAIR-NG-002.** Hair motion or simulation. Hair bones animate as
  authored; physics belongs to [PHYSAL](../engine/physal.md).
- **HAIR-NG-003.** Reproducing the original games' hair shaders. Per-pixel
  parity with Bethesda's look is anti-scope
  ([ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope)); the
  original shaders are reference, not target.
- **HAIR-NG-004.** Classifying hair from texture, mesh or node names. The
  no-guessing rule; the `classify_pbr_keyword` hair arm keeps its current
  role (PBR scalars) and is not a hair signal.
- **HAIR-NG-005.** Anisotropic specular for non-hair surfaces (brushed
  metal, satin, vinyl). M59 owns it through the sibling
  [material-decal-layering.md](material-decal-layering.md#56-anisotropic-specular-mdl-ani),
  whose Q5 recommends that M59 drive only non-hair materials; this PRD
  states that hair-class surfaces do not use that lobe (HAIR-BSDF-009, Q1).
  The ROADMAP M59 row still lists "hair" among its anisotropic targets; the
  two PRDs move it here.
- **HAIR-NG-006.** Skin subsurface scattering and eyes. M56, through the
  sibling [subsurface-scattering.md](subsurface-scattering.md).
- **HAIR-NG-007.** Changing hair colour or tint. The tint paths that ship
  (kind 6's albedo multiply, #2602's identity default, FO3/FNV's tint-mask
  bake #6130) are inputs to the fibre absorption, not things this PRD
  revisits.
- **HAIR-NG-008.** Order-independent transparency for the raster pass.
  That is M38.2's, filed "if/when a real workload demands it" in the
  retired M38 row
  ([roadmap history](../archive/roadmap-history.md)); Q4 decides whether
  hair is that workload.

---

## 5. Requirements by area

### 5.1 Classification (`HAIR-CLS`)

- **HAIR-CLS-001.** The translate step MUST set the hair class on the
  canonical `Material` for every mesh that carries a hair signal of §3.1,
  and the renderer MUST select the fibre BSDF from that class alone. Today
  no canonical hair class exists: kind 6 drives a tint multiply only, and
  the FO3/FNV bit stops at `ImportedMaterial`. The GPU carrier is Q2; it
  MUST NOT be `material_kind` 6 for non-Skyrim sources (HAIR-CLS-008).
- **HAIR-CLS-002.** A Skyrim or FO4 `BSLightingShaderProperty` of shader
  type 6 (S1) MUST translate to the hair class with source `shader`.
- **HAIR-CLS-003.** A Fallout 4 material whose BGSM sets `bHair` (S5) MUST
  translate to the hair class with source `material`, whether or not the
  NIF also authors S1.
- **HAIR-CLS-004.** A FO3 or FNV `BSShaderPPLightingProperty` with
  flags-1 bit 18 (S6) MUST translate to the hair class with source
  `shader`. The existing tint-mask bake MUST keep running before the
  colour reaches the fibre absorption (HAIR-BSDF-003).
- **HAIR-CLS-005.** A mesh loaded through a hair `HDPT` (S7) or a `HAIR`
  record (S8) that carries no shader or material signal SHOULD translate
  to the hair class with source `record`, applied in the NPC spawn's
  translate step, not in the renderer. Which `HDPT` types count is Q3.
  On Skyrim and FO4 the `record` source can apply only on the pre-baked
  arm's head-part fallback (S7); the FaceGen head mesh's hair shapes rely
  on S1, S5 or S11. **Phase 0: verify** that the hair NIFs of Oblivion
  carry no shader-level signal, so `record` is the only one available
  there.
- **HAIR-CLS-006.** A path, texture name, node name or editor ID MUST NOT
  set the hair class. A census test MUST pin that a mesh whose texture
  path contains `hair` but carries no signal translates to `surface`.
- **HAIR-CLS-007.** Creature fur MAY take the hair class only through a
  signal of §3.1 on the creature's own mesh or material (Q5).
  **Phase 0: verify** whether any vanilla creature mesh authors S1, S5 or
  S6.
- **HAIR-CLS-008.** A per-game difference in how a signal is read (bit
  position, record type, BGSM version) MUST live in the translate step;
  the GLSL MUST NOT gain a branch on the source game or on the signal
  source.
- **HAIR-CLS-009.** A Fallout 4 `BSLightingShaderProperty` with `F4SF1`
  bit 18 `Hair` set (S11) MUST translate to the hair class with source
  `shader`, unless the Phase 0 census shows the bit on non-hair vanilla
  meshes, in which case the design records it as not a signal and this ID
  is withdrawn in the next revision.
- **HAIR-CLS-010.** The hair class MUST be its own field on the canonical
  `Material`, independent of the subsurface class of SSS-CLS-001, and the
  Phase 0 census MUST report the count of materials that carry both a hair
  signal and a subsurface signal (expected 0); neither class MAY be
  derived from the other.

### 5.2 Fibre BSDF (`HAIR-BSDF`)

- **HAIR-BSDF-001.** For direct light, a `fibre` fragment MUST evaluate a
  BSDF factored into R, TT and TRT lobes, each a product of a
  longitudinal and an azimuthal term (Marschner et al. 2003; Chiang et al.
  2016 Eq. 1-2), in place of the surface GGX lobe. Today hair-class
  surfaces do not exist and kind 6 is lit by GGX (P1). The model family is
  Q1.
- **HAIR-BSDF-002.** The fibre's index of refraction MUST default to 1.55,
  the value Karis 2016 (slide 26) gives for human hair, and MUST be a
  named constant in the generated Rust→GLSL constants contract, not a
  literal.
- **HAIR-BSDF-003.** Fibre absorption MUST be derived from the material's
  resolved colour (albedo × the tint the translate step already applies)
  through a published colour-to-absorption mapping — Chiang et al. 2016
  §4.2, Eq. 9, maps a multiple-scattering colour `C` and azimuthal
  roughness to the absorption coefficient — and MUST NOT require any new
  authored field. Because Eq. 9 takes `ln C`, the input MUST be clamped
  into the open interval (0, 1); FO3/FNV's ×2 tint overlay can exceed 1
  (§6, row 2).
- **HAIR-BSDF-004.** Longitudinal and azimuthal roughness MUST come from
  the canonical `roughness` through one documented mapping in the design
  (Chiang et al. 2016 §4.1, Eq. 7-8, is the candidate). **Phase 0:
  verify** which authored scalar carries a hair-meaningful value per game
  (Skyrim glossiness, BGSM smoothness, FO3/FNV glossiness): §3a of
  [material-abstraction.md](../engine/material-abstraction.md) shows the
  FO3/FNV glossiness gradient over-shines non-glass content, so the hair
  mapping MUST be measured on hair, not inherited.
- **HAIR-BSDF-005.** The BSDF MUST NOT create energy: a Rust `f64` mirror
  of the GLSL MUST pass a white-furnace test (`hair_bsdf_white_furnace_never_exceeds_unity`,
  new, beside `anisotropic_ggx_reduces_to_the_isotropic_ndf_at_zero_anisotropy`
  in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`)
  over incident angles and the clamped absorption range of HAIR-BSDF-003.
- **HAIR-BSDF-006.** The BSDF MUST return finite, non-negative values for
  every input in its clamped domain, pinned by a mirror test
  (`hair_bsdf_is_finite_over_the_clamped_domain`, new).
- **HAIR-BSDF-007.** Light scattered between fibres (the volume of a hair
  style) SHOULD be approximated as the design chooses (Karis 2016 uses a
  wrapped diffuse term with absorption from the shadow path length, and
  calls it "a giant artistic hack"; Chiang et al. 2016 §3 fold all lobes
  past TRT into one isotropic fourth lobe), and the design MUST say which,
  with its source.
- **HAIR-BSDF-008.** GI and reflection rays that hit a hair-class surface
  SHOULD shade it with the same BSDF or with a diffuse proxy, and the design
  MUST name which, with the mirror test that pins it.
- **HAIR-BSDF-009.** Hair-class fragments MUST NOT also evaluate the
  anisotropic GGX lobe or the Disney sheen lobe; the three lobes are the
  whole specular response.

### 5.3 Strand frame (`HAIR-TAN`)

- **HAIR-TAN-001.** The fibre BSDF MUST read the strand direction from
  one canonical per-vertex tangent chosen in the translate step; the
  shader MUST NOT pick a UV axis per game.
- **HAIR-TAN-002.** The translate step MUST choose the strand tangent from
  authored data in this order: an authored tangent stream (`BSTriShape`
  tangents, the `NiBinaryExtraData` tangent blob), then a tangent
  synthesised from the UV axis the Phase 0 census finds the strands follow.
  **Phase 0: verify**, per game, whether hair-card strands run along the
  texture's U or V axis, and whether any FO4 hair BGSM authors a
  `flow_texture` that gives a per-texel direction (S5's sibling field,
  parsed but unsampled today).
- **HAIR-TAN-003.** A `fibre` fragment whose tangent, projected onto the
  shading normal, is degenerate MUST shade as `fibre-fallback` (the surface
  lobe) rather than produce NaN, reusing the #3984 projection guard in
  `include/lighting.glsl`.
- **HAIR-TAN-004.** A `render.debug` view (new mode `hair_tangent`) MUST
  draw the strand tangent of hair-class fragments as colour and leave
  other fragments grey, so a wrong axis is visible in one capture.
- **HAIR-TAN-005.** A debug command MUST report the per-frame count of
  `fibre-fallback` fragments (HAIR-TAN-003), the numerator of the fibre
  fallback rate (§8).

### 5.4 Shadows through hair (`HAIR-SHD`)

- **HAIR-SHD-001.** A shadow ray that hits a hair-class surface MUST treat
  it as a blocker with probability equal to its coverage at the hit
  (stochastic transparency, Enderton et al. 2010), instead of the binary
  test of P5. Non-hair surfaces MUST keep today's binary rule.
- **HAIR-SHD-002.** The random number for HAIR-SHD-001 MUST come from the
  existing per-frame RT noise seed
  ([trade-offs §4](../engine/shadow-pipeline-tradeoffs.md#4-24-bit-masked-frame-counter-as-rt-noise-seed))
  decorrelated per layer, so the temporal accumulators average it.
- **HAIR-SHD-003.** The expected visibility through a stack of `n` hair
  cards of coverage `αᵢ` MUST converge to `Π(1 − αᵢ)` within the tolerance
  the Cornell rung (HAIR-OBS-002) declares, for `n` up to
  `MAX_ALPHA_SKIP_LAYERS`.
- **HAIR-SHD-004.** When the walk exhausts `MAX_ALPHA_SKIP_LAYERS` inside
  hair, the segment MUST NOT be reported fully lit by default; the design
  MUST choose the exhaustion value (for example, the transmittance
  accumulated so far). **Phase 0: verify** the hair layer depth on the Wave 1
  close-ups, which decides whether the budget needs a hair-specific value.
- **HAIR-SHD-005.** Hair MUST stay in its render layer's opaque visibility
  bucket (`VISIBILITY_LAYER_DYNAMIC_ACTOR` for actor draws, P6); the
  feature MUST NOT add a visibility bit.
- **HAIR-SHD-006.** The TT lobe's visibility MUST be attenuated by the
  hair cards of the same instance between the shading point and the light
  (other than the shaded triangle), which the own-instance skip of P7
  removes today for transmission rays. A Cornell rung MUST show a
  back-lit stack's TT radiance falling monotonically with layer count.
- **HAIR-SHD-007.** The shadow cost of HAIR-SHD-001 SHOULD be bounded by
  the existing `AdaptiveRayBudget` tiers, without a new budget class.
- **HAIR-SHD-008.** A debug command MUST report the per-frame count of
  shadow rays that exhaust `MAX_ALPHA_SKIP_LAYERS` inside hair
  (HAIR-SHD-004), the numerator of the shadow walk exhaustion metric (§8).

### 5.5 Raster coverage and temporal stability (`HAIR-TMP`)

- **HAIR-TMP-001.** Hair-class raster fragments MUST keep their authored
  alpha test, and the `coverageOnlyBlend` exception that kind 6 has today
  (P3) MUST be keyed on the hair class instead of on kind 6, so every
  hair-class blended fragment outputs texture-alpha coverage. Today FO3,
  FNV and Oblivion hair blend at material alpha. **Phase 0: verify** with
  `dump_alpha` which vanilla hair meshes are alpha-blended rather than
  only alpha-tested, which bounds how many meshes this changes (§6 row 7).
  Any further change to raster transparency for hair is Q4.
- **HAIR-TMP-002.** Hair-class alpha-blended fragments MUST write
  `min(coverage, 0.9)` to the FSR reactive mask, as kind 6 does today
  (P8); a test MUST pin it for the hair class.
- **HAIR-TMP-003.** On a still camera over the Wave 1 close-ups, the
  frame-to-frame luminance variance of hair pixels (selected by the hair
  class, not by colour) under `--upscaler taa` and under FSR SHOULD NOT
  exceed the Phase 0 baseline by more than the tolerance Phase 0 sets from
  two baseline runs. **Phase 0: verify** how often TAA's mesh-ID rejection
  fires on hair edges today, which bounds what this requirement can
  achieve.

### 5.6 Observability and evaluation (`HAIR-OBS`)

- **HAIR-OBS-001.** `mat.dump <entity>` MUST print the classification
  outcome (§3.3) with its signal source and §3.1 row (§2) for every
  material, in the `<field>: <value> (<signal>)` shape SSS-DX-001 uses.
- **HAIR-OBS-002.** The Cornell oracle MUST gain a hair rung: a hair-card
  probe with a known coverage stack, a front light and a back light, with
  analytic expectations for HAIR-SHD-003 and HAIR-SHD-006 and finite,
  non-zero R, TT and TRT terms (`cornell_hair_card_stack_matches_analytic_visibility`,
  `cornell_hair_fibre_lobes_are_finite_and_present`, new, in
  `byroredux/tests/cornell_rt_oracle.rs`).
- **HAIR-OBS-003.** Each fibre lobe MUST be switchable off at runtime for
  A/B (a debug flag or `render.debug` mode), so HAIR-G-002 is measured by
  difference images, not by eye.
- **HAIR-OBS-004.** `mat.set <entity> hair 0|1` MAY toggle the hair class
  on a live material for A/B; it MUST NOT be a way to classify content.
- **HAIR-OBS-005.** The paired close-up captures of
  `scripts/lighting-game-matrix.py` MUST be retained per wave as the
  before/after evidence, with the hair-class view of HAIR-TAN-004 beside
  each final image.
- **HAIR-OBS-006.** The `material_lobe` debug view
  (`crates/renderer/src/vulkan/render_debug.rs`) MUST show `fibre` and
  `fibre-fallback` primary fragments as two categories distinct from every
  existing lobe category.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Skyrim and FO4 hair is drawn by the games' own HairTint pixel shaders. Their lighting terms are not documented; nif.xml documents only the inputs (shader type 6, `Hair Tint Color`, `Hair_Soft_Lighting`, `Anisotropic_Lighting`). | A fibre BSDF from the published literature (HAIR-BSDF-001). | Texture mods that bake a highlight band into hair diffuse to fake an anisotropic look. | Baked highlights double up with the fibre R lobe and look over-shiny. HAIR-OBS-004 gives A/B; the Phase 0 baseline names the mods to watch. |
| 2 | FO3/FNV hair: the shipped HairTint pixel shader lerps from neutral 0.5 to the tint by vertex green and doubles the result. Source: the disassembly of the games' own `shaderpackage003.sdp` (`SM3002.pso` and siblings) recorded with #6130; no published documentation exists. **Phase 0: verify** the disassembly once more against the shipped package before the clamp of HAIR-BSDF-003 is sized on it. | The same bake runs first, then the result feeds absorption (HAIR-CLS-004). | — | The ×2 overlay can push colour above 1, where Chiang Eq. 9's `ln C` breaks. HAIR-BSDF-003 clamps the input. |
| 3 | `Hair_Soft_Lighting` "keeps [hair] from going too bright under lights" (nif.xml, S3). | Not honoured; energy is bounded by the BSDF itself (HAIR-BSDF-005). | — | Hair authored to rely on the clamp may read brighter under strong lights. Q6. |
| 4 | How the original engines treat hair alpha in their shadow passes is not documented in any source in the reference set; Redux does not depend on it. | Stochastic RT coverage (HAIR-SHD-001). | — | Per-frame noise in hair shadows. HAIR-SHD-002 feeds it to the existing accumulators; HAIR-TMP-003 measures the result. |
| 5 | Redux today: an alpha-blended hair fringe is a full shadow blocker (P5). | Fringe blocks by its coverage. | — | Lighter shadows under hair and hats than the current build shows. Expected; reviewed in the HAIR-OBS-005 captures. |
| 6 | Redux today: transmission-lobe shadow rays ignore the receiver's own instance (P7, #5249). | Hair TT is attenuated by its own cards (HAIR-SHD-006). | — | Changing a shared transport function can regress foliage and skin transmission. The change is gated to the hair class and the existing transmission tests MUST stay green. |
| 7 | Redux today: FO3, FNV and Oblivion blended hair blends at material alpha; Skyrim/FO4 HairTint blends at texture alpha (P3). | All hair-class blends use texture-alpha coverage (HAIR-TMP-001). | The kind-6 special case in `coverageOnlyBlend`. | Fringes of legacy hair become translucent and can show sorting artefacts inside one hair mesh. The A6 captures show it; Q4 owns any further raster change. |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the strength of each game's hair signal and the engine tiers
in [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
Every wave reuses the same BSDF and shadow path; only the translate rules
differ.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 3. Only the `HAIR` record (S8) is known as a signal; the NIF-level signal is a Phase 0 verify (S9). NPCs spawn with hair through the resumable runtime. |
| Fallout 3 | yes | Wave 2. Same authored bit as FNV (S6) and the same runtime; adds no new translate rule. |
| Fallout: New Vegas | yes | Wave 1. Engine reference title; S6 is authored on the vanilla hair shaders and already crosses the import boundary (#6130). |
| Skyrim (LE / SE / AE) | yes | Wave 1 for SE (S1 is the most explicit hair signal of any title); LE and AE share the data and join in Wave 2 with no new rule. |
| Fallout 4 | yes | Wave 2. Needs the BGSM `bHair` sink (S5) and the S1/S5 agreement check. |
| Fallout 76 | no | No cell loader and no NPC spawn ([Game Compatibility](../engine/game-compatibility.md#per-game-support-detail)); the S2 normalisation already exists, so FO76 joins when its actors render. |
| Starfield | conditional | Wave 3, only if Phase 0 finds a hair signal in the material data the engine reads (S10). Otherwise out, with the census as the reason. |

### 7.2 Acceptance

A scenario passes when every listed observable holds. Game data is the
user's own installation; nothing is checked in.

| # | Scenario | Runs as | Data | Pass rule |
|---|---|---|---|---|
| A1 | Classification per signal | `cargo test -p byroredux --bin byroredux hair_class` (new, in `material_translate.rs`: `skyrim_hair_tint_sets_hair_class`, `fo3nv_hair_bit_sets_hair_class`, `fo4_hair_flag_sets_hair_class`, `hair_keyword_path_without_signal_has_no_hair_class`, `bgsm_hair_flag_sets_hair_class`), run with the pinned toolchain per CLAUDE.md | Synthetic fixtures | All pass; HAIR-CLS-002, -003, -004, -006, -009 |
| A2 | Classification census | New example `hair_signal_census` in `byroredux/examples/` (it needs the BGSM merge for S5, beside `r5_check_bgsm.rs`), run per game | Vanilla archives | Hair-signal meshes classed = census count; keyword-only meshes classed = 0 (HAIR-G-001); hair-and-subsurface overlap reported (HAIR-CLS-010) |
| A3 | BSDF contract | `cargo test -p byroredux-renderer hair_bsdf` | None | HAIR-BSDF-005, -006 |
| A4 | Cornell hair rung | `cargo test --release -p byroredux --test cornell_rt_oracle -- --ignored --nocapture cornell_hair` | None (RT device) | HAIR-SHD-003, HAIR-SHD-006, HAIR-OBS-002 |
| A5 | Live classification | `BYRO_DEBUG_SERVER=1` release build with `--bench-hold` on the game's `lighting-game-matrix.py` scene, then `byro-dbg`: `mat.dump <hair entity>`, `render.debug hair_tangent`, `screenshot hair_tangent.png` | Wave's games | Outcome `fibre` with the expected source; no grey hair pixels in the tangent view |
| A6 | Close-up pairs | `python3 scripts/lighting-game-matrix.py <game>` before and after, under `xvfb-run -a` when headless | Wave's games | Captures retained (HAIR-OBS-005); lobe-off difference images non-zero for R, TT, TRT (HAIR-G-002) |
| A7 | Temporal stability | Same scene, still camera, `--upscaler taa` then FSR, 64-frame captures at the pinned pose | Wave 1 | HAIR-TMP-003 |
| A8 | Cost | `cargo run --release -- --game <profile> --cell <scene> --bench-frames 300` with pinned `--camera-pos` / `--camera-forward`, against a same-machine control of the pre-feature commit ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)) | Wave 1 | Delta within the Phase 0 budget (HAIR-G-005) |
| A9 | Existing gates | `cargo test -p byroredux-renderer --lib`, the existing `cornell_rt_oracle` rungs, `scripts/check-shader-artifacts.sh` | None | Unchanged results; transmission and alpha-coverage tests green (§6 row 6) |

---

## 8. Metrics

Definitions only; the numbers are published in the M57 row of the
[Tier 8 table](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon)
and, for cost, beside the bench of record.

| Metric | Definition | Granularity |
|---|---|---|
| Classification coverage | hair-signal meshes translated to the hair class ÷ hair-signal meshes in the census | Per game, per signal family |
| False classification | meshes with no signal translated to the hair class (must be 0) | Per game |
| Fibre fallback rate | `fibre-fallback` fragments ÷ hair-class fragments on the A5 scene | Per game, per scene |
| Shadow walk exhaustion | shadow rays ending at `MAX_ALPHA_SKIP_LAYERS` inside hair ÷ shadow rays hitting hair | Per scene |
| Hair temporal variance | mean frame-to-frame luminance variance over hair-class pixels, still camera | Per scene, per upscaler |
| GPU cost delta | main-render GPU ms with the feature − control, same machine and pose | Per Wave 1 scene |
| BSDF energy bound | maximum white-furnace albedo over the tested domain | Per release of the BSDF |

---

## 9. Delivery sequence

M41 (NPCs with hair) has shipped; nothing else blocks Phase 0. M51's path
tracer ([path-tracing-reference-mode.md](path-tracing-reference-mode.md)),
when it exists, is the natural reference for the fibre model and is not a
dependency. M56's classification phase
([subsurface-scattering.md](subsurface-scattering.md)) adds its class field
to the same canonical `Material`; whichever lands first, the second
extends the same `mat.dump` line (HAIR-CLS-010, HAIR-OBS-001). M56 is the sibling for face close-ups; the two share
the A6 captures but not code.

**Phase 0 — Census, baseline and design (no engine code).** Census tooling
is examples and scripts only. Count, per game, every signal of §3.1 on
the vanilla hair, beard and creature meshes, and the alpha test / blend
state of each (`dump_alpha`, `material_dump` and the new
`hair_signal_census` example). Close the **Phase 0: verify** items: S1/S5
agreement on FO4 (S5), S9 and Oblivion's signal (HAIR-CLS-005), creature
fur signals (HAIR-CLS-007), the authored roughness carrier
(HAIR-BSDF-004), the strand axis and FO4 flow maps (HAIR-TAN-002), hair
layer depth (HAIR-SHD-004), TAA mesh-ID rejection on hair (HAIR-TMP-003),
blended versus tested hair (HAIR-TMP-001), FO4's `F4SF1` `Hair` bit (S11)
and `HDPT` numbering (S7), the FO3/FNV HairTint disassembly (§6 row 2),
and Starfield (S10). Capture the A6 and A7 baselines and the A8 control;
set the cost budget and the temporal tolerance. Write
`docs/engine/hair-shading.md` (model, constants with sources, strand
frame, exhaustion value) and register it. Resolve Q1–Q6. Exit: the census
table with counts per game and the design are checked in, the baselines
are retained, and every §10 item carries **Decided**.

**Phase 1 — Classification and observability (Wave 1).** HAIR-CLS-001,
-002, -004, -006, -008, -010; HAIR-OBS-001, -004, -006; the `hair_tangent`
view (HAIR-TAN-004). The renderer still shades the class with the surface lobe.
Exit: A1 passes; A2 reports 100 % coverage and 0 false classification on
Skyrim SE and FNV.

**Phase 2 — Fibre BSDF and strand frame (Wave 1).** HAIR-BSDF-001 to -009,
HAIR-TAN-001 to -003 and -005, HAIR-OBS-002 (BSDF half), HAIR-OBS-003. Exit: A3
passes; the BSDF half of A4 passes; A6 lobe-off differences are non-zero
on Skyrim SE and FNV; A8's delta is published and within budget.

**Phase 3 — Stochastic shadow coverage.** HAIR-SHD-001 to -008,
HAIR-TMP-001 to -003, HAIR-OBS-002 (shadow half). Exit: A4 passes in full;
the exhaustion metric is published; A7 is within tolerance under TAA and
FSR on both Wave 1 games.

**Phase 4 — Waves 2 and 3.** HAIR-CLS-003 and -009 (FO4), HAIR-CLS-005 (record
source, per Q3), FO3 and Skyrim LE/AE through the Wave 1 rules, Oblivion,
Starfield if Phase 0 found a signal, creature fur per Q5
(HAIR-CLS-007). Exit: A2 at 100 % coverage and 0 false classification for
every in-scope game, and A5/A6 retained for each.

---

## 10. Decisions required

1. **Q1 — Which fibre model is the real-time default?** Recommended: a
   real-time approximation of the Marschner R/TT/TRT factorisation in the
   style of Karis 2016 (Gaussian longitudinal terms, single-lobe TRT, fixed
   IOR), parameterised by Chiang et al. 2016's colour (Eq. 9) and roughness
   (Eq. 7-8) mappings so existing albedo and tint drive it; Chiang's full
   model (near-field azimuthal integral, energy-conserving fourth lobe) is
   the offline reference the mirror tests and M51 compare against. Its
   consequence: one cheap direct-light evaluation per light, and a known,
   cited approximation error. Alternative A: Chiang's model in the raster
   pass, which is what the ROADMAP row's "Disney's hair shading model as
   the simpler default" names. Consequence: the row's premise is inverted,
   since Chiang et al. present it as a model "for production path tracing"
   (its title) built for many samples per pixel; its per-light cost is
   higher than the Karis approximation, not lower. Choosing it means
   re-wording the ROADMAP row and budgeting for that cost. With the
   recommendation the row also changes ("Disney's hair shading model as the
   simpler default" becomes "Karis-style approximation, Chiang as the
   reference"). Alternative B: drive the existing anisotropic GGX lobe from
   the S4/S5 enable bits. Consequence: cheapest and already written, but has
   no TT or TRT, so it does not meet HAIR-G-002, and it is the interim that
   [material-decal-layering.md Q5](material-decal-layering.md#10-decisions-required)
   lists as its own alternative; the two PRDs' answers must match.

2. **Q2 — May M57 spend one of the last free `material_flags` bits?**
   `GpuMaterial.materialFlags` has bits 24–31 free at HEAD on 2026-11-01
   (`material_flag::EFFECT_LI_SHIFT` docs, `crates/renderer/src/vulkan/material.rs`);
   M56, M59 and M61 will each want material state in `GpuMaterial` too.
   Recommended: yes, one bit, set by the translate step from the hair class
   (HAIR-CLS-010), orthogonal to `material_kind` and to M56's subsurface
   class. Consequence: kind 6 keeps its Skyrim tint semantics, FO3/FNV and
   record-sourced hair need no fake kind, the flag joins the generated
   constants contract, and seven bits remain for the sibling milestones.
   Alternative A: a new `GpuMaterial` shading-model field shared with M56
   (surface / fibre / the subsurface classes). Consequence: no flag spent,
   but a new `GpuMaterial` field, which under the M59 PRD's lane rule
   ([material-decal-layering](material-decal-layering.md) MDL-GPU-002/003,
   MDL-GPU-008) is one GLSL declaration plus its tests, not a lockstep
   change, and a
   revision of [subsurface-scattering.md](subsurface-scattering.md), whose
   class is a separate field today. Mapping every game's hair to
   `material_kind` 6 is not an alternative: kind 6's tint multiply would
   then run on FO3/FNV hair whose tint is already baked into vertex colour,
   which needs the per-source exception HAIR-CLS-008 forbids.

3. **Q3 — Which head parts count as hair for the `record` source?**
   Recommended: `HDPT` types 3 (Hair) and 4 (Beard), and every
   `HAIR` record; brows (type 6) only if the Phase 0 census shows they are
   alpha cards rather than painted face geometry. Consequence: beards and
   hair shade alike; brows wait for data. Alternative: type 3 and `HAIR`
   only. Consequence: beards keep the surface lobe beside fibre hair on
   the same face, the mismatch this milestone exists to remove. On Skyrim
   and FO4 the answer affects only the head-part fallback (HAIR-CLS-005),
   and FO4 waits for the S7 numbering check.

4. **Q4 — Does raster transparency for hair change in M57?** Recommended:
   no; keep the authored test and blend (HAIR-TMP-001) and limit
   stochastic transparency to shadow rays, which is what the ROADMAP row
   promises. Consequence: no new pass, no risk to TAA history; sorting
   artefacts inside a hair mesh stay as today. Alternative A: stochastic
   (dithered) alpha in raster, resolved by TAA/FSR. Consequence: order
   independence for hair, but TAA's mesh-ID rejection and variance clamp
   fight the dither, and FSR needs the composition mask; it needs its own
   measurement phase. Alternative B: OIT (M38.2). Consequence: hair becomes
   the workload M38.2 waited for, and M57 depends on it.

5. **Q5 — Is creature fur in M57?** Recommended: only where a creature's
   own mesh or material carries a §3.1 signal (HAIR-CLS-007), so fur ships
   when data supports it. Consequence: possibly no fur at all if the
   census finds none. Alternative: fur out of M57 entirely. Consequence: a
   simpler Phase 4, and a later milestone has to repeat the census.

6. **Q6 — Honour `Hair_Soft_Lighting` (S3)?** Recommended: no; record it
   as deviation 3 and rely on the BSDF's energy bound. Consequence: one
   less authored flag to interpret, at the risk of brighter Skyrim hair
   under strong lights. Alternative: map the flag to a reduced R-lobe
   weight. Consequence: an invented constant with no source for its value,
   which the no-guessing rule forbids unless Phase 0 finds one.

---

## 11. Document map

| Need | Go to |
|---|---|
| Translate-once rule and canonical `Material` | [NIFAL](../engine/nifal.md#1-the-three-tier-model) |
| How a material class converged before (glass) | [Canonical Material Abstraction](../engine/material-abstraction.md) |
| `GpuMaterial` fields and flags the class lands in | [Shader Pipeline](../engine/shader-pipeline.md#gpumaterial--432-bytes-ssbo-set-1-binding-13) |
| Existing BSDF lobes | [Vulkan Renderer § Disney BSDF](../engine/renderer.md#disney-bsdf) |
| Shadow transport and visibility buckets | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract) |
| RT noise seed for stochastic coverage | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md#4-24-bit-masked-frame-counter-as-rt-noise-seed) |
| TAA and FSR mask contracts | [Vulkan Renderer § TAA](../engine/renderer.md#taa-m375), [FSR 3.1 plan §1.4](../engine/fsr3-upscaler-integration-plan.md#14-fsr-input-contracts) |
| Captures, Cornell oracle, A/B method | [Renderer Evaluation](../engine/renderer-evaluation.md) |
| Per-game tiers | [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |
| Sibling skin / eye class and `mat.dump` provenance shape | [subsurface-scattering.md](subsurface-scattering.md#51-classification-at-the-translation-boundary-sss-cls) |
| Anisotropic lobe on non-hair content | [material-decal-layering.md](material-decal-layering.md#56-anisotropic-specular-mdl-ani) |
| Path-traced reference for the fibre model | [path-tracing-reference-mode.md](path-tracing-reference-mode.md) |
| M38.2 OIT deferral | [roadmap history](../archive/roadmap-history.md) |
| Fibre model, strand frame, exhaustion value | `docs/engine/hair-shading.md` (Phase 0) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

### 11.1 External sources

| Source | Used for |
|---|---|
| S. R. Marschner, H. W. Jensen, M. Cammarano, S. Worley, P. Hanrahan, "Light Scattering from Human Hair Fibers", ACM TOG 22(3), SIGGRAPH 2003. <https://doi.org/10.1145/882262.882345> | R / TT / TRT factorisation |
| E. d'Eon, G. Francois, M. Hill, J. Letteri, J.-M. Aubry, "An Energy-Conserving Hair Reflectance Model", Computer Graphics Forum 30(4), EGSR 2011. <https://doi.org/10.1111/j.1467-8659.2011.01976.x> | Energy-conserving longitudinal term, arbitrary lobe count |
| M. J.-Y. Chiang, B. Bitterli, C. Tappan, B. Burley, "A Practical and Controllable Hair and Fur Model for Production Path Tracing", Computer Graphics Forum 35(2), Eurographics 2016. <https://media.disneyanimation.com/uploads/production/publication_asset/152/asset/eurographics2016Fur_Smaller.pdf> | Eq. 1-2 factorisation, §3 fourth lobe, §4.1 roughness (Eq. 7-8), §4.2 colour → absorption (Eq. 9) |
| B. Karis, "Physically Based Hair Shading in Unreal", SIGGRAPH 2016 course *Physically Based Shading in Theory and Practice*. <https://blog.selfshadow.com/publications/s2016-shading-course/karis/s2016_pbs_epic_hair.pdf> | R / TT / TRT (slides 7-12), Gaussian longitudinal terms (slide 18), IOR 1.55 (slide 26), wrapped-diffuse multiple scattering, "a giant artistic hack" (slide 39) |
| E. Enderton, E. Sintorn, P. Shirley, D. Luebke, "Stochastic Transparency", I3D 2010. <https://www.cse.chalmers.se/~d00sint/StochasticTransparency_I3D2010.pdf> | Coverage-probability acceptance |
| niftools `nif.xml` (<https://github.com/niftools/nifxml>; local copy `/mnt/data/src/reference/nifxml/nif.xml`) | S1–S4, S6, S9 |
| ousnius, Material Editor, `MaterialLib/BGSM.cs` and `Main.cs` (<https://github.com/ousnius/Material-Editor>; local copy `/mnt/data/src/reference/Material-Editor/`) | S5 |
| UESP, `Skyrim Mod:Mod File Format/HDPT` and `Oblivion Mod:Mod File Format/HAIR` (local dump `/mnt/data/src/reference/uesp-wiki/`) | S7, S8 |
| Creation Kit wiki, `GetType - HeadPart` (SKSE); Fallout 4 CK wiki, `HeadPart`; GECK wiki, `Hair` (local dumps under `/mnt/data/src/reference/`) | S7, S8 |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: HEAD fixes (P3 blend rule, P4 ray flags, P8 FSR mask, Skyrim/FO4 hair spawn path); FO4 `F4SF1` Hair bit (S11, HAIR-CLS-009); record-signal citations (UESP HDPT/HAIR, GECK Hair); HairTint disassembly sourcing; split HAIR-TAN-005, HAIR-SHD-008, HAIR-OBS-006; HAIR-TMP-001/-002 rewritten; M56 class alignment (HAIR-CLS-010); Q2 reframed as the flag-budget call; sibling and archive links; coordinator cross-PRD sync (sibling links and citations) | claude: revise |
