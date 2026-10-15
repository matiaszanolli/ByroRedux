//! EXAL exterior sky / sun / cell-lighting / weather translation: the
//! WTHR-driven canonical resources (`CellLightingRes`, `SkyParamsRes`,
//! `WeatherDataRes`), the climate time-of-day breakpoints, and the exterior
//! image-space sources.

use std::collections::HashMap;

use byroredux_core::ecs::components::groundcover::GroundCoverDimmer;
use byroredux_plugin::esm::cell::WorldspaceRecord;
use byroredux_plugin::esm::records::weather::SKY_TIME_SLOTS;
use byroredux_plugin::esm::records::{ClimateRecord, WeatherRecord};

use crate::components::{
    CellLightingRes, DalcCubeYup, SkyParamsRes, WeatherDataRes, WeatherSkyState,
};
use super::fallback::FB_TOD_HOURS;
use super::{inherit_up_chain, pnam};

// ───────────────────────────────────────────────────────────────────────
// Exterior sky / sun / weather / lighting translation (EXAL step 3)
//
// The WTHR-driven canonical resources (`CellLightingRes`, `SkyParamsRes`,
// `WeatherDataRes`) and the no-climate procedural fallback are built here,
// behind the single boundary. The functions are **pure** — the caller
// (`scene::world_setup::apply_worldspace_weather`) pre-resolves the
// `VulkanContext`-coupled cloud / sun textures into a [`SkyTextures`] and
// hands them in, mirroring `material_translate`'s `ResolvedPaths`. World
// insertion, the bindless-handle lifecycle, and the WTHR cross-fade-vs-insert
// decision stay in the caller (orchestration, not translation).
// ───────────────────────────────────────────────────────────────────────

/// Cos-threshold of the rendered sun-disc half-angle (~1.8°). Matches the
/// pre-EXAL hardcoded `SkyParamsRes` literal.
pub(super) const SUN_SIZE_COS: f32 = 0.9995;
/// Peak directional-light intensity at full day — the **one** declaration
/// (#2813).
///
/// Three unrelated consumers must agree on this number or the exterior
/// directional ramp stops spanning `[0, 1]`:
///
/// * the producer, [`crate::systems::weather::compute_sun_arc`], which
///   ramps up to it and holds it between sunrise-end and sunset-begin;
/// * the bootstrap seed `scene::world_setup::apply_environment` installs
///   through [`translate_sky`] / [`translate_exterior_cell_lighting`] (or
///   their [`procedural_fallback_sky`] / [`procedural_fallback_cell_lighting`]
///   no-climate counterparts) before the per-frame `weather_system`
///   re-derives the live value from the TOD arc;
/// * the divisor in `render::compute_directional_upload`, which normalises
///   `sun_intensity / peak` into the surface-lighting scale.
///
/// It used to be spelled `4.0` independently at all three, with both sun-arc
/// tests asserting against their own hardcoded copies — so a one-sided change
/// stayed green while either saturating the ramp early (producer raised
/// alone) or capping daytime exterior directional below full strength
/// (producer lowered alone). Whole-frame exterior lighting, silently.
///
/// Lives here because `env_translate` is the EXAL boundary both other
/// modules already sit downstream of; `weather.rs` re-exports it under the
/// producer-side name.
pub(crate) const SUN_INTENSITY_PEAK: f32 = 4.0;
/// Tangent-plane half-radius of the directional disk in radians (~1.15°);
/// drives PCSS-lite shadow jitter (#1023). Pre-EXAL hardcoded constant.
pub(super) const SUN_ANGULAR_RADIUS: f32 = 0.020;

/// Pre-resolved cloud + sun-sprite bindless handles for [`translate_sky`].
/// The caller resolves these through the texture registry (the only
/// `VulkanContext`-coupled step); the translate stays pure.
pub(crate) struct SkyTextures {
    /// `(bindless handle, tile_scale)` per WTHR cloud layer 0..=3.
    /// `(0, 0.0)` = layer disabled (the shader branch-skips it).
    pub(crate) cloud_layers: [(u32, f32); 4],
    /// CLMT FNAM sun-sprite handle. `0` = composite shader's procedural disc.
    pub(crate) sun_sprite: u32,
}

/// WTHR → exterior [`CellLightingRes`] (the day-TOD-slot snapshot the
/// per-frame `weather_system` then animates through the stored NAM0 table).
/// Raw monitor-space colours (commit 0e8efc6) — no sRGB decode.
pub(crate) fn translate_exterior_cell_lighting(
    wthr: &WeatherRecord,
    sun_dir: [f32; 3],
) -> CellLightingRes {
    use byroredux_plugin::esm::records::weather::{SKY_AMBIENT, SKY_FOG, SKY_SUNLIGHT, TOD_DAY};
    CellLightingRes {
        // #3956 — day-TOD snapshot, so the day height profile, matching the
        // `fog_day_*` distance ramp beside it. Overrides `bare`'s
        // no-height-profile medium fit.
        fog_medium: {
            let medium = crate::fog::FogMedium::from_legacy_ramp(
                wthr.fog_day_near,
                wthr.fog_day_far,
                Some(wthr.fog_day_max),
            );
            match wthr.fog_height {
                Some(height) => medium.with_authored_height_range(height.day_near_height_range),
                None => medium,
            }
        },
        // WTHR-driven exterior lighting; the extended XCLL tail applies to
        // interior cells (and not-yet-wired exterior lighting overrides).
        // #861 / #5814 — `..bare(..)` supplies every optional tail field
        // as None; the one authored value below overrides.
        //
        // #3957 — `fog_clip` (a `bare` None) stays `None` for a different
        // reason than its neighbours: WTHR has no clip-distance field at
        // all. It is XCLL-only, so there is nothing to forward here, now
        // or later.
        //
        // #3957 — WTHR *does* carry its own falloff power on Skyrim, FO4 and
        // FO76 (`parse_wthr`'s `FNAM` arm decodes it), and this is the
        // day-TOD snapshot, so the day value is the consistent pick alongside
        // `fog_day_near`/`far`/`max` above.
        //
        // Captured even though `draw.rs` documents `fog_params[3]` as
        // currently unconsumed (#1926 / #1927 removed the composite branch
        // that read it): the EXAL boundary's job is to make authored data
        // canonical, and a value that is already correct when the
        // interior-scoped composite branch lands beats one that has to be
        // rediscovered then.
        fog_power: Some(wthr.fog_day_power),
        ..CellLightingRes::bare(
            wthr.sky_colors[SKY_AMBIENT][TOD_DAY].to_rgb_f32(),
            wthr.sky_colors[SKY_SUNLIGHT][TOD_DAY].to_rgb_f32(),
            sun_dir,
            false,
            wthr.sky_colors[SKY_FOG][TOD_DAY].to_rgb_f32(),
            wthr.fog_day_near,
            wthr.fog_day_far,
        )
    }
}

