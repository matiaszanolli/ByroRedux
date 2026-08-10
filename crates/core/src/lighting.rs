//! Canonical units and emitter contracts shared by simulation and rendering.
//!
//! Game files are allowed to speak in Bethesda world units, normalized RGB,
//! and renderer-era flag words only at their import boundary. Runtime systems
//! consume the types in this module instead: distances are physical metres,
//! emitted energy is explicitly scene-linear radiant intensity, attenuation
//! declares its model, and ray visibility names geometry layers directly.

use std::ops::{BitOr, BitOrAssign};

/// The engine-wide physical scale for Gamebryo/Creation content.
///
/// One metre is approximately 70 Bethesda world units. Keeping this value in
/// `core` prevents physics, fog, lighting, and gameplay reach from quietly
/// acquiring independent copies of the same conversion.
pub const BETHESDA_UNITS_PER_METER: f32 = 70.0;
/// Legacy authored influence range → compatibility cull-range multiplier.
pub const LEGACY_LIGHT_CULL_RANGE_MULTIPLIER: f32 = 2.0;

/// A finite, non-negative distance in metres.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Meters(f32);

impl Meters {
    pub const ZERO: Self = Self(0.0);

    pub fn new(value: f32) -> Self {
        Self(if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        })
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    pub fn from_bethesda_units(value: f32) -> Self {
        Self::new(value / BETHESDA_UNITS_PER_METER)
    }

    pub fn to_bethesda_units(self) -> f32 {
        self.0 * BETHESDA_UNITS_PER_METER
    }
}

/// A finite, non-negative extinction coefficient in inverse metres.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct ExtinctionPerMeter(f32);

impl ExtinctionPerMeter {
    pub fn new(value: f32) -> Self {
        Self(if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        })
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    pub fn per_bethesda_unit(self) -> f32 {
        self.0 / BETHESDA_UNITS_PER_METER
    }
}

/// Linear-sRGB radiant intensity in the renderer's calibrated scene unit.
///
/// The dimensional quantity is W·sr⁻¹. Legacy games have no absolute power
/// field, so their translator currently uses the explicit compatibility
/// calibration `1 authored RGB unit = 1 scene W·sr⁻¹`; physical and procedural
/// emitters can provide measured/calculated values directly. The important
/// invariant is that the renderer never has to guess which interpretation a
/// raw colour triple carries.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct RadiantIntensityRgb([f32; 3]);

impl RadiantIntensityRgb {
    pub const BLACK: Self = Self([0.0; 3]);

    pub fn new(rgb: [f32; 3]) -> Self {
        Self(rgb.map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        }))
    }

    pub const fn get(self) -> [f32; 3] {
        self.0
    }

    pub fn scaled(self, scale: f32) -> [f32; 3] {
        let scale = if scale.is_finite() {
            scale.max(0.0)
        } else {
            0.0
        };
        self.0.map(|channel| channel * scale)
    }
}

/// Geometry categories an emitter is allowed to query for visibility.
///
/// These bits are deliberately the same width as Vulkan's TLAS instance mask.
/// A light can select any union of layers; there is no renderer-specific
/// `NONE/STRUCTURE/FULL` policy ladder and therefore no ambiguous conversion
/// from a legacy shadow-map allocation flag inside a shader.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct VisibilityMask(u8);

impl VisibilityMask {
    pub const NONE: Self = Self(0);
    pub const ARCHITECTURE: Self = Self(1 << 0);
    pub const STATIC_PROP: Self = Self(1 << 1);
    pub const DYNAMIC_ACTOR: Self = Self(1 << 2);
    pub const FOLIAGE: Self = Self(1 << 3);
    pub const GLASS: Self = Self(1 << 4);
    pub const EFFECT: Self = Self(1 << 5);

    pub const ALL_OPAQUE: Self =
        Self(Self::ARCHITECTURE.0 | Self::STATIC_PROP.0 | Self::DYNAMIC_ACTOR.0 | Self::FOLIAGE.0);
    pub const FULL: Self = Self(Self::ALL_OPAQUE.0 | Self::GLASS.0 | Self::EFFECT.0);

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & Self::FULL.0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Compatibility mapping performed once by a legacy importer.
    pub const fn for_legacy_projection(casts_full_scene_shadows: bool) -> Self {
        if casts_full_scene_shadows {
            Self::FULL
        } else {
            Self::ARCHITECTURE
        }
    }
}

impl BitOr for VisibilityMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::from_bits(self.0 | rhs.0)
    }
}

