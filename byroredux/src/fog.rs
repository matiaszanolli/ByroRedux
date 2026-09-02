//! Legacy fog authoring → engine-native participating-medium conversion.
//!
//! Gamebryo and Creation records author a visibility ramp with near/far
//! distances. The volumetric renderer must never reinterpret that ramp per
//! pixel: conversion happens once when cell/weather data enters the ECS, and
//! runtime code consumes only [`FogMedium`].

use byroredux_core::combustion::CombustionRegime;
use byroredux_core::ecs::{
    CombustionState, EmitterShape, FogBounds, FogProfile, FogShape, FogSource, FogVolume,
    ParticleEmitter,
};
use byroredux_core::math::{Quat, Vec3};

/// Bethesda world units per metre. Positions remain in Bethesda units in the
/// renderer and TLAS; physical medium coefficients are stored per metre.
pub(crate) const WORLD_UNITS_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

const FIT_SAMPLES: usize = 256;
const FIT_ITERATIONS: usize = 80;
const MIN_SIGMA_PER_METER: f32 = 1.0e-8;
const MAX_SIGMA_PER_METER: f32 = 10.0;

const LOCAL_VOLUME_ALBEDO_PEAK: f32 = 0.9;
const LOCAL_VOLUME_EDGE_SOFTNESS: f32 = 0.45;
const LOCAL_VOLUME_ALPHA_FALLBACK: f32 = 0.35;
const SMOKE_FALLBACK_ALBEDO: [f32; 3] = [0.72, 0.67, 0.61];

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
    } else if contains_any(&[
        "explosion",
        "explode",
        "detonate",
        "fireball",
        "shockwave",
        "blast",
    ]) {
        ParticleEmitter::explosion()
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

/// The swept volume a particle emitter's plume occupies over one particle
/// lifetime, in the emitter's local frame.
///
/// Derived from emitter shape, lifetime, launch speed, cone angle, gravity and
/// particle size — i.e. from where the particles actually go, not from an
/// authored bounding box. Shared by the smoke and fire translations so the two
/// cannot drift into disagreeing about the same emitter's extent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlumeBounds {
    /// Offset from the emitter origin to the swept path's midpoint.
    pub(crate) center: Vec3,
    /// Half extents of the swept ellipsoid.
    pub(crate) half_extents: Vec3,
}

pub(crate) fn plume_bounds(emitter: &ParticleEmitter) -> PlumeBounds {
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

    PlumeBounds {
        // Center the primitive on the swept particle path, not just the
        // emitter origin. Curl/noise modulation happens in the shader.
        center: displacement * 0.5,
        half_extents,
    }
}

/// Translate a particle emitter into whichever participating medium it
/// describes, or `None` to leave it on the billboard path.
///
/// Smoke is tried first: the two classifiers key on disjoint blend modes
/// (alpha-over versus additive) so they cannot both match, but ordering them
/// explicitly keeps that a property of this function rather than an accident
/// of the token lists.
pub(crate) fn medium_from_particle(
    host_name: &str,
    emitter: &ParticleEmitter,
) -> Option<FogVolume> {
    fog_volume_from_particle(host_name, emitter)
        .or_else(|| explosion_volume_from_particle(host_name, emitter))
        .or_else(|| fire_volume_from_particle(host_name, emitter))
}

/// Build the one-shot timeline that makes an explosion genuinely transient.
/// Persistent fire/smoke return `None`; only an impulse needs authored age.
pub(crate) fn combustion_state_from_particle(
    volume: FogVolume,
    emitter: &ParticleEmitter,
    start_time_seconds: f32,
) -> Option<CombustionState> {
    volume
        .profile
        .is_explosion()
        .then(|| CombustionState::one_shot(start_time_seconds, emitter.life))
}

/// Whether additive thermal emitters are replaced by emissive media.
///
/// Set `BYRO_FIRE_VOLUMES=0` to keep them on the legacy billboard path — an
/// A/B escape hatch for comparing the volumetric fire against the sprites it
/// replaces without a rebuild.
fn fire_volumes_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| !matches!(std::env::var("BYRO_FIRE_VOLUMES").as_deref(), Ok("0")))
}

