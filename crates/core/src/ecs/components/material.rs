//! Material component — surface properties for rendering.
//!
//! Captures the rich material data from NIF properties (NiMaterialProperty,
//! BSLightingShaderProperty, BSEffectShaderProperty) that was previously
//! discarded during import.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Surface material properties extracted from NIF shader/property blocks.
///
/// SparseSetStorage: most static geometry shares a small set of unique
/// materials; sparse access pattern during rendering.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Material {
    /// Emissive color (RGB, linear). Self-illumination independent of lighting.
    pub emissive_color: [f32; 3],
    /// Emissive intensity multiplier.
    pub emissive_mult: f32,
    /// Provenance of [`Self::emissive_mult`] — disambiguates the three
    /// authoring sources whose "emissive multiplier" fields all flow
    /// into this slot but carry different semantics:
    /// - [`EmissiveSource::Material`]: legacy genuine emissive scalar.
    /// - [`EmissiveSource::Lighting`]: Skyrim+ shader-property scalar.
    /// - [`EmissiveSource::Effect`]: FO4+ effect-shader **diffuse-tint**
    ///   scale (conflated into this slot — semantically not emissive).
    ///
    /// Future renderer paths can pattern-match to drop the conflation;
    /// `BSEffectShaderProperty` surfaces should treat their "emissive"
    /// as diffuse modulation. Today the renderer reads `emissive_mult`
    /// without inspecting the source; this field is data-plumbing only
    /// (#1280 step 4).
    pub emissive_source: EmissiveSource,
    /// Specular highlight color (RGB, linear).
    pub specular_color: [f32; 3],
    /// Specular intensity multiplier.
    pub specular_strength: f32,
    /// Diffuse tint (RGB, linear) from `NiMaterialProperty.diffuse`.
    /// Multiplied into the sampled albedo by the fragment shader.
    /// Default `[1.0; 3]` (no tint) for meshes without an
    /// `NiMaterialProperty` — every BSShader-only mesh on
    /// Skyrim+/FO4 lands here. Audit
    /// `AUDIT_LEGACY_COMPAT_2026-04-10.md` D4-09 / #221.
    pub diffuse_color: [f32; 3],
    /// Ambient color (RGB) from `NiMaterialProperty.ambient`. Modulates
    /// the cell ambient lighting term per material so meshes with
    /// authored ambient response (lit-from-within glass, occluded
    /// alcoves) react correctly to cell ambient. Default `[1.0; 3]`.
    /// See #221.
    pub ambient_color: [f32; 3],
    /// Glossiness / smoothness (higher = tighter highlights).
    pub glossiness: f32,
    /// UV texture coordinate offset [u, v].
    pub uv_offset: [f32; 2],
    /// UV texture coordinate scale [u, v].
    pub uv_scale: [f32; 2],
    /// Material alpha/transparency (0.0 = fully transparent, 1.0 = opaque).
    pub alpha: f32,
    /// Environment map reflection scale (shader type 1).
    pub env_map_scale: f32,
    /// Normal map texture path (if available).
    pub normal_map: Option<String>,
    /// Diffuse texture path (for PBR material classification from path keywords).
    pub texture_path: Option<String>,
    /// BGSM/BGEM material file path (FO4+). When present with no texture_path,
    /// the real textures are inside this material file in the Materials BA2.
    pub material_path: Option<String>,
    /// Glow / self-illumination texture — `NiTexturingProperty` slot 4
    /// on Oblivion/FO3/FNV, or `BSShaderTextureSet` slot 2 on Skyrim+.
    /// Populated on import when the mesh has a dedicated emissive
    /// texture (enchanted weapons, torches, lava). Empty for most
    /// static geometry. See #214.
    pub glow_map: Option<String>,
    /// Detail overlay texture — `NiTexturingProperty` slot 2. Legacy
    /// high-frequency variation layer used by Oblivion terrain and
    /// some clothing. See #214.
    pub detail_map: Option<String>,
    /// Gloss texture — `NiTexturingProperty` slot 3. Per Gamebryo 2.3
    /// `HandleGlossMap(... pkGlossiness)` this feeds the
    /// **glossiness / shininess** (Phong exponent) channel — the
    /// fragment shader modulates per-texel `roughness` from it
    /// (gloss = 1 → authored roughness, gloss = 0 → fully rough).
    /// Enables "polished metal trim on dull leather strap" surfaces
    /// where the lobe shape varies across the mesh, not just the
    /// intensity. See #214 / #704.
    pub gloss_map: Option<String>,
    /// Dark / multiplicative lightmap — `NiTexturingProperty` slot 1.
    /// Baked shadow/grime modulation on Oblivion interior architecture.
    /// Applied as `albedo.rgb *= dark_sample.rgb`. See #264.
    pub dark_map: Option<String>,
    /// Vertex color source mode from `NiVertexColorProperty`. Matches
    /// Gamebryo's `SourceMode` enum:
    ///   * `0` = Ignore (vertex colors disabled)
    ///   * `1` = Emissive (colors drive self-illumination)
    ///   * `2` = AmbientDiffuse (default, colors drive diffuse)
    ///
    /// The NIF importer already honors `Ignore` by not populating the
    /// mesh's vertex color vec. `Emissive` is forwarded here so the
    /// material system can route the data later. See #214.
    pub vertex_color_mode: u8,
    /// Whether the renderer should `discard` fragments whose sampled
    /// texture alpha falls below `alpha_threshold`. Extracted from
    /// `NiAlphaProperty.flags` bit 9 (0x200). Mutually exclusive with
    /// the `AlphaBlend` marker component — the importer prefers
    /// alpha-test over alpha-blend when a material sets both bits.
    /// See issue #152.
    pub alpha_test: bool,
    /// Cutoff threshold for `alpha_test`, in [0, 1]
    /// (`NiAlphaProperty.threshold` divided by 255). Only meaningful
    /// when `alpha_test` is `true`.
    pub alpha_threshold: f32,
    /// Alpha test comparison function from NiAlphaProperty flags bits
    /// 10–12. 0=ALWAYS, 1=LESS, 2=EQUAL, 3=LESSEQUAL, 4=GREATER,
    /// 5=NOTEQUAL, 6=GREATEREQUAL (default), 7=NEVER. See #263.
    pub alpha_test_func: u8,
    /// Raw `BSLightingShaderProperty.shader_type` enum value (0–20
    /// vanilla; 100+ engine-synthesized: `MATERIAL_KIND_GLASS`,
    /// `MATERIAL_KIND_EFFECT_SHADER`). Plumbed through to
    /// `GpuInstance.material_kind` so the fragment shader can branch
    /// on the variant (SkinTint / HairTint / EyeEnvmap / SparkleSnow
    /// / MultiLayerParallax / …). 0 = Default lit — the safe fall-
    /// through for non-Skyrim+ meshes that have no
    /// BSLightingShaderProperty backing. Variant-specific shading is
    /// per-variant follow-up; this field just exposes the data so the
    /// next renderer milestone has something to consume. See #344.
    /// Widened to `u32` per #570 (SK-D3-03) — both ends of the
    /// pipeline (`shader_type` u32 → GPU u32) match now; the
    /// pre-fix `as u8` cast in the importer silently masked any value
    /// ≥ 256.
    pub material_kind: u32,
    /// `NiWireframeProperty` flag (flags=1 enables wireframe rendering).
    /// When true the renderer routes the batch through the
    /// `vk::PolygonMode::LINE` pipeline variant (#869). Falls back to
    /// FILL silently when the device lacks `fillModeNonSolid`.
    /// Default false. Oblivion vanilla ships zero wireframe meshes;
    /// the field exists for FO3/FNV mod content and future debug
    /// overlays.
    pub wireframe: bool,
    /// `NiShadeProperty` flag (flags=0 requests flat shading).
    /// When true the fragment shader replaces the interpolated vertex
    /// normal with the per-face derivative `cross(dFdx(world_pos),
    /// dFdy(world_pos))` so the mesh reads as faceted. Default false.
    /// Used by a handful of Oblivion architectural pieces.
    /// (#869 — flat-shading consumer lands in a follow-up commit.)
    pub flat_shading: bool,
    /// Depth test enabled (`NiZBufferProperty.z_test`). Default true.
    /// Forwarded into the per-batch `vkCmdSetDepthTestEnable` call
    /// in the draw loop. See #398 (OBL-D4-H1).
    pub z_test: bool,
    /// Depth write enabled (`NiZBufferProperty.z_write`). Default true.
    /// `false` is set by sky domes, first-person viewmodels, ghost
    /// overlays, HUD markers, billboarded particles, glow halos —
    /// pre-#398 it was extracted but never reached the GPU, causing
    /// z-fighting against world geometry.
    pub z_write: bool,
    /// Depth comparison function (Gamebryo `TestFunction` enum). 3
    /// (LESSEQUAL) is the Gamebryo default and the value used pre-#398
    /// for every mesh.
    pub z_function: u8,
    /// Per-variant scalar/vector payload from `BSLightingShaderProperty`
    /// Skyrim+ shader types (SkinTint, HairTint, EyeEnvmap, SparkleSnow,
    /// MultiLayerParallax). `None` for the vast majority of materials
    /// (Default lit, Envmap, Glow, Parallax, Decal). Boxed so the
    /// hot-path common case pays 8 bytes for the null pointer instead
    /// of inlining 56 bytes of zero. See #562.
    pub shader_type_fields: Option<Box<ShaderTypeFields>>,
    /// `BSEffectShaderProperty` (Skyrim+) / `BSShaderNoLightingProperty`
    /// (FO3/FNV) view-angle + soft-depth falloff cone. Inline because
    /// the struct is small (5 × f32 = 20 B) and the Option tag fits in
    /// the same alignment slot as the floats. `None` for non-effect
    /// materials. The fragment shader's `material_kind ==
    /// MATERIAL_KIND_EFFECT_SHADER` (101) branch consumes these via
    /// `GpuInstance.{falloff_*, soft_falloff_depth}`. See #620 / #451.
    pub effect_falloff: Option<EffectFalloff>,
    /// Packed `BSEffectShaderProperty` flag bits captured from
    /// `BsEffectShaderData.effect_{soft,palette_color,palette_alpha,lit}`
    /// at importer ingestion. Bit layout matches
    /// `byroredux_renderer::vulkan::material::material_flag::EFFECT_*`
    /// so the renderer OR's this word straight into
    /// `GpuMaterial.material_flags` without per-bit re-encoding.
    /// `0` on every non-BSEffect mesh + on the FO3/FNV
    /// `BSShaderNoLightingProperty` path (which uses the same
    /// `effect_falloff` slot but has no SLSF1/SLSF2 vocabulary).
    /// See #890 / SK-D4-NEW-04.
    pub effect_shader_flags: u32,
    /// #1147 Phase 2b — BGSM v>=8 translucency suite. Forwarded from
    /// `ImportedMesh.translucency_subsurface_color` etc.; gated at the
    /// renderer by `material_flags & MAT_FLAG_BGSM_TRANSLUCENCY`
    /// (packed via `pack_bgsm_material_flags`). `[0.0; 3]` and `0.0`
    /// defaults so legacy / non-BGSM-v>=8 content evaluates the SSS
    /// path as zero contribution even if the gating flag were
    /// erroneously set.
    pub translucency_subsurface_color: [f32; 3],
    pub translucency_transmissive_scale: f32,
    pub translucency_turbulence: f32,
    /// `BSEffectShaderProperty.greyscale_texture` path (Skyrim+) — the
    /// 1D-as-2D colour palette LUT indexed by the source texture's
    /// luminance when `EFFECT_PALETTE_COLOR` / `EFFECT_PALETTE_ALPHA`
    /// are set. Captured at NIF importer ingestion; resolved to a
    /// bindless texture handle by `cell_loader::resolve_material_textures`
    /// and forwarded to `GpuMaterial.greyscale_lut_index` at draw build
    /// time. `None` for every non-BSEffect mesh. See #890 Stage 2c.
    pub greyscale_texture: Option<String>,
    /// Canonical PBR metalness `[0, 1]` — **fully resolved, no Option,
    /// no render-time fallback**. Populated once at the translation
    /// boundary (`byroredux::material_translate::translate_material`):
    /// either from the BGSM/BGEM translator (`merge_bgsm_into_mesh`
    /// maps authored `specular_color * specular_mult` luminance —
    /// dielectric ≈ 0.04 → `0.0`, conductor ≈ 0.95 → near `1.0`), or
    /// from the keyword classifier ([`resolve_pbr`](Self::resolve_pbr))
    /// for inline-shader NIF content (Oblivion / FO3 / FNV). The
    /// renderer reads this as `GpuMaterial.metalness` directly — no
    /// shader-side branching on source format. See
    /// `feedback_format_translation.md` and `docs/engine/nifal.md`
    /// (NIFAL — the canonical translation tier).
    pub metalness: f32,
    /// Canonical PBR roughness `[0, 1]` — companion to
    /// [`Self::metalness`], same resolve-once contract. The BGSM
    /// translator sets it as `1.0 - bgsm.smoothness`; the keyword
    /// classifier supplies it otherwise; glass classification
    /// (`classify_glass_into_material`) forces it to `GLASS_ROUGHNESS`.
    pub roughness: f32,
}