impl BitOrAssign for VisibilityMask {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

/// Distance law used by a local emitter.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum AttenuationModel {
    /// Compatibility curve for authored Gamebryo/Creation lights. It is
    /// bounded and normalized against the authored influence range.
    #[default]
    LegacySoftRange = 0,
    /// Radiant intensity divided by squared physical distance, softened by
    /// the finite emitter radius and faded only at the explicit cull range.
    InverseSquare = 1,
}

/// Canonical emitter geometry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum EmitterKind {
    #[default]
    Point,
    Spot,
    Directional,
    Ambient,
}

/// Canonical light-emitter state consumed by simulation and rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Emitter {
    pub kind: EmitterKind,
    pub radiant_intensity: RadiantIntensityRgb,
    /// Authored influence range. Directional emitters use zero.
    pub range: Meters,
    /// Radius of the actual luminous surface, not its influence range.
    pub source_radius: Meters,
    /// Unit direction toward the light for directional emitters, or outward
    /// from the source for spots. Point/ambient emitters use zero.
    pub direction: [f32; 3],
    /// Cosine of a spot's outer half-angle. Other kinds use zero.
    pub outer_cone_cos: f32,
    /// Compatibility curve shape; ignored by inverse-square emitters.
    pub falloff_exponent: f32,
    pub attenuation: AttenuationModel,
    pub visibility: VisibilityMask,
}

impl Default for Emitter {
    fn default() -> Self {
        Self {
            kind: EmitterKind::Point,
            radiant_intensity: RadiantIntensityRgb::new([1.0; 3]),
            range: Meters::ZERO,
            source_radius: Meters::ZERO,
            direction: [0.0; 3],
            outer_cone_cos: 0.0,
            falloff_exponent: 1.0,
            attenuation: AttenuationModel::LegacySoftRange,
            visibility: VisibilityMask::ARCHITECTURE,
        }
    }
}

impl Emitter {
    /// Translate one legacy light at its import boundary.
    pub fn from_legacy_world_units(
        range_world_units: f32,
        color: [f32; 3],
        kind: EmitterKind,
        direction: [f32; 3],
        outer_half_angle_radians: f32,
        falloff_exponent: f32,
        visibility: VisibilityMask,
    ) -> Self {
        let range = Meters::from_bethesda_units(range_world_units);
        let source_radius_world = if kind == EmitterKind::Directional {
            0.0
        } else {
            (range_world_units * 0.05).clamp(1.0, 32.0)
        };
        Self {
            kind,
            radiant_intensity: RadiantIntensityRgb::new(color),
            range,
            source_radius: Meters::from_bethesda_units(source_radius_world),
            direction: sanitize_direction(direction),
            outer_cone_cos: if kind == EmitterKind::Spot && outer_half_angle_radians.is_finite() {
                outer_half_angle_radians
                    .clamp(0.0, std::f32::consts::PI)
                    .cos()
            } else {
                0.0
            },
            falloff_exponent: if falloff_exponent.is_finite() && falloff_exponent > 0.0 {
                falloff_exponent
            } else {
                1.0
            },
            attenuation: AttenuationModel::LegacySoftRange,
            visibility,
        }
    }

    /// Construct a unit-explicit physical point emitter.
    pub fn inverse_square_point(
        radiant_intensity: RadiantIntensityRgb,
        range: Meters,
        source_radius: Meters,
        visibility: VisibilityMask,
    ) -> Self {
        Self {
            kind: EmitterKind::Point,
            radiant_intensity,
            range,
            source_radius,
            direction: [0.0; 3],
            outer_cone_cos: 0.0,
            falloff_exponent: 1.0,
            attenuation: AttenuationModel::InverseSquare,
            visibility,
        }
    }