/// Translate a legacy smoke/fog particle emitter into a physical local volume.
///
/// Classification deliberately requires both authored fog semantics (host or
/// texture naming) and alpha-over blending. Additive magic emitters retain
/// their billboard path; additive *thermal* emitters (flame, ember) are
/// handled by [`fire_volume_from_particle`] instead, which gives them an
/// emission source term rather than discarding their luminous intent.
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
    let PlumeBounds {
        center,
        half_extents,
    } = plume_bounds(emitter);

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
    let single_scatter_albedo = if alpha_sum > 1.0e-5 {
        let tint = Vec3::new(
            (emitter.start_color[0] * a0 + emitter.end_color[0] * a1) / alpha_sum,
            (emitter.start_color[1] * a0 + emitter.end_color[1] * a1) / alpha_sum,
            (emitter.start_color[2] * a0 + emitter.end_color[2] * a1) / alpha_sum,
        )
        .max(Vec3::ZERO);
        let tint_peak = tint.max_element().max(1.0e-4);
        (tint / tint_peak * LOCAL_VOLUME_ALBEDO_PEAK).clamp(Vec3::ZERO, Vec3::splat(0.99))
    } else {
        // Missing interior colour keys must not turn soot into white steam.
        // This is an absolute albedo, not a tint normalized back to 0.9.
        Vec3::from_array(SMOKE_FALLBACK_ALBEDO)
    };

    Some(FogVolume {
        bounds: Some(FogBounds {
            center,
            rotation: Quat::IDENTITY,
            half_extents,
            shape: FogShape::Ellipsoid,
        }),
        extinction_per_meter,
        single_scatter_albedo: single_scatter_albedo.to_array(),
        edge_softness: LOCAL_VOLUME_EDGE_SOFTNESS,
        profile: FogProfile::Smoke,
        // Alpha-over smoke is passive: cooled soot scatters, it does not
        // radiate. The emissive path belongs to `fire_volume_from_particle`.
        emissive_radiance: [0.0; 3],
        emission_temperature_k: 0.0,
        source: FogSource::ParticleEmitter,
    })
}

/// Optical depth across the width of a flame primitive.
///
/// Unlike smoke, an additive emitter's authored alpha encodes brightness, not
/// opacity, so the Beer-Lambert inversion the smoke path uses has nothing
/// meaningful to invert. Anchor on the directly observable fact instead: a
/// flame is optically thin — you can see through one — which puts its optical
/// depth well below 1. Deriving `sigma_t` from this keeps a small candle and a
/// large bonfire equally translucent instead of making the bonfire opaque
/// purely because it is bigger.
const FLAME_OPTICAL_DEPTH: f32 = 0.4;

/// Short-lived explosion cores are hotter and brighter than an open wood
/// flame before expansion cools them into smoke in the GPU profile.
const EXPLOSION_OPTICAL_DEPTH: f32 = 0.9;