/// WTHR + pre-resolved [`SkyTextures`] → [`SkyParamsRes`]. `current_dalc_cube`
/// is seeded `None`; `weather_system` populates it per-frame from
/// [`WeatherDataRes::skyrim_dalc_per_tod`] (#993).
pub(crate) fn translate_sky(
    wthr: &WeatherRecord,
    sun_dir: [f32; 3],
    textures: SkyTextures,
    tod_hours: [f32; 4],
) -> SkyParamsRes {
    use byroredux_plugin::esm::records::weather::{
        SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_UPPER, TOD_DAY,
    };
    let [(c0, s0), (c1, s1), (c2, s2), (c3, s3)] = textures.cloud_layers;
    SkyParamsRes {
        zenith_color: wthr.sky_colors[SKY_UPPER][TOD_DAY].to_rgb_f32(),
        horizon_color: wthr.sky_colors[SKY_HORIZON][TOD_DAY].to_rgb_f32(),
        // #541 — real Sky-Lower (NAM0 slot 7) drives composite.frag's
        // below-horizon branch instead of the pre-fix `horizon * 0.3` fake.
        lower_color: wthr.sky_colors[SKY_LOWER][TOD_DAY].to_rgb_f32(),
        sun_direction: sun_dir,
        sun_color: wthr.sky_colors[SKY_SUN][TOD_DAY].to_rgb_f32(),
        sun_size: SUN_SIZE_COS,
        sun_intensity: SUN_INTENSITY_PEAK,
        sun_angular_radius: SUN_ANGULAR_RADIUS,
        is_exterior: true,
        cloud_tile_scale: s0,
        cloud_texture_index: c0,
        sun_texture_index: textures.sun_sprite,
        cloud_tile_scale_1: s1,
        cloud_texture_index_1: c1,
        cloud_tile_scale_2: s2,
        cloud_texture_index_2: c2,
        cloud_tile_scale_3: s3,
        cloud_texture_index_3: c3,
        current_dalc_cube: None,
        // #5179 — the climate's own breakpoints seed the resource, so even
        // the pre-first-tick frame reads the worldspace's dawn/dusk bands;
        // `weather_system` republishes (blending during a cross-fade).
        tod_hours,
        weather: weather_sky_state(wthr, TOD_DAY),
    }
}

/// Convert Bethesda WTHR classification flags into the procedural medium's
/// occupancy control. Precipitation takes priority when records combine flags;
/// unclassified legacy weather retains the historical neutral coverage.
///
/// Provenance (W3.14 census, `weather_coverage_census`): the constants are
/// a documented engine choice, not a translation of authored data — the
/// 2D engines' cloud coverage emerged from texture alpha × JNAM layer
/// multipliers, which do not reduce to a scalar (census: 32/84 Skyrim
/// weathers author no JNAM; authored records average max-layer alpha 0.99
/// regardless of classification). See skyal.md §2 for the full note.
fn fog_coverage_from_weather(classification: u8) -> f32 {
    use byroredux_plugin::esm::records::weather::{
        WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
    };
    if classification & WTHR_RAINY != 0 {
        0.86
    } else if classification & WTHR_SNOW != 0 {
        0.80
    } else if classification & WTHR_CLOUDY != 0 {
        0.70
    } else if classification & WTHR_PLEASANT != 0 {
        0.40
    } else {
        0.55
    }
}

fn precipitation_from_weather(classification: u8) -> f32 {
    use byroredux_plugin::esm::records::weather::{WTHR_RAINY, WTHR_SNOW};
    if classification & WTHR_RAINY != 0 {
        1.0
    } else if classification & WTHR_SNOW != 0 {
        0.12
    } else {
        0.0
    }
}

fn precipitation_components(classification: u8) -> [f32; 2] {
    use byroredux_plugin::esm::records::weather::{WTHR_RAINY, WTHR_SNOW};
    [
        if classification & WTHR_RAINY != 0 {
            1.0
        } else {
            0.0
        },
        if classification & WTHR_SNOW != 0 {
            1.0
        } else {
            0.0
        },
    ]
}

/// Neutral moon-glare for a WTHR that authors no moon-glare colour at all,
/// and the one declaration behind [`WeatherSkyState::default`]'s
/// `moon_glare` — the `FB_TOD_HOURS`/`weather::DEFAULT_TOD_HOURS`
/// arrangement (#2812): the default aliases this value so the two cannot
/// drift (#4494 — pre-fix the literal was duplicated verbatim in both files
/// with neither copy citing the other). There is no authored field to
/// translate here: Gamebryo-era WTHRs carry no moon data at all and even
/// Skyrim ships it only as a colour table that is legitimately all-zero on
/// overcast records, so this is a documented engine choice, not data — same
/// class as the cloud-coverage constants (`fog_coverage_from_weather`'s
/// provenance note, skyal.md §2).
pub(crate) const FALLBACK_MOON_GLARE: f32 = 0.35;

/// Neutral sun-glare for a WTHR whose `DATA` glare byte is 0, and the one
/// declaration behind [`WeatherSkyState::default`]'s `sun_glare` — same
/// one-declaration arrangement as [`FALLBACK_MOON_GLARE`]: an unauthored 0
/// is "no data", not an authored glareless sun, so the engine substitutes
/// its neutral multiplier rather than forwarding the sentinel.
pub(crate) const NEUTRAL_SUN_GLARE: f32 = 1.0;

/// Translate the authored non-colour WTHR controls into normalized render
/// values. Per-TOD cloud tint is sampled by `weather_system`; this seed is
/// useful for the first frame before that system has run.
fn weather_sky_state(wthr: &WeatherRecord, tod_slot: usize) -> WeatherSkyState {
    use crate::systems::weather::fold_to_four_tod_slots;
    use byroredux_plugin::esm::records::weather::{
        SKY_STARS, SKY_SUNLIGHT, WTHR_AURORA_ALWAYS_VISIBLE, WTHR_AURORA_FOLLOWS_SUN,
    };
    // #5178 — the seed reads the 4-slot tables through the one shared fold
    // (`fold_to_four_tod_slots`), not a private reduction: the previous
    // `.min(3)` read `TOD_HIGH_NOON` as NIGHT where the shared fold gives
    // DAY. The tint alpha is the JNAM multiplier alone — PNAM's fourth byte
    // is padding (UESP: "rgb per TOD"), which is exactly how the steady-state
    // sampler reads these tables (`cloud_layer_colors` goes through
    // `to_rgb_f32` and the alpha comes from JNAM); the seed disagreed with
    // it and scaled the first frame's cloud alpha by that padding.
    let slot = fold_to_four_tod_slots(tod_slot);
    let mut cloud_tints = [[1.0; 4]; 4];
    for (layer, tint) in cloud_tints.iter_mut().enumerate() {
        let color = wthr.cloud_layer_colors[layer][slot];
        *tint = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            // #5344 — same clamp the carried table gets below, so the
            // first frame and every later frame read one quantity one way.
            wthr.cloud_layer_alphas[layer][slot].clamp(0.0, 1.0),
        ];
    }

    let mut lightning_color = [
        wthr.lightning_color[0] as f32 / 255.0,
        wthr.lightning_color[1] as f32 / 255.0,
        wthr.lightning_color[2] as f32 / 255.0,
    ];
    if lightning_color.iter().all(|c| *c <= 0.001) {
        lightning_color = [1.0; 3];
    }

    let max_table_rgb = |table: &[byroredux_plugin::esm::records::weather::SkyColor; 4]| {
        table
            .iter()
            .flat_map(|c| [c.r, c.g, c.b])
            .max()
            .unwrap_or(0) as f32
            / 255.0
    };
    let authored_moon_glare =
        max_table_rgb(&wthr.skyrim_moon_glare).max(max_table_rgb(&wthr.skyrim_extra_colors[6]));
    let moon_glare = if authored_moon_glare > 0.001 {
        authored_moon_glare
    } else {
        FALLBACK_MOON_GLARE
    };
    let aurora_always = wthr.classification & WTHR_AURORA_ALWAYS_VISIBLE != 0;
    let aurora_follows_sun = wthr.classification & WTHR_AURORA_FOLLOWS_SUN != 0;
    let angle = (wthr.wind_direction as f32).to_radians();

    WeatherSkyState {
        // The same WTHR classification that controls atmospheric occupancy
        // is the stable cross-game signal for broad cloud cover. Individual
        // PNAM/JNAM alpha tables still shape the authored detail layers.
        cloud_coverage: fog_coverage_from_weather(wthr.classification),
        cloud_tints,
        precipitation: precipitation_components(wthr.classification),
        thunder_frequency: wthr.thunder_frequency as f32 / 255.0,
        lightning_color,
        stars_color: wthr.sky_colors[SKY_STARS][slot].to_rgb_f32(),
        sunlight_color: wthr.sky_colors[SKY_SUNLIGHT][slot].to_rgb_f32(),
        sun_glare: if wthr.sun_glare == 0 {
            NEUTRAL_SUN_GLARE
        } else {
            wthr.sun_glare as f32 / 255.0
        },
        moon_glare,
        aurora_intensity: if aurora_always || aurora_follows_sun {
            1.0
        } else {
            0.0
        },
        aurora_follows_sun,
        wind_direction: [angle.cos(), angle.sin()],
        wind_direction_authored: wthr.wind_direction_authored,
        wind_speed: wthr.wind_speed as f32 / 255.0,
    }
}

