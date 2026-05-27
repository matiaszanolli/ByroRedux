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
    /// Translation-layer PBR metalness override `[0, 1]`. `None`
    /// falls through to the legacy keyword-classifier path inside
    /// [`Self::classify_pbr`] (correct for inline-shader NIF
    /// content — Oblivion / FO3 / FNV — where no scalar metalness
    /// signal was authored). `Some` is set by the BGSM / BGEM
    /// translator in `merge_bgsm_into_mesh` from authored
    /// `specular_color * specular_mult` luminance: dielectric spec
    /// (≈ 0.04) maps to `0.0`, conductor spec (≈ 0.95) to near `1.0`.
    /// The renderer uses this value as `GpuMaterial.metalness`
    /// directly — no shader-side branching on source format. See
    /// `feedback_format_translation.md`.
    pub metalness_override: Option<f32>,
    /// Translation-layer PBR roughness override `[0, 1]`. Companion
    /// to [`Self::metalness_override`]; set together by the BGSM
    /// translator as `1.0 - bgsm.smoothness` so authored smoothness
    /// drives the GGX lobe width directly without round-tripping
    /// through `glossiness / 100`. `None` keeps the legacy derivation.
    pub roughness_override: Option<f32>,
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
            metalness_override: None,
            roughness_override: None,
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
}

/// Keyword-based PBR classifier shared by `Material::classify_pbr`
/// (per-frame draw build) and the NIF importer's mesh extractors
/// (per-`ImportedMesh` translation). Single source of truth for the
/// rule that texture-path keywords drive metalness / glass / cloth
/// classification with glossiness + env_map_scale as the no-keyword
/// fallback. See `feedback_format_translation.md` for the architectural
/// directive.
///
/// Pure function — no `&self`, no allocations (matching uses
/// `contains_any_ci`'s windowed byte comparison).
pub fn classify_pbr_keyword(inputs: PbrClassifierInputs<'_>) -> PbrMaterial {
    let path = inputs.texture_path.unwrap_or("");

    if contains_any_ci(
        path,
        &["metal", "iron", "steel", "dwemer", "dwarven", "chainmail"],
    ) {
        return PbrMaterial {
            roughness: 0.3,
            metalness: 0.9,
        };
    }
    if contains_any_ci(path, &["gold", "silver", "bronze", "copper"]) {
        return PbrMaterial {
            roughness: 0.25,
            metalness: 0.95,
        };
    }
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

    // env_map_scale arm — fires ONLY when the artist cranked the cube-map
    // intensity meaningfully ABOVE the neutral baseline. Empirical
    // ground-truth (canonical-material pass, 2026-05-27 `material_dump`
    // sweep over 9 FNV clutter/architecture meshes): `BSShaderPPLighting`
    // authors `env_map_scale = 1.0` as the *neutral default* on nearly
    // every surface — glass bottles, drinking glasses, sandbags, tin
    // cans, labels all read exactly 1.0. The previous `> 0.3` gate
    // therefore caught the default and clamped EVERY non-keyword surface
    // to the 0.8 ceiling (`1 - 1.0*0.2 = 0.8`), discarding the real
    // per-surface signal: glossiness (the authored Phong specular power,
    // a 10/30/50/60 gradient on those same meshes). A glass bottle
    // (gloss 50) and a matte label (gloss 10) both collapsed to roughness
    // 0.8 — "everything is rough plastic," the root of FNV's degenerate
    // look (whiskey bottles not glassy, walls flat).
    //
    // Gating at `> 1.0` lets the neutral baseline fall through to the
    // authored-glossiness arm below (deriving roughness from real data
    // per `feedback_no_guessing`), while genuinely env-mapped surfaces —
    // the FNV/FO3 interior door panels / bulkhead trim the artist marked
    // reflective at `env_map_scale ≈ 2.5` — still sharpen here. The 0.35
    // floor keeps those dielectrics at polished-plastic, never mirror
    // chrome (user-reported chrome on FNV/FO3 wall panels 2026-05-25).
    if inputs.env_map_scale > 1.0 {
        return PbrMaterial {
            roughness: (1.0 - inputs.env_map_scale * 0.2).clamp(0.35, 0.8),
            metalness: 0.0,
        };
    }

    // Glossiness fallback — normal-map presence shifts macro
    // roughness down slightly to compensate for the added detail.
    let mut roughness = (1.0 - inputs.glossiness / 100.0).clamp(0.05, 0.95);
    if inputs.has_normal_map {
        roughness = (roughness - 0.1).max(0.05);
    }
    PbrMaterial {
        roughness,
        metalness: 0.0,
    }
}

