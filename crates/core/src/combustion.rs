//! Canonical combustion regimes and dynamics shared across import and render.
//!
//! Source-game schemas stop at their translation boundaries: an imported
//! particle, a runtime effect, and a renderer lab all select the same regime
//! values here. The transported solver then consumes physical temperatures,
//! soot albedo, and second-based impulse constants without branching on game
//! identity or misusing an authored effect lifetime as a dynamics parameter.

use crate::radiometry::blackbody_radiance_srgb;

/// Equilibrium temperature approached by ordinary fuel combustion, kelvin.
///
/// The transported solver may heat a colder fuel/air mixture toward this
/// limit, but an already-hot explosion must not receive another fixed
/// temperature increment and run away toward the numerical safety ceiling.
pub const ADIABATIC_FLAME_TEMPERATURE_K: f32 = 2350.0;

/// First-order fuel/oxidizer reaction rate, per second, for the normalized
/// transported mixture-fraction field.
pub const REACTION_RATE_PER_SECOND: f32 = 5.8;

/// Soot extinction yield in a maximally fuel-rich reaction zone, per unit of
/// burned normalized fuel.
pub const RICH_SOOT_YIELD: f32 = 0.06;

/// Residual soot yield in an oxygen-rich reaction zone, per unit of burned
/// normalized fuel.
pub const LEAN_SOOT_YIELD: f32 = 0.004;

/// First-order oxidation rate for soot while a transported cell is hot and
/// oxygen-rich, per second.
pub const SOOT_OXIDATION_RATE_PER_SECOND: f32 = 0.15;

/// Single-scatter albedo of transported soot aerosol. Soot remains strongly
/// absorbing, but this non-zero scattering floor keeps a cooled plume visible
/// against a neutral interior instead of collapsing into a featureless black
/// hole.
pub const SOOT_SINGLE_SCATTER_ALBEDO: f32 = 0.18;

/// Temperature at which soot oxidation begins, kelvin.
pub const SOOT_OXIDATION_START_TEMPERATURE_K: f32 = 1100.0;

/// Temperature at which soot oxidation reaches its full hot-cell weight,
/// kelvin.
pub const SOOT_OXIDATION_FULL_TEMPERATURE_K: f32 = 1800.0;

/// Time for the initial fireball to traverse its canonical source radius.
///
/// This is deliberately independent of the effect lifetime: lifetime governs
/// cooling smoke persistence, while radial blast expansion is an impulse.
pub const EXPLOSION_EXPANSION_TIME_SECONDS: f32 = 0.35;

/// Duration over which a one-shot source may add outward momentum.
pub const EXPLOSION_IMPULSE_DURATION_SECONDS: f32 = 1.5;

/// First-order decay rate for transported specific overpressure.
///
/// The pressure phase should survive long enough to expand a fireball beyond
/// its source primitive, but be effectively gone before buoyancy owns the
/// cooled plume. The solver consumes this canonical field without retaining
/// explosion/source identity.
pub const OVERPRESSURE_DISSIPATION_PER_SECOND: f32 = 2.8;

/// Numerical safety ceiling for acceleration derived from an overpressure
/// gradient, metres per second squared.
///
/// This bounds under-resolved gradients without replacing their direction or
/// tying the dynamics to an authored effect lifetime.
pub const MAX_PRESSURE_ACCELERATION_MPS2: f32 = 18.0;

/// Upper bound for volumetric dilution from a resolved expanding flow, per
/// second. It limits under-resolved divergence without allowing an expanding
/// blast to retain constant-density chemistry.
pub const MAX_DILUTION_RATE_PER_SECOND: f32 = 3.5;

/// Velocity scale used by interface vorticity confinement, metres per second.
///
/// Semi-Lagrangian transport is deliberately stable but numerically diffuses
/// the rotational motion that rolls a flame or smoke boundary. This canonical
/// scale restores that lost motion from the transported field itself; it is
/// independent of the source profile and originating game.
pub const VORTICITY_CONFINEMENT_SPEED_MPS: f32 = 0.75;

