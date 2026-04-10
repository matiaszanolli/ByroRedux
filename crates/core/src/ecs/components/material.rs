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
            alpha_test: false,
            alpha_threshold: 0.0,
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
    pub fn classify_pbr(&self, texture_path: Option<&str>) -> PbrMaterial {
        let path = texture_path
            .unwrap_or("")
            .to_ascii_lowercase();

        // Keyword-based classification (highest priority).
        if contains_any(&path, &["metal", "iron", "steel", "dwemer", "dwarven", "chainmail"]) {
            return PbrMaterial { roughness: 0.3, metalness: 0.9 };
        }
        if contains_any(&path, &["gold", "silver", "bronze", "copper"]) {
            return PbrMaterial { roughness: 0.25, metalness: 0.95 };
        }
        if contains_any(&path, &["glass", "crystal", "ice", "gem"]) {
            return PbrMaterial { roughness: 0.1, metalness: 0.0 };
        }
        if contains_any(&path, &["wood", "plank", "barrel", "crate", "log"]) {
            return PbrMaterial { roughness: 0.7, metalness: 0.0 };
        }
        if contains_any(&path, &["stone", "rock", "cave", "brick", "ruins", "cobble"]) {
            return PbrMaterial { roughness: 0.85, metalness: 0.0 };
        }
        if contains_any(&path, &["fabric", "cloth", "leather", "fur", "linen", "carpet"]) {
            return PbrMaterial { roughness: 0.9, metalness: 0.0 };
        }
        if contains_any(&path, &["skin", "body", "head", "hand", "face"]) {
            return PbrMaterial { roughness: 0.5, metalness: 0.0 };
        }
        if contains_any(&path, &["hair"]) {
            return PbrMaterial { roughness: 0.6, metalness: 0.0 };
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

fn contains_any(path: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| path.contains(kw))
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
    }
}