/// View-angle + soft-depth falloff cone captured from
/// `BSEffectShaderProperty` (Skyrim+) and `BSShaderNoLightingProperty`
/// (FO3/FNV). The first four fields are shared by both block types;
/// `soft_falloff_depth` is `BSEffectShaderProperty`-only and is `0.0`
/// (no fade) on the BSShaderNoLightingProperty path.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct EffectFalloff {
    /// Cosine of the angle where alpha = `start_opacity`.
    pub start_angle: f32,
    /// Cosine of the angle where alpha = `stop_opacity`.
    pub stop_angle: f32,
    pub start_opacity: f32,
    pub stop_opacity: f32,
    /// Soft-depth fade distance in world units. `0.0` disables the
    /// fade. Always `0.0` on the `BSShaderNoLightingProperty` path
    /// since that block has no soft-depth field.
    pub soft_falloff_depth: f32,
}

/// Per-variant payload for `BSLightingShaderProperty` shader types
/// that carry extra parameters beyond the standard PBR set. Mirrors
/// `nif::import::material::ShaderTypeFields` so the ECS layer can be
/// populated without depending on the NIF crate.
///
/// Every field is `Option` — unset means "this variant doesn't use
/// it". See #562 for the full ladder.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct ShaderTypeFields {
    pub skin_tint_color: Option<[f32; 3]>,
    pub skin_tint_alpha: Option<f32>,
    pub hair_tint_color: Option<[f32; 3]>,
    pub eye_cubemap_scale: Option<f32>,
    pub eye_left_reflection_center: Option<[f32; 3]>,
    pub eye_right_reflection_center: Option<[f32; 3]>,
    pub parallax_max_passes: Option<f32>,
    pub parallax_height_scale: Option<f32>,
    pub multi_layer_inner_thickness: Option<f32>,
    pub multi_layer_refraction_scale: Option<f32>,
    pub multi_layer_inner_layer_scale: Option<[f32; 2]>,
    pub multi_layer_envmap_strength: Option<f32>,
    pub sparkle_parameters: Option<[f32; 4]>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 1.0,
            emissive_source: EmissiveSource::None,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
            diffuse_color: [1.0, 1.0, 1.0],
            ambient_color: [1.0, 1.0, 1.0],
            glossiness: 80.0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            alpha: 1.0,
            env_map_scale: 1.0,
            normal_map: None,
            texture_path: None,
            material_path: None,
            glow_map: None,
            detail_map: None,
            gloss_map: None,
            dark_map: None,
            // AmbientDiffuse — the Gamebryo default, matches pre-#214
            // behavior for meshes without an NiVertexColorProperty.
            vertex_color_mode: 2,
            alpha_test: false,
            alpha_threshold: 0.0,
            alpha_test_func: 6, // GREATEREQUAL default
            material_kind: 0,   // Default lit
            wireframe: false,
            flat_shading: false,
            z_test: true,
            z_write: true,
            z_function: 3, // LESSEQUAL — Gamebryo default
            shader_type_fields: None,
            effect_falloff: None,
            effect_shader_flags: 0,
            // #1147 Phase 2b — BGSM translucency suite defaults
            // (zeros; no SSS contribution when the gating flag is unset).
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            greyscale_texture: None,
            // Canonical PBR defaults — match the renderer's no-Material
            // fallback (`static_meshes.rs`): dielectric, mid roughness.
            metalness: 0.0,
            roughness: 0.5,
        }
    }
}

