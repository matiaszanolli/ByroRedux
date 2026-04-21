//! Animated property components — set by the animation system each frame.
//!
//! Sparse — only entities with active non-transform animation channels need these.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::Vec3;

/// Animated visibility toggle. When false, the renderer skips this entity.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedVisibility(pub bool);
impl Component for AnimatedVisibility {
    type Storage = SparseSetStorage<Self>;
}

/// Animated alpha override (0.0–1.0). Used for material alpha animation.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedAlpha(pub f32);
impl Component for AnimatedAlpha {
    type Storage = SparseSetStorage<Self>;
}

/// Animated diffuse color (RGB). Driven by `NiMaterialColorController`
/// with `target_color = 0`. Pre-#517 this was a single `AnimatedColor`
/// slot shared by every color controller regardless of target — an
/// animated emissive and an animated diffuse on the same mesh fought
/// last-write-wins at the ECS layer, and emissive-intensity flashes
/// on neon signs / plasma weapons / VATS hits wrote into what the
/// renderer was going to read as the diffuse multiplier.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedDiffuseColor(pub Vec3);
impl Component for AnimatedDiffuseColor {
    type Storage = SparseSetStorage<Self>;
}

/// Animated ambient color (RGB). `NiMaterialColorController` target = 1.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedAmbientColor(pub Vec3);
impl Component for AnimatedAmbientColor {
    type Storage = SparseSetStorage<Self>;
}

/// Animated specular color (RGB). `NiMaterialColorController` target = 2.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedSpecularColor(pub Vec3);
impl Component for AnimatedSpecularColor {
    type Storage = SparseSetStorage<Self>;
}

/// Animated emissive color (RGB). `NiMaterialColorController` target = 3.
/// This is the slot the vast majority of animated-color content actually
/// wants — FNV neon signs, plasma weapon glow, muzzle flashes, magic-effect
/// color-over-lifetime.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedEmissiveColor(pub Vec3);
impl Component for AnimatedEmissiveColor {
    type Storage = SparseSetStorage<Self>;
}

/// Animated BSShaderProperty color (RGB). Driven by
/// `BSEffectShaderPropertyColorController` /
/// `BSLightingShaderPropertyColorController` — Skyrim+ shader-level
/// color override, not one of the legacy NiMaterial slots.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedShaderColor(pub Vec3);
impl Component for AnimatedShaderColor {
    type Storage = SparseSetStorage<Self>;
}