impl Material {
    /// Explicit "this surface is glass / crystal / ice / gem / window"
    /// classifier for use by [`crate::ecs::components::Material`]-less
    /// glass-path gating in the renderer. Required because the
    /// glossiness-fallback in `classify_pbr` undershoots the 0.4
    /// roughness gate for Skyrim cloth banners (whose
    /// `BSLightingShaderProperty.glossiness ≈ 80` lands at
    /// roughness 0.2 via `1 - 80/100`), producing spurious glass
    /// classification that routes the cloth through the IOR
    /// refraction + chromatic-dispersion shader path → rainbow
    /// banners. This predicate requires an explicit texture-path
    /// keyword match, not just heuristic roughness, so unauthored /
    /// generic-path materials never trip the glass path. See
    /// Markarth probe 2026-05-13.
    pub fn path_indicates_glass(texture_path: Option<&str>) -> bool {
        let path = texture_path.unwrap_or("");
        contains_any_ci(
            path,
            &[
                "glass", "crystal", "ice", "gem", "window", "bottle", "jar", "vial",
            ],
        )
    }

    /// Infer PBR properties from legacy material data + texture path.
    ///
    /// **Translation-layer overrides win**: when `metalness_override`
    /// / `roughness_override` are set (i.e. the BGSM/BGEM translator
    /// already produced standardized PBR values from authored
    /// spec_color × smoothness), use them directly. The keyword
    /// classifier below is the legacy fallback for inline-shader NIF
    /// content (Oblivion / FO3 / FNV) where no scalar PBR signal was
    /// authored. This is the contract that lets the renderer stay
    /// format-agnostic — see `feedback_format_translation.md`.
    ///
    /// The texture path is the primary fallback signal — keywords like
    /// "metal", "glass", "wood" map to physically-plausible defaults.
    /// Final fallback uses the NIF glossiness value converted to
    /// roughness.
    ///
    /// Matching is case-insensitive but **does not allocate** — the
    /// previous `to_ascii_lowercase` copy ran per draw per frame (~39k
    /// allocations/sec on Prospector Saloon at 48 FPS). See #375.
    pub fn classify_pbr(&self, texture_path: Option<&str>) -> PbrMaterial {
        if let (Some(m), Some(r)) = (self.metalness_override, self.roughness_override) {
            return PbrMaterial {
                roughness: r.clamp(0.04, 1.0),
                metalness: m.clamp(0.0, 1.0),
            };
        }
        self.classify_pbr_from_path(texture_path)
    }

    /// Idempotent translation hook — eagerly populate
    /// `metalness_override` / `roughness_override` from the keyword
    /// classifier so the per-frame draw build hits the fast-path in
    /// [`Self::classify_pbr`] instead of re-scanning the texture path
    /// every frame. Call once at material-insert time
    /// (`cell_loader::spawn` / `scene::nif_loader`); BGSM-resolved
    /// overrides already in place are preserved unchanged.
    ///
    /// Per `feedback_format_translation.md` this is Stage 1 of pushing
    /// FO3 / FNV / Oblivion inline-shader content onto the same
    /// "single PBR contract" path BGSM-using FO4 / Skyrim use — every
    /// material lands at runtime with explicit `(metalness, roughness)`
    /// scalars, regardless of source format.
    pub fn resolve_classifier_overrides(&mut self) {
        if self.metalness_override.is_some() && self.roughness_override.is_some() {
            return;
        }
        let pbr = self.classify_pbr_from_path(self.texture_path.as_deref());
        self.metalness_override.get_or_insert(pbr.metalness);
        self.roughness_override.get_or_insert(pbr.roughness);
    }