impl Component for Material {
    type Storage = SparseSetStorage<Self>;
}

/// Physically-based material properties inferred from legacy NIF data.
///
/// Legacy Gamebryo materials have no PBR concept — we infer plausible
/// roughness/metalness from texture path keywords, shader type, and
/// the original glossiness/env_map_scale values. This produces better
/// lighting than faithfully reproducing the legacy Phong model.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct PbrMaterial {
    pub roughness: f32,
    pub metalness: f32,
}

/// Provenance of `emissive_mult` — which authoring slot the scalar came
/// from. Three NIF shader-property classes carry an "emissive multiplier"
/// in different fields with **different semantics**; pre-#1280 step 4
/// they all flowed into the same `Material.emissive_mult` slot and the
/// renderer had no way to tell them apart. The most important case:
/// `BSEffectShaderProperty.base_color_scale` is semantically a *diffuse
/// tint multiplier* (the effect shader's "glow" comes from
/// `base_color * base_color_scale`), NOT an emissive multiplier — but
/// the current pipeline routes it into `emissive_mult` because the
/// fragment shader's effect-shader branch consumes it from that slot.
///
/// This discriminator makes the conflation type-visible. Downstream
/// consumers (and the future BSEffect-proper-diffuse-tint render path)
/// can pattern-match instead of guessing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum EmissiveSource {
    /// No emissive authoring; `emissive_mult` defaulted to 0.0.
    /// Materials without any of the three shader-property classes (or
    /// where none of them authored a non-zero emissive) land here.
    #[default]
    None,
    /// `NiMaterialProperty.emissive_mult` (Oblivion / FO3 / FNV legacy
    /// path). Genuine emissive scalar in the Gamebryo material model.
    Material,
    /// `BSLightingShaderProperty.emissive_multiple` (Skyrim LE/SE / FO4 /
    /// FO76 / Starfield). Genuine emissive scalar on the Bethesda
    /// shader-property class. Authored in the 0–2+ range typically.
    Lighting,
    /// `BSEffectShaderProperty.base_color_scale` (FO4+ effect shader).
    /// **Semantically a diffuse-tint multiplier, NOT emissive** —
    /// conflated into this slot because the current fragment-shader
    /// effect-shader path reads its visible "glow" from
    /// `base_color * base_color_scale`. A future BSEffect-proper render
    /// path should branch on this variant to drop the conflation; see
    /// the walker site (`import/material/walker.rs`) for the
    /// in-source #166 rename note.
    Effect,
}