    /// CPU reference for the distance law implemented by surface and froxel
    /// shaders. Angular emission and visibility are separate terms.
    pub fn distance_attenuation(self, distance: Meters) -> f32 {
        let distance = distance.get();
        let authored_range = self.range.get().max(self.source_radius.get());
        let cull_range = authored_range * LEGACY_LIGHT_CULL_RANGE_MULTIPLIER;
        let cull_window = 1.0
            - smoothstep(
                authored_range,
                cull_range.max(authored_range + f32::EPSILON),
                distance,
            );
        match self.attenuation {
            AttenuationModel::LegacySoftRange => {
                let normalized = distance / authored_range.max(f32::EPSILON);
                (1.0 / (1.0 + self.falloff_exponent * normalized * normalized)) * cull_window
            }
            AttenuationModel::InverseSquare => {
                let softened_distance_sq =
                    (distance * distance).max(self.source_radius.get().max(0.001).powi(2));
                cull_window / softened_distance_sq
            }
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn sanitize_direction(direction: [f32; 3]) -> [f32; 3] {
    if !direction.iter().all(|value| value.is_finite()) {
        return [0.0; 3];
    }
    let length_sq = direction.iter().map(|value| value * value).sum::<f32>();
    if length_sq <= 1.0e-12 {
        [0.0; 3]
    } else {
        let inv_length = length_sq.sqrt().recip();
        direction.map(|value| value * inv_length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bethesda_scale_round_trips_through_metres() {
        for world in [0.0, 1.0, 70.0, 512.0, 12_000.0] {
            let metres = Meters::from_bethesda_units(world);
            assert!((metres.to_bethesda_units() - world).abs() < 1.0e-4);
        }
    }

    #[test]
    fn visibility_layers_are_independent_eight_bit_tlas_bits() {
        let layers = [
            VisibilityMask::ARCHITECTURE,
            VisibilityMask::STATIC_PROP,
            VisibilityMask::DYNAMIC_ACTOR,
            VisibilityMask::FOLIAGE,
            VisibilityMask::GLASS,
            VisibilityMask::EFFECT,
        ];
        for (index, layer) in layers.iter().enumerate() {
            assert_ne!(layer.bits(), 0);
            assert_eq!(layer.bits().count_ones(), 1);
            for other in layers.iter().skip(index + 1) {
                assert_eq!(layer.bits() & other.bits(), 0);
            }
        }
        assert_eq!(VisibilityMask::FULL.bits(), 0x3f);
    }

    #[test]
    fn legacy_translation_resolves_units_shape_and_visibility_once() {
        let emitter = Emitter::from_legacy_world_units(
            512.0,
            [0.8, 0.4, 0.2],
            EmitterKind::Spot,
            [10.0, 0.0, 0.0],
            std::f32::consts::FRAC_PI_4,
            0.0,
            VisibilityMask::FULL,
        );
        assert!((emitter.range.get() - 512.0 / 70.0).abs() < 1.0e-6);
        assert!((emitter.source_radius.to_bethesda_units() - 25.6).abs() < 1.0e-5);
        assert_eq!(emitter.direction, [1.0, 0.0, 0.0]);
        assert!((emitter.outer_cone_cos - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
        assert_eq!(emitter.falloff_exponent, 1.0);
        assert_eq!(emitter.visibility, VisibilityMask::FULL);
    }

    #[test]
    fn inverse_square_emitters_keep_physical_source_size() {
        let emitter = Emitter::inverse_square_point(
            RadiantIntensityRgb::new([12.0, 6.0, 2.0]),
            Meters::new(20.0),
            Meters::new(0.08),
            VisibilityMask::FULL,
        );
        assert_eq!(emitter.attenuation, AttenuationModel::InverseSquare);
        assert_eq!(emitter.source_radius, Meters::new(0.08));
    }

    #[test]
    fn inverse_square_reference_conserves_flux_before_the_cull_window() {
        let emitter = Emitter::inverse_square_point(
            RadiantIntensityRgb::new([12.0; 3]),
            Meters::new(20.0),
            Meters::new(0.05),
            VisibilityMask::FULL,
        );
        let near = emitter.distance_attenuation(Meters::new(2.0));
        let far = emitter.distance_attenuation(Meters::new(4.0));
        assert!((far / near - 0.25).abs() < 1.0e-6);
        let near_flux = near * 4.0 * std::f32::consts::PI * 2.0f32.powi(2);
        let far_flux = far * 4.0 * std::f32::consts::PI * 4.0f32.powi(2);
        assert!((near_flux - far_flux).abs() < 1.0e-5);
    }

    #[test]
    fn candle_room_legacy_visibility_is_resolved_at_import() {
        let fill = Emitter::from_legacy_world_units(
            350.0,
            [1.0, 0.55, 0.2],
            EmitterKind::Point,
            [0.0; 3],
            0.0,
            1.0,
            VisibilityMask::for_legacy_projection(false),
        );
        assert_eq!(fill.visibility, VisibilityMask::ARCHITECTURE);
        assert!(!fill.visibility.contains(VisibilityMask::STATIC_PROP));

        let shadowed = Emitter {
            visibility: VisibilityMask::for_legacy_projection(true),
            ..fill
        };
        assert!(shadowed.visibility.contains(VisibilityMask::STATIC_PROP));
        assert!(shadowed.visibility.contains(VisibilityMask::DYNAMIC_ACTOR));
        assert!(shadowed.visibility.contains(VisibilityMask::GLASS));
    }
}