/// Build the shared-medium representation of an impulsive combustion event.
/// The fixed coefficients describe its initial hot state; the explosion GPU
/// profile evolves density, temperature, scattering, and emission together.
pub(crate) fn explosion_volume_from_particle(
    host_name: &str,
    emitter: &ParticleEmitter,
) -> Option<FogVolume> {
    if !fire_volumes_enabled() || !is_explosion_particle(host_name, emitter.texture_path.as_deref())
    {
        return None;
    }

    let PlumeBounds {
        center,
        half_extents,
    } = plume_bounds(emitter);
    // The impulse starts at the emitter origin. Preserve the entire swept
    // particle plume by enlarging the sphere instead of displacing its hot
    // core halfway up the future trajectory.
    let radius = (half_extents.max_element() + center.length()).max(0.25);
    let representative_width_m = (2.0 * radius / WORLD_UNITS_PER_METER).max(0.01);
    let extinction_per_meter = (EXPLOSION_OPTICAL_DEPTH / representative_width_m)
        .clamp(MIN_SIGMA_PER_METER, MAX_SIGMA_PER_METER);
    let profile = explosion_profile_for_particle(host_name, emitter.texture_path.as_deref())?;
    let regime = if profile.is_nuclear_explosion() {
        CombustionRegime::NUCLEAR_EXPLOSION
    } else {
        CombustionRegime::EXPLOSION
    };
    let emissive_radiance = regime.emissive_radiance()?;

    Some(FogVolume {
        bounds: Some(FogBounds {
            center: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            half_extents: Vec3::splat(radius),
            shape: FogShape::Sphere,
        }),
        extinction_per_meter,
        single_scatter_albedo: regime.single_scatter_albedo(),
        edge_softness: 0.3,
        profile,
        emissive_radiance,
        emission_temperature_k: regime.temperature_k(),
        source: FogSource::ParticleEmitter,
    })
}

/// Translate an additive thermal particle emitter into an emissive medium.
///
/// This is the fire counterpart of [`fog_volume_from_particle`], and the two
/// deliberately produce the *same kind of object*. A flame and its smoke are
/// one physical material — soot — separated only by temperature: hot soot has
/// low albedo and a large emission term, cooled soot has high albedo and none.
/// Modelling them as one medium with temperature-dependent coefficients is
/// what lets a fire, its plume, and (later) an explosion's fireball share a
/// single render path instead of three bespoke effects.
///
/// Returns `None` for anything that is not an additive, thermally-named
/// emitter — magic sparkles stay billboards, since their colour comes from an
/// authored palette rather than a blackbody spectrum and inventing a
/// temperature for them would be fabrication.
pub(crate) fn fire_volume_from_particle(
    host_name: &str,
    emitter: &ParticleEmitter,
) -> Option<FogVolume> {
    if !fire_volumes_enabled() {
        return None;
    }
    let regime = fire_regime(host_name, emitter.texture_path.as_deref())?;

    let PlumeBounds {
        center,
        half_extents,
    } = plume_bounds(emitter);

    // Optical depth is defined across the primitive's narrowest horizontal
    // span — the direction a viewer most often looks through a flame.
    let representative_width_m =
        (2.0 * half_extents.x.min(half_extents.z) / WORLD_UNITS_PER_METER).max(0.01);
    let extinction_per_meter = (FLAME_OPTICAL_DEPTH / representative_width_m)
        .clamp(MIN_SIGMA_PER_METER, MAX_SIGMA_PER_METER);

    let emissive_radiance = regime.emissive_radiance()?;

    Some(FogVolume {
        bounds: Some(FogBounds {
            center,
            rotation: Quat::IDENTITY,
            half_extents,
            shape: FogShape::Ellipsoid,
        }),
        extinction_per_meter,
        // Deliberately NOT the authored sprite tint. A flame's colour is a
        // consequence of its temperature, and it already reaches the shader
        // through `emissive_radiance`; tinting the scattering term with the
        // same hue would double-count it. What little light a flame scatters,
        // it scatters neutrally.
        single_scatter_albedo: regime.single_scatter_albedo(),
        edge_softness: LOCAL_VOLUME_EDGE_SOFTNESS,
        profile: FogProfile::Flame,
        emissive_radiance,
        emission_temperature_k: regime.temperature_k(),
        source: FogSource::ParticleEmitter,
    })
}