/// Numerical ceiling for interface-confinement acceleration, metres per
/// second squared.
///
/// Under-resolved velocity gradients can otherwise create a one-frame impulse
/// at a froxel-size discontinuity. The cap preserves direction while keeping
/// the dynamics inside the shared solver's physical range.
pub const MAX_VORTICITY_ACCELERATION_MPS2: f32 = 5.0;

/// Diameter of the broad transported-combustion eddies, metres.
pub const TURBULENCE_COARSE_EDDY_SCALE_METERS: f32 = 1.6;

/// Diameter of the interface-detail eddies, metres.
///
/// This remains above the combustion lab's near-field froxel spacing, so the
/// solver resolves the motion instead of aliasing procedural detail into
/// frame noise.
pub const TURBULENCE_DETAIL_EDDY_SCALE_METERS: f32 = 0.48;

/// Upward translation speed of the broad eddy field, metres per second.
pub const TURBULENCE_COARSE_RISE_SPEED_MPS: f32 = 0.30;

/// Upward translation speed of the smaller interface eddies, metres per
/// second.
pub const TURBULENCE_DETAIL_RISE_SPEED_MPS: f32 = 0.55;

/// First-order removal rate for transported combustion aerosol.
///
/// Absorption and scattering decay together so a plume keeps its canonical
/// single-scatter albedo as it disperses instead of changing material with
/// age. This is a medium property, not an authored effect lifetime.
pub const AEROSOL_DISSIPATION_PER_SECOND: f32 = 0.045;

/// Remaining optical coefficient at which an emitter-less field may stop.
pub const AEROSOL_LINGER_CUTOFF_FRACTION: f32 = 0.03;

/// Renderer latch long enough for first-order aerosol decay to reach the
/// canonical cutoff after the final emitter disappears.
pub const AEROSOL_LINGER_SECONDS: f32 = 78.0;

/// Upward entrainment acceleration retained by a cooled aerosol plume,
/// metres per second squared.
pub const AEROSOL_LIFT_ACCELERATION_MPS2: f32 = 0.20;

/// Extinction-to-lift scale, inverse metres. Dense transported aerosol gets
/// the full residual lift while a thin trailing edge fades smoothly.
pub const AEROSOL_LIFT_EXTINCTION_SCALE: f32 = 20.0;

/// Residual temperature of the cooling aerosol shell at the source boundary,
/// kelvin. It gives newly seeded smoke a short-lived buoyant impulse before
/// thermal relaxation and transported dynamics take over.
pub const COOLED_AEROSOL_SOURCE_TEMPERATURE_K: f32 = 620.0;

/// Extinction multiplier for the finite cooling shell that seeds transported
/// explosion smoke. The shell is non-emissive; this controls only enough
/// aerosol mass for the post-fire plume to remain optically resolved.
pub const EXPLOSION_SMOKE_EXTINCTION_SCALE: f32 = 4.0;

/// First-order removal rate for unburned fuel vapour, per second.
pub const FUEL_VAPOUR_REMOVAL_PER_SECOND: f32 = 0.025;

/// First-order removal rate for transported visible-radiance calibration, per
/// second.
pub const RADIANCE_REMOVAL_PER_SECOND: f32 = 0.08;

/// First-order thermal relaxation rate toward ambient temperature, per second.
pub const THERMAL_COOLING_PER_SECOND: f32 = 0.42;

/// Thermal buoyancy acceleration per unit normalized temperature excess,
/// metres per second squared.
pub const THERMAL_BUOYANCY_ACCELERATION_MPS2: f32 = 0.58;

/// First-order damping rate for transported velocity, per second.
pub const VELOCITY_DAMPING_PER_SECOND: f32 = 0.48;

/// Reaction heat response factor per unit burned normalized fuel.
pub const REACTION_HEAT_RESPONSE: f32 = 4.0;

/// Fraction of a persistent flame primitive occupied by the replenished fuel
/// boundary, measured upward from its base. The remaining authored extent is
/// a transport bound: buoyancy/advection must create the visible flame there.
pub const FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION: f32 = 0.32;

