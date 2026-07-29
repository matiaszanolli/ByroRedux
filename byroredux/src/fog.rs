//! Legacy fog authoring → engine-native participating-medium conversion.
//!
//! Gamebryo and Creation records author a visibility ramp with near/far
//! distances. The volumetric renderer must never reinterpret that ramp per
//! pixel: conversion happens once when cell/weather data enters the ECS, and
//! runtime code consumes only [`FogMedium`].

use byroredux_core::ecs::{
    EmitterShape, FogBounds, FogShape, FogSource, FogVolume, ParticleEmitter,
};
use byroredux_core::math::{Quat, Vec3};

/// Bethesda world units per metre. Positions remain in Bethesda units in the
/// renderer and TLAS; physical medium coefficients are stored per metre.
pub(crate) const WORLD_UNITS_PER_METER: f32 = 70.0;

const FIT_SAMPLES: usize = 256;
const FIT_ITERATIONS: usize = 80;
const MIN_SIGMA_PER_METER: f32 = 1.0e-8;
const MAX_SIGMA_PER_METER: f32 = 10.0;

const LOCAL_VOLUME_ALBEDO_PEAK: f32 = 0.9;
const LOCAL_VOLUME_EDGE_SOFTNESS: f32 = 0.45;
const LOCAL_VOLUME_ALPHA_FALLBACK: f32 = 0.35;

/// Canonical, game-independent fog parameters stored on cell/weather runtime
/// resources. No legacy near/far distances participate in volumetric shading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FogMedium {
    /// Extinction coefficient σ_t in inverse metres.
    pub(crate) extinction_per_meter: f32,
    /// Single-scatter albedo ρ = σ_s / σ_t.
    pub(crate) single_scatter_albedo: f32,
    /// Nubis-style procedural occupancy control.
    pub(crate) coverage: f32,
}

impl FogMedium {
    pub(crate) const DISABLED: Self = Self {
        extinction_per_meter: 0.0,
        single_scatter_albedo: 0.95,
        coverage: 0.55,
    };

    /// Convert an authored visibility ramp into a homogeneous physical medium.
    ///
    /// The target transmittance is the legacy linear visibility curve over
    /// `[near, far]`, optionally capped by Skyrim's authored maximum opacity:
    ///
    /// `T_target(d) = 1 - max_opacity * (1 - (far-d)/(far-near))`
    ///
    /// We fit `exp(-σ_t d)` by weighted least squares with `1/d²` weights so
    /// near-field appearance dominates. A deterministic log-space golden
    /// search makes this conversion stable across platforms and cheap enough
    /// for load-time use.
    pub(crate) fn from_legacy_ramp(
        near_world: f32,
        far_world: f32,
        max_opacity: Option<f32>,
    ) -> Self {
        let extinction_per_meter = fit_legacy_fog_extinction(near_world, far_world, max_opacity);
        Self {
            extinction_per_meter,
            ..Self::DISABLED
        }
    }

    /// Blend canonical weather media directly. This intentionally does not
    /// reconstruct or interpolate legacy near/far ramps at runtime.
    pub(crate) fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            extinction_per_meter: lerp(self.extinction_per_meter, other.extinction_per_meter, t),
            single_scatter_albedo: lerp(self.single_scatter_albedo, other.single_scatter_albedo, t),
            coverage: lerp(self.coverage, other.coverage, t),
        }
    }
}

