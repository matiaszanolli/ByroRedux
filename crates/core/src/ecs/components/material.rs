//! Material component — surface properties for rendering.
//!
//! Captures the rich material data from NIF properties (NiMaterialProperty,
//! BSLightingShaderProperty, BSEffectShaderProperty) that was previously
//! discarded during import.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Generic dielectric IOR used when a source format does not author a more
/// specific optical behavior. This preserves the renderer's historical
/// `F0 ~= 0.04` fallback.
pub const DEFAULT_DIELECTRIC_IOR: f32 = 1.5;

/// Source-format-independent optical behavior applied before authored texture
/// maps and scalar overlays are bound.
///
/// BGSM/BGEM, NIF texture sets, and future material formats describe how a
/// surface is textured; they must not each invent a separate implementation of
/// glass. Importers select one shared behavior, then retain their own map set on
/// the [`Material`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceBehavior {
    pub roughness: f32,
    pub metalness: f32,
    /// Refractive index for this behavior's glass/dielectric surface
    /// (~1.0-2.5). Uploaded verbatim into `GpuMaterial.ior`.
    ///
    /// #2232 — `GpuMaterial.ior` is discriminated by `materialKind` and also
    /// carries a THIRD, incompatible-range meaning for
    /// `MATERIAL_KIND_FIRE_REFRACTION` (a 0-1 heat-haze distortion scalar,
    /// not a refractive index — see `bindings.glsl`'s `GpuMaterial::ior` doc
    /// and `triangle.frag`'s fire-refraction branch). `SurfaceBehavior` is
    /// never used to construct fire-refraction materials, but readers of
    /// the shared `GpuMaterial.ior` slot should not assume this field's
    /// physical-IOR contract applies to every `materialKind`.
    pub ior: f32,
}

/// Canonical glass behavior used by legacy FNV/FO3 NIF glass and FO4+ BGEM
/// glass alike. A small non-zero roughness suppresses aliasing while remaining
/// below the clear-glass scatter threshold in the shared shader.
pub const GLASS_SURFACE_BEHAVIOR: SurfaceBehavior = SurfaceBehavior {
    roughness: 0.10,
    metalness: 0.0,
    ior: 1.45,
};