/// Fraction of the authored flame height occupied by the luminous reaction
/// zone. It is deliberately larger than the fuel boundary but leaves the tip
/// and downstream plume to transported flow.
pub const FLAME_REACTION_ZONE_HEIGHT_FRACTION: f32 = 0.55;

/// Fully luminous basal portion before the reaction zone begins tapering.
pub const FLAME_REACTION_ZONE_FADE_START_FRACTION: f32 = 0.14;

/// Maximum source-scale lateral velocity that breaks a diffusion flame's
/// otherwise perfectly vertical inlet, metres per second.
pub const FLAME_SOURCE_LATERAL_SPEED_MPS: f32 = 0.38;

/// First-order response rate toward the turbulent inlet velocity.
pub const FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND: f32 = 6.0;

/// Canonical thermal-emitter regime produced at content/runtime boundaries.
///
/// Keeping the radiance anchor beside temperature and soot albedo prevents
/// production importers and renderer probes from restating calibration
/// literals. Downstream systems consume the fields and never need to know
/// which game or authoring schema supplied the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombustionRegime {
    temperature_k: f32,
    reference_temperature_k: f32,
    reference_radiance: f32,
    single_scatter_albedo: f32,
}

impl CombustionRegime {
    /// Luminous soot in an open hydrocarbon diffusion flame.
    ///
    /// The radiance anchor is the one exposure choice in the fire path. With
    /// the importer's 0.4 optical-depth contract and this absorbing albedo,
    /// the initial 1850 K source integral lands near 3.6 linear HDR units;
    /// subsequent heat and cooling remain the transported solver's concern.
    pub const FLAME: Self = Self {
        temperature_k: 1850.0,
        reference_temperature_k: 1850.0,
        reference_radiance: 12.0,
        single_scatter_albedo: 0.25,
    };

    /// Cooled glowing charcoal after volatile flame combustion has ended.
    pub const EMBER: Self = Self {
        temperature_k: 1100.0,
        ..Self::FLAME
    };

    /// Initial hot-soot state of a short-lived explosive impulse.
    pub const EXPLOSION: Self = Self {
        temperature_k: 2800.0,
        reference_radiance: 24.0,
        single_scatter_albedo: 0.26,
        ..Self::FLAME
    };

    pub const fn temperature_k(self) -> f32 {
        self.temperature_k
    }

    pub const fn reference_temperature_k(self) -> f32 {
        self.reference_temperature_k
    }

    pub const fn reference_radiance(self) -> f32 {
        self.reference_radiance
    }

    pub const fn single_scatter_albedo(self) -> [f32; 3] {
        [self.single_scatter_albedo; 3]
    }

