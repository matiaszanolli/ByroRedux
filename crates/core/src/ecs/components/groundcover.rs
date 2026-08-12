//! EXAL ground cover — canonical types (Phase 0).
//!
//! Implements the type layer of [`docs/engine/exal-groundcover.md`]. Ground
//! cover is the vegetation stratum sitting on the terrain surface: grass,
//! ferns, moss, low scrub. Trees and harvestable `FLOR` flora are explicitly
//! *not* ground cover (design §10) and keep their own authorities.
//!
//! # Placement is not in this file, and not in this engine's data
//!
//! This is the one exterior category where EXAL deliberately does not
//! reproduce the source engines. Bethesda maps a `GRAS` record onto an `LTEX`
//! landscape texture and scatters on a fixed per-cell grid wherever that
//! texture is painted, which is *why* vanilla grass reads as patches: density
//! is keyed to texture identity, so it steps hard exactly where one splat layer
//! stops. The design's counter is a continuous density field evaluated per
//! candidate point on the GPU (§3), with per-game data demoted to a **palette
//! hint only** (§7).
//!
//! So: a `GRAS` record contributes a *species* here — colour, size, stiffness.
//! Its density and placement fields are ignored on purpose. Anyone reading this
//! later and reaching for `GRAS.density` to "finish the port" should read §1
//! first; that omission is the feature.
//!
//! # What Phase 0 covers
//!
//! Types, the palette, the wind field, and the affinity contract — all pure
//! CPU and unit-testable. The density field itself lives in GLSL and only
//! there (§3, "Where the evaluation lives"): a Rust mirror would be a second
//! source of truth for a formula that must match exactly, which this codebase
//! has already been burned by. Rust owns the *parameters*; GLSL owns the
//! formula.

use crate::ecs::resource::Resource;

/// Linear RGB. Matches the convention in [`super::water`] — raw monitor-space
/// floats, never sRGB-decoded (see the colour-space note in `material.rs`).
pub type Rgb = [f32; 3];

/// Per-climate weighting for species selection.
///
/// The density field picks among a palette's species by weight × local
/// conditions, so species transitions come out as gradients rather than
/// boundaries — the same anti-patch principle as §1, applied one level up.
///
/// Values are relative, not normalised: a species with `temperate: 2.0` is
/// twice as likely as one with `1.0` in temperate conditions, and `0.0` means
/// "never here".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateWeights {
    pub temperate: f32,
    pub arid: f32,
    pub alpine: f32,
    pub wetland: f32,
}

impl ClimateWeights {
    /// Equal presence everywhere. The right default for a species with no
    /// authored climate information, since suppressing it entirely would be a
    /// stronger claim than the data supports.
    pub const UNIFORM: Self = Self {
        temperate: 1.0,
        arid: 1.0,
        alpine: 1.0,
        wetland: 1.0,
    };

    /// Weight for `climate`, used by the scatter pass's species draw.
    pub fn weight_for(&self, climate: Climate) -> f32 {
        match climate {
            Climate::Temperate => self.temperate,
            Climate::Arid => self.arid,
            Climate::Alpine => self.alpine,
            Climate::Wetland => self.wetland,
        }
    }
}

impl Default for ClimateWeights {
    fn default() -> Self {
        Self::UNIFORM
    }
}

/// Coarse climate bucket a worldspace resolves to.
///
/// Deliberately only four. This is a *vegetation* classification, not a weather
/// one — it exists to bias species selection, and finer buckets would need
/// authored data no source game provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Climate {
    Temperate,
    Arid,
    Alpine,
    Wetland,
}