/// Free-form inputs to the keyword-based PBR classifier. Decoupled
/// from `Material` so the NIF importer can call the classifier at
/// `MaterialInfo → ImportedMesh` time (Stage 2 of the
/// `feedback_format_translation.md` rollout) without going through a
/// fully-constructed `Material`.
///
/// All fields are *primary inputs the classifier reads*; adding a
/// new input here is the single point of change. `texture_path` is
/// the dominant signal; `glossiness` / `env_map_scale` /
/// `has_normal_map` drive the no-keyword fallback arms.
#[derive(Debug, Clone, Copy)]
pub struct PbrClassifierInputs<'a> {
    pub texture_path: Option<&'a str>,
    pub glossiness: f32,
    pub env_map_scale: f32,
    pub has_normal_map: bool,
    /// `NiMaterialProperty.specular` RGB — white/grey means metallic
    /// response, coloured/dark means dielectric. Used to lift metalness
    /// on non-keyword surfaces (desks, doors, panels) that otherwise
    /// fall to metalness=0. Default `[1.0; 3]` → specular luminance 1.0.
    pub specular_color: [f32; 3],
    /// Whether `specular_color` was actually authored by a bound
    /// `NiMaterialProperty` / `BSLightingShaderProperty`, as opposed to
    /// sitting at `MaterialInfo`'s unauthored `[1.0; 3]` struct default.
    /// A `BSShaderPPLightingProperty`-only mesh (no co-bound
    /// `NiMaterialProperty`) authors `env_map_scale` but never touches
    /// `specular_color` — without this flag the env-map arm below reads
    /// the default's luminance (1.0) as "authored white specular" and
    /// chromes decorative FO3/FNV flyers/posters that never had a real
    /// specular tint. See REN-2026-07-04-M01 / #1873.
    pub specular_authored: bool,
    /// Whether the surface ships a dedicated specular/gloss MAP (Oblivion
    /// `NiTexturingProperty` slot 3 / FO4 BGSM smooth-spec). Its presence is
    /// the authored signal that the surface has real per-pixel shine; absent,
    /// the no-keyword fallback stays MATTE instead of inventing glossiness
    /// from the bare specular-power scalar (which made matte Skyrim
    /// architecture read mirror-glossy — Skyrim's spec mask lives in the
    /// normal-map alpha, wired separately).
    pub has_gloss_map: bool,
}

/// Keyword-based PBR classifier formerly shared with the (deleted)
/// `Material::classify_pbr` (the per-draw fallback that was removed in
/// the NIFAL canonical-material-translation refactor; see
/// `byroredux/src/material_translate.rs`). Now used only at the
/// `translate_material` parse-time boundary for the NIF importer's mesh
/// extractors (per-`ImportedMesh` translation). Single source of truth for the
/// rule that texture-path keywords drive metalness / glass / cloth
/// classification with glossiness + env_map_scale as the no-keyword
/// fallback. See `feedback_format_translation.md` for the architectural
/// directive.
///
/// Pure function — no `&self`, no allocations (matching uses
/// `contains_any_ci`'s windowed byte comparison).
/// The single authoritative glass texture-path keyword list, shared by
/// the roughness classifier's glass arm ([`classify_pbr_keyword`]) and
/// the render-gate predicate ([`Material::path_indicates_glass`]).
///
/// Before the canonical-material pass (2026-05-27) these two sites kept
/// *divergent* lists — the classifier had only `glass/crystal/ice/gem`
/// while the render gate had the fuller `…+window/bottle/jar/vial`
/// (design-doc Leak A). The consequence: glass containers (whiskey
/// bottles, drinking-glass jars, windows) matched the render gate but
/// missed the classifier's glass arm, so they took the *generic*
/// glossiness-derived roughness (≈ 0.40 for an FNV whiskey bottle)
/// instead of glass-smooth 0.1 — and then failed both the CPU glass
/// gate (`roughness < 0.4`) and the shader gate
/// (`triangle.frag` `roughness < 0.35`). Net effect: "whiskey bottles
/// don't look glassy." Routing both sites through one list makes a
/// glass-keyword surface smooth (0.1) AND glass-classified, so it
/// renders through the IOR refraction path — with no shader change.
pub fn is_glass_keyword_path(path: &str) -> bool {
    contains_any_ci(
        path,
        &[
            "glass", "crystal", "ice", "gem", "window", "bottle", "jar", "vial",
        ],
    )
}