    /// Linear-sRGB emitted radiance for this complete canonical regime.
    pub fn emissive_radiance(self) -> Option<[f32; 3]> {
        blackbody_radiance_srgb(
            self.temperature_k,
            self.reference_temperature_k,
            self.reference_radiance,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radiometry::linear_srgb_luminance;

    #[test]
    fn canonical_regimes_pin_one_shared_physical_contract() {
        assert_eq!(CombustionRegime::EMBER.temperature_k(), 1100.0);
        assert_eq!(CombustionRegime::FLAME.temperature_k(), 1850.0);
        assert_eq!(CombustionRegime::EXPLOSION.temperature_k(), 2800.0);
        assert_eq!(CombustionRegime::FLAME.reference_temperature_k(), 1850.0);
        assert_eq!(CombustionRegime::FLAME.reference_radiance(), 12.0);
        assert_eq!(CombustionRegime::EXPLOSION.reference_radiance(), 24.0);
        assert_eq!(CombustionRegime::FLAME.single_scatter_albedo(), [0.25; 3]);
        assert_eq!(
            CombustionRegime::EXPLOSION.single_scatter_albedo(),
            [0.26; 3]
        );
        assert_eq!(ADIABATIC_FLAME_TEMPERATURE_K, 2350.0);
        assert!(REACTION_RATE_PER_SECOND > 0.0);
        assert!(RICH_SOOT_YIELD > LEAN_SOOT_YIELD);
        assert!(LEAN_SOOT_YIELD > 0.0);
        assert!(SOOT_OXIDATION_RATE_PER_SECOND > 0.0);
        assert!(SOOT_SINGLE_SCATTER_ALBEDO > 0.0 && SOOT_SINGLE_SCATTER_ALBEDO < 1.0);
        assert!(SOOT_OXIDATION_FULL_TEMPERATURE_K > SOOT_OXIDATION_START_TEMPERATURE_K);
        assert!(EXPLOSION_EXPANSION_TIME_SECONDS > 0.0);
        assert!(EXPLOSION_IMPULSE_DURATION_SECONDS > EXPLOSION_EXPANSION_TIME_SECONDS);
        assert!(MAX_PRESSURE_ACCELERATION_MPS2 > 0.0);
        assert!(MAX_DILUTION_RATE_PER_SECOND > 0.0);
        assert!(VORTICITY_CONFINEMENT_SPEED_MPS > 0.0);
        assert!(MAX_VORTICITY_ACCELERATION_MPS2 > 0.0);
        assert!(TURBULENCE_COARSE_EDDY_SCALE_METERS > TURBULENCE_DETAIL_EDDY_SCALE_METERS);
        assert!(TURBULENCE_DETAIL_EDDY_SCALE_METERS > 0.0);
        assert!(TURBULENCE_DETAIL_RISE_SPEED_MPS > TURBULENCE_COARSE_RISE_SPEED_MPS);
        let pressure_remaining =
            (-OVERPRESSURE_DISSIPATION_PER_SECOND * EXPLOSION_IMPULSE_DURATION_SECONDS).exp();
        assert!(pressure_remaining < 0.02);
        let remaining = (-AEROSOL_DISSIPATION_PER_SECOND * AEROSOL_LINGER_SECONDS).exp();
        assert!(remaining <= AEROSOL_LINGER_CUTOFF_FRACTION);
        assert!(remaining > AEROSOL_LINGER_CUTOFF_FRACTION * 0.95);
        assert!(AEROSOL_LIFT_ACCELERATION_MPS2 > 0.0);
        assert!(AEROSOL_LIFT_EXTINCTION_SCALE > 0.0);
        assert!(FUEL_VAPOUR_REMOVAL_PER_SECOND > 0.0);
        assert!(RADIANCE_REMOVAL_PER_SECOND > 0.0);
        assert!(THERMAL_COOLING_PER_SECOND > 0.0);
        assert!(THERMAL_BUOYANCY_ACCELERATION_MPS2 > 0.0);
        assert!(VELOCITY_DAMPING_PER_SECOND > 0.0);
        assert!(REACTION_HEAT_RESPONSE > 0.0);
        assert!(FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION > 0.0);
        assert!(FLAME_REACTION_ZONE_FADE_START_FRACTION < FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION);
        assert!(FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION < FLAME_REACTION_ZONE_HEIGHT_FRACTION);
        assert!(FLAME_REACTION_ZONE_HEIGHT_FRACTION < 1.0);
        assert!(FLAME_SOURCE_LATERAL_SPEED_MPS > 0.0);
        assert!(FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND > 0.0);
    }

    #[test]
    fn canonical_radiance_orders_cooling_and_impulse_states() {
        let ember = CombustionRegime::EMBER
            .emissive_radiance()
            .expect("ember regime is finite");
        let flame = CombustionRegime::FLAME
            .emissive_radiance()
            .expect("flame regime is finite");
        let explosion = CombustionRegime::EXPLOSION
            .emissive_radiance()
            .expect("explosion regime is finite");
        assert!(ember
            .iter()
            .chain(flame.iter())
            .chain(explosion.iter())
            .all(|v| v.is_finite()));

        let ember_luma = linear_srgb_luminance(ember);
        let flame_luma = linear_srgb_luminance(flame);
        let explosion_luma = linear_srgb_luminance(explosion);
        assert!(ember_luma < flame_luma * 0.25);
        assert!(explosion_luma > flame_luma);

        let green_over_red = |rgb: [f32; 3]| rgb[1] / rgb[0].max(1.0e-6);
        assert!(green_over_red(ember) < green_over_red(flame));
    }
}
