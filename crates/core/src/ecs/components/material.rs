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
    /// Specular-mask / gloss texture — `NiTexturingProperty` slot 3.
    /// Per-texel specular strength mask; enables "leather with metal
    /// trim" effects on armor. See #214.
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
    /// Raw `BSLightingShaderProperty.shader_type` enum value (0–19).
    /// Plumbed through to `GpuInstance.material_kind` so the fragment
    /// shader can branch on the variant (SkinTint / HairTint /
    /// EyeEnvmap / SparkleSnow / MultiLayerParallax / …). 0 = Default
    /// lit — the safe fall-through for non-Skyrim+ meshes that have
    /// no BSLightingShaderProperty backing. Variant-specific shading
    /// is per-variant follow-up; this field just exposes the data so
    /// the next renderer milestone has something to consume. See #344.
    pub material_kind: u8,
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
}

impl Default for Material {
    fn default() -> Self {
        Self {
            emissive_color: [0.0, 0.0, 0.0],
            emissive_mult: 1.0,
            specular_color: [1.0, 1.0, 1.0],
            specular_strength: 1.0,
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
            z_test: true,
            z_write: true,
            z_function: 3, // LESSEQUAL — Gamebryo default
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

impl Material {
    /// Infer PBR properties from legacy material data + texture path.
    ///
    /// The texture path is the primary signal — keywords like "metal",
    /// "glass", "wood" map to physically-plausible defaults. Fallback
    /// uses the NIF glossiness value converted to roughness.
    ///
    /// Matching is case-insensitive but **does not allocate** — the
    /// previous `to_ascii_lowercase` copy ran per draw per frame (~39k
    /// allocations/sec on Prospector Saloon at 48 FPS). See #375.
    pub fn classify_pbr(&self, texture_path: Option<&str>) -> PbrMaterial {
        let path = texture_path.unwrap_or("");

        // Keyword-based classification (highest priority).
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
                "fabric", "cloth", "leather", "fur", "linen", "carpet", "rug", "tapestry",
                "banner", "curtain", "drape", "bedding", "pillow", "sack", "burlap", "wool",
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

        // Environment map scale as metalness proxy.
        if self.env_map_scale > 0.3 {
            return PbrMaterial {
                roughness: (1.0 - self.env_map_scale * 0.5).clamp(0.1, 0.8),
                metalness: self.env_map_scale.clamp(0.0, 1.0),
            };
        }

        // Fallback: convert glossiness to roughness.
        let roughness = (1.0 - self.glossiness / 100.0).clamp(0.05, 0.95);
        // Adjust if normal map is present (surface detail → slightly smoother macro).
        let roughness = if self.normal_map.is_some() {
            (roughness - 0.1).max(0.05)
        } else {
            roughness
        };
        PbrMaterial {
            roughness,
            metalness: 0.0,
        }
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

    #[test]
    fn classify_pbr_falls_back_to_glossiness() {
        let mut m = Material::default();
        m.glossiness = 20.0; // matte
        m.env_map_scale = 0.0; // disable env-map branch so glossiness wins
        let p = m.classify_pbr(Some("textures/unknown/thing.dds"));
        assert_eq!(p.metalness, 0.0);
        assert!(p.roughness > 0.5);
    }
}
