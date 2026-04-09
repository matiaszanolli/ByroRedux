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
