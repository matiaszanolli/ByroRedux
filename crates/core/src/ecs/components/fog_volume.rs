//! Engine-native participating-medium volume.
//!
//! Legacy cell/weather depth ramps are converted once into the canonical
//! `FogMedium` resource used by the global froxel medium. `FogVolume` is the
//! complementary ECS representation for spatially bounded authored content:
//! particle smoke, fog billboards, and explicit volume meshes. Render code
//! transforms these local primitives to world space and injects their
//! extinction/scattering into the same froxel V-buffer as atmospheric fog.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::math::{Quat, Vec3};

/// One coherent local participating-medium region.
///
/// `bounds == None` is reserved for cell-scope sources. Runtime XCLL/WTHR fog
/// currently bypasses that form and reaches the renderer through the canonical
/// medium resource, so the froxel primitive collector consumes bounded volumes
/// only.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FogVolume {
    /// Local-space primitive bounds. `None` means cell scope.
    pub bounds: Option<FogBounds>,
    /// Extinction coefficient sigma_t in inverse metres.
    pub extinction_per_meter: f32,
    /// RGB single-scatter albedo. This permits authored smoke tint while
    /// preserving the physical `sigma_s = sigma_t * albedo` relationship.
    pub single_scatter_albedo: [f32; 3],
    /// Fraction of the normalized primitive radius occupied by the soft edge.
    /// `0` is a hard boundary; `1` fades from the primitive center outward.
    pub edge_softness: f32,
    /// Emitted radiance `L_e` in linear RGB (Rec. 709 primaries) — the
    /// emission source term of the radiative transfer equation.
    ///
    /// The shader multiplies this by the *locally evaluated* absorption
    /// coefficient `sigma_a = sigma_t * (1 - albedo)` rather than by a
    /// constant, so emission follows the same procedural density profile as
    /// extinction. That is both physically correct (hotter, denser soot
    /// radiates more) and what makes a flame's interior structure visible
    /// instead of reading as a uniform glowing blob.
    ///
    /// `[0.0; 3]` for passive media — fog, mist, and cooled smoke.
    ///
    /// The fire/smoke distinction is entirely a matter of these coefficients:
    /// a flame is a **high** `sigma_t`, **low** albedo, high `L_e` medium
    /// (soot hot enough to incandesce absorbs and emits, it barely scatters),
    /// while smoke is the same soot after cooling — moderate `sigma_t`,
    /// **high** albedo, zero `L_e`. There is no separate "fire system"; there
    /// is one medium whose coefficients follow its temperature.
    pub emissive_radiance: [f32; 3],
    /// Blackbody temperature in kelvin that produced [`Self::emissive_radiance`].
    ///
    /// Retained rather than discarded after the colour conversion because the
    /// cooling curve — not the colour — is the thing a fire simulation
    /// integrates. `0.0` means either a passive medium or authored
    /// non-thermal emission (magical fire, plasma) whose colour did not come
    /// from a blackbody spectrum.
    pub emission_temperature_k: f32,
    /// Provenance for diagnostics and future per-source tuning.
    pub source: FogSource,
}

impl FogVolume {
    /// Whether this volume carries finite, positive medium data suitable for
    /// GPU injection.
    pub fn is_renderable(self) -> bool {
        self.bounds.is_some()
            && self.extinction_per_meter.is_finite()
            && self.extinction_per_meter > 0.0
            && self
                .single_scatter_albedo
                .iter()
                .all(|component| component.is_finite())
            && self.emissive_radiance.iter().all(|c| c.is_finite())
    }

    /// Whether this volume contributes an emission source term.
    ///
    /// Passive media skip the emission path entirely, so ordinary fog and
    /// smoke cost nothing extra for the fire feature existing.
    pub fn is_emissive(self) -> bool {
        self.emissive_radiance
            .iter()
            .any(|component| component.is_finite() && *component > 0.0)
    }
}

/// Local-space extent for a bounded fog primitive.
///
/// The owning entity's `GlobalTransform` supplies the outer transform.
/// `center` and `rotation` allow an emitter-derived volume to follow the
/// particle plume rather than remaining centered on the emitter origin.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct FogBounds {
    pub center: Vec3,
    pub rotation: Quat,
    pub half_extents: Vec3,
    pub shape: FogShape,
}

impl FogBounds {
    /// Conservative local-space sphere radius used for CPU frustum culling.
    pub fn bounding_radius(self) -> f32 {
        match self.shape {
            FogShape::Sphere => self.half_extents.x.abs(),
            FogShape::Ellipsoid | FogShape::Box => self.half_extents.abs().length(),
        }
    }
}

/// Analytic density primitive evaluated by the froxel inject shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FogShape {
    Sphere,
    Ellipsoid,
    Box,
}

/// Provenance of a fog volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum FogSource {
    /// Cell-wide medium converted from XCLL/LGTM.
    Xcll,
    /// Per-node `NiFogProperty`.
    NiFog,
    /// Per-region medium.
    Regn,
    /// Weather-driven medium.
    Wthr,
    /// Alpha-blended fog plane or explicit authored volume mesh.
    AuthoredMesh,
    /// A legacy fog/smoke particle emitter replaced by a local volume.
    ParticleEmitter,
}

impl Component for FogVolume {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_physical_medium_is_renderable() {
        let fog = FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: Vec3::new(10.0, 20.0, 10.0),
                shape: FogShape::Ellipsoid,
            }),
            extinction_per_meter: 0.4,
            single_scatter_albedo: [0.8, 0.75, 0.7],
            edge_softness: 0.35,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::ParticleEmitter,
        };
        assert!(fog.is_renderable());
        assert_eq!(fog.source, FogSource::ParticleEmitter);
    }

    #[test]
    fn cell_scope_and_invalid_density_are_not_gpu_primitives() {
        let cell_scope = FogVolume {
            bounds: None,
            extinction_per_meter: 0.1,
            single_scatter_albedo: [0.9; 3],
            edge_softness: 0.0,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::Xcll,
        };
        assert!(!cell_scope.is_renderable());

        let invalid = FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: Vec3::ONE,
                shape: FogShape::Sphere,
            }),
            extinction_per_meter: f32::NAN,
            ..cell_scope
        };
        assert!(!invalid.is_renderable());
    }

    #[test]
    fn primitive_bound_radius_is_conservative() {
        let ellipsoid = FogBounds {
            center: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            half_extents: Vec3::new(3.0, 4.0, 12.0),
            shape: FogShape::Ellipsoid,
        };
        assert_eq!(ellipsoid.bounding_radius(), 13.0);

        let sphere = FogBounds {
            half_extents: Vec3::splat(7.0),
            shape: FogShape::Sphere,
            ..ellipsoid
        };
        assert_eq!(sphere.bounding_radius(), 7.0);
    }
}