/// Per-climate sunrise/sunset breakpoints in hours. CLMT TNAM bytes
/// are in 10-min units (`hour = byte / 6`); the valid authored range is
/// `1..=144` (`1` = 0:10, `144` = 24:00). Returns the pre-#463 hardcoded
/// `[6.0, 10.0, 18.0, 22.0]` fallback when:
///   * the worldspace has no climate (stub or unresolved record),
///   * the CLMT TNAM is all-zero (a stub field, not authored data),
///   * any of the four bytes lies outside `1..=144` — corruption guard
///     for modded ESMs that ship out-of-range bytes (e.g.
///     `[0, 0, 0, 0xFF]` would otherwise pass the pre-#530 OR-of-bytes
///     filter and produce a sunset_end of 42.5h, breaking the TOD
///     interpolator). See #530 / FNV-CELL-8.
pub(crate) fn climate_tod_hours(
    climate: Option<&byroredux_plugin::esm::records::ClimateRecord>,
) -> [f32; 4] {
    // #2812 — the shared EXAL boundary quad, not a private copy.
    const FALLBACK: [f32; 4] = FB_TOD_HOURS;
    let Some(c) = climate else {
        return FALLBACK;
    };
    let valid = |b: u8| (1..=144).contains(&b);
    if valid(c.sunrise_begin)
        && valid(c.sunrise_end)
        && valid(c.sunset_begin)
        && valid(c.sunset_end)
    {
        [
            c.sunrise_begin as f32 / 6.0,
            c.sunrise_end as f32 / 6.0,
            c.sunset_begin as f32 / 6.0,
            c.sunset_end as f32 / 6.0,
        ]
    } else {
        FALLBACK
    }
}