/// Surface material properties extracted from NIF shader/property blocks.
///
/// SparseSetStorage: most static geometry shares a small set of unique
/// materials; sparse access pattern during rendering.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Material {
    /// Authored Skyrim+ water shader flags; non-zero routes mesh water through
    /// the dedicated water pipeline while preserving ordinary material data.
    pub water_shader_flags: u32,
    /// Dedicated mesh-water shader source marker (including legacy games
    /// whose water block has no authored flag word).
    pub is_water_shader: bool,
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
    /// (packed via `pack_imported_material_flags`). `[0.0; 3]` and `0.0`
    /// defaults so legacy / non-BGSM-v>=8 content evaluates the SSS
    /// path as zero contribution even if the gating flag were
    /// erroneously set.
    pub translucency_subsurface_color: [f32; 3],
    pub translucency_transmissive_scale: f32,
    pub translucency_turbulence: f32,
    /// `BSLightingShaderProperty.lighting_effect_1` — Skyrim subsurface
    /// scattering scalar (BSVER < FO4, gated by `SLSF2_Soft_Lighting`).
    /// `BSLightingShaderProperty.lighting_effect_2` — Skyrim backlight
    /// scalar (BSVER < FO4, gated by `SLSF2_Back_Lighting`).
    /// `BSLightingShaderProperty.subsurface_rolloff` /
    /// `.rimlight_power` / `.backlight_power` — the FO4/FO76/Starfield
    /// (BSVER 130+) per-material SSS-rolloff / rim-light / backlight
    /// exponents. `.fresnel_power` — the FO4+ per-material Schlick
    /// exponent for the Fresnel rim term.
    ///
    /// #2284 (MAT-D1-NEW-04) — captured at the NIF importer boundary
    /// since `#1241` (`ImportedMaterial::{lighting_effect_1,2,
    /// subsurface_rolloff, rimlight_power, backlight_power,
    /// fresnel_power}`) but dead-ended there with zero consumers: no
    /// field existed here, so `translate_material` had nothing to copy
    /// into. Skin/hair/cloth materials authoring non-default rim-
    /// lighting, backlight, subsurface-rolloff, or Fresnel-exponent
    /// values rendered with the engine's fixed Disney BSDF response
    /// instead of the author's tuned curve.
    ///
    /// Landed here (captured, not yet shaded) rather than also wiring a
    /// `GpuMaterial`/`triangle.frag` consumer in the same change, so the
    /// canonical `Material` no longer silently drops authored data while
    /// the GPU-side shading consumer lands as separate,
    /// independently-reviewable follow-up work. Defaults mirror
    /// `ImportedMaterial`'s own parser-stub defaults (`fresnel_power`
    /// 5.0 = standard Schlick exponent; the rest 0.0 = no contribution).
    ///
    /// #2592 (SKY-D7-04) — this doc used to justify that shape by citing
    /// "the existing `grayscale_to_palette_scale` precedent (see that
    /// field's doc)". At the time there was no such field on `Material`,
    /// and the pointer was worse than dangling: `grayscale_to_palette_scale`
    /// lived only on `byroredux_nif`'s `ImportedMaterial` — the raw tier —
    /// with `translate_material` dropping it, so it never reached this type
    /// at all. It was therefore not a precedent for "captured here but
    /// unshaded" but a strictly *earlier* failure mode, one tier back.
    ///
    /// #2443 (MAT-D3-01) closed that gap:
    /// [`grayscale_to_palette_scale`](Self::grayscale_to_palette_scale) is
    /// now a canonical field copied at the boundary, so it has caught up to
    /// these six and the two groups are genuinely the same shape — captured,
    /// awaiting a `GpuMaterial`/shader consumer. Both remain listed in
    /// `docs/engine/nifal.md`'s parked-passthrough inventory until that
    /// consumer lands.
    pub lighting_effect_1: f32,
    pub lighting_effect_2: f32,
    pub subsurface_rolloff: f32,
    pub rimlight_power: f32,
    pub backlight_power: f32,
    pub fresnel_power: f32,
    /// Source-authored feature gates for the legacy soft/rim/back-light
    /// response. Kept separate from the scalar values because Bethesda files
    /// may serialize a non-zero default while leaving the feature disabled.
    pub soft_lighting: bool,
    pub rim_lighting: bool,
    pub back_lighting: bool,
    /// `BSEffectShaderProperty.greyscale_texture` path (Skyrim+) — the
    /// 1D-as-2D colour palette LUT indexed by the source texture's
    /// luminance when `EFFECT_PALETTE_COLOR` / `EFFECT_PALETTE_ALPHA`
    /// are set. Captured at NIF importer ingestion; resolved to a
    /// bindless texture handle by `cell_loader::resolve_material_textures`
    /// and forwarded to `GpuMaterial.greyscale_lut_index` at draw build
    /// time. `None` for every non-BSEffect mesh. See #890 Stage 2c.
    pub greyscale_texture: Option<String>,
    /// `BSEffectShaderProperty` / BGEM palette-remap strength (BSVER >= 130,
    /// FO4+). Modulates the [`greyscale_texture`](Self::greyscale_texture)
    /// palette lookup above; `1.0` (the format default) means "full-strength
    /// remap", which is the behaviour every material got before this field
    /// existed.
    ///
    /// #2443 (MAT-D3-01) — captured by both producers (the inline
    /// `BSLightingShaderProperty`/`BSEffectShaderProperty` parser and the
    /// BGSM/BGEM merge, the latter with parent-template precedence and its
    /// own round-trip test) but dropped at the translation boundary: there
    /// was no canonical field for `translate_material` to copy into. Because
    /// `EFFECT_PALETTE_COLOR`/`ALPHA` is a *replace*, not a blend, an authored
    /// 0.5 that should soften a shared greyscale ramp rendered as the full
    /// palette colour instead.
    ///
    /// Captured here, not yet shaded — `triangle.frag`'s palette branch still
    /// performs an unmodulated direct lookup, and the `GpuMaterial` slot plus
    /// the multiply in its `MAT_FLAG_EFFECT_PALETTE_COLOR` block are a
    /// separate, independently-reviewable follow-up. This is the same
    /// captured-then-shaded staging the #2284 scalars above use; what it is
    /// no longer is the *earlier* failure mode those field docs contrast
    /// themselves against ("`grayscale_to_palette_scale` never reaches
    /// `Material` at all" — true until this landed, see #2592 / SKY-D7-04).
    pub grayscale_to_palette_scale: f32,
    /// Canonical PBR metalness `[0, 1]` — **fully resolved, no Option,
    /// no render-time fallback**. Populated once at the translation
    /// boundary (`byroredux::material_translate::translate_material`):
    /// either from the BGSM/BGEM translator (`merge_external_material`
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
    /// (`classify_glass_into_material`) applies [`GLASS_SURFACE_BEHAVIOR`].
    pub roughness: f32,
    /// Refractive index used by dielectric Fresnel and the glass IOR path.
    /// Source-format-independent behavior selection owns the fallback; texture
    /// maps remain independent overlays. Generic materials default to 1.5,
    /// while canonical glass uses [`GLASS_SURFACE_BEHAVIOR`]'s 1.45.
    pub ior: f32,
    /// Source-authored tint for the dielectric reflection/Fresnel lobe.
    /// Neutral white on legacy content; populated by BGEM v21+ glass.
    pub glass_fresnel_color: [f32; 3],
    /// Source-authored refraction deviation scale (BGEM v21+, default 0.05).
    pub glass_refraction_scale: f32,
    /// Source-authored optical blur base and v22+ multiplier.
    pub glass_blur_scale: f32,
    pub glass_blur_scale_factor: f32,
    /// Disney/Burley fake-subsurface-scattering weight `[0, 1]`, consumed
    /// by `disneyDiffuseSplit` (`include/pbr.glsl`) only when
    /// `MAT_FLAG_PBR_BSDF` is set on `material_flags`. No source format
    /// (BGSM/BGEM/inline-NIF) authors an equivalent concept — this is a
    /// Disney-BSDF-only parameter, unlike `subsurface_rolloff` above
    /// (Skyrim/FO4's own SSS-rolloff exponent, which DOES have real
    /// source data). Reachable only via `mat.set` today; `0.0` = the
    /// shader's Burley-only fallback. #2514 / REN-D21-2026-08-07-02.
    pub subsurface: f32,
    /// Disney sheen weight `[0, 1]` — cloth-like grazing-angle
    /// retroreflection. Same no-source-format-equivalent caveat as
    /// [`Self::subsurface`]; `mat.set`-only. #2514.
    pub sheen: f32,
    /// Disney sheen tint `[0, 1]` — how much the sheen term picks up the
    /// surface's own hue vs. staying neutral white. Same caveat as
    /// [`Self::subsurface`]; `mat.set`-only. #2514.
    pub sheen_tint: f32,
    /// Disney anisotropic GGX weight `[0, 1]` — elongates the specular
    /// highlight along the tangent (hair, brushed metal). Same
    /// no-source-format-equivalent caveat as [`Self::subsurface`] (BGSM
    /// authors no anisotropy metadata either); `mat.set`-only. #2514.
    pub anisotropic: f32,
    /// `NiTexturingProperty`/`BSShaderProperty` texture-address mode, per
    /// nif.xml's `TexClampMode` enum: 0=CLAMP_S_CLAMP_T, 1=CLAMP_S_WRAP_T,
    /// 2=WRAP_S_CLAMP_T, 3=WRAP_S_WRAP_T. Authored on Oblivion architecture
    /// trim/signs/banners (#610) via `CLAMP_S_CLAMP_T` so edge texels don't
    /// bleed into the sampler's wrap. Copied verbatim from
    /// `ImportedMaterial::texture_clamp_mode`, including that struct's own
    /// `0` (CLAMP_S_CLAMP_T) parser-stub default.
    ///
    /// #2571 (OBL-D5-01) — added so spawn sites can read this off the
    /// canonical `Material` instead of re-reading the raw `ImportedMaterial`
    /// tier independently at each call site (the exact hand-synced-
    /// duplication failure mode NIFAL exists to eliminate).
    pub texture_clamp_mode: u8,
    /// `NiAlphaProperty` source blend factor (Gamebryo `AlphaFunction`
    /// enum; `6` = SRC_ALPHA is the Gamebryo default). Only meaningful
    /// when the material's alpha-blend path is active. Copied verbatim
    /// from `ImportedMaterial::src_blend_mode`. See
    /// [`Self::texture_clamp_mode`]'s doc for why this field exists.
    pub src_blend_mode: u8,
    /// `NiAlphaProperty` destination blend factor (same enum as
    /// [`Self::src_blend_mode`]; `7` = INV_SRC_ALPHA is the Gamebryo
    /// default). Copied verbatim from `ImportedMaterial::dst_blend_mode`.
    pub dst_blend_mode: u8,
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