pub fn classify_pbr_keyword(inputs: PbrClassifierInputs<'_>) -> PbrMaterial {
    let path = inputs.texture_path.unwrap_or("");

    if contains_any_ci(
        path,
        &["metal", "iron", "steel", "dwemer", "dwarven", "chainmail"],
    ) {
        // Weathered/industrial metal. Pre-2026-06-03 this was
        // roughness=0.3 (mirror chrome), which is correct for polished
        // steel but wrong for the worn post-apocalyptic surfaces in FNV
        // / FO3. Raised to 0.6 → brushed/oxidised metal. Still clearly
        // metallic (metalness=0.9) but the GGX highlight is much softer.
        return PbrMaterial {
            roughness: 0.6,
            metalness: 0.9,
        };
    }
    if contains_any_ci(path, &["gold", "silver", "bronze", "copper"]) {
        return PbrMaterial {
            roughness: 0.25,
            metalness: 0.95,
        };
    }
    // Alpha-UNAWARE arm: only "glass *material*" tokens
    // (glass/crystal/ice/gem) earn unconditional glass-smooth roughness —
    // a texture named these IS glass regardless of blend state. The
    // wider "glass *container/object*" tokens (window/bottle/jar/vial in
    // `is_glass_keyword_path`) are deliberately NOT here: a "window"
    // texture may be an opaque frame, a "bottle" an opaque cap. Those are
    // resolved alpha-gated at material-insert time (`classify_glass_*` in
    // the spawn path), where the blend flag disambiguates pane-from-frame
    // and roughness is forced as a consequence of the GLASS classification
    // — never an alpha-unaware roughness guess here (which over-shone
    // opaque container surfaces, the reverted step-3 side effect).
    if contains_any_ci(path, &["glass", "crystal", "ice", "gem"]) {
        return PbrMaterial {
            roughness: 0.1,
            metalness: 0.0,
        };
    }
    if contains_any_ci(path, &["wood", "plank", "barrel", "crate", "log"]) {
        return PbrMaterial {
            roughness: 0.7,
            metalness: 0.0,
        };
    }
    if contains_any_ci(path, &["stone", "rock", "cave", "brick", "ruins", "cobble"]) {
        return PbrMaterial {
            roughness: 0.85,
            metalness: 0.0,
        };
    }
    if contains_any_ci(
        path,
        &[
            "fabric", "cloth", "leather", "fur", "linen", "carpet", "rug", "tapestry", "banner",
            "curtain", "drape", "bedding", "pillow", "sack", "burlap", "wool",
        ],
    ) {
        return PbrMaterial {
            roughness: 0.95,
            metalness: 0.0,
        };
    }
    if contains_any_ci(path, &["skin", "body", "head", "hand", "face"]) {
        return PbrMaterial {
            roughness: 0.5,
            metalness: 0.0,
        };
    }
    if contains_any_ci(path, &["hair"]) {
        return PbrMaterial {
            roughness: 0.6,
            metalness: 0.0,
        };
    }

    // env_map_scale arm — base roughness for non-keyword surfaces.
    // `BSShaderPPLighting` authors `env_map_scale = 1.0` as the neutral
    // default on nearly every FNV surface, so this arm catches the vast
    // majority of interior content.
    //
    // METALNESS from specular luminance: `NiMaterialProperty.specular`
    // encodes the surface's Phong specular tint. White/grey (lum > 0.6)
    // is the Gamebryo convention for metallic surfaces with no explicit
    // metal texture-path keyword — cabinets, desks, corridor doors, hulls.
    // Derive metalness as `(spec_lum - 0.5) * 0.8` (lum=1.0 → 0.4;
    // lum=0.7 → 0.16; lum < 0.5 → 0).
    //
    // ROUGHNESS cap from specular: the RT reflection path gates at
    // `roughness < 0.6` (triangle.frag:2652). Default env_map_scale=1.0
    // gives roughness=0.8 — metal surfaces with metalness=0.4 but no RT
    // reflections still look flat. High-specular surfaces (lum > 0.6)
    // cap at 0.55, pushing them below the RT threshold so they get a
    // proper metallic sheen. Low-specular surfaces (plastic, concrete,
    // cloth) keep the full 0.8 ceiling.
    // The `min()` with the base roughness preserves explicit artist
    // intent — an env_map_scale-authored surface that already earned
    // a lower roughness (e.g. scale=3.0 → 0.4) keeps it.
    if inputs.env_map_scale > 0.3 {
        let base_roughness = (1.0 - inputs.env_map_scale * 0.2).clamp(0.35, 0.8);
        if !inputs.specular_authored {
            // No bound NiMaterialProperty/BSLightingShaderProperty —
            // `specular_color` is still the unauthored `[1,1,1]` struct
            // default, not a real Gamebryo specular tint. Reading its
            // luminance here would chrome every PPLighting-only
            // decorative surface (flyers, posters). Treat as dielectric;
            // the base_roughness ceiling already stays >= 0.35.
            return PbrMaterial {
                roughness: base_roughness,
                metalness: 0.0,
            };
        }
        let [sr, sg, sb] = inputs.specular_color;
        let spec_lum = 0.2126 * sr + 0.7152 * sg + 0.0722 * sb;
        let metalness = ((spec_lum - 0.5) * 0.8).clamp(0.0, 0.4);
        // spec_lum > 0.6 → metallic tier → roughness ceiling 0.55 (< RT threshold 0.6)
        // spec_lum ≤ 0.6 → dielectric tier → roughness ceiling 0.8 (no RT reflection)
        let roughness_ceiling = if spec_lum > 0.6 { 0.55_f32 } else { 0.8_f32 };
        return PbrMaterial {
            roughness: base_roughness.min(roughness_ceiling),
            metalness,
        };
    }

    // No keyword match and no env_map_scale authoring — the bulk of Skyrim
    // architecture (plaster, trims, generic walls/floors). DEFAULT MATTE.
    //
    // A surface's real specular response is authored in its MAP SET, not in
    // the bare glossiness (specular-power) scalar. Converting that scalar to
    // roughness (`1 - gloss/100`) made matte stone/plaster read mirror-glossy
    // (Skyrim glossiness 80 → roughness 0.10), so it passed the RT reflection
    // gate (< 0.6) and reflected the room — the close-range "wet floor".
    //
    // Only deviate from matte when a dedicated gloss/spec MAP says the surface
    // has authored shine (Oblivion `NiTexturingProperty` slot 3 / FO4 BGSM
    // smooth-spec). There the scalar sets the smooth-end base that the
    // in-shader gloss-map modulation (`mix(1, roughness, glossSample)`) then
    // roughens per-pixel. Skyrim ships no separate gloss map — its spec mask
    // lives in the normal-map ALPHA (wired in a separate step); until then it
    // stays correctly matte here rather than mirror-glossy.
    if inputs.has_gloss_map {
        let mut roughness = (1.0 - inputs.glossiness / 100.0).clamp(0.05, 0.95);
        if inputs.has_normal_map {
            roughness = (roughness - 0.1).max(0.05);
        }
        return PbrMaterial {
            roughness,
            metalness: 0.0,
        };
    }
    PbrMaterial {
        roughness: 0.85,
        metalness: 0.0,
    }
}

impl Material {
    /// Explicit "this surface is glass / crystal / ice / gem / window"
    /// classifier for use by [`crate::ecs::components::Material`]-less
    /// glass-path gating in the renderer. Required because the
    /// glossiness-fallback in the (deleted per-draw) `classify_pbr`
    /// undershot the 0.4 roughness gate for Skyrim cloth banners (whose
    /// `BSLightingShaderProperty.glossiness ≈ 80` lands at
    /// roughness 0.2 via `1 - 80/100`), producing spurious glass
    /// classification that routes the cloth through the IOR
    /// refraction + chromatic-dispersion shader path → rainbow
    /// banners. This predicate requires an explicit texture-path
    /// keyword match, not just heuristic roughness, so unauthored /
    /// generic-path materials never trip the glass path. See
    /// Markarth probe 2026-05-13.
    pub fn path_indicates_glass(texture_path: Option<&str>) -> bool {
        is_glass_keyword_path(texture_path.unwrap_or(""))
    }