/// WTHR (+ climate for TOD breakpoints, + the IMGS/worldspace sources for
/// the exterior's base image space, #4914) → [`WeatherDataRes`], the full
/// NAM0 table the per-frame interpolator walks. `skyrim_dalc_per_tod` is
/// `Some` only for Skyrim WTHR (converted Z-up → Y-up once here); `None`
/// elsewhere. `image_space` is resolved here — IMSP, else the worldspace's
/// inherited INAM, else the identity grade — so no caller ever has to patch
/// an identity placeholder.
pub(crate) fn translate_weather(
    wthr: &WeatherRecord,
    climate: Option<&ClimateRecord>,
    imgs: &ImageSpaceSources<'_>,
) -> WeatherDataRes {
    use byroredux_plugin::esm::records::weather::{SKY_COLOR_GROUPS, SKY_TIME_SLOTS, TOD_DAY};
    let mut sky_colors = [[[0.0f32; 3]; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS];
    for (dst_group, src_group) in sky_colors.iter_mut().zip(wthr.sky_colors.iter()) {
        for (dst, src) in dst_group.iter_mut().zip(src_group.iter()) {
            *dst = src.to_rgb_f32();
        }
    }
    let skyrim_dalc_per_tod = wthr.skyrim_ambient_cube.as_ref().map(|cubes| {
        [
            DalcCubeYup::from_skyrim_zup(&cubes[0]),
            DalcCubeYup::from_skyrim_zup(&cubes[1]),
            DalcCubeYup::from_skyrim_zup(&cubes[2]),
            DalcCubeYup::from_skyrim_zup(&cubes[3]),
        ]
    });
    let mut cloud_layer_velocities = [[0.0f32; 2]; 4];
    for (dst, src) in cloud_layer_velocities
        .iter_mut()
        .zip(wthr.cloud_layer_velocities.iter())
    {
        *dst = [src[0] as f32 / 255.0, src[1] as f32 / 255.0];
    }
    let mut cloud_layer_colors = [[[0.0f32; 3]; 4]; 4];
    for (dst_layer, src_layer) in cloud_layer_colors
        .iter_mut()
        .zip(wthr.cloud_layer_colors.iter())
    {
        for (dst, src) in dst_layer.iter_mut().zip(src_layer.iter()) {
            *dst = src.to_rgb_f32();
        }
    }
    let coverage = fog_coverage_from_weather(wthr.classification);
    // #3956 — FO4/FO76 author a per-weather altitude profile in the same
    // `FNAM` tail these distance ramps come from. Adopt the day profile for
    // the day medium and the night profile for the night one, so the TOD blend
    // `weather_system` already runs over this pair carries the height too.
    // `None` (every other game, and the 8 FO4 weathers whose `FNAM` predates
    // the extension) leaves the engine default in place.
    let mut fog_media = [
        crate::fog::FogMedium::from_legacy_ramp(
            wthr.fog_day_near,
            wthr.fog_day_far,
            Some(wthr.fog_day_max),
        ),
        crate::fog::FogMedium::from_legacy_ramp(
            wthr.fog_night_near,
            wthr.fog_night_far,
            Some(wthr.fog_night_max),
        ),
    ];
    if let Some(height) = wthr.fog_height {
        fog_media[0] = fog_media[0].with_authored_height_range(height.day_near_height_range);
        fog_media[1] = fog_media[1].with_authored_height_range(height.night_near_height_range);
    }
    for medium in &mut fog_media {
        medium.coverage = coverage;
    }

    WeatherDataRes {
        sky_colors,
        fog: [
            wthr.fog_day_near,
            wthr.fog_day_far,
            wthr.fog_night_near,
            wthr.fog_night_far,
        ],
        fog_media,
        // #463 — per-climate sunrise/sunset breakpoints (validated helper).
        tod_hours: climate_tod_hours(climate),
        skyrim_dalc_per_tod,
        // #1033 — WTHR DATA wind_speed drives per-weather cloud-scroll rate.
        wind_speed: wthr.wind_speed,
        precipitation: precipitation_from_weather(wthr.classification),
        cloud_layer_velocities,
        cloud_layer_velocities_authored: wthr.cloud_layer_velocities_authored,
        cloud_layer_colors,
        // #5344 — the table carries canonical [0, 1] alphas: the per-frame
        // sampler (`sample_weather_sky`) lerps this array as-is, so a JNAM
        // outside the unit range (mod or corrupt data) used to give every
        // later frame a different cloud alpha than the clamped first-frame
        // seed below. Clamping here — the one place the table is built —
        // makes seed and sampler agree by construction; the transition
        // completion promotes these tables verbatim from another
        // `WeatherDataRes`, so it inherits the clamp.
        cloud_layer_alphas: std::array::from_fn(|layer| {
            std::array::from_fn(|slot| wthr.cloud_layer_alphas[layer][slot].clamp(0.0, 1.0))
        }),
        // #5178 — the named constant, not a bare literal: both seed callers
        // seed at DAY, and a magic index is how the third TOD reduction
        // beside `fold_to_four_tod_slots` grew in the first place.
        weather: weather_sky_state(wthr, TOD_DAY),
        // #4057 — `HNAM` is Oblivion-only, and its absence must be neutral:
        // FNV / FO3 / Skyrim+ ground cover has to render exactly as it did
        // before this field existed.
        grass_dimmer: wthr.oblivion_hdr.map_or(1.0, |hdr| {
            GroundCoverDimmer::from_authored(hdr.grass_dimmer).0
        }),
        // #sunlight-dimmer — same HNAM block, directional-sunlight half.
        // A short/degenerate HNAM decodes 0.0 fields; clamp the negative
        // side (an upside-down sign is never authored) and let 0 read as
        // authored black sun, same trust-the-data rule as grass above.
        sunlight_dimmer: wthr
            .oblivion_hdr
            .map_or(1.0, |hdr| hdr.sunlight_dimmer.max(0.0)),
        // #4914 — resolved at this boundary, not patched by the caller:
        // the weather's IMSP, else the worldspace's inherited INAM, else
        // the identity grade (#4416), with FO3/FNV's per-TOD weather IMAD
        // folded on top.
        image_space: imgs.resolve(Some(wthr)),
    }
}

/// #4416 — the worldspace, IMGS and IMAD tables an exterior's base image
/// space is resolved from, handed to [`translate_weather`] so the
/// translation is the single authority for the whole `WeatherDataRes`
/// (#4914: the caller-side patch left `image_space` identity-shaped at the
/// boundary, and a second caller would have silently rendered ungraded
/// exteriors).
pub(crate) struct ImageSpaceSources<'a> {
    pub worldspaces: &'a HashMap<String, WorldspaceRecord>,
    pub worldspace_key: &'a str,
    pub image_spaces: &'a HashMap<u32, byroredux_plugin::esm::records::ImgsRecord>,
    /// The IMAD table, for FO3/FNV's per-TOD weather modifiers.
    pub modifiers: &'a HashMap<u32, byroredux_plugin::esm::records::ImadRecord>,
}