impl Default for FogMedium {
    fn default() -> Self {
        Self::DISABLED
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Select the legacy particle preset from both host and texture semantics.
///
/// Bethesda commonly names a particle host generically (`SuperSpray01-
/// Emitter`) while the sprite carries the only useful semantic
/// (`fxsmokewispsthin01.dds`). Texture intent therefore participates before
/// authored emitter overlays are applied. Smoke wins over a containing fire
/// node because the texture describes the actual emitted domain.
pub(crate) fn particle_preset(host_name: &str, texture_path: Option<&str>) -> ParticleEmitter {
    let host_name = host_name.to_ascii_lowercase();
    let texture_path = texture_path.unwrap_or("").to_ascii_lowercase();
    let contains_any = |tokens: &[&str]| {
        tokens
            .iter()
            .any(|token| host_name.contains(token) || texture_path.contains(token))
    };
    let has_component_prefix = |prefix: &str| {
        [&host_name, &texture_path].iter().any(|value| {
            value
                .split(|character: char| !character.is_ascii_alphanumeric())
                .any(|component| {
                    component.starts_with(prefix)
                        || component
                            .strip_prefix("fx")
                            .is_some_and(|suffix| suffix.starts_with(prefix))
                })
        })
    };

    if has_fog_token(&host_name) || has_fog_token(&texture_path) || has_component_prefix("ash") {
        ParticleEmitter::smoke()
    } else if contains_any(&["spark", "ember", "cinder"]) {
        ParticleEmitter::embers()
    } else if contains_any(&["torch", "fire", "flame", "brazier", "candle"]) {
        ParticleEmitter::torch_flame()
    } else if contains_any(&["magic", "enchant", "sparkle", "glow"]) {
        ParticleEmitter::magic_sparkles()
    } else {
        ParticleEmitter::torch_flame()
    }
}

/// Fit `exp(-σ_t d)` to a legacy linear fog ramp. Returns inverse metres.
pub(crate) fn fit_legacy_fog_extinction(
    near_world: f32,
    far_world: f32,
    max_opacity: Option<f32>,
) -> f32 {
    if !near_world.is_finite() || !far_world.is_finite() {
        return 0.0;
    }
    let near_m = near_world.max(0.0) / WORLD_UNITS_PER_METER;
    let far_m = far_world / WORLD_UNITS_PER_METER;
    if !far_m.is_finite() || far_m <= near_m + 1.0e-5 {
        return 0.0;
    }

    let max_opacity = max_opacity
        .filter(|value| value.is_finite())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    if max_opacity <= 0.0 {
        return 0.0;
    }

    // Search in log σ because shipped fog spans small interiors through
    // kilometre-scale exteriors. The objective is smooth and unimodal over
    // this physical range; golden-section search avoids derivative edge cases
    // near σ=0 and underflow at the far endpoint.
    let mut lo = MIN_SIGMA_PER_METER.ln();
    let mut hi = MAX_SIGMA_PER_METER.ln();
    let golden = (5.0_f32.sqrt() - 1.0) * 0.5;
    let mut x1 = hi - golden * (hi - lo);
    let mut x2 = lo + golden * (hi - lo);
    let mut f1 = weighted_fit_error(x1.exp(), near_m, far_m, max_opacity);
    let mut f2 = weighted_fit_error(x2.exp(), near_m, far_m, max_opacity);

    for _ in 0..FIT_ITERATIONS {
        if f1 > f2 {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + golden * (hi - lo);
            f2 = weighted_fit_error(x2.exp(), near_m, far_m, max_opacity);
        } else {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - golden * (hi - lo);
            f1 = weighted_fit_error(x1.exp(), near_m, far_m, max_opacity);
        }
    }

    let fitted = ((lo + hi) * 0.5).exp();
    let fitted_error = weighted_fit_error(fitted, near_m, far_m, max_opacity);
    let clear_error = weighted_fit_error(0.0, near_m, far_m, max_opacity);
    if fitted_error < clear_error && fitted.is_finite() {
        fitted
    } else {
        0.0
    }
}

fn weighted_fit_error(sigma: f32, near_m: f32, far_m: f32, max_opacity: f32) -> f32 {
    let span = far_m - near_m;
    let mut weighted_squared_error = 0.0;
    let mut weight_sum = 0.0;
    for sample in 0..FIT_SAMPLES {
        let u = (sample as f32 + 0.5) / FIT_SAMPLES as f32;
        let distance = near_m + span * u;
        let linear_visibility = 1.0 - u;
        let target_transmittance = 1.0 - max_opacity * (1.0 - linear_visibility);
        let predicted_transmittance = (-sigma * distance).exp();
        let weight = 1.0 / (distance * distance).max(1.0e-8);
        let error = predicted_transmittance - target_transmittance;
        weighted_squared_error += weight * error * error;
        weight_sum += weight;
    }
    weighted_squared_error / weight_sum.max(f32::MIN_POSITIVE)
}

/// Translate a legacy smoke/fog particle emitter into a physical local volume.
///
/// Classification deliberately requires both authored fog semantics (host or
/// texture naming) and alpha-over blending. Additive flame, ember, and magic
/// emitters retain their billboard path: replacing luminous particles with a
/// passive medium would discard their emissive intent.
pub(crate) fn fog_volume_from_particle(
    host_name: &str,
    emitter: &ParticleEmitter,
) -> Option<FogVolume> {
    if !is_fog_particle(
        host_name,
        emitter.texture_path.as_deref(),
        emitter.dst_blend,
    ) {
        return None;
    }

    let life = emitter.life.max(0.0);
    let speed = (emitter.speed.abs() + emitter.speed_variation.abs() * 0.5).max(0.0);
    let cone_angle = (emitter.declination.abs() + emitter.declination_variation.abs() * 0.5)
        .clamp(0.0, std::f32::consts::FRAC_PI_2);
    let vertical_distance = speed * cone_angle.cos() * life;
    let lateral_distance = speed * cone_angle.sin() * life;
    let gravity = Vec3::from_array(emitter.gravity);
    let displacement = Vec3::Y * vertical_distance + gravity * (0.5 * life * life);

    let spawn_half_extents = match emitter.shape {
        EmitterShape::Point => Vec3::ZERO,
        EmitterShape::Box { half_extents } => Vec3::from_array(half_extents).abs(),
        EmitterShape::Sphere { radius } => Vec3::splat(radius.abs()),
        EmitterShape::Cylinder { radius, height } => {
            Vec3::new(radius.abs(), height.abs() * 0.5, radius.abs())
        }
    };
    let particle_radius = ((emitter.start_size.abs() + emitter.start_size_variation.abs() * 0.5)
        .max(emitter.end_size.abs())
        * 0.5)
        .max(0.25);
    let mut half_extents =
        spawn_half_extents + Vec3::splat(particle_radius) + displacement.abs() * 0.5;
    half_extents.x += lateral_distance;
    half_extents.z += lateral_distance;
    half_extents = half_extents.max(Vec3::splat(0.25));

    // The legacy billboard alpha describes transmittance through the visible
    // particle footprint. Fold in expected live occupancy so a sparse emitter
    // does not become a solid ellipsoid, then recover sigma_t from Beer-Lambert.
    let endpoint_alpha = ((emitter.start_color[3] + emitter.end_color[3]) * 0.5).clamp(0.0, 0.98);
    // ImportedParticleEmitter currently retains only the first/last color
    // keys. Vanilla smoke often fades to zero at both endpoints while its
    // omitted interior keys and sprite alpha carry the visible density. A
    // semantically explicit alpha-over medium must not collapse to vacuum in
    // that representation; use a conservative proxy until texture-average
    // alpha is available directly at this boundary.
    let mean_alpha = if endpoint_alpha <= 1.0e-4 {
        LOCAL_VOLUME_ALPHA_FALLBACK
    } else {
        endpoint_alpha
    };
    let expected_live = emitter.rate.max(0.0) * life;
    let occupancy = if emitter.max_particles == 0 {
        0.0
    } else {
        (expected_live / emitter.max_particles as f32).clamp(0.05, 1.0)
    };
    let effective_alpha = (mean_alpha * occupancy).clamp(0.0, 0.98);
    if effective_alpha <= 1.0e-4 {
        return None;
    }
    let representative_width_m =
        (2.0 * half_extents.x.min(half_extents.z) / WORLD_UNITS_PER_METER).max(0.01);
    let extinction_per_meter =
        (-(1.0 - effective_alpha).ln() / representative_width_m).clamp(0.0, 20.0);

    let a0 = emitter.start_color[3].max(0.0);
    let a1 = emitter.end_color[3].max(0.0);
    let alpha_sum = a0 + a1;
    let tint = if alpha_sum > 1.0e-5 {
        Vec3::new(
            (emitter.start_color[0] * a0 + emitter.end_color[0] * a1) / alpha_sum,
            (emitter.start_color[1] * a0 + emitter.end_color[1] * a1) / alpha_sum,
            (emitter.start_color[2] * a0 + emitter.end_color[2] * a1) / alpha_sum,
        )
    } else {
        Vec3::ONE
    }
    .max(Vec3::ZERO);
    let tint_peak = tint.max_element().max(1.0e-4);
    let single_scatter_albedo =
        (tint / tint_peak * LOCAL_VOLUME_ALBEDO_PEAK).clamp(Vec3::ZERO, Vec3::splat(0.99));

    Some(FogVolume {
        bounds: Some(FogBounds {
            // Center the primitive on the swept particle path, not just the
            // emitter origin. Curl/noise modulation happens in the shader.
            center: displacement * 0.5,
            rotation: Quat::IDENTITY,
            half_extents,
            shape: FogShape::Ellipsoid,
        }),
        extinction_per_meter,
        single_scatter_albedo: single_scatter_albedo.to_array(),
        edge_softness: LOCAL_VOLUME_EDGE_SOFTNESS,
        source: FogSource::ParticleEmitter,
    })
}

fn is_fog_particle(host_name: &str, texture_path: Option<&str>, dst_blend: u8) -> bool {
    // Gamebryo destination factor 7 is ONE_MINUS_SRC_ALPHA. Restrict the
    // conversion to alpha-over media; destination ONE (0) is additive light.
    if dst_blend != 7 {
        return false;
    }
    has_fog_token(host_name) || has_fog_token(texture_path.unwrap_or(""))
}

/// Replace an alpha-over fog/smoke quad mesh with an extruded analytic medium.
///
/// Geometry supplies the authored silhouette/bounds; a thin axis is expanded
/// into a real volume so a legacy plane no longer depends on camera-facing
/// transparency. Vertex/material alpha seeds extinction, while procedural
/// density modulation remains shader-owned.
pub(crate) fn fog_volume_from_mesh(
    model_path: Option<&str>,
    texture_path: Option<&str>,
    mesh: &byroredux_nif::import::ImportedMesh,
) -> Option<FogVolume> {
    if mesh.material.dst_blend_mode != 7 || !mesh.material.has_alpha {
        return None;
    }
    let name = mesh.name.as_deref().unwrap_or("");
    if !has_fog_token(model_path.unwrap_or(""))
        && !has_fog_token(texture_path.unwrap_or(""))
        && !has_fog_token(name)
    {
        return None;
    }

    let first = Vec3::from_array(*mesh.positions.first()?);
    if !first.is_finite() {
        return None;
    }
    let (mut lower, mut upper) = (first, first);
    for position in &mesh.positions[1..] {
        let position = Vec3::from_array(*position);
        if !position.is_finite() {
            continue;
        }
        lower = lower.min(position);
        upper = upper.max(position);
    }
    let center = (lower + upper) * 0.5;
    let mut half_extents = (upper - lower).abs() * 0.5;
    let dominant_extent = half_extents.max_element();
    if dominant_extent <= 1.0e-4 {
        return None;
    }
    let extrusion = (dominant_extent * 0.15).max(4.0);
    if half_extents.x < extrusion {
        half_extents.x = extrusion;
    }
    if half_extents.y < extrusion {
        half_extents.y = extrusion;
    }
    if half_extents.z < extrusion {
        half_extents.z = extrusion;
    }

    let vertex_alpha = if mesh.colors.is_empty() {
        1.0
    } else {
        mesh.colors
            .iter()
            .map(|color| color[3].clamp(0.0, 1.0))
            .sum::<f32>()
            / mesh.colors.len() as f32
    };
    let effective_alpha =
        (vertex_alpha * mesh.material.mat_alpha.clamp(0.0, 1.0)).clamp(1.0e-4, 0.98);
    let thickness_m = (2.0 * half_extents.min_element() / WORLD_UNITS_PER_METER).max(0.01);
    let extinction_per_meter = (-(1.0 - effective_alpha).ln() / thickness_m).clamp(0.0, 20.0);

    let vertex_tint = if mesh.colors.is_empty() {
        Vec3::ONE
    } else {
        mesh.colors.iter().fold(Vec3::ZERO, |sum, color| {
            sum + Vec3::new(color[0], color[1], color[2])
        }) / mesh.colors.len() as f32
    };
    let material_tint = Vec3::from_array(mesh.material.diffuse_color).max(Vec3::ZERO);
    let tint = (vertex_tint * material_tint).max(Vec3::ZERO);
    let tint_peak = tint.max_element().max(1.0e-4);
    let single_scatter_albedo =
        (tint / tint_peak * LOCAL_VOLUME_ALBEDO_PEAK).clamp(Vec3::ZERO, Vec3::splat(0.99));

    Some(FogVolume {
        bounds: Some(FogBounds {
            center,
            rotation: Quat::IDENTITY,
            half_extents,
            shape: FogShape::Box,
        }),
        extinction_per_meter,
        single_scatter_albedo: single_scatter_albedo.to_array(),
        edge_softness: 0.35,
        source: FogSource::AuthoredMesh,
    })
}

pub(crate) fn has_fog_token(value: &str) -> bool {
    const FOG_TOKENS: [&str; 6] = ["fog", "smoke", "mist", "steam", "vapor", "cloud"];
    let value = value.to_ascii_lowercase();
    FOG_TOKENS.iter().any(|token| value.contains(token))
        || value
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|component| component.starts_with("dust") || component.starts_with("fxdust"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_or_disabled_ramps_produce_no_medium() {
        assert_eq!(fit_legacy_fog_extinction(100.0, 100.0, None), 0.0);
        assert_eq!(fit_legacy_fog_extinction(200.0, 100.0, None), 0.0);
        assert_eq!(fit_legacy_fog_extinction(f32::NAN, 100.0, None), 0.0);
        assert_eq!(fit_legacy_fog_extinction(0.0, 1000.0, Some(0.0)), 0.0);
    }

    #[test]
    fn fitted_medium_beats_clear_air_for_a_typical_fnv_cell() {
        let near_world = 64.0;
        let far_world = 4000.0;
        let sigma = fit_legacy_fog_extinction(near_world, far_world, None);
        let near_m = near_world / WORLD_UNITS_PER_METER;
        let far_m = far_world / WORLD_UNITS_PER_METER;
        assert!(sigma.is_finite() && sigma > 0.0);
        assert!(
            weighted_fit_error(sigma, near_m, far_m, 1.0)
                < weighted_fit_error(0.0, near_m, far_m, 1.0)
        );
    }

    #[test]
    fn near_field_weight_prevents_fitting_only_the_far_endpoint() {
        let near_world = 350.0;
        let far_world = 7000.0;
        let sigma = fit_legacy_fog_extinction(near_world, far_world, None);
        let near_m = near_world / WORLD_UNITS_PER_METER;
        let far_m = far_world / WORLD_UNITS_PER_METER;
        let far_endpoint_sigma = -(0.001_f32).ln() / far_m;
        let fitted_near_error = ((-sigma * near_m).exp() - 1.0).abs();
        let endpoint_near_error = ((-far_endpoint_sigma * near_m).exp() - 1.0).abs();
        assert!(fitted_near_error < endpoint_near_error);
    }

    #[test]
    fn lower_authored_max_opacity_fits_lower_extinction() {
        let full = fit_legacy_fog_extinction(0.0, 8000.0, Some(1.0));
        let capped = fit_legacy_fog_extinction(0.0, 8000.0, Some(0.5));
        assert!(capped > 0.0);
        assert!(capped < full);
    }

    #[test]
    fn canonical_weather_blend_never_reconstructs_a_legacy_ramp() {
        let day = FogMedium::from_legacy_ramp(0.0, 20_000.0, None);
        let night = FogMedium::from_legacy_ramp(0.0, 5000.0, None);
        let dusk = day.lerp(night, 0.5);
        assert_eq!(
            dusk.extinction_per_meter,
            (day.extinction_per_meter + night.extinction_per_meter) * 0.5
        );
        assert!(dusk.extinction_per_meter > day.extinction_per_meter);
        assert!(dusk.extinction_per_meter < night.extinction_per_meter);
    }

    #[test]
    fn smoke_particle_becomes_a_finite_ellipsoid() {
        let emitter = ParticleEmitter::smoke();
        let volume = fog_volume_from_particle("CampfireSmoke", &emitter).unwrap();
        let bounds = volume.bounds.unwrap();
        assert_eq!(bounds.shape, FogShape::Ellipsoid);
        assert!(bounds.half_extents.y > bounds.half_extents.x);
        assert!(volume.extinction_per_meter.is_finite());
        assert!(volume.extinction_per_meter > 0.0);
        assert!(volume.is_renderable());
    }

    #[test]
    fn additive_and_non_fog_particles_keep_billboards() {
        assert!(fog_volume_from_particle("FireSparks", &ParticleEmitter::embers()).is_none());
        assert!(
            fog_volume_from_particle("MagicGlow", &ParticleEmitter::magic_sparkles()).is_none()
        );

        let mut alpha_over_flame = ParticleEmitter::torch_flame();
        alpha_over_flame.dst_blend = 7;
        assert!(fog_volume_from_particle("TorchFlame", &alpha_over_flame).is_none());
    }

    #[test]
    fn generic_host_can_be_classified_by_authored_texture() {
        let mut emitter = ParticleEmitter::smoke();
        emitter.texture_path = Some("textures\\fx\\groundmist01.dds".to_string());
        assert!(fog_volume_from_particle("EmitterNode", &emitter).is_some());
    }

    #[test]
    fn smoke_texture_overrides_a_generic_particle_host() {
        let preset = particle_preset(
            "SuperSpray01-Emitter",
            Some("textures\\effects\\fxsmokewispsthin01.dds"),
        );
        assert_eq!(preset.dst_blend, 7);
        assert_eq!(preset.start_color, ParticleEmitter::smoke().start_color);

        let sparks = particle_preset("FireSparks", None);
        assert_eq!(sparks.start_size, ParticleEmitter::embers().start_size);
        assert_eq!(sparks.dst_blend, 0);

        let flash = particle_preset("EmitterNode", Some("textures\\effects\\flash01.dds"));
        assert_eq!(flash.dst_blend, ParticleEmitter::torch_flame().dst_blend);
    }

    #[test]
    fn zero_endpoint_smoke_curve_uses_a_finite_density_fallback() {
        let mut emitter = particle_preset(
            "SuperSpray01-Emitter",
            Some("textures\\effects\\fxsmokewispsthin01.dds"),
        );
        emitter.start_color[3] = 0.0;
        emitter.end_color[3] = 0.0;
        emitter.rate = 35.0;
        emitter.life = 8.333;
        emitter.start_size = 1.5;
        emitter.end_size = 1.5;

        let volume = fog_volume_from_particle("SuperSpray01-Emitter", &emitter).unwrap();
        assert!(volume.extinction_per_meter.is_finite());
        assert!(volume.extinction_per_meter > 0.0);
    }

    #[test]
    fn dust_requires_a_component_boundary() {
        assert!(has_fog_token("textures\\effects\\fxdustcloud01.dds"));
        assert!(has_fog_token("textures/effects/dust_plume.dds"));
        assert!(!has_fog_token(
            "textures\\dungeons\\industrial\\wastelandfakesky01.dds"
        ));
    }

    #[test]
    fn alpha_fog_quad_is_extruded_into_a_box_medium() {
        let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![
                [-20.0, 0.0, -10.0],
                [20.0, 0.0, -10.0],
                [20.0, 0.0, 10.0],
                [-20.0, 0.0, 10.0],
            ],
            vec![[0.7, 0.8, 0.9, 0.5]; 4],
            vec![[0.0, 1.0, 0.0]; 4],
            Vec::new(),
            vec![[0.0, 0.0]; 4],
            vec![0, 1, 2, 0, 2, 3],
        );
        mesh.material.has_alpha = true;
        mesh.material.dst_blend_mode = 7;
        let volume =
            fog_volume_from_mesh(Some("meshes\\fx\\fxmistlow01.nif"), None, &mesh).unwrap();
        let bounds = volume.bounds.unwrap();
        assert_eq!(bounds.shape, FogShape::Box);
        assert!(bounds.half_extents.y >= 4.0);
        assert!(volume.extinction_per_meter.is_finite());
    }

    #[test]
    fn ordinary_alpha_surface_is_not_reclassified_as_fog() {
        let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[1.0; 4]; 3],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![0, 1, 2],
        );
        mesh.material.has_alpha = true;
        mesh.material.dst_blend_mode = 7;
        assert!(fog_volume_from_mesh(
            Some("meshes\\architecture\\windowpane.nif"),
            Some("textures\\architecture\\window.dds"),
            &mesh,
        )
        .is_none());
    }
}