/// One kind of ground cover in a worldspace's palette.
///
/// Field-for-field the design's §7 shape. Every field is a *rendering* or
/// *selection* property; none of them is placement, by construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundCoverSpecies {
    /// Blade height range in Gamebryo units. The scatter seed picks within it,
    /// so a patch has natural height variance rather than a uniform lawn.
    pub height_range: (f32, f32),
    /// Blade width range in Gamebryo units.
    pub width_range: (f32, f32),
    /// Base → tip colour. Blades are darker at the base in nearly all real
    /// vegetation because the base is self-shadowed by the canopy above it.
    pub colour_gradient: [Rgb; 2],
    /// Resistance to wind bend, `0.0` = limp, `1.0` = rigid. Feeds the Bezier
    /// control-point displacement in the blade vertex shader (§8).
    pub bend_stiffness: f32,
    /// How strongly this species responds to the terrain affinity term (§3).
    /// A species with low affinity appears only where the ground strongly
    /// supports cover; a high-affinity one spreads onto marginal ground.
    pub cover_affinity: f32,
    /// Relative presence per climate.
    pub climate_weight: ClimateWeights,
}

impl GroundCoverSpecies {
    /// The built-in temperate grass.
    ///
    /// Guarantees that a game supplying no vegetation data at all still renders
    /// organic ground cover, which is the stated "generic" requirement (§7
    /// precedence 3). Sizes are in Gamebryo units, where an exterior cell is
    /// 4096 across and a human is ~128 tall — so a 6–14 unit blade is
    /// ankle-height, which is what vanilla grass reads as.
    pub const DEFAULT_TEMPERATE: Self = Self {
        height_range: (6.0, 14.0),
        width_range: (0.7, 1.4),
        colour_gradient: [[0.18, 0.26, 0.10], [0.42, 0.52, 0.22]],
        bend_stiffness: 0.35,
        cover_affinity: 1.0,
        climate_weight: ClimateWeights::UNIFORM,
    };

    /// Sparse arid scrub — shorter, wider, stiffer, and yellowed.
    pub const DEFAULT_ARID: Self = Self {
        height_range: (4.0, 9.0),
        width_range: (0.9, 1.8),
        colour_gradient: [[0.24, 0.20, 0.09], [0.52, 0.46, 0.24]],
        bend_stiffness: 0.65,
        cover_affinity: 0.7,
        climate_weight: ClimateWeights {
            temperate: 0.4,
            arid: 2.0,
            alpine: 0.1,
            wetland: 0.1,
        },
    };

    /// True if every numeric field is finite and every range is ordered.
    ///
    /// Species can arrive from parsed `GRAS` records, so a malformed record
    /// must not be able to push a NaN into the blade shader — a non-finite
    /// height silently removes the blade from the raster with no diagnostic.
    pub fn is_well_formed(&self) -> bool {
        let ranges_ok = self.height_range.0 > 0.0
            && self.height_range.1 >= self.height_range.0
            && self.width_range.0 > 0.0
            && self.width_range.1 >= self.width_range.0;
        let finite = [
            self.height_range.0,
            self.height_range.1,
            self.width_range.0,
            self.width_range.1,
            self.bend_stiffness,
            self.cover_affinity,
        ]
        .iter()
        .all(|v| v.is_finite())
            && self
                .colour_gradient
                .iter()
                .flatten()
                .all(|c| c.is_finite() && *c >= 0.0);
        ranges_ok && finite && self.bend_stiffness >= 0.0 && self.cover_affinity >= 0.0
    }
}

/// The species set a worldspace resolves to, plus its climate bucket.
///
/// Never empty: [`Self::resolve`] falls back to the built-in default so the
/// scatter pass has no empty-palette branch to get wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundCoverPalette {
    pub species: Vec<GroundCoverSpecies>,
    pub climate: Climate,
}

impl GroundCoverPalette {
    /// Build a palette, dropping malformed species and substituting the
    /// built-in default if nothing usable survives.
    pub fn resolve(mut species: Vec<GroundCoverSpecies>, climate: Climate) -> Self {
        species.retain(|s| s.is_well_formed());
        if species.is_empty() {
            species.push(match climate {
                Climate::Arid => GroundCoverSpecies::DEFAULT_ARID,
                _ => GroundCoverSpecies::DEFAULT_TEMPERATE,
            });
        }
        Self { species, climate }
    }