impl ImageSpaceSources<'_> {
    /// The no-index sources: every lookup misses, so resolution yields the
    /// identity grade — the test paths' pre-#4914 behaviour. Production
    /// callers always hand the real record-index tables.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        static EMPTY_WORLDSPACES: std::sync::LazyLock<HashMap<String, WorldspaceRecord>> =
            std::sync::LazyLock::new(HashMap::new);
        static EMPTY_IMAGE_SPACES: std::sync::LazyLock<
            HashMap<u32, byroredux_plugin::esm::records::ImgsRecord>,
        > = std::sync::LazyLock::new(HashMap::new);
        static EMPTY_MODIFIERS: std::sync::LazyLock<
            HashMap<u32, byroredux_plugin::esm::records::ImadRecord>,
        > = std::sync::LazyLock::new(HashMap::new);
        Self {
            worldspaces: &EMPTY_WORLDSPACES,
            worldspace_key: "",
            image_spaces: &EMPTY_IMAGE_SPACES,
            modifiers: &EMPTY_MODIFIERS,
        }
    }

    /// An exterior's image space per sky-colour time-of-day slot (Sunrise,
    /// Day, Sunset, Night, High Noon, Midnight — the NAM0 order), the
    /// canonical grade `weather_system` samples. The base grade, per the
    /// record definitions (xEdit, #4416):
    ///
    /// 1. The weather's own `IMSP` (Skyrim/FO4). A NULL slot is the identity;
    ///    High Noon / Midnight read Day / Night (IMSP authors four slots).
    /// 2. Otherwise the worldspace's `INAM` (FO3/FNV), inherited up the `WNAM`
    ///    chain through PNAM bit 5 ("Use Image Space Data") like the other
    ///    inheritable worldspace fields, in every slot.
    /// 3. Otherwise the identity grade.
    ///
    /// An IMGS FormID with no decodable grade also reads as the identity.
    ///
    /// FO3/FNV then fold the weather's per-slot IMAD (`\x00IAD`..`\x05IAD`)
    /// over that base — the weather's colour grade: the Mojave's amber noon,
    /// The Pitt's haze, Point Lookout's swamp cast, Anchorage's simulation
    /// grade. Per the GECK ("an imagespace modifier attached to a weather is
    /// active whenever that weather is active"; a non-*Animatable* modifier
    /// ignores its duration) it applies at full strength, held at its first
    /// key (`t = 0`): every vanilla weather IMAD is non-animatable and keys
    /// its authored grade at `t = 0`, with editor-default keys after it (FO3
    /// `WastelandDayISFX`'s `t = 1` tint is pure blue at full weight). A slot
    /// with no IMAD — every FO3 High Noon / Midnight — reads Day / Night's,
    /// the same synthesis the 4-slot NAM0 tables get (#533).
    pub(crate) fn resolve(
        &self,
        weather: Option<&WeatherRecord>,
    ) -> [byroredux_scripting::ImageSpace; SKY_TIME_SLOTS] {
        use crate::systems::weather::fold_to_four_tod_slots;
        let decode = |form: Option<u32>| {
            form.and_then(|form| self.image_spaces.get(&form))
                .and_then(|imgs| imgs.image_space)
                .unwrap_or_default()
        };
        let base: [byroredux_scripting::ImageSpace; SKY_TIME_SLOTS] = match weather
            .map(|w| w.image_spaces)
            .filter(|slots| slots.iter().any(Option::is_some))
        {
            Some(slots) => std::array::from_fn(|slot| decode(slots[fold_to_four_tod_slots(slot)])),
            None => {
                let worldspace = inherit_up_chain(
                    self.worldspaces,
                    self.worldspace_key,
                    pnam::INHERIT_IMAGE_SPACE,
                    |_, w| w.image_space_form,
                );
                [decode(worldspace); SKY_TIME_SLOTS]
            }
        };
        let Some(weather) = weather else {
            return base;
        };
        let modifiers = &weather.image_space_modifiers;
        std::array::from_fn(|slot| {
            let form = modifiers[slot].or(modifiers[fold_to_four_tod_slots(slot)]);
            match form.and_then(|form| self.modifiers.get(&form)) {
                Some(imad) => {
                    byroredux_scripting::cinematic::apply_image_space_modifier(base[slot], imad, 0.0, 1.0)
                }
                None => base[slot],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_translate::test_support::parent_child;

    /// #4416 — exterior image-space precedence: the weather's own `IMSP`
    /// (Skyrim/FO4) wins; otherwise the worldspace's `INAM` (FO3/FNV),
    /// inherited through PNAM bit 5 only when the child authors none; a
    /// NULL slot or an undecodable IMGS is the identity.
    #[test]
    fn exterior_image_space_prefers_weather_then_inherited_worldspace() {
        use byroredux_plugin::esm::records::ImgsRecord;
        use byroredux_scripting::ImageSpace;
        let grade = |saturation| ImageSpace {
            saturation,
            ..ImageSpace::default()
        };
        let image_spaces = HashMap::from([
            (
                0xA1,
                ImgsRecord {
                    form_id: 0xA1,
                    image_space: Some(grade(0.5)),
                    ..Default::default()
                },
            ),
            (
                0xA2,
                ImgsRecord {
                    form_id: 0xA2,
                    image_space: Some(grade(0.8)),
                    ..Default::default()
                },
            ),
        ]);
        let inheriting = parent_child(
            WorldspaceRecord {
                image_space_form: Some(0xA2),
                ..Default::default()
            },
            pnam::INHERIT_IMAGE_SPACE,
            WorldspaceRecord::default(),
        );
        let resolve = |weather: Option<&WeatherRecord>, worldspaces| {
            ImageSpaceSources {
                worldspaces,
                worldspace_key: "c",
                image_spaces: &image_spaces,
                modifiers: &HashMap::new(),
            }
            .resolve(weather)
        };

        assert_eq!(
            resolve(None, &inheriting),
            [grade(0.8); 6],
            "inherited INAM"
        );
        let not_flagged = parent_child(
            WorldspaceRecord {
                image_space_form: Some(0xA2),
                ..Default::default()
            },
            0,
            WorldspaceRecord::default(),
        );
        assert_eq!(resolve(None, &not_flagged), [ImageSpace::default(); 6]);

        let weather = WeatherRecord {
            image_spaces: [Some(0xA1), None, Some(0xA1), Some(0xFFFF)],
            ..Default::default()
        };
        assert_eq!(
            resolve(Some(&weather), &inheriting),
            [
                grade(0.5),
                ImageSpace::default(),
                grade(0.5),
                ImageSpace::default(),
                ImageSpace::default(),
                ImageSpace::default()
            ],
            "IMSP wins; NULL and unknown slots are the identity; High Noon / \
             Midnight read Day / Night"
        );
    }

    /// FO3/FNV — the weather's per-TOD IMAD folds over the worldspace's
    /// `INAM` grade at full strength, held at its first key: "multiply by
    /// zero and add the target" sets a channel, a plain multiply scales the
    /// base, and the tint composites by alpha. A slot with no IMAD (FO3's
    /// High Noon / Midnight) reads Day / Night's; FNV's authored High Noon
    /// stays its own.
    #[test]
    fn weather_imad_folds_over_the_worldspace_grade_per_slot() {
        use byroredux_plugin::esm::records::{ImadColorKey, ImadRecord, ImadScalarKey, ImgsRecord};
        use byroredux_plugin::esm::records::weather::{TOD_DAY, TOD_HIGH_NOON, TOD_MIDNIGHT, TOD_NIGHT, TOD_SUNRISE};
        use byroredux_scripting::ImageSpace;

        let key = |time, value| ImadScalarKey { time, value };
        // NVDefaultExterior-shaped base: warm tint at 33%.
        let base = ImageSpace {
            saturation: 1.1,
            brightness: 1.0,
            contrast: 1.1,
            tint_color: [1.0, 0.5, 0.0, 0.33],
        };
        let image_spaces = HashMap::from([(
            0xB0,
            ImgsRecord {
                form_id: 0xB0,
                image_space: Some(base),
                ..Default::default()
            },
        )]);
        let worldspaces = HashMap::from([(
            "w".to_string(),
            WorldspaceRecord {
                image_space_form: Some(0xB0),
                ..Default::default()
            },
        )]);
        let day = ImadRecord {
            form_id: 0xD1,
            // Set: mult 0 + add 0.875 at t=0, editor-default identity at t=1.
            saturation_mult: vec![key(0.0, 0.0), key(1.0, 1.0)],
            saturation_add: vec![key(0.0, 0.875), key(1.0, 0.0)],
            // Scale: x1.3 at t=0.
            brightness_mult: vec![key(0.0, 1.3), key(1.0, 1.1)],
            tint_color: vec![
                ImadColorKey { time: 0.0, color: [0.0, 0.0, 1.0, 0.5] },
                // The editor default after the authored key: pure blue at full
                // weight. Sampling it would paint the exterior blue.
                ImadColorKey { time: 1.0, color: [0.0, 0.0, 1.0, 1.0] },
            ],
            ..Default::default()
        };
        let noon = ImadRecord {
            form_id: 0xD2,
            contrast_mult: vec![key(0.0, 0.0)],
            contrast_add: vec![key(0.0, 1.5)],
            ..Default::default()
        };
        let modifiers = HashMap::from([(0xD1, day), (0xD2, noon)]);
        let sources = ImageSpaceSources {
            worldspaces: &worldspaces,
            worldspace_key: "w",
            image_spaces: &image_spaces,
            modifiers: &modifiers,
        };

        let mut weather = WeatherRecord::default();
        weather.image_space_modifiers[TOD_DAY] = Some(0xD1);
        let slots = sources.resolve(Some(&weather));
        assert_eq!(slots[TOD_SUNRISE], base, "no IMAD: the worldspace grade");
        assert_eq!(slots[TOD_NIGHT], base);
        assert_eq!(slots[TOD_MIDNIGHT], base, "Midnight reads Night's (none)");
        let graded = slots[TOD_DAY];
        assert_eq!(graded.saturation, 0.875, "mult 0 + add sets the channel");
        assert!((graded.brightness - 1.3).abs() < 1e-6, "a plain mult scales the base");
        assert_eq!(graded.contrast, base.contrast, "an unauthored channel is untouched");
        // GECK tint rule: alpha-weighted average of the base's warm 0.33 and
        // the IMAD's blue 0.5, at the highest alpha.
        assert_eq!(graded.tint_color[3], 0.5);
        assert!((graded.tint_color[2] - 0.5 / 0.83).abs() < 1e-6, "blue weighs 0.5 of 0.83");
        assert!((graded.tint_color[0] - 0.33 / 0.83).abs() < 1e-6);
        assert_eq!(slots[TOD_HIGH_NOON], graded, "FO3: High Noon reads Day's IMAD");

        weather.image_space_modifiers[TOD_HIGH_NOON] = Some(0xD2);
        let slots = sources.resolve(Some(&weather));
        assert_eq!(slots[TOD_HIGH_NOON].contrast, 1.5, "FNV: High Noon keeps its own IMAD");
        assert_eq!(slots[TOD_HIGH_NOON].saturation, base.saturation);
    }

    /// #4914 — the boundary, not the caller, owns the image space: a
    /// `WeatherDataRes` out of `translate_weather` already carries the
    /// resolved grade. The old shape left it identity-shaped for the one
    /// orchestration caller to patch, so any second caller silently
    /// rendered ungraded exteriors.
    #[test]
    fn translate_weather_resolves_the_exterior_image_space_itself() {
        use byroredux_plugin::esm::records::ImgsRecord;
        use byroredux_scripting::ImageSpace;

        let image_spaces = HashMap::from([(
            0xA1,
            ImgsRecord {
                form_id: 0xA1,
                image_space: Some(ImageSpace {
                    saturation: 0.5,
                    brightness: 1.25,
                    contrast: 0.9,
                    tint_color: [0.9, 0.8, 0.7, 0.3],
                }),
                ..Default::default()
            },
        )]);
        let weather = WeatherRecord {
            image_spaces: [Some(0xA1), Some(0xA1), Some(0xA1), None],
            ..Default::default()
        };

        let sources = ImageSpaceSources {
            worldspaces: &HashMap::new(),
            worldspace_key: "c",
            image_spaces: &image_spaces,
            modifiers: &HashMap::new(),
        };
        let wd = translate_weather(&weather, None, &sources);
        assert_eq!(wd.image_space[0].saturation, 0.5);
        assert_eq!(wd.image_space[2].tint_color, [0.9, 0.8, 0.7, 0.3]);
        assert_eq!(
            wd.image_space[3],
            ImageSpace::default(),
            "a NULL slot stays the identity"
        );

        // The empty sources reproduce the pre-#4914 fallback: identity.
        let fallback = translate_weather(&weather, None, &ImageSpaceSources::empty());
        assert_eq!(fallback.image_space, [ImageSpace::default(); 6]);
    }

    // ── exterior sky / sun / weather translation (EXAL step 3) ────

    use byroredux_plugin::esm::records::weather::{
        SkyColor, SKY_AMBIENT, SKY_FOG, SKY_HORIZON, SKY_LOWER, SKY_SUN, SKY_SUNLIGHT, SKY_UPPER,
        TOD_DAY,
    };

    /// Build a WTHR with a distinct colour in one (group, day-slot) cell.
    fn wthr_with(group: usize, c: SkyColor) -> WeatherRecord {
        let mut w = WeatherRecord::default();
        w.sky_colors[group][TOD_DAY] = c;
        w
    }

    #[test]
    fn translate_cell_lighting_reads_day_slot_and_marks_exterior() {
        let mut w = WeatherRecord::default();
        w.sky_colors[SKY_AMBIENT][TOD_DAY] = SkyColor {
            r: 51,
            g: 0,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_SUNLIGHT][TOD_DAY] = SkyColor {
            r: 0,
            g: 102,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_FOG][TOD_DAY] = SkyColor {
            r: 0,
            g: 0,
            b: 204,
            a: 255,
        };
        w.fog_day_near = 1500.0;
        w.fog_day_far = 9000.0;

        let sun_dir = [0.1, 0.9, 0.2];
        let cl = translate_exterior_cell_lighting(&w, sun_dir);
        assert_eq!(cl.ambient, [51.0 / 255.0, 0.0, 0.0]);
        assert_eq!(cl.directional_color, [0.0, 102.0 / 255.0, 0.0]);
        assert_eq!(cl.fog_color, [0.0, 0.0, 204.0 / 255.0]);
        assert_eq!(cl.directional_dir, sun_dir);
        assert_eq!((cl.fog_near, cl.fog_far), (1500.0, 9000.0));
        assert!(!cl.is_interior);
    }

    #[test]
    fn translate_sky_routes_each_cloud_layer_and_sun_sprite() {
        // Distinct day-slot colours so the slot→field mapping is pinned.
        let mut w = wthr_with(
            SKY_UPPER,
            SkyColor {
                r: 10,
                g: 0,
                b: 0,
                a: 255,
            },
        );
        w.sky_colors[SKY_HORIZON][TOD_DAY] = SkyColor {
            r: 0,
            g: 20,
            b: 0,
            a: 255,
        };
        w.sky_colors[SKY_LOWER][TOD_DAY] = SkyColor {
            r: 0,
            g: 0,
            b: 30,
            a: 255,
        };
        w.sky_colors[SKY_SUN][TOD_DAY] = SkyColor {
            r: 40,
            g: 40,
            b: 0,
            a: 255,
        };

        let textures = SkyTextures {
            cloud_layers: [(11, 0.11), (22, 0.22), (33, 0.33), (44, 0.44)],
            sun_sprite: 99,
        };
        let sun_dir = [0.0, 1.0, 0.0];
        let sky = translate_sky(&w, sun_dir, textures, crate::env_translate::FB_TOD_HOURS);

        // Colour slot routing.
        assert_eq!(sky.zenith_color, [10.0 / 255.0, 0.0, 0.0]);
        assert_eq!(sky.horizon_color, [0.0, 20.0 / 255.0, 0.0]);
        assert_eq!(sky.lower_color, [0.0, 0.0, 30.0 / 255.0]);
        assert_eq!(sky.sun_color, [40.0 / 255.0, 40.0 / 255.0, 0.0]);
        // Cloud-layer handle/scale routing — each of the 4 lands in its slot.
        assert_eq!((sky.cloud_texture_index, sky.cloud_tile_scale), (11, 0.11));
        assert_eq!(
            (sky.cloud_texture_index_1, sky.cloud_tile_scale_1),
            (22, 0.22)
        );
        assert_eq!(
            (sky.cloud_texture_index_2, sky.cloud_tile_scale_2),
            (33, 0.33)
        );
        assert_eq!(
            (sky.cloud_texture_index_3, sky.cloud_tile_scale_3),
            (44, 0.44)
        );
        assert_eq!(sky.sun_texture_index, 99);
        // Canonical seeds.
        assert_eq!(sky.sun_direction, sun_dir);
        assert_eq!(sky.sun_size, 0.9995);
        assert_eq!(sky.sun_intensity, 4.0);
        assert_eq!(sky.sun_angular_radius, 0.020);
        assert!(sky.is_exterior);
        // DALC is populated per-frame by weather_system, not at translate.
        assert!(sky.current_dalc_cube.is_none());
    }

    /// #3956 — an authored FO4/FO76 height profile must reach the canonical
    /// medium, per TOD slot, and the day snapshot must agree with the day
    /// slot.
    ///
    /// Measured on vanilla masters before this was wired: 121/121 FO76
    /// weathers and 96/104 FO4 weathers ship the 72-byte `FNAM`, with a median
    /// `day_near_height_range` of 10000 units. At 70 units/m that is ~143 m
    /// against the 30 m the engine used for all of them, so the shipped
    /// profile was ~4.8x too thin wherever it was authored.
    #[test]
    fn authored_height_fog_reaches_the_canonical_medium_per_tod() {
        use byroredux_plugin::esm::records::weather::WeatherHeightFog;
        const UNITS_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;

        let mut w = WeatherRecord {
            fog_day_near: 100.0,
            fog_day_far: 200.0,
            fog_night_near: 300.0,
            fog_night_far: 400.0,
            ..Default::default()
        };
        w.fog_height = Some(WeatherHeightFog {
            // The vanilla FO4 median shape: ground-anchored mid, 10000-unit
            // range by day, deliberately different by night so a day/night
            // mix-up cannot pass.
            day_near_height_mid: 64.0,
            day_near_height_range: 10_000.0,
            night_near_height_mid: 64.0,
            night_near_height_range: 3_000.0,
            ..Default::default()
        });

        let wd = translate_weather(&w, None, &ImageSpaceSources::empty());
        assert!(
            (wd.fog_media[0].scale_height_meters - 10_000.0 / UNITS_PER_METER).abs() < 1e-3,
            "day medium must adopt the authored day range, got {}",
            wd.fog_media[0].scale_height_meters
        );
        assert!(
            (wd.fog_media[1].scale_height_meters - 3_000.0 / UNITS_PER_METER).abs() < 1e-3,
            "night medium must adopt the authored NIGHT range, not the day one"
        );

        // The day-TOD snapshot the cell path uses must agree with slot 0.
        let lighting = translate_exterior_cell_lighting(&w, [0.0, -1.0, 0.0]);
        assert_eq!(
            lighting.fog_medium.scale_height_meters, wd.fog_media[0].scale_height_meters,
            "translate_exterior_cell_lighting is the day snapshot; its height \
             profile must match fog_media[0] or the two paths disagree"
        );
    }

    /// The HNAM "Sunlight Dimmer" rides the EXAL boundary into
    /// `WeatherDataRes` (default 1.0 with no HDR block), so an authored
    /// Oblivion dim reaches `weather_system`'s sunlight sample instead of
    /// being dropped — the field was parsed since #537 but never consumed.
    #[test]
    fn sunlight_dimmer_translates_from_the_hnam_block() {
        use byroredux_plugin::esm::records::weather::{OblivionHdrLighting, WeatherRecord};
        let w = WeatherRecord {
            oblivion_hdr: Some(OblivionHdrLighting {
                sunlight_dimmer: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        let wd = translate_weather(&w, None, &ImageSpaceSources::empty());
        assert!(
            (wd.sunlight_dimmer - 0.5).abs() < 1e-6,
            "an authored HNAM dim must translate verbatim, got {}",
            wd.sunlight_dimmer
        );

        // No HDR block — every non-Oblivion game, and Oblivion weathers
        // without HNAM — keeps the neutral multiplier.
        let plain = WeatherRecord::default();
        assert_eq!(translate_weather(&plain, None, &ImageSpaceSources::empty()).sunlight_dimmer, 1.0);
    }

    /// #3956 — the no-authored-data fallback. Every non-FO4/FO76 game, and the
    /// 8 vanilla FO4 weathers whose `FNAM` predates the extension, must keep
    /// the engine default, resolved here at the boundary rather than by a
    /// branch at draw time.
    #[test]
    fn absent_height_fog_keeps_the_engine_default_scale_height() {
        let w = WeatherRecord {
            fog_day_near: 100.0,
            fog_day_far: 200.0,
            ..Default::default()
        };
        assert!(
            w.fog_height.is_none(),
            "fixture sanity: no authored profile"
        );

        let wd = translate_weather(&w, None, &ImageSpaceSources::empty());
        for (slot, medium) in wd.fog_media.iter().enumerate() {
            assert_eq!(
                medium.scale_height_meters,
                crate::fog::FogMedium::DEFAULT_SCALE_HEIGHT_METERS,
                "TOD slot {slot} must fall back to the engine default"
            );
        }
        assert_eq!(
            translate_exterior_cell_lighting(&w, [0.0, -1.0, 0.0])
                .fog_medium
                .scale_height_meters,
            crate::fog::FogMedium::DEFAULT_SCALE_HEIGHT_METERS,
        );
    }

    /// #3956 — a degenerate authored range must not reach the shader.
    /// `heightFogOpticalDepth` clamps `scaleHeight` to 1e-4, so forwarding a
    /// zero would silently become a 0.1 mm atmosphere rather than a no-op.
    #[test]
    fn a_degenerate_authored_height_range_falls_back_rather_than_collapsing() {
        use byroredux_plugin::esm::records::weather::WeatherHeightFog;
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut w = WeatherRecord {
                fog_day_near: 100.0,
                fog_day_far: 200.0,
                ..Default::default()
            };
            w.fog_height = Some(WeatherHeightFog {
                day_near_height_range: bad,
                night_near_height_range: bad,
                ..Default::default()
            });
            assert_eq!(
                translate_weather(&w, None, &ImageSpaceSources::empty()).fog_media[0].scale_height_meters,
                crate::fog::FogMedium::DEFAULT_SCALE_HEIGHT_METERS,
                "authored range {bad} must be declined, not forwarded"
            );
        }
    }

    /// #3957 — WTHR's own fog falloff power must reach the canonical field.
    /// Captured ahead of a consumer (`fog_params[3]` is documented unconsumed
    /// since #1926/#1927 removed the composite branch), so the value is
    /// already right when the interior-scoped branch lands.
    #[test]
    fn authored_fog_power_reaches_cell_lighting() {
        let w = WeatherRecord {
            fog_day_power: 0.45,
            fog_night_power: 0.25,
            ..Default::default()
        };
        let lighting = translate_exterior_cell_lighting(&w, [0.0, -1.0, 0.0]);
        assert_eq!(
            lighting.fog_power,
            Some(0.45),
            "the day snapshot must forward the DAY power, matching fog_day_near/far beside it"
        );
        assert_eq!(
            lighting.fog_clip, None,
            "fog_clip stays None for a different reason: WTHR has no clip field at all (XCLL-only)"
        );
    }

    #[test]
    fn translate_weather_copies_fog_wind_and_falls_back_tod_without_climate() {
        let mut w = WeatherRecord {
            fog_day_near: 100.0,
            fog_day_far: 200.0,
            fog_night_near: 300.0,
            fog_night_far: 400.0,
            fog_day_max: 0.9,
            fog_night_max: 0.4,
            wind_speed: 7,
            ..Default::default()
        };
        w.classification = byroredux_plugin::esm::records::weather::WTHR_RAINY
            | byroredux_plugin::esm::records::weather::WTHR_AURORA_ALWAYS_VISIBLE;
        w.sun_glare = 128;
        w.thunder_frequency = 64;
        w.lightning_color = [10, 20, 30];
        w.wind_direction = 90;
        w.cloud_layer_velocities[0] = [32, 16];
        w.cloud_layer_velocities_authored[0] = true;
        w.cloud_layer_colors[0][1] = SkyColor {
            r: 80,
            g: 90,
            b: 100,
            a: 200,
        };
        w.cloud_layer_alphas[0][1] = 0.5;
        w.sky_colors[SKY_UPPER][TOD_DAY] = SkyColor {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };

        let wd = translate_weather(&w, None, &ImageSpaceSources::empty());
        assert_eq!(wd.fog, [100.0, 200.0, 300.0, 400.0]);
        let mut expected_day = crate::fog::FogMedium::from_legacy_ramp(100.0, 200.0, Some(0.9));
        let mut expected_night = crate::fog::FogMedium::from_legacy_ramp(300.0, 400.0, Some(0.4));
        // Rainy weather raises atmospheric occupancy above the neutral ramp;
        // the weather classifier is part of the canonical translation.
        expected_day.coverage = 0.86;
        expected_night.coverage = 0.86;
        assert_eq!(wd.fog_media[0], expected_day);
        assert_eq!(wd.fog_media[1], expected_night);
        assert_eq!(wd.wind_speed, 7);
        assert_eq!(wd.cloud_layer_velocities[0], [32.0 / 255.0, 16.0 / 255.0]);
        // #3985 — presence flag must reach WeatherDataRes: layer 0 authored,
        // every other layer left at the record's own unauthored default.
        assert_eq!(
            wd.cloud_layer_velocities_authored,
            [true, false, false, false]
        );
        assert_eq!(
            wd.cloud_layer_colors[0][1],
            [80.0 / 255.0, 90.0 / 255.0, 100.0 / 255.0]
        );
        assert_eq!(wd.cloud_layer_alphas[0][1], 0.5);
        assert_eq!(wd.weather.precipitation, [1.0, 0.0]);
        assert!((wd.weather.thunder_frequency - 64.0 / 255.0).abs() < 1e-6);
        assert_eq!(
            wd.weather.lightning_color,
            [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]
        );
        assert!(wd.weather.wind_direction[0].abs() < 1e-6);
        assert!((wd.weather.wind_direction[1] - 1.0).abs() < 1e-6);
        assert_eq!(wd.weather.aurora_intensity, 1.0);
        // No climate → the validated `climate_tod_hours` fallback.
        assert_eq!(wd.tod_hours, [6.0, 10.0, 18.0, 22.0]);
        // FNV/FO3/Oblivion WTHR (no DALC sub-records) → None.
        assert!(wd.skyrim_dalc_per_tod.is_none());
        // The NAM0 table round-trips to f32.
        assert_eq!(wd.sky_colors[SKY_UPPER][TOD_DAY], [1.0, 0.0, 0.0]);
        // #5178 — the seed's cloud tint samples the DAY PNAM/JNAM tables
        // through the same rule the steady-state sampler applies: RGB from
        // PNAM, alpha from JNAM alone. PNAM's fourth byte is padding
        // (UESP: "rgb per TOD"), not an alpha multiplier — the pre-fix seed
        // scaled this tint's alpha by 200/255.
        assert_eq!(
            wd.weather.cloud_tints[0],
            [80.0 / 255.0, 90.0 / 255.0, 100.0 / 255.0, 0.5],
            "the seed tint reads DAY's PNAM rgb + JNAM alpha, no fourth-byte \
             padding multiplier (#5178)"
        );
    }

    /// #5178 — the seed must read the 4-slot WTHR tables through the shared
    /// `fold_to_four_tod_slots`, not a private `.min(3)`: the old reduction
    /// mapped `TOD_HIGH_NOON` (4) onto NIGHT (3) where the shared fold gives
    /// DAY. Both seed callers pass DAY today, so the misfold was unreachable
    /// — pinned here so a future caller seeding at another TOD cannot
    /// resurrect it.
    #[test]
    fn weather_sky_state_seed_folds_high_noon_onto_day() {
        use byroredux_plugin::esm::records::weather::{TOD_HIGH_NOON, TOD_NIGHT};
        let mut w = WeatherRecord::default();
        w.cloud_layer_colors[0][TOD_DAY] = SkyColor {
            r: 200,
            g: 210,
            b: 220,
            a: 0,
        };
        w.cloud_layer_colors[0][TOD_NIGHT] = SkyColor {
            r: 10,
            g: 12,
            b: 14,
            a: 0,
        };
        w.cloud_layer_alphas[0][TOD_DAY] = 0.75;
        w.cloud_layer_alphas[0][TOD_NIGHT] = 0.25;

        let seeded = weather_sky_state(&w, TOD_HIGH_NOON);
        assert_eq!(
            &seeded.cloud_tints[0][..3],
            &[200.0 / 255.0, 210.0 / 255.0, 220.0 / 255.0],
            "HIGH_NOON must fold onto DAY's cloud tint, not NIGHT's (#5178)"
        );
        assert!(
            (seeded.cloud_tints[0][3] - 0.75).abs() < 1e-6,
            "HIGH_NOON must read DAY's JNAM alpha (#5178)"
        );
        // #5344 — the REAL parity check: the per-frame sampler over the
        // translated table must reproduce the seed, including for a JNAM
        // outside the unit range (the old test compared `wd.weather` with
        // `seeded` — seed@DAY vs seed@HIGH_NOON, a second fold check that
        // never ran the sampler, while the seed clamped and the sampler
        // did not).
        let mut out_of_range = w.clone();
        out_of_range.cloud_layer_alphas[0][TOD_DAY] = 1.5;
        let wd = translate_weather(&out_of_range, None, &ImageSpaceSources::empty());
        let sampled = crate::systems::weather::sample_weather_sky(&wd, TOD_DAY, TOD_DAY, 0.0);
        assert_eq!(
            sampled.cloud_tints[0], wd.weather.cloud_tints[0],
            "the seed and the per-frame sampler must derive one quantity one way"
        );
        assert_eq!(
            wd.weather.cloud_tints[0][3], 1.0,
            "an out-of-range JNAM clamps in both the seed and the carried table"
        );
    }

    #[test]
    fn weather_classification_drives_density_coverage_with_precipitation_priority() {
        use byroredux_plugin::esm::records::weather::{
            WTHR_CLOUDY, WTHR_PLEASANT, WTHR_RAINY, WTHR_SNOW,
        };
        for (classification, expected) in [
            (0, 0.55),
            (WTHR_PLEASANT, 0.40),
            (WTHR_CLOUDY, 0.70),
            (WTHR_SNOW, 0.80),
            (WTHR_RAINY, 0.86),
            (WTHR_PLEASANT | WTHR_RAINY, 0.86),
        ] {
            assert_eq!(fog_coverage_from_weather(classification), expected);
            let weather = WeatherRecord {
                classification,
                ..WeatherRecord::default()
            };
            let translated = translate_weather(&weather, None, &ImageSpaceSources::empty());
            assert_eq!(translated.fog_media[0].coverage, expected);
            assert_eq!(translated.fog_media[1].coverage, expected);
            assert_eq!(translated.weather.cloud_coverage, expected);
            let expected_precipitation = if classification & WTHR_RAINY != 0 {
                1.0
            } else if classification & WTHR_SNOW != 0 {
                0.12
            } else {
                0.0
            };
            assert_eq!(translated.precipitation, expected_precipitation);
        }
    }
}