/// Classify an emitter as a thermal source and return its temperature.
///
/// Classification is by authored naming alone — deliberately **not** by blend
/// mode. An earlier revision required additive blending (Gamebryo destination
/// factor `ONE == 0`) on the theory that luminous effects are always authored
/// additively. Real data says otherwise: every flame emitter in Skyrim's
/// `WhiterunBanneredMare` (`FlamesSmall03-Emitter` / `fxfireatlas02.dds`,
/// `Firearticles-Emitter` / `fxfirecolumnanimloop.dds`) is authored
/// **alpha-over**, so that gate rejected the entire cell. `ParticleEmitter`'s
/// heuristic presets do default to additive, which is what made the
/// assumption look right in unit tests — but `apply_emitter_overlays`
/// overwrites the preset with the authored NIF value before this runs.
///
/// Smoke is excluded explicitly rather than relying on
/// [`medium_from_particle`]'s ordering, so the two classifiers are provably
/// disjoint as standalone functions. This matters because smoke plumes are
/// routinely named for the fire that produced them (`campfiresmoke01`), and
/// the fog token describes what the emitter *emits* while a thermal token in
/// the same string may only describe its parent.
///
/// Ember-family names resolve to the cooler [`CombustionRegime::EMBER`] and are
/// checked first, because vanilla content frequently hosts embers under a
/// parent whose name also contains "fire" — there the cooler, dimmer reading
/// is the correct one.
fn fire_regime(host_name: &str, texture_path: Option<&str>) -> Option<CombustionRegime> {
    let texture_path = texture_path.unwrap_or("");
    if has_fog_token(host_name) || has_fog_token(texture_path) {
        return None;
    }
    if has_explosion_token(host_name) || has_explosion_token(texture_path) {
        return None;
    }
    if has_ember_token(host_name) || has_ember_token(texture_path) {
        return Some(CombustionRegime::EMBER);
    }
    if has_gas_flame_token(host_name) || has_gas_flame_token(texture_path) {
        return Some(CombustionRegime::GAS_FLAME);
    }
    if has_flame_token(host_name) || has_flame_token(texture_path) {
        return Some(CombustionRegime::FLAME);
    }
    None
}

/// Ember / spark / cinder naming. `spark` is matched as a whole-ish component
/// prefix rather than a substring so `sparkle` — the blue magic preset, which
/// is not thermal at all — does not get swept in.
fn has_ember_token(value: &str) -> bool {
    const EMBER_TOKENS: [&str; 3] = ["ember", "cinder", "coal"];
    let value = value.to_ascii_lowercase();
    if EMBER_TOKENS.iter().any(|token| value.contains(token)) {
        return true;
    }
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|component| component.starts_with("spark") && !component.starts_with("sparkle"))
}

fn has_flame_token(value: &str) -> bool {
    const FLAME_TOKENS: [&str; 6] = ["fire", "flame", "torch", "candle", "blaze", "campfire"];
    let value = value.to_ascii_lowercase();
    FLAME_TOKENS.iter().any(|token| value.contains(token))
}

/// Gas appliances need a compact blue reaction zone instead of the orange,
/// soot-rich diffusion flame selected by the generic fire tokens. Keep the
/// table deliberately source-oriented: a future magic or energy weapon can
/// add a regime without pretending that an authored sprite tint is fuel
/// chemistry.
fn has_gas_flame_token(value: &str) -> bool {
    const GAS_FLAME_TOKENS: [&str; 8] = [
        "gas flame",
        "gasflame",
        "natural gas",
        "naturalgas",
        "methane",
        "propane",
        "butane",
        "bunsen",
    ];
    let value = value.to_ascii_lowercase();
    GAS_FLAME_TOKENS.iter().any(|token| value.contains(token))
}

fn has_explosion_token(value: &str) -> bool {
    const EXPLOSION_TOKENS: [&str; 6] = [
        "explosion",
        "explode",
        "detonate",
        "fireball",
        "shockwave",
        "blast",
    ];
    let value = value.to_ascii_lowercase();
    EXPLOSION_TOKENS.iter().any(|token| value.contains(token))
}

fn has_nuclear_explosion_token(value: &str) -> bool {
    const NUCLEAR_TOKENS: [&str; 8] = [
        "nuclear",
        "nuke",
        "atomic",
        "atom bomb",
        "fatman",
        "mini nuke",
        "mininuke",
        "mushroom",
    ];
    let value = value.to_ascii_lowercase();
    NUCLEAR_TOKENS.iter().any(|token| value.contains(token))
}