    /// Total selection weight in this palette's climate. Zero when every
    /// species is excluded from the climate — the caller then draws uniformly
    /// rather than dividing by zero.
    pub fn total_weight(&self) -> f32 {
        self.species
            .iter()
            .map(|s| s.climate_weight.weight_for(self.climate))
            .sum()
    }
}

impl Default for GroundCoverPalette {
    fn default() -> Self {
        Self::resolve(Vec::new(), Climate::Temperate)
    }
}

impl Resource for GroundCoverPalette {}

/// Canonical wind, translated from `WTHR`'s wind-speed byte (§8).
///
/// The blade shader samples a continuous 2D flow-noise field advected along
/// `direction`, so neighbouring blades bend *together* — travelling gust waves
/// across a meadow rather than per-blade jitter, which is most of what sells
/// grass as a living surface. Wind here is deliberately neither simulated nor
/// collided; interaction is out of scope (§10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindField {
    /// Unit horizontal direction. Normalised on construction.
    pub direction: [f32; 2],
    /// Base speed, canonical units per second.
    pub speed: f32,
    /// Peak additional speed at gust crest.
    pub gust_amplitude: f32,
    /// Gust cycles per second.
    pub gust_frequency: f32,
}

impl WindField {
    /// Dead calm. Blades stand upright with only their seed-derived lean.
    pub const CALM: Self = Self {
        direction: [1.0, 0.0],
        speed: 0.0,
        gust_amplitude: 0.0,
        gust_frequency: 0.0,
    };

    /// Translate `WTHR`'s wind-speed byte into a canonical field.
    ///
    /// `wind_speed` is the raw 0–255 record byte; `direction` need not be
    /// normalised or non-zero. Gusts scale super-linearly with base speed
    /// because still air is genuinely steady while strong wind is turbulent —
    /// a constant gust ratio makes calm weather shimmer.
    pub fn from_weather_byte(wind_speed: u8, direction: [f32; 2]) -> Self {
        let normalised = f32::from(wind_speed) / 255.0;
        let speed = normalised * MAX_WIND_SPEED;
        Self {
            direction: normalise_or_east(direction),
            speed,
            gust_amplitude: speed * (0.25 + 0.55 * normalised),
            gust_frequency: 0.15 + 0.45 * normalised,
        }
    }

    /// True when every field is finite and the direction is unit-length.
    ///
    /// The renderer hard-fails non-finite environment values (EX-05), so the
    /// translation boundary is where a bad value has to be caught.
    pub fn is_well_formed(&self) -> bool {
        let finite = self.direction.iter().all(|v| v.is_finite())
            && self.speed.is_finite()
            && self.gust_amplitude.is_finite()
            && self.gust_frequency.is_finite();
        let len_sq = self.direction[0] * self.direction[0] + self.direction[1] * self.direction[1];
        finite && (len_sq - 1.0).abs() < 1e-3 && self.speed >= 0.0
    }
}

impl Default for WindField {
    fn default() -> Self {
        Self::CALM
    }
}

impl Resource for WindField {}

/// Canonical speed at `wind_speed == 255`.
///
/// Calibration parameter, not a measured constant — see §11.3, which pins the
/// density field's numbers with real-cell telemetry rather than unit tests.
/// Chosen so a mid-range byte produces visible but not frantic motion at the
/// blade scale above.
pub const MAX_WIND_SPEED: f32 = 220.0;

fn normalise_or_east(v: [f32; 2]) -> [f32; 2] {
    let len_sq = v[0] * v[0] + v[1] * v[1];
    if !len_sq.is_finite() || len_sq < 1e-12 {
        return [1.0, 0.0];
    }
    let inv = len_sq.sqrt().recip();
    [v[0] * inv, v[1] * inv]
}

#[cfg(test)]
#[path = "groundcover_tests.rs"]
mod tests;