    /// Clamp and, if still NaN, classify the canonical
    /// [`metalness`](Self::metalness) / [`roughness`](Self::roughness)
    /// scalars in place. Called once from the translation boundary
    /// (`material_translate::translate_material`).
    ///
    /// # Structure: classify-at-import + clamp-at-translate (#1346 / D7-01)
    ///
    /// For **NIF-imported** content the keyword classifier already ran at
    /// import time (`classify_legacy_pbr` in `crates/nif/src/import/mesh/`)
    /// and wrote `metalness_override`/`roughness_override` as `Some(…)` on
    /// the `ImportedMesh`. The caller seeds those values via
    /// `unwrap_or(NaN)`, so **both fields arrive non-NaN here** — the
    /// `if is_nan()` guard below is skipped and only the final clamp runs.
    ///
    /// For **BGSM/BGEM** content the authored scalars also arrive as `Some`.
    /// The classifier arm is a sentinel-backstop for future non-pre-classified
    /// sources only.
    ///
    /// Either way, after this returns the renderer reads `metalness` /
    /// `roughness` directly — no render-time fallback. Every material
    /// lands with explicit PBR scalars (`feedback_format_translation.md`).
    ///
    /// Both fields are clamped to `metalness ∈ [0, 1]` and
    /// `roughness ∈ [0.04, 1]`. Matching is case-insensitive and **does
    /// not allocate** ([`classify_pbr_keyword`]'s windowed byte compare).
    /// See #375.
    pub fn resolve_pbr(&mut self) {
        if self.metalness.is_nan() || self.roughness.is_nan() {
            let pbr = classify_pbr_keyword(PbrClassifierInputs {
                texture_path: self.texture_path.as_deref(),
                glossiness: self.glossiness,
                env_map_scale: self.env_map_scale,
                has_normal_map: self.normal_map.is_some(),
                specular_color: self.specular_color,
                // This backstop path (real content is pre-classified at
                // NIF import via `classify_legacy_pbr`, or via BGSM,
                // both of which leave metalness/roughness non-NaN) has
                // no way to know whether `specular_color` was ever
                // authored on this `Material` — assume not, matching
                // the conservative default in `classify_legacy_pbr`.
                specular_authored: false,
                has_gloss_map: self.gloss_map.is_some(),
            });
            if self.metalness.is_nan() {
                self.metalness = pbr.metalness;
            }
            if self.roughness.is_nan() {
                self.roughness = pbr.roughness;
            }
        }
        self.metalness = self.metalness.clamp(0.0, 1.0);
        self.roughness = self.roughness.clamp(0.04, 1.0);
    }
}