/// Canonical per-variant payload for lighting-shader types that carry
/// parameters beyond the standard PBR set.
///
/// Every field is `Option` — unset means "this variant doesn't use
/// it". Source-format importers populate this core-owned shape directly,
/// so adding a field cannot desynchronize an importer-side mirror from the
/// ECS material contract. See #562.
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

impl ShaderTypeFields {
    /// `true` when no shader variant authored an additional payload.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for Material {
    fn default() -> Self {
        Self {
            water_shader_flags: 0,
            is_water_shader: false,
            emissive_color: [0.0, 0.0, 0.0],
            // 0.0, not a "neutral" 1.0 — matches `EmissiveSource::None`'s own
            // doc ("no emissive authoring; `emissive_mult` defaulted to
            // 0.0") and `ImportedMaterial::default()`'s already-0.0 value
            // (crates/nif/src/import/types.rs), which this struct's default
            // otherwise silently contradicted (#2556). No call site sets
            // `emissive_color` non-zero via `..Material::default()` without
            // also setting `emissive_mult` explicitly (verified: every such
            // site sets both together), so this has no behavioral effect —
            // paired with the zero `emissive_color` above either value
            // renders identically; this just makes the pairing honest.
            emissive_mult: 0.0,
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
            // #2284 (MAT-D1-NEW-04) — mirror ImportedMaterial's own
            // parser-stub defaults; fresnel_power's 5.0 is the standard
            // Schlick exponent, the rest are "no contribution".
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            fresnel_power: 5.0,
            soft_lighting: false,
            rim_lighting: false,
            back_lighting: false,
            greyscale_texture: None,
            // 1.0 = full-strength palette remap, the BGEM/nif.xml format
            // default and the pre-#2443 hardcoded shader behaviour.
            grayscale_to_palette_scale: 1.0,
            // Canonical PBR defaults — match the renderer's no-Material
            // fallback (`static_meshes.rs`): dielectric, mid roughness.
            metalness: 0.0,
            roughness: 0.5,
            ior: DEFAULT_DIELECTRIC_IOR,
            glass_fresnel_color: [1.0; 3],
            glass_refraction_scale: 0.05,
            glass_blur_scale: 0.4,
            glass_blur_scale_factor: 1.0,
            // #2514 — no source format authors these; zero = the
            // shader's Burley/isotropic-only fallback (Lambert-adjacent
            // behavior, matching pre-#2514 rendering exactly).
            subsurface: 0.0,
            sheen: 0.0,
            sheen_tint: 0.0,
            anisotropic: 0.0,
            // #2571 — mirror `ImportedMaterial::default()`'s own values
            // (crates/nif/src/import/types.rs) exactly: SRC_ALPHA (6) /
            // INV_SRC_ALPHA (7) is the Gamebryo default blend-factor pair;
            // `0` (CLAMP_S_CLAMP_T) is that struct's stub clamp default.
            texture_clamp_mode: 0,
            src_blend_mode: 6,
            dst_blend_mode: 7,
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
    /// Materials without any of the three shader-property classes bound
    /// land here. All three writers (`dedicated_shader.rs`,
    /// `legacy_properties.rs`, `asset_provider/material.rs`) set their
    /// variant unconditionally once their property class is bound — there
    /// is no non-zero-emissive gate, so e.g. a `BSLightingShaderProperty`
    /// with `emissive_multiple == 0.0` still reports `Lighting`, not
    /// `None` (#2641).
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
    /// the set-site (`import/material/dedicated_shader.rs`) for the
    /// in-source #166 rename note.
    Effect,
}

/// Whether a captured `(color, multiplier)` pair is a genuine emissive
/// (or, for [`EmissiveSource::Effect`], diffuse-tint) authoring —
/// versus the unauthored struct default the source NIF block ships
/// when nothing was actually set. Shared by all three
/// `EmissiveSource::{Material,Lighting,Effect}` set-sites (#2591 /
/// SKY-D7-03) so `EmissiveSource::None`'s own doc — "materials without
/// any of the three shader-property classes **or** where none of them
/// authored a non-zero emissive" — is actually true. Pre-fix, every
/// BSLightingShaderProperty (the overwhelming majority of vanilla
/// Skyrim content, almost none of which authors emissive) was tagged
/// `Lighting` regardless, degenerating the discriminator to "has a
/// BSLightingShaderProperty" rather than "has an authored emissive".
///
/// `color == [0,0,0]` alone zeroes the contribution regardless of
/// `mult` (black times anything is still black); `mult == 0.0` alone
/// zeroes it regardless of `color`. Either is sufficient for "not
/// authored" — both must be non-zero for "authored".
pub fn emissive_contribution_is_authored(color: [f32; 3], mult: f32) -> bool {
    color != [0.0, 0.0, 0.0] && mult != 0.0
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
        &["glass", "crystal", "window", "bottle", "jar", "vial"],
    ) || contains_any_ci_word(path, &["ice", "gem"])
}

pub fn classify_pbr_keyword(inputs: PbrClassifierInputs<'_>) -> PbrMaterial {
    let path = inputs.texture_path.unwrap_or("");
    // Parent directories describe an asset family, not necessarily the
    // surface material. In particular, Skyrim stores stone walls, rubble,
    // and bronze trim together under `dungeons\dwemerruins`; matching
    // `dwemer` against the whole path turns the entire ruin into metal.
    // Keep the broad material-word rules below, but constrain the cultural
    // aliases to the actual texture filename.
    let filename = path.rsplit(['/', '\\']).next().unwrap_or(path);

    if contains_any_ci(path, &["metal", "iron", "steel", "chainmail"])
        || contains_any_ci(filename, &["dwemer", "dwarven"])
    {
        // Weathered/industrial metal. Pre-2026-06-03 this was
        // roughness=0.3 (mirror chrome), which is correct for polished
        // steel but wrong for the worn post-apocalyptic surfaces in FNV
        // / FO3. Raised to 0.55 → brushed/oxidised metal. This remains
        // below the renderer's rough-reflection cutoff (`roughness < 0.6`),
        // so a known conductor still receives environment response instead
        // of reading as dark diffuse paint. The GGX highlight remains much
        // softer than the old 0.3 mirror-chrome classification.
        return PbrMaterial {
            roughness: 0.55,
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
    // #2009 / MAT-D1-01 — "ice"/"gem" are word-boundary-checked
    // (`contains_any_ci_word`), not plain substring: they're short
    // enough to collide with ordinary English words that plausibly
    // appear in legacy diffuse texture paths (office/notice/device/
    // justice/invoice/spice/voice/twice/advice/entice/artifice/
    // sacrifice/practice/police/juice/dice/slice for "ice"; management
    // for "gem"). "glass"/"crystal" stay on the unbounded matcher —
    // they're long enough to have no such collisions, and Bethesda's
    // own concatenated-compound naming convention (`brokenglasssheet*`)
    // relies on the mid-word match still firing for them.
    if contains_any_ci(path, &["glass", "crystal"]) || contains_any_ci_word(path, &["ice", "gem"]) {
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
    // "fur" is word-boundary-checked (#2009 / MAT-D1-01) — a bare
    // substring match makes it a literal prefix of "furniture", a
    // common Bethesda clutter-asset directory/mesh token across every
    // game. The rest of the list is long enough to have no such risk.
    if contains_any_ci(
        path,
        &[
            "fabric", "cloth", "leather", "linen", "carpet", "rug", "tapestry", "banner",
            "curtain", "drape", "bedding", "pillow", "sack", "burlap", "wool",
        ],
    ) || contains_any_ci_word(path, &["fur"])
    {
        return PbrMaterial {
            roughness: 0.95,
            metalness: 0.0,
        };
    }
    // #3315 / FNV-D2-01 — "skin" is a *material* word and stays global (a
    // deathclaw hide texture is skin wherever it lives), but "body"/"head"/
    // "hand"/"face" are *anatomy* words that only mean skin inside a
    // character/creature asset family. Left unbounded they matched by mere
    // substring: FNV's entire weapon corpus lives under `weapons\1handpistol\`,
    // `\2handrifle\` and `\1handmelee\`, so 3 458 weapon meshes took skin
    // roughness 0.50 instead of the 0.80-0.85 the metal/generic arms would
    // give them — ~91% of every SKIN classification on FNV was a false
    // positive. Same class of collision as "bobblehead"/"headgear" (head) and
    // "interface" (face).
    //
    // Word-boundary matching (`contains_any_ci_word`, the #2009 remedy for
    // "ice"/"gem"/"fur") does NOT work here: it rejects `1handpistol` but also
    // rejects the real skin paths, because Bethesda concatenates them too
    // (`femaleupperbody.dds`, `textures\characters\bodymods\...modbodymale.dds`).
    // So this follows the *other* precedent already in this function — the way
    // the metal arm scopes the cultural aliases `dwemer`/`dwarven` to a
    // narrower haystack than the material words beside them.
    if contains_any_ci(path, &["skin"])
        || (is_character_asset_path(path)
            && contains_any_ci(path, &["body", "head", "hand", "face"]))
    {
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
    // Pre-#2315, `BSShaderPPLighting`'s on-disk `env_map_scale = 1.0`
    // reached this arm unconditionally, making it FNV's majority path.
    // #2315 gated the FO3/FNV legacy import at the authored
    // Environment_Mapping/Eye_Environment_Mapping/Window_Environment_Mapping
    // shader-flag bits (`legacy_env_map_scale`,
    // `crates/nif/src/import/material/legacy_properties.rs`) — most
    // surfaces don't author those, so this arm is now a **minority** path
    // on FO3/FNV content (measured: `env_map_scale = 0.00` on 15 of 18
    // sampled FNV meshes, #2555). Kept here rather than removed: it's
    // still the correct classification for the surfaces that DO author
    // real environment mapping (glass, eyes, windows), and other games'
    // paths may reach it with different reachability than FO3/FNV's.
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
    /// Apply source-format-independent optical behavior without disturbing the
    /// material's authored texture paths, UVs, alpha state, tints, or flags.
    pub fn apply_surface_behavior(&mut self, behavior: SurfaceBehavior) {
        self.roughness = behavior.roughness;
        self.metalness = behavior.metalness;
        self.ior = behavior.ior;
    }

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

    /// Reset every non-finite (NaN / ±inf) scalar to its
    /// [`Material::default()`] value. `metalness`/`roughness` defer to
    /// [`Self::resolve_pbr`] instead of a blind default, since that already
    /// runs the keyword classifier and clamp for NaN and also normalizes
    /// ±inf via the same clamp. Returns `true` if any field was non-finite
    /// (and has now been repaired), `false` if the material was already
    /// clean — callers use this to decide whether to log/reject rather than
    /// diffing the struct themselves.
    ///
    /// #2687 (SAFE-D9-01) — `translate_material` is the only NIF-import
    /// producer of a renderer-bound `Material`, and it is NaN-safe end to
    /// end. Save/restore is a second producer with no equivalent gate: a
    /// hand-edited or corrupted save file can carry a non-finite scalar
    /// straight through `restore_world` into `GpuMaterial` on the next
    /// frame — NaN/Inf feeding the GPU is undefined behavior, not just a
    /// visual glitch. This is the *repair* half, called once per restored
    /// `Material` (`crates/save/src/driver.rs::restore_world`); the
    /// *prevention* half is `crates/save/src/validate.rs`'s pre-save gate
    /// (which probes a clone with this same method rather than duplicating
    /// the field list), refusing to persist an already-poisoned world in
    /// the first place. Every field is reset independently — a NaN `ior`
    /// does not take down an otherwise-valid `metalness`.
    pub fn sanitize_finite(&mut self) -> bool {
        let mut changed = !self.metalness.is_finite() || !self.roughness.is_finite();
        self.resolve_pbr();

        let default = Self::default();

        macro_rules! fix_scalar {
            ($field:ident) => {
                if !self.$field.is_finite() {
                    self.$field = default.$field;
                    changed = true;
                }
            };
        }
        macro_rules! fix_vec {
            ($field:ident) => {
                for i in 0..self.$field.len() {
                    if !self.$field[i].is_finite() {
                        self.$field[i] = default.$field[i];
                        changed = true;
                    }
                }
            };
        }

        fix_vec!(emissive_color);
        fix_scalar!(emissive_mult);
        fix_vec!(specular_color);
        fix_scalar!(specular_strength);
        fix_vec!(diffuse_color);
        fix_vec!(ambient_color);
        fix_scalar!(glossiness);
        fix_vec!(uv_offset);
        fix_vec!(uv_scale);
        fix_scalar!(alpha);
        fix_scalar!(env_map_scale);
        fix_scalar!(alpha_threshold);
        fix_vec!(translucency_subsurface_color);
        fix_scalar!(translucency_transmissive_scale);
        fix_scalar!(translucency_turbulence);
        fix_scalar!(lighting_effect_1);
        fix_scalar!(lighting_effect_2);
        fix_scalar!(subsurface_rolloff);
        fix_scalar!(rimlight_power);
        fix_scalar!(backlight_power);
        fix_scalar!(fresnel_power);
        fix_scalar!(grayscale_to_palette_scale);
        fix_scalar!(ior);
        fix_scalar!(subsurface);
        fix_scalar!(sheen);
        fix_scalar!(sheen_tint);
        fix_scalar!(anisotropic);
        // #3373 — the BGEM glass-optics tail. Added after this method and
        // missed by it, so both save-path gates (the pre-save probe in
        // `crates/save/src/validate.rs` and the post-restore repair in
        // `crates/save/src/driver.rs`) had a hole in exactly the newest
        // scalars. These reach `GpuMaterial` like any other field.
        fix_vec!(glass_fresnel_color);
        fix_scalar!(glass_refraction_scale);
        fix_scalar!(glass_blur_scale);
        fix_scalar!(glass_blur_scale_factor);

        changed
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

/// ASCII case-insensitive **word-boundary** substring match (#2009 /
/// MAT-D1-01). Like [`contains_any_ci`], but a match only counts when
/// the byte immediately before/after the matched span (if any) is not
/// an ASCII *letter* — i.e. the keyword must not be embedded inside a
/// longer run of letters (another English word). Digits are
/// deliberately treated as valid boundaries, not blocked like letters:
/// Bethesda's own `keyword01.dds`/`keywordNN.dds` numbering convention
/// butts a digit run directly against the keyword with no separator
/// (`fur01.dds`), and that must still match. Path separators,
/// underscores, and string boundaries are valid edges too.
///
/// Reserved for keywords short/common enough to collide with ordinary
/// English words (`"ice"` inside "office"/"notice"/"device", `"gem"`
/// inside "management", `"fur"` inside "furniture") — NOT a blanket
/// replacement for `contains_any_ci`. Several keywords in this file
/// (`"glass"`, `"head"`, `"body"`, `"hand"`, `"face"`, `"hair"`) rely on
/// the unbounded match firing on Bethesda's own concatenated-compound
/// naming convention with no separator (`brokenglasssheet01.dds`,
/// `malehead.dds`, `femalebody_1.dds`) — switching those to
/// word-boundary matching would silently stop matching real content.
/// Does `path` live in a character / creature / actor asset family?
///
/// Gates the anatomy words in [`classify_pbr_keyword`]'s skin arm. The token
/// list is deliberately singular-stemmed so one entry covers every game's
/// directory convention: `character` matches Oblivion/FO3/FNV's
/// `textures\characters\...` *and* Skyrim's `textures\actors\character\...`;
/// `creature` matches `textures\creatures\...`; `actors` covers Skyrim/FO4
/// actor trees that don't repeat `character` deeper in the path.
fn is_character_asset_path(path: &str) -> bool {
    contains_any_ci(path, &["character", "creature", "actors"])
}

fn contains_any_ci_word(haystack: &str, keywords: &[&str]) -> bool {
    let hs = haystack.as_bytes();
    keywords.iter().any(|kw| {
        let kb = kw.as_bytes();
        if kb.is_empty() || kb.len() > hs.len() {
            return false;
        }
        hs.windows(kb.len()).enumerate().any(|(i, w)| {
            if !w.eq_ignore_ascii_case(kb) {
                return false;
            }
            let before_ok = i == 0 || !hs[i - 1].is_ascii_alphabetic();
            let after = i + kb.len();
            let after_ok = after == hs.len() || !hs[after].is_ascii_alphabetic();
            before_ok && after_ok
        })
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
        assert_eq!(m.ior, DEFAULT_DIELECTRIC_IOR);
    }

    #[test]
    fn glass_behavior_preserves_authored_map_overlay() {
        let mut m = Material {
            texture_path: Some("textures/fo4/glass_d.dds".into()),
            normal_map: Some("textures/fo4/glass_n.dds".into()),
            glow_map: Some("textures/fo4/glass_g.dds".into()),
            uv_scale: [2.0, 3.0],
            alpha: 0.25,
            roughness: 0.8,
            metalness: 0.4,
            ..Default::default()
        };

        m.apply_surface_behavior(GLASS_SURFACE_BEHAVIOR);

        assert_eq!(m.roughness, GLASS_SURFACE_BEHAVIOR.roughness);
        assert_eq!(m.metalness, GLASS_SURFACE_BEHAVIOR.metalness);
        assert_eq!(m.ior, GLASS_SURFACE_BEHAVIOR.ior);
        assert_eq!(m.texture_path.as_deref(), Some("textures/fo4/glass_d.dds"));
        assert_eq!(m.normal_map.as_deref(), Some("textures/fo4/glass_n.dds"));
        assert_eq!(m.glow_map.as_deref(), Some("textures/fo4/glass_g.dds"));
        assert_eq!(m.uv_scale, [2.0, 3.0]);
        assert_eq!(m.alpha, 0.25);
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
        // Worn/industrial metal stays rough, but must remain inside the
        // renderer's `< 0.6` environment-reflection tier.
        assert_eq!(metal.roughness, 0.55);

        let wood = classify(&m, "textures/clutter/barrel/barrel01.dds");
        assert_eq!(wood.metalness, 0.0);
        assert!(wood.roughness > 0.6);

        let glass = classify(&m, "textures/clutter/ICE/IceShard01.dds");
        assert!(glass.roughness < 0.2);
    }

    /// #3315 / FNV-D2-01 — FNV's whole weapon corpus lives under
    /// `weapons\\1handpistol\\`, `\\2handrifle\\`, `\\1handmelee\\`. Before the fix the
    /// unbounded `hand` substring pulled 3 458 real weapon meshes into the skin
    /// arm (roughness 0.50), which is ~91% of every SKIN classification on the
    /// game. The anatomy words must not fire outside a character/creature
    /// family — while real skin, which concatenates the same words, still must.
    #[test]
    fn classify_pbr_anatomy_words_require_a_character_asset_family() {
        let m = Material::default();

        // Real vanilla FNV/DLC weapon paths — none of these are skin.
        for path in [
            r"textures\dlc05\weapons\1handpistol\dlc05alienpistol.dds",
            r"textures\dlcanch\weapons\1handpistol\dlcanch1stperson10mmsilencer.dds",
            r"textures\weapons\2handrifle\varmintrifle.dds",
            r"textures\weapons\1handmelee\combatknife.dds",
        ] {
            let p = classify(&m, path);
            assert_ne!(
                p.roughness, 0.5,
                "{path} is a weapon, not skin — the `hand` substring must not reach the skin arm"
            );
        }

        // Other collisions in the same arm.
        for path in [
            r"textures\clutter\bobblehead\bobbleheadluck.dds",
            r"textures\interface\pipboy_vaultboy.dds",
        ] {
            let p = classify(&m, path);
            assert_ne!(p.roughness, 0.5, "{path} must not classify as skin");
        }

        // Real skin still classifies — note these concatenate the anatomy word
        // exactly the way the weapon paths do, which is why word-boundary
        // matching is the wrong remedy here.
        for path in [
            r"textures\characters\bodymods\falloutnv.esm\00132e91modbodymale.dds",
            r"textures\characters\facemods\falloutnv.esm\00176317_0.dds",
            r"textures\characters\male\femaleupperbody.dds",
            r"textures\actors\character\male\malehead.dds",
            r"textures\creatures\deathclaw\deathclawhand.dds",
        ] {
            let p = classify(&m, path);
            assert_eq!(
                p.roughness, 0.5,
                "{path} is character skin and must still reach the skin arm"
            );
        }

        // "skin" stays a global material word, family or not.
        let hide = classify(&m, r"textures\creatures\brahmin\brahminskin.dds");
        assert_eq!(hide.roughness, 0.5);
    }

    /// Mzulft's Dwemer architecture uses the `dwemerruins` directory for
    /// both bronze trim and ordinary stone. Explicit metal filenames must
    /// enter the reflection tier without letting that shared parent directory
    /// turn every wall and floor into a conductor.
    #[test]
    fn classify_pbr_dwemer_material_words_ignore_asset_family_directory() {
        let m = Material::default();
        let bronze = classify(&m, r"textures\dungeons\dwemerruins\dwemetaltiles01.dds");
        assert_eq!(bronze.metalness, 0.9);
        assert!(bronze.roughness < 0.6);

        for path in [
            r"textures\dungeons\dwemerruins\dwestonewall01.dds",
            r"textures\dungeons\dwemerruins\dwestonefloor02.dds",
            r"textures\dungeons\dwemerruins\dwekingrock01.dds",
        ] {
            let stone = classify(&m, path);
            assert_eq!(
                stone.metalness, 0.0,
                "the dwemerruins directory alone must not classify {path} as metal"
            );
        }
    }

    /// Prospector Saloon (`GSProspectorSaloonInterior`) bar-counter panel
    /// and Goodsprings/Megaton shack-wall siding both use
    /// `textures\architecture\megaton\metalscrap{panels,shingles,beams}*.dds`.
    /// These are metal receivers and must retain conductor response. The
    /// general metal arm now uses a brushed/oxidised roughness of 0.55, so
    /// the old matte-dielectric exception is no longer needed to avoid the
    /// pre-2026-06-03 chrome look.
    #[test]
    fn classify_pbr_scrap_metal_is_weathered_conductor() {
        let m = Material::default();
        let panel = classify(&m, r"textures\architecture\megaton\metalscrappanels04.dds");
        assert_eq!(panel.metalness, 0.9);
        assert_eq!(panel.roughness, 0.55);

        let shingles = classify(
            &m,
            r"textures\architecture\megaton\metalscrapshingles04.dds",
        );
        assert_eq!(shingles.metalness, panel.metalness);
        assert_eq!(shingles.roughness, panel.roughness);

        let beams = classify(&m, r"textures\architecture\megaton\metalscrapbeams01.dds");
        assert_eq!(beams.metalness, panel.metalness);
        assert_eq!(beams.roughness, panel.roughness);

        // Genuine bare-metal paths (no "scrap") are unaffected.
        let steel = classify(&m, r"textures\weapons\steel\barrel01.dds");
        assert!(steel.metalness > 0.8);
    }

    /// Both common scrap-metal token orders must reach the conductive-metal
    /// arm. A broad "scrap" dielectric exception used to make these paths
    /// order-dependent even though both describe the same receiver class.
    #[test]
    fn classify_pbr_bare_scrap_reaches_metal_arm() {
        let m = Material::default();
        let pile = classify(&m, r"textures\clutter\scrapmetal\scrapmetalpile01_d.dds");
        assert!(
            pile.metalness > 0.8,
            "scrap metal must reach the conductive-metal arm"
        );

        // Megaton's reversed token order has the same physical response.
        let cladding = classify(&m, r"textures\architecture\megaton\metalscrappanels04.dds");
        assert_eq!(cladding.metalness, pile.metalness);
        assert_eq!(cladding.roughness, pile.roughness);
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
        let mut m = Material {
            glossiness: 50.0,
            env_map_scale: 2.5,
            ..Material::default()
        };
        // Painted plastic wall panel: low specular (dielectric).
        // 2.5 = previously-clamped "power-armor tier" on the non-
        // keyword arm. Now plateaus at polished-plastic territory.
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
        let mut m = Material {
            glossiness: 50.0,
            env_map_scale: 0.5, // cushion-with-sheen tier — low specular, dielectric
            ..Material::default()
        };
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
    /// revert). At the time this landed, `env_map_scale = 1.0` was the
    /// `BSShaderPPLighting` on-disk default reaching nearly every FNV
    /// surface unconditionally; #2315 later gated that forwarding on an
    /// authored environment-mapping flag, making this input shape a
    /// minority case today (see #2555) — but the invariant this test
    /// pins is still correct and still needed for whichever surfaces
    /// (real env-mapped or otherwise) DO carry this value: it MUST clamp
    /// to the matte 0.8 ceiling — NOT fall through to the glossiness arm.
    /// A brief
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

    /// Regression for #2009 / MAT-D1-01 — the glass arm's "ice"/"gem"
    /// keywords are short enough to be substrings of ordinary English
    /// words that plausibly appear in legacy diffuse texture paths.
    /// Pre-fix these all misfired the glass arm (roughness=0.1),
    /// pushing them below the RT reflection gate for a spurious
    /// mirror/"wet floor" look.
    #[test]
    fn common_english_words_do_not_misfire_the_glass_arm() {
        for path in [
            "textures/architecture/office/officedesk01.dds",
            "textures/clutter/noticeboard01.dds",
            "textures/clutter/devicepanel01.dds",
            "textures/clutter/justicestatue01.dds",
            "textures/clutter/invoicepaper01.dds",
            "textures/plants/spice01.dds",
            "textures/clutter/voicebox01.dds",
            "textures/effects/twicebaked01.dds",
            "textures/clutter/advicecolumn01.dds",
            "textures/clutter/policeuniform01.dds",
            "textures/clutter/juicebottlecap01.dds",
            "textures/clutter/dicepair01.dds",
            "textures/clutter/slicebread01.dds",
        ] {
            let p = classify_pbr_keyword(PbrClassifierInputs {
                texture_path: Some(path),
                glossiness: 50.0,
                env_map_scale: 0.0,
                has_normal_map: false,
                specular_color: [0.2; 3],
                specular_authored: true,
                has_gloss_map: false,
            });
            assert!(
                p.roughness > 0.2,
                "'{path}' must NOT hit the glass arm (embedded \"ice\"/\"gem\" \
                 false positive); got roughness {}",
                p.roughness,
            );
        }
        assert!(
            !is_glass_keyword_path("textures/architecture/office/officedesk01.dds"),
            "render-gate predicate shares the same list and must reject it too"
        );
    }

    /// Regression for #2009 / MAT-D1-01 — the fabric arm's "fur"
    /// keyword is a literal prefix of "furniture", a common Bethesda
    /// clutter-asset directory/mesh token across every game.
    #[test]
    fn furniture_does_not_misfire_the_fabric_arm_via_fur() {
        let m = Material::default();
        let p = classify(&m, "meshes/furniture/genericfurniture01.nif");
        assert!(
            (p.roughness - 0.95).abs() > 1e-6,
            "furniture path must NOT hit the fabric arm via embedded \"fur\"; \
             got roughness {}",
            p.roughness,
        );

        // Genuine standalone "fur" still matches (real fur/pelt texture).
        let fur = classify(&m, "textures/creatures/wolf/wolf_fur01.dds");
        assert_eq!(
            fur.roughness, 0.95,
            "standalone fur texture must still match"
        );
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
        let m = Material {
            glossiness: 20.0,   // matte
            env_map_scale: 0.0, // disable env-map branch so glossiness wins
            ..Material::default()
        };
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
        // Seed the sentinel exactly as `translate_material` does for
        // legacy inline-shader content (no BGSM override).
        let mut m = Material {
            texture_path: Some(r"Textures\Weapons\Iron\IronSword.dds".to_string()),
            metalness: f32::NAN,
            roughness: f32::NAN,
            ..Material::default()
        };

        m.resolve_pbr();
        assert!(m.metalness > 0.8, "metal keyword routes to conductor");
        // Roughness raised from 0.3 → 0.6 (worn metal, not mirror chrome).
        assert!(m.roughness >= 0.5 && m.roughness < 0.8);
        assert!(m.metalness.is_finite() && m.roughness.is_finite());
    }

    #[test]
    fn resolve_pbr_is_idempotent() {
        let mut m = Material {
            texture_path: Some("textures/clutter/barrel/barrel01.dds".to_string()),
            metalness: f32::NAN,
            roughness: f32::NAN,
            ..Material::default()
        };
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
        let mut m = Material {
            texture_path: Some(r"Textures\Weapons\Iron\IronSword.dds".to_string()),
            metalness: 0.42,
            roughness: 0.13,
            ..Material::default()
        };

        m.resolve_pbr();
        assert_eq!(m.metalness, 0.42);
        assert_eq!(m.roughness, 0.13);
    }

    #[test]
    fn resolve_pbr_fills_only_missing_slot() {
        // Half-populated: one authored, the other a NaN sentinel. The
        // keyword fallback fills the gap without touching the populated
        // slot.
        let mut m = Material {
            texture_path: Some(r"Textures\Weapons\Iron\IronSword.dds".to_string()),
            metalness: 0.42,
            roughness: f32::NAN,
            ..Material::default()
        };

        m.resolve_pbr();
        assert_eq!(m.metalness, 0.42);
        assert!(m.roughness.is_finite());
    }

    #[test]
    fn resolve_pbr_clamps_authored_out_of_range() {
        // Authored BGSM values outside the renderer ranges are clamped
        // (replicating the pre-canonical render-time `classify_pbr`).
        let mut m = Material {
            metalness: 1.7,
            roughness: 0.0,
            ..Material::default()
        };
        m.resolve_pbr();
        assert_eq!(m.metalness, 1.0);
        assert_eq!(m.roughness, 0.04);
    }

    // ── `sanitize_finite` — #2687 (SAFE-D9-01), the save/restore boundary's
    //   NaN/Inf gate (repair half; `crates/save/src/validate.rs`'s
    //   pre-save check is the prevention half, via a clone-and-probe of
    //   this same method).

    #[test]
    fn sanitize_finite_repairs_metalness_and_roughness_via_resolve_pbr() {
        let mut m = Material {
            texture_path: Some(r"Textures\Weapons\Iron\IronSword.dds".to_string()),
            metalness: f32::NAN,
            roughness: f32::INFINITY,
            ..Material::default()
        };
        assert!(m.sanitize_finite());
        assert!(m.metalness.is_finite());
        assert!(m.roughness.is_finite());
        assert!(m.roughness <= 1.0, "resolve_pbr's clamp catches the +inf");
    }

    #[test]
    fn sanitize_finite_resets_other_scalars_to_default_independently() {
        // A poisoned `ior` must not disturb an otherwise-valid `alpha`,
        // and vice versa — every field is checked and reset on its own.
        let mut m = Material {
            ior: f32::NAN,
            alpha: 0.42,
            ..Material::default()
        };
        assert!(m.sanitize_finite());
        assert_eq!(m.ior, Material::default().ior);
        assert_eq!(
            m.alpha, 0.42,
            "a valid field must survive the sweep untouched"
        );
    }

    #[test]
    fn sanitize_finite_resets_a_poisoned_vec3_component_only() {
        let mut m = Material {
            specular_color: [1.0, f32::NAN, 0.5],
            ..Material::default()
        };
        assert!(m.sanitize_finite());
        let default_specular = Material::default().specular_color;
        assert_eq!(m.specular_color[0], 1.0, "clean component untouched");
        assert_eq!(m.specular_color[1], default_specular[1]);
        assert_eq!(m.specular_color[2], 0.5, "clean component untouched");
    }

    #[test]
    fn sanitize_finite_is_a_noop_on_an_already_clean_material() {
        let mut m = Material::default();
        assert!(!m.sanitize_finite());
        assert_eq!(m.diffuse_color, Material::default().diffuse_color);
    }

    #[test]
    fn sanitize_finite_handles_negative_infinity_too() {
        let mut m = Material {
            fresnel_power: f32::NEG_INFINITY,
            ..Material::default()
        };
        assert!(m.sanitize_finite());
        assert!(m.fresnel_power.is_finite());
    }

    /// #3373 — the BGEM glass-optics tail was added to `Material` after
    /// `sanitize_finite` and never wired into it, so a poisoned save carried
    /// NaN through both save-path gates into `GpuMaterial`.
    #[test]
    fn sanitize_finite_repairs_the_bgem_glass_optics_fields() {
        let mut m = Material {
            glass_fresnel_color: [f32::NAN, 0.5, f32::INFINITY],
            glass_refraction_scale: f32::NAN,
            glass_blur_scale: f32::NEG_INFINITY,
            glass_blur_scale_factor: f32::NAN,
            ..Material::default()
        };

        assert!(
            m.sanitize_finite(),
            "a poisoned glass tail must report repair"
        );

        let d = Material::default();
        assert_eq!(m.glass_fresnel_color[0], d.glass_fresnel_color[0]);
        assert_eq!(
            m.glass_fresnel_color[1], 0.5,
            "a finite component must survive untouched"
        );
        assert_eq!(m.glass_fresnel_color[2], d.glass_fresnel_color[2]);
        assert_eq!(m.glass_refraction_scale, d.glass_refraction_scale);
        assert_eq!(m.glass_blur_scale, d.glass_blur_scale);
        assert_eq!(m.glass_blur_scale_factor, d.glass_blur_scale_factor);
    }

    /// The whole-struct pin: poison **every** float field at once and require
    /// the material to come back finite. This is the guard that catches the
    /// #3373 defect *class* — a float field added to `Material` without a
    /// matching `fix_scalar!`/`fix_vec!` line — rather than only the four
    /// fields that were missing this time. Extend the literal below whenever
    /// `Material` gains a float.
    #[test]
    fn sanitize_finite_leaves_no_non_finite_float_anywhere() {
        let n = f32::NAN;
        let mut m = Material {
            metalness: n,
            roughness: n,
            emissive_color: [n; 3],
            emissive_mult: n,
            specular_color: [n; 3],
            specular_strength: n,
            diffuse_color: [n; 3],
            ambient_color: [n; 3],
            glossiness: n,
            uv_offset: [n; 2],
            uv_scale: [n; 2],
            alpha: n,
            env_map_scale: n,
            alpha_threshold: n,
            translucency_subsurface_color: [n; 3],
            translucency_transmissive_scale: n,
            translucency_turbulence: n,
            lighting_effect_1: n,
            lighting_effect_2: n,
            subsurface_rolloff: n,
            rimlight_power: n,
            backlight_power: n,
            fresnel_power: n,
            grayscale_to_palette_scale: n,
            ior: n,
            subsurface: n,
            sheen: n,
            sheen_tint: n,
            anisotropic: n,
            glass_fresnel_color: [n; 3],
            glass_refraction_scale: n,
            glass_blur_scale: n,
            glass_blur_scale_factor: n,
            ..Material::default()
        };

        assert!(m.sanitize_finite());

        let floats: Vec<(&str, f32)> = vec![
            ("metalness", m.metalness),
            ("roughness", m.roughness),
            ("emissive_mult", m.emissive_mult),
            ("specular_strength", m.specular_strength),
            ("glossiness", m.glossiness),
            ("alpha", m.alpha),
            ("env_map_scale", m.env_map_scale),
            ("alpha_threshold", m.alpha_threshold),
            (
                "translucency_transmissive_scale",
                m.translucency_transmissive_scale,
            ),
            ("translucency_turbulence", m.translucency_turbulence),
            ("lighting_effect_1", m.lighting_effect_1),
            ("lighting_effect_2", m.lighting_effect_2),
            ("subsurface_rolloff", m.subsurface_rolloff),
            ("rimlight_power", m.rimlight_power),
            ("backlight_power", m.backlight_power),
            ("fresnel_power", m.fresnel_power),
            ("grayscale_to_palette_scale", m.grayscale_to_palette_scale),
            ("ior", m.ior),
            ("subsurface", m.subsurface),
            ("sheen", m.sheen),
            ("sheen_tint", m.sheen_tint),
            ("anisotropic", m.anisotropic),
            ("glass_refraction_scale", m.glass_refraction_scale),
            ("glass_blur_scale", m.glass_blur_scale),
            ("glass_blur_scale_factor", m.glass_blur_scale_factor),
        ];
        for (name, v) in floats {
            assert!(v.is_finite(), "{name} left non-finite by sanitize_finite");
        }

        for (name, v) in [
            ("emissive_color", &m.emissive_color[..]),
            ("specular_color", &m.specular_color[..]),
            ("diffuse_color", &m.diffuse_color[..]),
            ("ambient_color", &m.ambient_color[..]),
            ("uv_offset", &m.uv_offset[..]),
            ("uv_scale", &m.uv_scale[..]),
            (
                "translucency_subsurface_color",
                &m.translucency_subsurface_color[..],
            ),
            ("glass_fresnel_color", &m.glass_fresnel_color[..]),
        ] {
            assert!(
                v.iter().all(|c| c.is_finite()),
                "{name} left non-finite by sanitize_finite"
            );
        }
    }

    /// #2591 (SKY-D7-03) — either a black color or a zero multiplier
    /// alone is sufficient to mark a contribution unauthored; both must
    /// be non-zero for "authored".
    #[test]
    fn emissive_contribution_is_authored_requires_both_nonzero_color_and_mult() {
        assert!(emissive_contribution_is_authored([0.5, 0.5, 0.5], 1.25));
        assert!(
            !emissive_contribution_is_authored([0.0, 0.0, 0.0], 1.0),
            "black color contributes nothing regardless of the multiplier"
        );
        assert!(
            !emissive_contribution_is_authored([0.5, 0.5, 0.5], 0.0),
            "a zero multiplier contributes nothing regardless of the color"
        );
        assert!(!emissive_contribution_is_authored([0.0, 0.0, 0.0], 0.0));
        // Partial-channel authoring still counts.
        assert!(emissive_contribution_is_authored([0.0, 0.2, 0.0], 1.0));
    }
}