    /// Internal — keyword classifier body without the override
    /// fast-path. Thin shim over the free
    /// [`classify_pbr_keyword`] so the per-frame draw build and the
    /// NIF importer share one classifier definition (Stage 2 of the
    /// `feedback_format_translation.md` rollout).
    fn classify_pbr_from_path(&self, texture_path: Option<&str>) -> PbrMaterial {
        classify_pbr_keyword(PbrClassifierInputs {
            texture_path,
            glossiness: self.glossiness,
            env_map_scale: self.env_map_scale,
            has_normal_map: self.normal_map.is_some(),
        })
    }
}

/// ASCII case-insensitive substring match. Zero allocations. Assumes
/// every keyword in `keywords` is non-empty and ASCII — both hold for
/// the hard-coded lists in [`Material::classify_pbr`].
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
        let metal = m.classify_pbr(Some(r"Textures\Weapons\Iron\IronSword.dds"));
        assert!(metal.metalness > 0.8);
        assert!(metal.roughness < 0.4);

        let wood = m.classify_pbr(Some("textures/clutter/barrel/barrel01.dds"));
        assert_eq!(wood.metalness, 0.0);
        assert!(wood.roughness > 0.6);

        let glass = m.classify_pbr(Some("textures/clutter/ICE/IceShard01.dds"));
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
        // 2.5 = previously-clamped "power-armor tier" on the non-
        // keyword arm. Now plateaus at polished-plastic territory.
        m.env_map_scale = 2.5;
        let p = m.classify_pbr(Some("textures/interior/wallpanel01.dds"));
        assert!(
            p.roughness >= 0.35,
            "non-keyword env_map_scale must not produce chrome floor; got {}",
            p.roughness,
        );
        assert_eq!(p.metalness, 0.0);

        // Extreme env_map_scale still bottoms at the new floor —
        // a dielectric never looks like a mirror.
        m.env_map_scale = 10.0;
        let p = m.classify_pbr(Some("textures/unknown/shiny.dds"));
        assert!(p.roughness >= 0.35);
        assert_eq!(p.metalness, 0.0);
    }

    #[test]
    fn classify_pbr_env_map_scale_does_not_imply_metalness() {
        let mut m = Material::default();
        m.glossiness = 50.0;
        m.env_map_scale = 0.5; // cushion-with-sheen tier
        let p = m.classify_pbr(Some("textures/clutter/medical/hospitalbed01.dds"));
        assert_eq!(
            p.metalness, 0.0,
            "env_map_scale must not drive metalness — that's the chrome-cushion bug"
        );
        // Roughness should drop relative to a no-envmap default (the
        // sheen IS visible, just as a dielectric lobe).
        assert!(p.roughness < 1.0);

        // Power-armor tier (env_map_scale ≈ 2.5) on a non-keyword path
        // also stays dielectric — the artist needs to put `metal` /
        // `armor` in the texture path to mark conductor authoring.
        m.env_map_scale = 2.5;
        let p = m.classify_pbr(Some("textures/clutter/unknown/shiny.dds"));
        assert_eq!(p.metalness, 0.0);
    }

    /// Canonical-material-pass regression (2026-05-27). Ground-truth
    /// `material_dump` over real FNV meshes showed `env_map_scale = 1.0`
    /// is the neutral `BSShaderPPLighting` default on essentially every
    /// surface — the old `> 0.3` env arm caught it and clamped ALL of
    /// them to roughness 0.8, throwing away the authored glossiness
    /// gradient. At the neutral baseline the classifier must instead
    /// derive roughness from glossiness, so a smoother surface (glass
    /// bottle, gloss 50) reads distinctly smoother than a matte one
    /// (label, gloss 10) instead of both collapsing to 0.8.
    #[test]
    fn classify_pbr_neutral_envmap_default_uses_glossiness_gradient() {
        let p10 = classify_pbr_keyword(PbrClassifierInputs {
            texture_path: Some("textures/clutter/liquorbottles/whiskeybottle01.dds"),
            glossiness: 10.0, // matte label / trim
            env_map_scale: 1.0, // neutral FNV default — must NOT preempt
            has_normal_map: false,
        });
        let p50 = classify_pbr_keyword(PbrClassifierInputs {
            texture_path: Some("textures/clutter/liquorbottles/whiskeybottle01.dds"),
            glossiness: 50.0, // glass body — smoother
            env_map_scale: 1.0,
            has_normal_map: false,
        });
        // Neither lands on the degenerate 0.8 env-ceiling…
        assert!(
            (p10.roughness - 0.8).abs() > 0.01 || (p50.roughness - 0.8).abs() > 0.01,
            "neutral env_map_scale=1.0 collapsed glossiness to the 0.8 ceiling \
             (p10={}, p50={})",
            p10.roughness,
            p50.roughness,
        );
        // …and the glossier surface is genuinely smoother.
        assert!(
            p50.roughness < p10.roughness,
            "gloss 50 ({}) must be smoother than gloss 10 ({})",
            p50.roughness,
            p10.roughness,
        );
        assert_eq!(p10.metalness, 0.0);
        assert_eq!(p50.metalness, 0.0);
    }

    #[test]
    fn classify_pbr_falls_back_to_glossiness() {
        let mut m = Material::default();
        m.glossiness = 20.0; // matte
        m.env_map_scale = 0.0; // disable env-map branch so glossiness wins
        let p = m.classify_pbr(Some("textures/unknown/thing.dds"));
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

    // ── `resolve_classifier_overrides` — Stage 1 of
    //   feedback_format_translation.md: every material lands at
    //   runtime with explicit PBR overrides so the per-frame draw
    //   build hits the override fast-path regardless of source
    //   format.

    #[test]
    fn resolve_classifier_overrides_populates_from_keyword_path() {
        let mut m = Material::default();
        m.texture_path = Some(r"Textures\Weapons\Iron\IronSword.dds".to_string());
        assert!(m.metalness_override.is_none());
        assert!(m.roughness_override.is_none());

        m.resolve_classifier_overrides();
        let metalness = m.metalness_override.expect("metalness populated");
        let roughness = m.roughness_override.expect("roughness populated");
        assert!(metalness > 0.8, "metal keyword routes to conductor");
        assert!(roughness < 0.4);

        // Subsequent draws hit the override fast-path — no string scan.
        let pbr = m.classify_pbr(m.texture_path.as_deref());
        assert!((pbr.metalness - metalness).abs() < 1e-6);
        assert!((pbr.roughness - roughness).abs() < 1e-6);
    }

    #[test]
    fn resolve_classifier_overrides_is_idempotent() {
        let mut m = Material::default();
        m.texture_path = Some("textures/clutter/barrel/barrel01.dds".to_string());
        m.resolve_classifier_overrides();
        let first_metal = m.metalness_override.unwrap();
        let first_rough = m.roughness_override.unwrap();

        m.resolve_classifier_overrides();
        assert_eq!(m.metalness_override.unwrap(), first_metal);
        assert_eq!(m.roughness_override.unwrap(), first_rough);
    }

    #[test]
    fn resolve_classifier_overrides_preserves_upstream_translator_values() {
        // BGSM merge layer ran first and wrote authoritative scalars;
        // the keyword classifier must NOT overwrite them.
        let mut m = Material::default();
        m.texture_path = Some(r"Textures\Weapons\Iron\IronSword.dds".to_string());
        m.metalness_override = Some(0.42);
        m.roughness_override = Some(0.13);

        m.resolve_classifier_overrides();
        assert_eq!(m.metalness_override, Some(0.42));
        assert_eq!(m.roughness_override, Some(0.13));
    }

    #[test]
    fn resolve_classifier_overrides_fills_only_missing_slot() {
        // Half-populated: BGSM wrote one but not the other (rare but
        // representable). The keyword fallback fills the gap without
        // touching the populated slot.
        let mut m = Material::default();
        m.texture_path = Some(r"Textures\Weapons\Iron\IronSword.dds".to_string());
        m.metalness_override = Some(0.42);
        m.roughness_override = None;

        m.resolve_classifier_overrides();
        assert_eq!(m.metalness_override, Some(0.42));
        assert!(m.roughness_override.is_some());
    }
}