/// ASCII case-insensitive substring match. Zero allocations. Assumes
/// every keyword in `keywords` is non-empty and ASCII — both hold for
/// the hard-coded lists in the (deleted) `Material::classify_pbr`
/// and now in [`classify_pbr_keyword`] (the surviving free function).
fn contains_any_ci(haystack: &str, keywords: &[&str]) -> bool {
    let hs = haystack.as_bytes();
    keywords.iter().any(|kw| {
        let kb = kw.as_bytes();
        if kb.is_empty() || kb.len() > hs.len() {
            return false;
        }
        hs.windows(kb.len()).any(|w| w.eq_ignore_ascii_case(kb))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test shim — exercise the keyword classifier with a `Material`'s
    /// fields, the way the deleted `Material::classify_pbr` used to (the
    /// render path now reads the resolved `metalness`/`roughness`
    /// directly; these tests still validate the classifier itself).
    fn classify(m: &Material, texture_path: &str) -> PbrMaterial {
        classify_pbr_keyword(PbrClassifierInputs {
            texture_path: Some(texture_path),
            glossiness: m.glossiness,
            env_map_scale: m.env_map_scale,
            has_normal_map: m.normal_map.is_some(),
            specular_color: m.specular_color,
            specular_authored: false,
            has_gloss_map: m.gloss_map.is_some(),
        })
    }

    fn classify_with_spec(m: &Material, texture_path: &str, specular: [f32; 3]) -> PbrMaterial {
        classify_pbr_keyword(PbrClassifierInputs {
            texture_path: Some(texture_path),
            glossiness: m.glossiness,
            env_map_scale: m.env_map_scale,
            has_normal_map: m.normal_map.is_some(),
            specular_color: specular,
            specular_authored: true,
            has_gloss_map: m.gloss_map.is_some(),
        })
    }

    #[test]
    fn default_material() {
        let m = Material::default();
        assert_eq!(m.emissive_color, [0.0, 0.0, 0.0]);
        assert_eq!(m.specular_strength, 1.0);
        assert_eq!(m.glossiness, 80.0);
        assert_eq!(m.uv_scale, [1.0, 1.0]);
        assert_eq!(m.alpha, 1.0);
        assert!(m.normal_map.is_none());
        assert!(m.texture_path.is_none());
    }

    #[test]
    fn contains_any_ci_matches_case_insensitively() {
        // Real texture paths ship mixed case (e.g. "Textures\Clutter").
        // The classifier must still match lowercase keywords.
        assert!(contains_any_ci(r"Textures\Metal\Iron01.dds", &["metal"]));
        assert!(contains_any_ci("TEXTURES/WOOD/plank.dds", &["wood"]));
        assert!(contains_any_ci("effects/FxGlowSoft01.dds", &["fxglow"]));
        assert!(!contains_any_ci("textures/cloth/linen.dds", &["metal"]));
    }

    #[test]
    fn contains_any_ci_rejects_empty_needle_and_overlong_needle() {
        assert!(!contains_any_ci("short", &[""]));
        assert!(!contains_any_ci("short", &["longerthanhaystack"]));
    }

    #[test]
    fn classify_pbr_keyword_dispatch() {
        let m = Material::default();
        let metal = classify(&m, r"Textures\Weapons\Iron\IronSword.dds");
        assert!(metal.metalness > 0.8);
        // Roughness raised from 0.3 → 0.6 (worn/industrial metal, not mirror chrome).
        assert!(metal.roughness >= 0.5 && metal.roughness < 0.8);

        let wood = classify(&m, "textures/clutter/barrel/barrel01.dds");
        assert_eq!(wood.metalness, 0.0);
        assert!(wood.roughness > 0.6);

        let glass = classify(&m, "textures/clutter/ICE/IceShard01.dds");
        assert!(glass.roughness < 0.2);
    }

    /// `env_map_scale > 0.3` (legacy BSShaderPPLighting cube-map
    /// intensity) must NOT produce non-zero metalness. Pre-fix the
    /// classifier piped env_map_scale straight into metalness, which
    /// routed every dielectric-with-sheen (vinyl cushions, plastic,
    /// lacquered wood, glass) into the metal-reflection branch and
    /// produced "chrome cushion" looks on FNV medical gurneys / hospital
    /// beds. env_map_scale is a reflection-intensity authoring knob,
    /// not a conductor signal — true metals are caught by the texture-
    /// path keyword arms.
    /// Regression for the user-reported "chrome wall panel" 2026-05-25.
    /// `BSShaderPPLighting`-authored `env_map_scale ≈ 2.5` on FNV/FO3
    /// interior door panels / bulkhead trim used to land at the
    /// classifier's previous floor `roughness = 0.2` — mirror chrome
    /// for a dielectric. The floor is now 0.35 (polished plastic /
    /// vinyl); reflections still sharpen with authored scale but
    /// never reach mirror tier.
    #[test]
    fn classify_pbr_env_map_scale_floor_is_polished_plastic_not_chrome() {
        let mut m = Material::default();
        m.glossiness = 50.0;
        // Painted plastic wall panel: low specular (dielectric).
        // 2.5 = previously-clamped "power-armor tier" on the non-
        // keyword arm. Now plateaus at polished-plastic territory.
        m.env_map_scale = 2.5;
        let p = classify_with_spec(&m, "textures/interior/wallpanel01.dds", [0.2; 3]);
        assert!(
            p.roughness >= 0.35,
            "non-keyword env_map_scale must not produce chrome floor; got {}",
            p.roughness,
        );
        assert_eq!(p.metalness, 0.0, "low-specular surface must be dielectric");

        // Extreme env_map_scale still bottoms at the new floor —
        // a dielectric never looks like a mirror.
        m.env_map_scale = 10.0;
        let p = classify_with_spec(&m, "textures/unknown/shiny.dds", [0.2; 3]);
        assert!(p.roughness >= 0.35);
        assert_eq!(p.metalness, 0.0);
    }

    #[test]
    fn classify_pbr_env_map_scale_does_not_imply_metalness() {
        let mut m = Material::default();
        m.glossiness = 50.0;
        m.env_map_scale = 0.5; // cushion-with-sheen tier — low specular, dielectric
                               // Vinyl/cloth hospital bed: env_map_scale alone does NOT mean metallic.
                               // Metalness comes from specular_color luminance; cloth/vinyl has grey/dark specular.
        let p = classify_with_spec(&m, "textures/clutter/medical/hospitalbed01.dds", [0.2; 3]);
        assert_eq!(
            p.metalness, 0.0,
            "low specular + env_map_scale must not drive metalness — that's the chrome-cushion bug"
        );
        assert!(p.roughness < 1.0);

        // Power-armor tier on a non-keyword path with low specular stays dielectric.
        m.env_map_scale = 2.5;
        let p = classify_with_spec(&m, "textures/clutter/unknown/shiny.dds", [0.2; 3]);
        assert_eq!(p.metalness, 0.0);
    }

    /// Canonical-material-pass guard (2026-05-27, post-"chrome thugs"
    /// revert). `env_map_scale = 1.0` is the neutral `BSShaderPPLighting`
    /// default on nearly every FNV surface and MUST clamp to the matte
    /// 0.8 ceiling — NOT fall through to the glossiness arm. A brief
    /// experiment gated this at `> 1.0` to "restore the glossiness
    /// gradient"; that mapped gloss-60 cloth to roughness 0.30, which
    /// engages the RT reflection path (`< 0.6`) and rendered Chairman
    /// suits as mirror chrome at the Tops. Glass smoothness does not
    /// depend on this arm — it is forced at material-insert by the spawn
    /// glass classifier — so the matte default is correct for non-glass.
    #[test]
    fn classify_pbr_neutral_envmap_default_clamps_matte_not_chrome() {
        // Generic (non-keyword) surface at the neutral env default, with
        // the high glossiness FNV authors on cloth / weathered metal.
        // Must clamp to the matte ceiling — falling through to the
        // glossiness arm (gloss 60 -> 0.30) engages the RT reflection
        // path and renders chrome (the "chrome thugs" at the Tops).
        // Cloth/leather suit — low specular (dielectric). Specular on
        // worn cloth is ~0.2-0.3 in Gamebryo. Must not go chrome.
        let p = classify_pbr_keyword(PbrClassifierInputs {
            texture_path: Some("textures/armor/1950stylesuit/outfitweatheredm.dds"),
            glossiness: 60.0,
            env_map_scale: 1.0, // neutral FNV default
            has_normal_map: true,
            specular_color: [0.25; 3], // cloth: dark/grey specular → dielectric
            specular_authored: true,
            has_gloss_map: false,
        });
        assert!(
            p.roughness >= 0.6,
            "neutral env_map_scale=1.0 must stay matte (>=0.6) so the RT \
             reflection path (<0.6) does not engage; got {} (chrome regression)",
            p.roughness,
        );
        assert_eq!(p.metalness, 0.0, "cloth surface must be dielectric");
    }

    /// Canonical-material-pass step 3 (2026-05-27). Two-tier glass
    /// keyword contract:
    ///   * The alpha-UNAWARE classifier glass arm fires only for "glass
    ///     *material*" tokens (glass/crystal/ice/gem) → unconditional
    ///     smooth 0.1 (those textures ARE glass).
    ///   * The wide `is_glass_keyword_path` (+ window/bottle/jar/vial)
    ///     drives the alpha-GATED glass classification at material-insert
    ///     and the render gate. A container token alone does NOT earn
    ///     smooth roughness from the alpha-unaware classifier (that
    ///     over-shone opaque window frames / bottle caps).
    #[test]
    fn glass_material_tokens_are_unconditionally_smooth() {
        for path in [
            "textures/clutter/cafeteria/glasspitcher01.dds",
            "textures/clutter/brokenglasssheet01.dds",
            "textures/sky/ice/snowice01.dds",
            "textures/clutter/gem/ruby01.dds",
        ] {
            let p = classify_pbr_keyword(PbrClassifierInputs {
                texture_path: Some(path),
                glossiness: 50.0,
                env_map_scale: 1.0,
                has_normal_map: false,
                specular_color: [0.9; 3],
                specular_authored: true,
                has_gloss_map: false,
            });
            assert!(
                p.roughness <= 0.2,
                "'{path}' glass material should be smooth, got {}",
                p.roughness,
            );
            assert_eq!(p.metalness, 0.0, "glass is dielectric");
        }
    }

    /// Container/object tokens (window/bottle/jar/vial) match the wide
    /// render-gate predicate but do NOT short-circuit the alpha-unaware
    /// classifier to 0.1 — their glass-ness is decided alpha-gated at
    /// insert time. The two predicates intentionally differ in breadth;
    /// they must NOT have re-diverged on the shared material tokens.
    #[test]
    fn glass_container_tokens_match_render_gate_but_not_classifier_arm() {
        for path in [
            "textures/clutter/liquorbottles/whiskeybottle01.dds",
            "textures/architecture/whiterun/whiterunwindow01.dds",
        ] {
            // Wide render-gate predicate matches (alpha-gated downstream).
            assert!(
                Material::path_indicates_glass(Some(path)),
                "render gate should match container token '{path}'",
            );
            // …but the alpha-unaware classifier does not force 0.1; it
            // takes the glossiness-derived roughness (well above 0.2).
            let p = classify_pbr_keyword(PbrClassifierInputs {
                texture_path: Some(path),
                glossiness: 50.0,
                env_map_scale: 1.0,
                has_normal_map: false,
                specular_color: [0.9; 3],
                specular_authored: true,
                has_gloss_map: false,
            });
            assert!(
                p.roughness > 0.2,
                "container token '{path}' must NOT be auto-smooth in the \
                 alpha-unaware classifier (over-shine guard); got {}",
                p.roughness,
            );
        }
        // Material tokens stay shared between the two predicates.
        assert!(Material::path_indicates_glass(Some("x/glass01.dds")));
        assert!(is_glass_keyword_path("x/glass01.dds"));
    }

    #[test]
    fn classify_pbr_falls_back_to_glossiness() {
        let mut m = Material::default();
        m.glossiness = 20.0; // matte
        m.env_map_scale = 0.0; // disable env-map branch so glossiness wins
        let p = classify(&m, "textures/unknown/thing.dds");
        assert_eq!(p.metalness, 0.0);
        assert!(p.roughness > 0.5);
    }

    // ── path_indicates_glass — Markarth banner-as-glass false-positive
    //   fix (#993 follow-up; commit 2026-05-13). Pre-fix the
    //   MATERIAL_KIND_GLASS heuristic in `render.rs` used only
    //   alpha_blend + metalness + roughness, so Skyrim banner cloth
    //   whose glossiness-derived roughness fell below 0.4 trips the
    //   glass path and rendered with rainbow chromatic dispersion.
    //   Requiring an explicit texture-path glass-keyword signal
    //   eliminates the false-positive.

    #[test]
    fn path_indicates_glass_matches_common_glass_keywords() {
        for path in [
            r"Textures\Clutter\Glass\GlassBottle01.dds",
            "textures/clutter/crystal/crystal01.dds",
            "TEXTURES/SKY/ICE/SnowIce01.dds",
            r"textures\clutter\gem\ruby01.dds",
            "textures/architecture/whiterun/whiterunwindow01.dds",
            "textures/clutter/jars/winejar01.dds",
            "TEXTURES/CLUTTER/BOTTLES/wineBottle01.dds",
            "textures/dungeons/vials/healthvial01.dds",
        ] {
            assert!(
                Material::path_indicates_glass(Some(path)),
                "expected '{path}' to be classified as glass-bearing",
            );
        }
    }

    #[test]
    fn path_indicates_glass_rejects_cloth_and_architecture() {
        // The originating bug: Skyrim banner cloth whose path is
        // `architecture/markarth/markarthbanner01.dds` was being
        // misclassified as glass because the heuristic in render.rs
        // didn't consult the texture path. The new explicit gate must
        // reject these.
        for path in [
            "textures/architecture/markarth/markarthbanner01.dds",
            "textures/architecture/markarth/markarthtower01.dds",
            "textures/clutter/banner01.dds",
            "textures/clutter/tapestry01.dds",
            r"Textures\Architecture\Markarth\MarkarthWall01.dds",
            "textures/dungeons/markarthstone01.dds",
            "textures/clutter/fabric/linen.dds",
            "textures/dungeons/wood/woodplank01.dds",
        ] {
            assert!(
                !Material::path_indicates_glass(Some(path)),
                "expected '{path}' to NOT be classified as glass-bearing",
            );
        }
    }

    #[test]
    fn path_indicates_glass_handles_none_and_empty() {
        assert!(!Material::path_indicates_glass(None));
        assert!(!Material::path_indicates_glass(Some("")));
    }

    // ── `resolve_pbr` — the canonical translation hook
    //   (feedback_format_translation.md): every material lands with
    //   explicit `metalness` / `roughness` scalars regardless of
    //   source format. The caller seeds authored (BGSM) values or a
    //   `NaN` sentinel for "fill me from the keyword classifier".

    #[test]
    fn resolve_pbr_populates_from_keyword_path() {
        let mut m = Material::default();
        m.texture_path = Some(r"Textures\Weapons\Iron\IronSword.dds".to_string());
        // Seed the sentinel exactly as `translate_material` does for
        // legacy inline-shader content (no BGSM override).
        m.metalness = f32::NAN;
        m.roughness = f32::NAN;

        m.resolve_pbr();
        assert!(m.metalness > 0.8, "metal keyword routes to conductor");
        // Roughness raised from 0.3 → 0.6 (worn metal, not mirror chrome).
        assert!(m.roughness >= 0.5 && m.roughness < 0.8);
        assert!(m.metalness.is_finite() && m.roughness.is_finite());
    }

    #[test]
    fn resolve_pbr_is_idempotent() {
        let mut m = Material::default();
        m.texture_path = Some("textures/clutter/barrel/barrel01.dds".to_string());
        m.metalness = f32::NAN;
        m.roughness = f32::NAN;
        m.resolve_pbr();
        let first_metal = m.metalness;
        let first_rough = m.roughness;

        // Re-running on already-resolved (finite) values only re-clamps.
        m.resolve_pbr();
        assert_eq!(m.metalness, first_metal);
        assert_eq!(m.roughness, first_rough);
    }

    #[test]
    fn resolve_pbr_preserves_upstream_translator_values() {
        // BGSM merge layer ran first and wrote authoritative scalars
        // (finite, in-range); the keyword classifier must NOT overwrite.
        let mut m = Material::default();
        m.texture_path = Some(r"Textures\Weapons\Iron\IronSword.dds".to_string());
        m.metalness = 0.42;
        m.roughness = 0.13;

        m.resolve_pbr();
        assert_eq!(m.metalness, 0.42);
        assert_eq!(m.roughness, 0.13);
    }

    #[test]
    fn resolve_pbr_fills_only_missing_slot() {
        // Half-populated: one authored, the other a NaN sentinel. The
        // keyword fallback fills the gap without touching the populated
        // slot.
        let mut m = Material::default();
        m.texture_path = Some(r"Textures\Weapons\Iron\IronSword.dds".to_string());
        m.metalness = 0.42;
        m.roughness = f32::NAN;

        m.resolve_pbr();
        assert_eq!(m.metalness, 0.42);
        assert!(m.roughness.is_finite());
    }

    #[test]
    fn resolve_pbr_clamps_authored_out_of_range() {
        // Authored BGSM values outside the renderer ranges are clamped
        // (replicating the pre-canonical render-time `classify_pbr`).
        let mut m = Material::default();
        m.metalness = 1.7;
        m.roughness = 0.0;
        m.resolve_pbr();
        assert_eq!(m.metalness, 1.0);
        assert_eq!(m.roughness, 0.04);
    }
}
