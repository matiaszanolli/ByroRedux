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

/// Time for the initial fireball to traverse its canonical source radius.
///
/// This is deliberately independent of the effect lifetime: lifetime governs
/// cooling smoke persistence, while radial blast expansion is an impulse.
pub const EXPLOSION_EXPANSION_TIME_SECONDS: f32 = 0.35;

/// Duration over which a one-shot source may add outward momentum.
pub const EXPLOSION_IMPULSE_DURATION_SECONDS: f32 = 1.5;

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
        single_scatter_albedo: 0.12,
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
            [0.12; 3]
        );
        assert_eq!(ADIABATIC_FLAME_TEMPERATURE_K, 2350.0);
        assert!(EXPLOSION_EXPANSION_TIME_SECONDS > 0.0);
        assert!(EXPLOSION_IMPULSE_DURATION_SECONDS > EXPLOSION_EXPANSION_TIME_SECONDS);
        let remaining = (-AEROSOL_DISSIPATION_PER_SECOND * AEROSOL_LINGER_SECONDS).exp();
        assert!(remaining <= AEROSOL_LINGER_CUTOFF_FRACTION);
        assert!(remaining > AEROSOL_LINGER_CUTOFF_FRACTION * 0.95);
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
