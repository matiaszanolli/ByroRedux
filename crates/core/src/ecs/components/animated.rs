//! Animated property components — set by the animation system each frame.
//!
//! Sparse — only entities with active non-transform animation channels need these.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::{Vec2, Vec3};

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

/// Animated UV transform: per-axis offset + per-axis scale + rotation.
/// Driven by `NiTextureTransformController` (channel index in
/// 0..=4 maps to `UvOffsetU` / `UvOffsetV` / `UvScaleU` / `UvScaleV`
/// / `UvRotation` — each animated independently). Pre-#525 every
/// `FloatTarget::UvOffset*` / `FloatTarget::UvScale*` /
/// `FloatTarget::UvRotation` sample was computed by `animation_system`
/// then dropped on the floor because the only `FloatTarget` arm with
/// a sink was `Alpha` — animated water / lava / conveyor belts /
/// flickering HUD backdrops were silently static.
///
/// Identity defaults (`offset = (0, 0)`, `scale = (1, 1)`,
/// `rotation = 0`) so the component can be inserted with all five
/// slots zeroed and the animation system fills in only the slots the
/// active controller drives. The renderer reads `offset` / `scale`
/// to override the static `Material::uv_offset` / `Material::uv_scale`
/// at draw-command build time. `rotation` is captured here pending
/// shader-side `mat2x2` UV rotation support.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedUvTransform {
    pub offset: Vec2,
    pub scale: Vec2,
    pub rotation: f32,
}

impl AnimatedUvTransform {
    /// Identity defaults — surface no animation when no float channel
    /// has written a slot yet. The renderer's `mat.uv_offset` /
    /// `mat.uv_scale` defaults match this exactly so an entity carrying
    /// an as-yet-unwritten `AnimatedUvTransform` renders identically
    /// to one without the component.
    pub fn identity() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
        }
    }
}

impl Default for AnimatedUvTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl Component for AnimatedUvTransform {
    type Storage = SparseSetStorage<Self>;
}

/// Per-target morph-target weights. Index = morph-target slot from
/// `FloatTarget::MorphWeight(idx)`. Vec resizes on first write past
/// the current length (zero-padded). Driven by
/// `NiGeomMorpherController` for FaceGen lip-sync, talking-head
/// animations, and bake-driven facial morphs.
///
/// Pre-#525 every `MorphWeight(idx)` sample was dropped because the
/// only `FloatTarget` sink was `Alpha`. The component lands the
/// values today; the renderer's morph-target mesh deformation is
/// downstream work — same precedent as `AnimatedAlpha`, which is
/// likewise written by the animation system pending consumer wiring.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedMorphWeights(pub Vec<f32>);

impl AnimatedMorphWeights {
    /// Read a morph weight, returning `0.0` when `index` is past the
    /// vec's current length. Mirrors the legacy engine's behaviour
    /// where unset morph slots default to zero contribution.
    pub fn get(&self, index: usize) -> f32 {
        self.0.get(index).copied().unwrap_or(0.0)
    }

    /// Write a morph weight at `index`, growing the vec with `0.0`
    /// padding if necessary. Stable across re-runs because the
    /// morph-target index range comes from the source NIF and the
    /// animation system always writes the same slot per channel.
    pub fn set(&mut self, index: usize, value: f32) {
        if self.0.len() <= index {
            self.0.resize(index + 1, 0.0);
        }
        self.0[index] = value;
    }
}

impl Component for AnimatedMorphWeights {
    type Storage = SparseSetStorage<Self>;
}

/// Animated shader-float scalar. Driven by
/// `BSLightingShaderPropertyFloatController` /
/// `BSEffectShaderPropertyFloatController` — Skyrim+ per-shader
/// scalar uniform (alpha multiplier on effect shaders, refraction
/// scale, parallax scale, etc.) animated over time.
///
/// Today's renderer doesn't multiplex per-named-uniform shader floats,
/// so a single scalar slot covers every BSShader-float controller
/// without disambiguating the target uniform — extend to a
/// `(FixedString, f32)` map when shader-side per-uniform dispatch
/// lands. The audit recommendation lists the future shape; the
/// component name is forward-compatible with that growth.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct AnimatedShaderFloat(pub f32);
impl Component for AnimatedShaderFloat {
    type Storage = SparseSetStorage<Self>;
}