fn explosion_profile_for_particle(
    host_name: &str,
    texture_path: Option<&str>,
) -> Option<FogProfile> {
    let texture_path = texture_path.unwrap_or("");
    if has_fog_token(host_name) || has_fog_token(texture_path) {
        return None;
    }
    // Some Fallout high-yield emitters are named for the weapon (`FatMan`,
    // `MiniNuke`) rather than the event. Treat a nuclear signature as an
    // explosion signature too, while keeping explicit smoke/fog names above
    // authoritative.
    let nuclear =
        has_nuclear_explosion_token(host_name) || has_nuclear_explosion_token(texture_path);
    let explosion = has_explosion_token(host_name) || has_explosion_token(texture_path) || nuclear;
    if !explosion {
        return None;
    }
    Some(if nuclear {
        FogProfile::NuclearExplosion
    } else {
        FogProfile::OilExplosion
    })
}

fn is_explosion_particle(host_name: &str, texture_path: Option<&str>) -> bool {
    explosion_profile_for_particle(host_name, texture_path).is_some()
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
        profile: FogProfile::Homogeneous,
        emissive_radiance: [0.0; 3],
        emission_temperature_k: 0.0,
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

    /// The fire path is env-gated for A/B; every test below asserts the
    /// enabled behaviour, so skip rather than fail if a developer has the
    /// escape hatch set in their shell.
    fn fire_path_active() -> bool {
        fire_volumes_enabled()
    }

    #[test]
    fn additive_flame_emitter_becomes_an_emissive_medium() {
        if !fire_path_active() {
            return;
        }
        let volume = fire_volume_from_particle("TorchFire01", &ParticleEmitter::torch_flame())
            .expect("an additive flame emitter must become an emissive medium");
        assert!(volume.is_emissive(), "a flame must carry an emission term");
        assert!(volume.is_renderable());
        assert_eq!(
            volume.emission_temperature_k,
            CombustionRegime::FLAME.temperature_k()
        );
        // Blackbody hue at flame temperature: warm, red-dominant.
        let [r, g, b] = volume.emissive_radiance;
        assert!(
            r > g && g > b,
            "flame emission must be warm, got {:?}",
            [r, g, b]
        );
        // Soot absorbs far more than it scatters — this is what makes the
        // emission term dominant rather than decorative.
        assert!(volume.single_scatter_albedo[0] < 0.5);
    }

    #[test]
    fn gas_flame_uses_a_hotter_bluer_source_regime() {
        if !fire_path_active() {
            return;
        }
        let ordinary = fire_volume_from_particle("TorchFire01", &ParticleEmitter::torch_flame())
            .expect("ordinary flame");
        let gas = fire_volume_from_particle("NaturalGasFlame01", &ParticleEmitter::torch_flame())
            .expect("gas flame");
        assert_eq!(
            gas.emission_temperature_k,
            CombustionRegime::GAS_FLAME.temperature_k()
        );
        assert!(gas.emission_temperature_k > ordinary.emission_temperature_k);
        let ordinary_blue_ratio =
            ordinary.emissive_radiance[2] / ordinary.emissive_radiance[0].max(1.0e-6);
        let gas_blue_ratio = gas.emissive_radiance[2] / gas.emissive_radiance[0].max(1.0e-6);
        assert!(
            gas_blue_ratio > ordinary_blue_ratio,
            "gas flame must shift blueward: ordinary {:?}, gas {:?}",
            ordinary.emissive_radiance,
            gas.emissive_radiance
        );
    }

    /// Embers are cooler than flame, so they must be both redder AND
    /// dramatically dimmer. The second half is the part a hand-authored
    /// colour ramp always gets wrong.
    #[test]
    fn embers_are_cooler_dimmer_and_redder_than_flame() {
        if !fire_path_active() {
            return;
        }
        let flame = fire_volume_from_particle("TorchFire01", &ParticleEmitter::torch_flame())
            .expect("flame");
        let embers =
            fire_volume_from_particle("FireEmbers01", &ParticleEmitter::embers()).expect("embers");

        assert!(embers.emission_temperature_k < flame.emission_temperature_k);

        let flame_luma = byroredux_core::radiometry::linear_srgb_luminance(flame.emissive_radiance);
        let ember_luma =
            byroredux_core::radiometry::linear_srgb_luminance(embers.emissive_radiance);
        assert!(
            ember_luma < flame_luma * 0.25,
            "embers ({ember_luma}) must be far dimmer than flame ({flame_luma}) — \
             the T^4 plus spectral-shift coupling, not a small tint difference"
        );

        // Hue is compared on GREEN over red, not blue over red. Across the
        // whole flame/ember range the blue primary is outside the sRGB gamut
        // and clamps to exactly zero, so a blue-based ratio is 0 for both and
        // discriminates nothing. Green is well inside the gamut and is what
        // actually separates a deep-red ember from a yellow-orange flame.
        let warmth = |c: [f32; 3]| c[1] / c[0].max(1.0e-6);
        assert!(
            warmth(embers.emissive_radiance) < warmth(flame.emissive_radiance),
            "cooler embers must be relatively redder than the flame: \
             embers {:?} vs flame {:?}",
            embers.emissive_radiance,
            flame.emissive_radiance
        );
    }

    /// Ember naming wins over flame naming, because vanilla content hosts
    /// ember emitters under parents whose names also say "fire".
    #[test]
    fn ember_naming_takes_precedence_over_a_fire_host() {
        assert_eq!(
            fire_regime("FireEmberSpray", Some("textures\\effects\\embers_d.dds")),
            Some(CombustionRegime::EMBER)
        );
        assert_eq!(
            fire_regime("CampfireFlame", Some("textures\\effects\\flame.dds")),
            Some(CombustionRegime::FLAME)
        );
    }

    /// Regression for the classifier's original blend-mode gate, which
    /// rejected every flame in Skyrim's `WhiterunBanneredMare` because
    /// vanilla authors them ALPHA-OVER, not additive. These are the exact
    /// host/texture pairs observed in that cell's engine log.
    #[test]
    fn vanilla_skyrim_alpha_over_flames_are_classified_as_fire() {
        for (host, texture) in [
            (
                "FlamesSmall03-Emitter",
                "textures\\effects\\fxfireatlas02.dds",
            ),
            (
                "FlamesSmall01-Emitter",
                "textures\\effects\\fxfireatlas02.dds",
            ),
            (
                "Firearticles-Emitter",
                "textures\\effects\\fxfirecolumnanimloop.dds",
            ),
        ] {
            assert_eq!(
                fire_regime(host, Some(texture)),
                Some(CombustionRegime::FLAME),
                "{host} / {texture} is a vanilla Skyrim flame emitter and must \
                 classify as fire regardless of its alpha-over blend mode"
            );
        }
    }

    /// The sibling half of the same cell: its smoke emitters must stay smoke.
    #[test]
    fn vanilla_skyrim_smoke_emitters_stay_passive() {
        for host in ["smoke02-Emitter", "smoke-Emitter"] {
            assert_eq!(
                fire_regime(host, Some("textures\\effects\\smokeparticles01.dds")),
                None,
                "{host} emits smoke and must never be classified as fire"
            );
        }
    }

    /// Blue magic sparkles are not thermal. Inventing a temperature for them
    /// would fabricate physics that the authored content never described.
    #[test]
    fn magic_sparkles_stay_on_the_billboard_path() {
        assert_eq!(
            fire_regime(
                "MagicSparkleEmitter",
                Some("textures\\effects\\glowsoft01.dds")
            ),
            None
        );
        assert!(fire_volume_from_particle(
            "MagicSparkleEmitter",
            &ParticleEmitter::magic_sparkles()
        )
        .is_none());
    }

    /// A fog token means smoke, never fire, regardless of what else the name
    /// says — a plume rising off a fire is routinely named for the fire, and
    /// the fog token describes what is actually emitted.
    #[test]
    fn a_fog_token_always_wins_over_a_thermal_one() {
        assert_eq!(fire_regime("CampfireSmoke", Some("smoke.dds")), None);
        assert_eq!(fire_regime("TorchSteamVent", Some("steam.dds")), None);
        let mut alpha_over_smoke = ParticleEmitter::smoke();
        alpha_over_smoke.dst_blend = 7;
        assert!(fire_volume_from_particle("CampfireSmoke01", &alpha_over_smoke).is_none());
    }

    /// Blend mode must not gate fire classification in either direction: the
    /// same emitter authored additively or alpha-over is the same fire. This
    /// is the invariant whose absence hid the Bannered Mare miss.
    #[test]
    fn blend_mode_does_not_gate_fire_classification() {
        if !fire_path_active() {
            return;
        }
        let mut additive = ParticleEmitter::torch_flame();
        additive.dst_blend = 0;
        let mut alpha_over = ParticleEmitter::torch_flame();
        alpha_over.dst_blend = 7;

        let a = fire_volume_from_particle("FlamesSmall03-Emitter", &additive)
            .expect("additive flame is fire");
        let b = fire_volume_from_particle("FlamesSmall03-Emitter", &alpha_over)
            .expect("alpha-over flame is equally fire");
        assert_eq!(a.emission_temperature_k, b.emission_temperature_k);
        assert_eq!(a.emissive_radiance, b.emissive_radiance);
    }

    /// The two classifiers must partition emitters, never both claim one.
    #[test]
    fn smoke_and_fire_classifiers_are_disjoint() {
        let cases = [
            ("CampfireSmoke01", ParticleEmitter::smoke()),
            ("TorchFire01", ParticleEmitter::torch_flame()),
            ("FireEmbers01", ParticleEmitter::embers()),
            ("ExplosionFireball01", ParticleEmitter::explosion()),
            ("MagicSparkle", ParticleEmitter::magic_sparkles()),
        ];
        for (host, emitter) in cases {
            let smoke = fog_volume_from_particle(host, &emitter).is_some();
            let fire = fire_volume_from_particle(host, &emitter).is_some();
            let explosion = explosion_volume_from_particle(host, &emitter).is_some();
            assert!(
                u8::from(smoke) + u8::from(fire) + u8::from(explosion) <= 1,
                "{host} was claimed by multiple combustion classifiers"
            );
        }
    }

    #[test]
    fn oil_explosion_is_a_grounded_hot_sphere_with_a_one_shot_timeline() {
        if !fire_path_active() {
            return;
        }
        let emitter = ParticleEmitter::explosion();
        let volume = explosion_volume_from_particle("ExplosionFireball01", &emitter)
            .expect("an explosion emitter must become a transient medium");
        let bounds = volume.bounds.expect("explosion bounds");
        assert_eq!(volume.source, FogSource::ParticleEmitter);
        assert_eq!(volume.profile, FogProfile::OilExplosion);
        assert_eq!(bounds.shape, FogShape::Sphere);
        assert_eq!(bounds.center, Vec3::ZERO);
        assert_eq!(bounds.half_extents, Vec3::splat(bounds.half_extents.x));
        assert_eq!(
            volume.emission_temperature_k,
            CombustionRegime::EXPLOSION.temperature_k()
        );
        assert!(volume.is_emissive());

        let state = combustion_state_from_particle(volume, &emitter, 7.0)
            .expect("an explosion needs a one-shot timeline");
        assert_eq!(state.normalized_age(7.0), Some(0.0));
        assert!(state.normalized_age(7.0 + emitter.life).is_none());
    }

    #[test]
    fn smoke_semantics_win_over_explosion_tokens() {
        let mut emitter = ParticleEmitter::explosion();
        emitter.dst_blend = 7;
        emitter.texture_path = Some("textures\\effects\\explosion_smoke.dds".to_string());
        assert!(explosion_volume_from_particle("ExplosionSmoke", &emitter).is_none());
        assert!(fog_volume_from_particle("ExplosionSmoke", &emitter).is_some());
    }

    #[test]
    fn nuclear_tokens_select_the_high_yield_profile_and_regime() {
        if !fire_path_active() {
            return;
        }
        let emitter = ParticleEmitter::explosion();
        let volume = explosion_volume_from_particle("MiniNukeExplosion", &emitter)
            .expect("a nuclear explosion emitter must become a transient medium");
        assert_eq!(volume.profile, FogProfile::NuclearExplosion);
        assert_eq!(
            volume.emission_temperature_k,
            CombustionRegime::NUCLEAR_EXPLOSION.temperature_k()
        );
        assert!(volume.emissive_radiance.iter().sum::<f32>() > 0.0);

        let weapon_named_emitter = explosion_volume_from_particle("FatMan", &emitter)
            .expect("a weapon-named high-yield emitter must still become nuclear medium");
        assert_eq!(weapon_named_emitter.profile, FogProfile::NuclearExplosion);
    }

    /// Smoke must stay passive — the emission field exists, so it would be
    /// easy to accidentally leak a nonzero value into it.
    #[test]
    fn converted_smoke_remains_passive() {
        let smoke = fog_volume_from_particle("CampfireSmoke01", &ParticleEmitter::smoke())
            .expect("smoke converts");
        assert!(!smoke.is_emissive());
        assert_eq!(smoke.emission_temperature_k, 0.0);
    }

    /// Optical depth is pinned by construction, so a bonfire is exactly as
    /// translucent as a candle rather than turning opaque with size.
    #[test]
    fn flame_optical_depth_is_size_invariant() {
        if !fire_path_active() {
            return;
        }
        let mut small = ParticleEmitter::torch_flame();
        small.shape = EmitterShape::Sphere { radius: 1.0 };
        let mut large = ParticleEmitter::torch_flame();
        large.shape = EmitterShape::Sphere { radius: 40.0 };

        for emitter in [small, large] {
            let volume = fire_volume_from_particle("Fire", &emitter).expect("flame");
            let half = volume.bounds.expect("bounded").half_extents;
            let width_m = 2.0 * half.x.min(half.z) / WORLD_UNITS_PER_METER;
            let optical_depth = volume.extinction_per_meter * width_m;
            assert!(
                (optical_depth - FLAME_OPTICAL_DEPTH).abs() < 1.0e-3,
                "optical depth drifted to {optical_depth} for half extents {half:?}"
            );
        }
    }

    /// `medium_from_particle` is the single entry point both spawn paths use;
    /// it must route each emitter family to the right medium.
    #[test]
    fn medium_entry_point_routes_both_families() {
        assert!(
            medium_from_particle("CampfireSmoke01", &ParticleEmitter::smoke())
                .is_some_and(|v| !v.is_emissive())
        );
        if fire_path_active() {
            assert!(
                medium_from_particle("TorchFire01", &ParticleEmitter::torch_flame())
                    .is_some_and(|v| v.is_emissive())
            );
            assert!(
                medium_from_particle("ExplosionFireball", &ParticleEmitter::explosion())
                    .is_some_and(|v| v.profile == FogProfile::OilExplosion)
            );
        }
        assert!(medium_from_particle("MagicSparkle", &ParticleEmitter::magic_sparkles()).is_none());
    }

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
        assert_eq!(volume.single_scatter_albedo, SMOKE_FALLBACK_ALBEDO);
    }

    #[test]
    fn explosion_texture_selects_the_impulse_preset_before_flame() {
        let preset = particle_preset(
            "FireEmitter",
            Some("textures\\effects\\explosionfireball01.dds"),
        );
        assert_eq!(preset.life, ParticleEmitter::explosion().life);
        assert_eq!(preset.end_size, ParticleEmitter::explosion().end_size);
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
