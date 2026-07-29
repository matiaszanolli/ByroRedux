//! Legacy fog authoring → engine-native participating-medium conversion.
//!
//! Gamebryo and Creation records author a visibility ramp with near/far
//! distances. The volumetric renderer must never reinterpret that ramp per
//! pixel: conversion happens once when cell/weather data enters the ECS, and
//! runtime code consumes only [`FogMedium`].

/// Bethesda world units per metre. Positions remain in Bethesda units in the
/// renderer and TLAS; physical medium coefficients are stored per metre.
pub(crate) const WORLD_UNITS_PER_METER: f32 = 70.0;

const FIT_SAMPLES: usize = 256;
const FIT_ITERATIONS: usize = 80;
const MIN_SIGMA_PER_METER: f32 = 1.0e-8;
const MAX_SIGMA_PER_METER: f32 = 10.0;

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
}
