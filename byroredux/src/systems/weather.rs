//! Weather + time-of-day system.
//!
//! Advances the game clock, samples the climate-driven TOD colour and
//! fog tables on each frame, cross-fades through any in-flight WTHR
//! transition, and writes the result into `SkyParamsRes` /
//! `CloudSimState` / `CellLightingRes`.

use byroredux_core::ecs::World;

use crate::components::{
    CellLightingRes, CloudSimState, GameTimeRes, SkyParamsRes, WeatherDataRes, WeatherTransitionRes,
};

/// Build the time-of-day key table used by the `weather_system`
/// interpolator from a climate's `tod_hours`.
///
/// `tod_hours = [sunrise_begin, sunrise_end, sunset_begin, sunset_end]`
/// in floating-point game hours (CLMT TNAM bytes divided by 6). The
/// returned 7-entry table is `(hour, TOD slot index)` pairs the
/// interpolator walks in increasing-hour order:
///
///  - `midnight` (synthetic — TNAM doesn't encode it; anchored at 1h)
///  - `sunrise_begin` → `TOD_SUNRISE`
///  - `sunrise_end`   → `TOD_DAY`
///  - midpoint(sunrise_end, sunset_begin) → `TOD_HIGH_NOON`
///  - `sunset_begin - 2h` (clamped) → `TOD_DAY` re-anchor — preserves
///    the `day → sunset` ease-in the pre-#463 hardcoded path had
///  - `sunset_begin` → `TOD_SUNSET`
///  - `sunset_end + 2h` (clamped to 23h) → `TOD_NIGHT`
///
/// Kept `pub(crate)` so the unit test in this module can pin the
/// formula independently of a full World setup.
pub(crate) fn build_tod_keys(tod_hours: [f32; 4]) -> [(f32, usize); 7] {
    use byroredux_plugin::esm::records::weather::*;
    let [sunrise_begin, sunrise_end, sunset_begin, sunset_end] = tod_hours;
    let afternoon_peak = (sunrise_end + sunset_begin) * 0.5;
    let afternoon_cool = (sunset_begin - 2.0).max(sunrise_end + 0.1);
    let midnight = 1.0f32;
    let night = (sunset_end + 2.0).min(23.0);
    [
        (midnight, TOD_MIDNIGHT),
        (sunrise_begin, TOD_SUNRISE),
        (sunrise_end, TOD_DAY),
        (afternoon_peak, TOD_HIGH_NOON),
        (afternoon_cool, TOD_DAY),
        (sunset_begin, TOD_SUNSET),
        (night, TOD_NIGHT),
    ]
}

/// Walk a `build_tod_keys` table at `hour` and return the bracketing
/// `(slot_a, slot_b, t)` tuple for piecewise-linear palette + fog
/// interpolation. `t` is the fraction along the `[slot_a → slot_b]`
/// segment; pre/post-key hours land on the wrap segment
/// `keys[last] → keys[0] + 24`.
///
/// Hoisted out of `weather_system` so the current snapshot walk and
/// the WTHR cross-fade target walk share one implementation —
/// REN-D15-NEW-05 (audit `2026-05-09`).
pub(crate) fn pick_tod_pair(keys: &[(f32, usize); 7], hour: f32) -> (usize, usize, f32) {
    // Wrap pre-midnight hours (e.g. 0.5) into the [1, 25) range so the
    // last-key → first-key wrap segment is reachable from a single
    // monotonic compare below.
    let h = if hour < keys[0].0 { hour + 24.0 } else { hour };
    let last = keys.len() - 1;
    let mut found = (keys[last].1, keys[0].1, 0.0f32);
    for i in 0..last {
        let (h0, s0) = keys[i];
        let (h1, s1) = keys[i + 1];
        if h >= h0 && h < h1 {
            found = (s0, s1, (h - h0) / (h1 - h0));
            break;
        }
    }
    // After last key (typically 22h+): interpolate night → midnight.
    if h >= keys[last].0 {
        let h0 = keys[last].0;
        let h1 = keys[0].0 + 24.0;
        let frac = ((h - h0) / (h1 - h0)).clamp(0.0, 1.0);
        found = (keys[last].1, keys[0].1, frac);
    }
    found
}

/// Map a TOD slot to its `night_factor` contribution in `[0.0, 1.0]`
/// (`0.0 = full daytime fog distance, 1.0 = full night fog distance`).
/// Used by `weather_system` to lerp fog distance through the same TOD
/// slot pair the colour interpolator just walked, keeping palette and
/// fog in lockstep.
///
/// Pre-#897 the fog distance used hardcoded hour breakpoints (6, 18,
/// 20, 4) while colours used the climate-driven `build_tod_keys` table.
/// On non-default-hour CLMTs (FO3 Capital Wasteland's `[5.333, 10, 17,
/// 22]` is the canonical case) the palette transitioned at the
/// authored hours while fog snapped at 6/18 — palette and fog
/// disagreed on "day" vs "transitioning" for ~0.3-2h windows. See #897
/// / REN-D15-01.
///
/// Slot mapping:
/// - `TOD_DAY`, `TOD_HIGH_NOON` → `0.0` (full day fog)
/// - `TOD_NIGHT`, `TOD_MIDNIGHT` → `1.0` (full night fog)
/// - `TOD_SUNRISE`, `TOD_SUNSET` → `0.5` (half-transitioned — the
///   per-key lerp toward the adjacent DAY/NIGHT slot completes the
///   smooth transition)
pub(crate) fn tod_slot_night_factor(slot: usize) -> f32 {
    use byroredux_plugin::esm::records::weather::*;
    if slot == TOD_DAY || slot == TOD_HIGH_NOON {
        0.0
    } else if slot == TOD_NIGHT || slot == TOD_MIDNIGHT {
        1.0
    } else {
        // TOD_SUNRISE / TOD_SUNSET — half-transitioned. The lerp
        // through `(slot_a, slot_b, t)` covers [0.5, 0.0] (sunrise→day)
        // and [0.5, 1.0] (sunset→night) smoothly.
        0.5
    }
}

#[inline]
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
fn lerp1(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Sample a `WeatherDataRes`-shaped snapshot at the given `(slot_a, slot_b, t)`
/// tuple. Returns the seven blended fields the WTHR cross-fade composer
/// needs: zenith, horizon, lower, sun_col, ambient, sunlight, fog_col.
///
/// Pulled out of `weather_system` so the current-snapshot path and the
/// cross-fade target path share one implementation — saves seven copy-pasted
/// `lerp3` calls each on a 6-tuple of indices.
#[inline]
fn sample_wthr_colors(
    sky_colors: &[[[f32; 3]; 6]; 10],
    slot_a: usize,
    slot_b: usize,
    t: f32,
) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    use byroredux_plugin::esm::records::weather::*;
    (
        lerp3(sky_colors[SKY_UPPER][slot_a], sky_colors[SKY_UPPER][slot_b], t),
        lerp3(sky_colors[SKY_HORIZON][slot_a], sky_colors[SKY_HORIZON][slot_b], t),
        lerp3(sky_colors[SKY_LOWER][slot_a], sky_colors[SKY_LOWER][slot_b], t),
        lerp3(sky_colors[SKY_SUN][slot_a], sky_colors[SKY_SUN][slot_b], t),
        lerp3(sky_colors[SKY_AMBIENT][slot_a], sky_colors[SKY_AMBIENT][slot_b], t),
        lerp3(sky_colors[SKY_SUNLIGHT][slot_a], sky_colors[SKY_SUNLIGHT][slot_b], t),
        lerp3(sky_colors[SKY_FOG][slot_a], sky_colors[SKY_FOG][slot_b], t),
    )
}

/// Weather & time-of-day system: advances game clock, interpolates WTHR
/// NAM0 sky colors, computes sun arc, and updates SkyParamsRes + CellLightingRes.
///
/// Only runs when WeatherDataRes + GameTimeRes exist (exterior cells with weather).
///
/// M33.1 — when `WeatherTransitionRes` is present, the system blends the
/// per-TOD-sampled colours between the current `WeatherDataRes` and the
/// transition's `target` snapshot by `t = elapsed_secs / duration_secs`.
/// Each weather is independently TOD-sampled (so the transition stays
/// correct across midnight wraps where each side might land on a
/// different slot); only the final per-channel lerp uses `t`. When the
/// transition completes (`t >= 1.0`) the resource is removed and the
/// live `WeatherDataRes` is replaced with `target` for subsequent frames.
pub(crate) fn weather_system(world: &World, dt: f32) {
    // Advance game clock.
    let hour = {
        let Some(mut game_time) = world.try_resource_mut::<GameTimeRes>() else {
            return;
        };
        game_time.hour += dt * game_time.time_scale / 3600.0;
        if game_time.hour >= 24.0 {
            game_time.hour -= 24.0;
        }
        game_time.hour
    };

    // M33.1 — advance the in-flight WTHR cross-fade timer (if any) and
    // capture the blend weight + finished flag for use below. When the
    // transition completes we swap WeatherDataRes to the target snapshot
    // and drop the transition resource.
    let (transition_t, transition_done) =
        if let Some(mut tr) = world.try_resource_mut::<WeatherTransitionRes>() {
            // Once `done` latches, freeze the timer and skip the
            // blend ratio computation entirely — pre-#REN-D15-NEW-07
            // the elapsed counter advanced every frame forever and
            // eventually saturated f32 toward INFINITY. See
            // `WeatherTransitionRes.done` doc for the full rationale.
            if tr.done {
                (0.0, false)
            } else {
                tr.elapsed_secs += dt;
                let dur = tr.duration_secs.max(1e-3);
                let t = (tr.elapsed_secs / dur).clamp(0.0, 1.0);
                (t, t >= 1.0)
            }
        } else {
            (0.0, false)
        };

    let Some(wd) = world.try_resource::<WeatherDataRes>() else {
        return;
    };

    // Interpolate NAM0 colors based on game hour.
    // The 6 time slots map to these hours:
    //   0 = sunrise, 1 = day, 2 = sunset,
    //   3 = night, 4 = high_noon, 5 = midnight.
    //
    // Pre-#463 the breakpoints were hardcoded:
    //   midnight(1h) → sunrise(6h) → day(10h) → high_noon(13h) →
    //   day(16h) → sunset(18h) → night(22h) → midnight(25h/1h)
    // FO3 Capital Wasteland and FNV Mojave ship different CLMT TNAM
    // values (Wasteland sunrise is ~0.3 hr earlier). `tod_hours` on
    // WeatherDataRes now carries the climate-driven breakpoints; the
    // `high_noon` midpoint and the `midnight` anchor stay synthetic
    // (TNAM doesn't encode either). The afternoon `day` re-anchor is
    // picked at sunset_begin - 2h so we retain a `day → sunset` ease-
    // in rather than jumping straight from high_noon to sunset.
    let keys = build_tod_keys(wd.tod_hours);

    // Find which two keys we're between and compute blend factor.
    let (slot_a, slot_b, t) = pick_tod_pair(&keys, hour);

    let (zenith, horizon, lower, sun_col, ambient, sunlight, fog_col) =
        sample_wthr_colors(&wd.sky_colors, slot_a, slot_b, t);

    // Fog distance: lerp between day and night fog based on the same
    // TOD slot pair the colour interpolator just walked. Pre-#897 this
    // used hardcoded hour breakpoints (6, 18, 20, 4) which disagreed
    // with the climate-driven colour breakpoints on non-default CLMTs
    // (FO3 Capital Wasteland's earlier sunrise was the canonical case
    // — palette transitioned at hour 5.333 while fog snapped at 6.0).
    // Sharing `(slot_a, slot_b, t)` keeps fog distance in lockstep with
    // sky palette across every shipped CLMT. See #897 / REN-D15-01.
    let night_a = tod_slot_night_factor(slot_a);
    let night_b = tod_slot_night_factor(slot_b);
    let night_factor = night_a + (night_b - night_a) * t;
    let fog_near = wd.fog[0] + (wd.fog[2] - wd.fog[0]) * night_factor;
    let fog_far = wd.fog[1] + (wd.fog[3] - wd.fog[1]) * night_factor;

    // M33.1 — if a WTHR cross-fade is in flight, run the same TOD-slot
    // pick + per-group sampling on the target snapshot and blend each
    // colour channel by `transition_t`. The TOD slots are independent
    // per-side (target may use the same `keys` table since `tod_hours`
    // is on WeatherDataRes; we re-derive it from the target's own
    // breakpoints to stay correct if the target ships a different CLMT).
    let (zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far) =
        if transition_t > 0.0 {
            let tr = world
                .try_resource::<WeatherTransitionRes>()
                .expect("transition_t > 0 implies WeatherTransitionRes");
            let target = &tr.target;

            let keys_b = build_tod_keys(target.tod_hours);
            let (b_a, b_b, b_t) = pick_tod_pair(&keys_b, hour);

            let (
                target_zenith,
                target_horizon,
                target_lower,
                target_sun_col,
                target_ambient,
                target_sunlight,
                target_fog_col,
            ) = sample_wthr_colors(&target.sky_colors, b_a, b_b, b_t);
            let target_fog_near = target.fog[0] + (target.fog[2] - target.fog[0]) * night_factor;
            let target_fog_far = target.fog[1] + (target.fog[3] - target.fog[1]) * night_factor;

            (
                lerp3(zenith, target_zenith, transition_t),
                lerp3(horizon, target_horizon, transition_t),
                lerp3(lower, target_lower, transition_t),
                lerp3(sun_col, target_sun_col, transition_t),
                lerp3(ambient, target_ambient, transition_t),
                lerp3(sunlight, target_sunlight, transition_t),
                lerp3(fog_col, target_fog_col, transition_t),
                lerp1(fog_near, target_fog_near, transition_t),
                lerp1(fog_far, target_fog_far, transition_t),
            )
        } else {
            (
                zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far,
            )
        };

    // Sun direction: semicircular arc from east (6h) through zenith (12h) to west (18h).
    // Below horizon at night. Y-up coordinate system.
    let sun_dir = {
        // Solar angle: 0 at sunrise (6h), π at sunset (18h).
        let solar_hour = (hour - 6.0).clamp(0.0, 12.0);
        let angle = solar_hour / 12.0 * std::f32::consts::PI;
        // Sun arcs from east (+X) through up (+Y) to west (-X) with a
        // slight south tilt — #802 / SUN-N2. Per the Z-up → Y-up swap
        // in `crates/nif/src/import/coord.rs:18` (`(x, y, z) → (x, z, -y)`),
        // Bethesda's authored +Y (north) maps to engine -Z, so SOUTH is
        // engine +Z. All four Bethesda settings (Mojave, Capital
        // Wasteland, Tamriel, Commonwealth) sit at NH latitudes where
        // the real sun arcs through the southern sky; pre-#802 this
        // constant was -0.15 (a NORTH tilt) despite the comment
        // claiming south.
        let x = angle.cos();
        let y = angle.sin();
        let z = 0.15_f32; // slight south tilt (engine +Z = Bethesda -Y = south)
        let len = (x * x + y * y + z * z).sqrt();
        if (6.0..=18.0).contains(&hour) {
            [x / len, y / len, z / len]
        } else {
            // Night: sun below horizon. Push it down so no sun disc renders.
            [0.0, -1.0, 0.0]
        }
    };

    // Sun intensity: fade in/out at sunrise/sunset.
    let sun_intensity = if (7.0..=17.0).contains(&hour) {
        4.0
    } else if (6.0..7.0).contains(&hour) {
        (hour - 6.0) * 4.0 // fade in
    } else if hour > 17.0 && hour <= 18.0 {
        (18.0 - hour) * 4.0 // fade out
    } else {
        0.0 // night
    };

    // Cloud layer 0 scroll rate. Pre-#535 the rate was "derived" from
    // `wd.cloud_speeds[0] / 128.0 * 0.02`, but that byte was actually
    // the first character of the DNAM cloud-path zstring (typically
    // `'s'` = 0x73 = 115 → factor 0.898 → ≈0.018 UV/sec). The visible
    // result looked fine because the authored constant was close, so
    // keep it here as a named baseline while the real per-weather
    // scroll source stays unknown. WTHR has ONAM (4 B, looks f32-ish)
    // and INAM (304 B, per-image transition data) that plausibly carry
    // the speed; sourcing that is deferred — cross-cuts #541's
    // "unused WTHR fields" scope and needs UESP-authoritative byte
    // sampling before committing to an offset.
    let cloud_scroll_rate: f32 = 0.018;

    drop(wd);

    // Update SkyParamsRes.
    if let Some(mut sky) = world.try_resource_mut::<SkyParamsRes>() {
        sky.zenith_color = zenith;
        sky.horizon_color = horizon;
        // #541 — SKY_LOWER drives the renderer's below-horizon
        // gradient. Pre-fix the value was discarded and the shader
        // faked it as `horizon * 0.3`.
        sky.lower_color = lower;
        sky.sun_color = sun_col;
        sky.sun_direction = sun_dir;
        sky.sun_intensity = sun_intensity;
    }

    // #803 — cloud scroll lives on `CloudSimState`, which survives
    // cell transitions (unlike `SkyParamsRes`, which `unload_cell`
    // removes on every cell unload). Writing here keeps the
    // accumulator alive across interior visits so the renderer's
    // next-frame sample lands at the same UV the player saw before
    // entering the interior, rather than snapping back to origin.
    //
    // Wrap scroll at 1.0 so it never grows unboundedly; sampler
    // REPEAT makes the wrap invisible.
    if let Some(mut clouds) = world.try_resource_mut::<CloudSimState>() {
        clouds.cloud_scroll[0] = (clouds.cloud_scroll[0] + cloud_scroll_rate * dt).rem_euclid(1.0);
        clouds.cloud_scroll[1] =
            (clouds.cloud_scroll[1] + cloud_scroll_rate * 0.3 * dt).rem_euclid(1.0);
        // Layer 1 drifts in the opposite U direction at 1.35× speed.
        // Creates visible parallax against layer 0 with no per-weather
        // source needed. See #541 (ONAM/INAM decode) for eventual
        // authoritative values.
        clouds.cloud_scroll_1[0] =
            (clouds.cloud_scroll_1[0] - cloud_scroll_rate * 1.35 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_1[1] =
            (clouds.cloud_scroll_1[1] + cloud_scroll_rate * 0.5 * dt).rem_euclid(1.0);
        // Layer 2 (WTHR ANAM) and layer 3 (BNAM) used to mirror layer 0
        // and layer 1 verbatim — when ANAM/BNAM resolved to the same
        // texture as DNAM/CNAM (or were absent), the four-layer composite
        // collapsed to two visually identical pairs. Until WTHR ONAM
        // (4 B, looks f32-ish) and INAM (304 B per-image transition data)
        // are decoded as the authoritative per-weather scroll source,
        // pick distinct multipliers so the four layers always have four
        // visibly different drifts. Slower base U on the high layers
        // matches the conventional cirrus-vs-stratus authoring pattern
        // (cirrus drifts slowly relative to the lower deck). #899.
        clouds.cloud_scroll_2[0] =
            (clouds.cloud_scroll_2[0] + cloud_scroll_rate * 0.85 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_2[1] =
            (clouds.cloud_scroll_2[1] + cloud_scroll_rate * 0.45 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_3[0] =
            (clouds.cloud_scroll_3[0] - cloud_scroll_rate * 1.15 * dt).rem_euclid(1.0);
        clouds.cloud_scroll_3[1] =
            (clouds.cloud_scroll_3[1] + cloud_scroll_rate * 0.6 * dt).rem_euclid(1.0);
    }

    // Update CellLightingRes — exterior cells only. Interior cells own
    // their own ambient / directional / fog values from XCLL or LGTM
    // records (see `scene.rs::load_cell` interior path); the weather
    // system would otherwise clobber them with sky-tinted exterior fog
    // and time-of-day-driven ambient/directional from the most recent
    // exterior worldspace, producing visibly wrong lighting on every
    // interior cell loaded after any exterior session. See #782.
    if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
        if !cell_lit.is_interior {
            cell_lit.ambient = ambient;
            cell_lit.directional_color = sunlight;
            cell_lit.directional_dir = sun_dir;
            cell_lit.fog_color = fog_col;
            cell_lit.fog_near = fog_near;
            cell_lit.fog_far = fog_far;
        }
    }

    // M33.1 — promote the in-flight transition target into the live
    // WeatherDataRes once the cross-fade completes. Uses in-place
    // mutation via try_resource_mut (interior mutability, &World safe).
    // elapsed_secs is saturated at duration_secs so subsequent frames
    // skip the blend path without removing the resource (remove_resource
    // needs &mut World which systems do not have).
    if transition_done {
        if let Some(tr) = world.try_resource::<WeatherTransitionRes>() {
            let new_sky = tr.target.sky_colors;
            let new_fog = tr.target.fog;
            let new_tod = tr.target.tod_hours;
            drop(tr);
            if let Some(mut wd) = world.try_resource_mut::<WeatherDataRes>() {
                wd.sky_colors = new_sky;
                wd.fog = new_fog;
                wd.tod_hours = new_tod;
            }
            // Latch the transition as done. Pre-fix this set
            // `duration_secs = f32::INFINITY` and relied on float
            // arithmetic to keep the blend ratio at 0 — the dormant
            // state machine then accumulated `elapsed_secs += dt`
            // every frame forever, eventually saturating to INFINITY
            // itself and making the ratio NaN. The explicit `done`
            // bool drops both hazards. See REN-D15-NEW-07 (audit
            // 2026-05-09).
            if let Some(mut tr) = world.try_resource_mut::<WeatherTransitionRes>() {
                tr.done = true;
            }
        }
    }
}

/// Regression tests for #463 — climate-driven TOD breakpoints on
/// `WeatherDataRes.tod_hours` flow through `build_tod_keys` so the
/// time-of-day interpolator runs on the right schedule per worldspace.
#[cfg(test)]
mod tod_keys_tests {
    use super::*;
    use byroredux_plugin::esm::records::weather::*;

    /// Pre-#463 default — FNV Mojave-style hardcoded breakpoints.
    /// Verifies the fallback path still produces the same key table
    /// synthetic test cells used to get.
    #[test]
    fn default_tod_hours_reproduce_pre_fix_fnv_keys() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        let expected = [
            (1.0, TOD_MIDNIGHT),
            (6.0, TOD_SUNRISE),
            (10.0, TOD_DAY),
            (14.0, TOD_HIGH_NOON), // midpoint(10, 18)
            (16.0, TOD_DAY),       // sunset_begin - 2
            (18.0, TOD_SUNSET),
            (23.0, TOD_NIGHT), // min(22+2, 23) = 23 (clamped)
        ];
        for (i, ((h, s), (eh, es))) in keys.iter().zip(expected.iter()).enumerate() {
            assert!(
                (h - eh).abs() < 1e-5,
                "key[{i}]: expected hour {eh:.2}, got {h:.2}"
            );
            assert_eq!(s, es, "key[{i}]: slot mismatch");
        }
    }

    /// FO3 Capital Wasteland ships slightly earlier sunrise per the
    /// audit. Feed representative Wasteland TNAM-derived hours and
    /// verify the interpolator hits those exact breakpoints instead
    /// of the hardcoded FNV values.
    #[test]
    fn fo3_wasteland_climate_shifts_sunrise_earlier() {
        // Hypothetical FO3 TNAM: sunrise_begin=32, sunrise_end=60,
        // sunset_begin=102, sunset_end=132 (in 10-minute units).
        //   → hours 5.33, 10.0, 17.0, 22.0.
        let wasteland = build_tod_keys([5.333, 10.0, 17.0, 22.0]);
        let fnv = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // SUNRISE anchor moved earlier.
        assert!(
            wasteland[1].0 < fnv[1].0,
            "Wasteland SUNRISE key must fire before FNV SUNRISE"
        );
        // SUNSET anchor moved earlier too.
        assert!(
            wasteland[5].0 < fnv[5].0,
            "Wasteland SUNSET key must fire before FNV SUNSET"
        );
        // Slot identities stay put — only the hour anchors change.
        for i in 0..7 {
            assert_eq!(
                wasteland[i].1, fnv[i].1,
                "slot ordering must match across climates"
            );
        }
    }

    /// Keys must stay monotonically non-decreasing in hour so the
    /// piecewise-linear interpolator walks them in order.
    #[test]
    fn tod_keys_are_monotonic_on_realistic_climates() {
        for tod_hours in [
            [6.0, 10.0, 18.0, 22.0],  // FNV
            [5.33, 10.0, 17.0, 22.0], // FO3 Wasteland
            [4.5, 9.0, 19.5, 22.0],   // Skyrim Tundra (hypothetical)
            [7.0, 11.0, 16.0, 19.0],  // compressed-day winter
        ] {
            let keys = build_tod_keys(tod_hours);
            for w in keys.windows(2) {
                assert!(
                    w[0].0 <= w[1].0 + 1e-5,
                    "TOD keys must be monotonic: {:?} → {:?} for tod_hours {:?}",
                    w[0],
                    w[1],
                    tod_hours,
                );
            }
        }
    }

    /// Afternoon_cool clamp — when `sunset_begin <= sunrise_end + 2`
    /// (very compressed day), the `sunset_begin - 2h` re-anchor would
    /// be at or before `sunrise_end`, breaking monotonicity. The
    /// `.max(sunrise_end + 0.1)` clamp guards against that.
    #[test]
    fn tod_keys_clamp_afternoon_cool_on_compressed_days() {
        // sunrise_end=10, sunset_begin=11 — only 1h of clear "day".
        let keys = build_tod_keys([5.0, 10.0, 11.0, 20.0]);
        let day_anchor = keys[2].0; // TOD_DAY at sunrise_end
        let afternoon_cool = keys[4].0; // TOD_DAY re-anchor
        assert!(
            afternoon_cool > day_anchor,
            "afternoon_cool ({afternoon_cool:.2}) must be strictly after \
             sunrise_end ({day_anchor:.2}) to keep keys monotonic"
        );
    }

    /// `tod_slot_night_factor` — the per-slot fog-distance contribution
    /// that pairs with `build_tod_keys` to keep fog in lockstep with
    /// the sky palette. DAY-class slots map to 0, NIGHT-class to 1,
    /// transition slots to 0.5 so the per-key lerp covers the
    /// half-transitioned span smoothly. See #897 / REN-D15-01.
    #[test]
    fn night_factor_full_day_slots_are_zero() {
        assert_eq!(tod_slot_night_factor(TOD_DAY), 0.0);
        assert_eq!(tod_slot_night_factor(TOD_HIGH_NOON), 0.0);
    }

    #[test]
    fn night_factor_full_night_slots_are_one() {
        assert_eq!(tod_slot_night_factor(TOD_NIGHT), 1.0);
        assert_eq!(tod_slot_night_factor(TOD_MIDNIGHT), 1.0);
    }

    #[test]
    fn night_factor_transition_slots_are_half() {
        // The midpoint values let the per-key lerp through
        // `(slot_a, slot_b, t)` cover SUNRISE→DAY (0.5→0.0) and
        // SUNSET→NIGHT (0.5→1.0) smoothly.
        assert_eq!(tod_slot_night_factor(TOD_SUNRISE), 0.5);
        assert_eq!(tod_slot_night_factor(TOD_SUNSET), 0.5);
    }

    /// Regression for #897 / REN-D15-01.
    ///
    /// Pre-fix: at hour 5.7 with FO3 Capital Wasteland-style climate
    /// (`tod_hours = [5.333, 10.0, 17.0, 22.0]`), the colour
    /// interpolator landed in the `(SUNRISE, DAY)` slot pair (palette
    /// = sunrise) while the hardcoded fog `night_factor` returned
    /// `(6.0 - 5.7) / 2.0 = 0.15` (fog mostly day) — palette and fog
    /// disagreed on "day" vs "transitioning" by ~0.3 h window.
    ///
    /// Post-fix: fog uses the same `(slot_a, slot_b, t)` tuple and the
    /// `tod_slot_night_factor` helper. At hour 5.7 the lerp from
    /// SUNRISE (0.5) toward DAY (0.0) at `t = (5.7 - 5.333) / (10.0
    /// - 5.333) ≈ 0.0786` produces `night_factor ≈ 0.461` —
    /// half-transitioned, matching the SUNRISE-class palette.
    #[test]
    fn fo3_wasteland_sunrise_fog_lockstep_with_palette() {
        let keys = build_tod_keys([5.333, 10.0, 17.0, 22.0]);
        let h = 5.7_f32;
        // Walk the keys exactly the way `weather_system` does.
        let mut slot_a = keys[keys.len() - 1].1;
        let mut slot_b = keys[0].1;
        let mut t = 0.0_f32;
        for i in 0..keys.len() - 1 {
            let (h0, s0) = keys[i];
            let (h1, s1) = keys[i + 1];
            if h >= h0 && h < h1 {
                slot_a = s0;
                slot_b = s1;
                t = (h - h0) / (h1 - h0);
                break;
            }
        }
        assert_eq!(
            slot_a, TOD_SUNRISE,
            "slot_a at FO3 hour 5.7 must be SUNRISE"
        );
        assert_eq!(slot_b, TOD_DAY, "slot_b at FO3 hour 5.7 must be DAY");
        let na = tod_slot_night_factor(slot_a);
        let nb = tod_slot_night_factor(slot_b);
        let night_factor = na + (nb - na) * t;
        assert!(
            night_factor > 0.4 && night_factor < 0.5,
            "night_factor at FO3 hour 5.7 must be half-transitioned \
             (in [0.4, 0.5]) so fog tracks the SUNRISE-class palette. \
             Pre-#897 hardcoded hours produced 0.15 here. \
             Got {night_factor:.3}",
        );
    }

    /// `pick_tod_pair` mid-segment — hour lands inside a key bracket
    /// and returns the surrounding slot pair plus the linear fraction.
    /// This is the common path every gameplay frame walks.
    #[test]
    fn pick_tod_pair_mid_segment_lerp() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 7.0 sits between SUNRISE (6.0) and DAY (10.0) → t = 0.25.
        let (a, b, t) = pick_tod_pair(&keys, 7.0);
        assert_eq!(a, TOD_SUNRISE);
        assert_eq!(b, TOD_DAY);
        assert!((t - 0.25).abs() < 1e-5, "expected t≈0.25, got {t}");
    }

    /// `pick_tod_pair` wrap branch — pre-midnight hours (< first key)
    /// must reach into the [last, first+24) wrap segment so the night
    /// → midnight blend stays smooth across the day boundary.
    #[test]
    fn pick_tod_pair_pre_midnight_wraps_into_night_segment() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 0.5 wraps to 24.5; falls inside NIGHT (23) → MIDNIGHT (25).
        let (a, b, t) = pick_tod_pair(&keys, 0.5);
        assert_eq!(a, TOD_NIGHT, "pre-midnight hour 0.5 wraps into NIGHT");
        assert_eq!(b, TOD_MIDNIGHT);
        // t = (24.5 - 23) / (25 - 23) = 0.75.
        assert!((t - 0.75).abs() < 1e-5, "expected t≈0.75, got {t}");
    }

    /// `pick_tod_pair` post-last-key branch — hour after the last
    /// authored key (typically 22h+) interpolates NIGHT → MIDNIGHT
    /// through the same wrap segment as the pre-midnight case.
    #[test]
    fn pick_tod_pair_post_night_anchor_returns_night_to_midnight() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 24.0 (equivalently 0.0 next day, but the wrap normalizes
        // pre-keys[0]; this test hits the >= keys[last] branch directly).
        let (a, b, t) = pick_tod_pair(&keys, 23.5);
        assert_eq!(a, TOD_NIGHT);
        assert_eq!(b, TOD_MIDNIGHT);
        assert!(t > 0.0 && t <= 1.0);
    }

    /// Default FNV-style climate at noon must yield zero night_factor
    /// (the easy case — both sides DAY-class, lerp stays at 0).
    #[test]
    fn fnv_default_noon_fog_is_full_day() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        let h = 12.0_f32;
        let mut slot_a = keys[0].1;
        let mut slot_b = keys[0].1;
        let mut t = 0.0_f32;
        for i in 0..keys.len() - 1 {
            let (h0, s0) = keys[i];
            let (h1, s1) = keys[i + 1];
            if h >= h0 && h < h1 {
                slot_a = s0;
                slot_b = s1;
                t = (h - h0) / (h1 - h0);
                break;
            }
        }
        let na = tod_slot_night_factor(slot_a);
        let nb = tod_slot_night_factor(slot_b);
        let night_factor = na + (nb - na) * t;
        assert_eq!(
            night_factor, 0.0,
            "noon must produce full-day fog (both endpoints DAY-class)"
        );
    }
}

/// Regression tests for #782 — `weather_system` was unconditionally
/// writing time-of-day-derived `ambient` / `directional` / `fog_color`
/// (etc.) into `CellLightingRes` regardless of whether the active cell
/// was interior or exterior. Interior cells loaded after any exterior
/// session inherited the most-recent WTHR fog tint (typically sky-blue
/// `[0.65, 0.7, 0.8]`) instead of their own XCLL-authored fog. The
/// composite pass blended that into distant pixels at up to 70%
/// opacity in HDR linear space pre-ACES, producing a visibly chromy /
/// posterized look on every distant interior surface.
///
/// The fix gates all six `cell_lit.*` writes on `!is_interior` —
/// interior cells preserve their XCLL/LGTM-authored values from the
/// cell loader; exterior cells continue to be driven by weather TOD.
#[cfg(test)]
mod interior_gate_tests {
    use super::*;
    use byroredux_core::ecs::World;

    /// Insert the minimum resource set that lets `weather_system` reach
    /// the `CellLightingRes` update without early-returning, with a
    /// `WeatherDataRes` populated to a deliberately bright sky-blue
    /// fog so any leak into `cell_lit.fog_color` is unambiguous.
    fn build_world(is_interior: bool) -> World {
        let mut world = World::new();

        // Interior fog the cell loader supposedly placed — a dim
        // brownish tint that we expect to survive `weather_system`.
        const INTERIOR_FOG_COLOR: [f32; 3] = [0.05, 0.06, 0.08];
        const INTERIOR_FOG_NEAR: f32 = 64.0;
        const INTERIOR_FOG_FAR: f32 = 4000.0;

        world.insert_resource(CellLightingRes {
            ambient: [0.1, 0.1, 0.1],
            directional_color: [0.3, 0.3, 0.3],
            directional_dir: [0.0, 1.0, 0.0],
            is_interior,
            fog_color: INTERIOR_FOG_COLOR,
            fog_near: INTERIOR_FOG_NEAR,
            fog_far: INTERIOR_FOG_FAR,
            // Test fixture — extended XCLL fields not exercised here.
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        });

        world.insert_resource(GameTimeRes {
            hour: 12.0,      // mid-day so the TOD slot is unambiguous
            time_scale: 0.0, // freeze the clock so dt advances are no-ops
        });

        // Build a WTHR snapshot with sky-blue fog at every TOD slot so
        // any unconditional write would clobber the interior fog with
        // (0.65, 0.7, 0.8) — the symptom from #782.
        let bright_sky_blue = [0.65_f32, 0.7, 0.8];
        let mut sky_colors = [[[0.0_f32; 3]; 6]; 10];
        for slot in 0..6 {
            sky_colors[byroredux_plugin::esm::records::weather::SKY_FOG][slot] = bright_sky_blue;
            sky_colors[byroredux_plugin::esm::records::weather::SKY_AMBIENT][slot] =
                [0.5, 0.5, 0.5];
            sky_colors[byroredux_plugin::esm::records::weather::SKY_SUNLIGHT][slot] =
                [1.0, 1.0, 1.0];
        }
        world.insert_resource(WeatherDataRes {
            sky_colors,
            fog: [100.0, 60000.0, 200.0, 30000.0],
            tod_hours: [6.0, 10.0, 18.0, 22.0],
        });

        world
    }

    /// Interior gate — `cell_lit.fog_color` (and the rest of the gated
    /// fields) must NOT change after `weather_system` runs against a
    /// world whose `CellLightingRes.is_interior == true`, even when
    /// `WeatherDataRes` carries a fog target wildly different from the
    /// XCLL-authored value.
    #[test]
    fn interior_cell_fog_is_not_overwritten_by_weather() {
        let world = build_world(true);
        weather_system(&world, 0.016);

        let cell_lit = world.try_resource::<CellLightingRes>().unwrap();
        assert_eq!(
            cell_lit.fog_color,
            [0.05, 0.06, 0.08],
            "interior fog_color was overwritten by weather_system — \
             #782 regression"
        );
        assert!(
            (cell_lit.fog_near - 64.0).abs() < 1e-5,
            "interior fog_near was overwritten — #782 regression"
        );
        assert!(
            (cell_lit.fog_far - 4000.0).abs() < 1e-5,
            "interior fog_far was overwritten — #782 regression"
        );
        // Sibling fields gated together with fog — same regression risk.
        assert_eq!(
            cell_lit.ambient,
            [0.1, 0.1, 0.1],
            "interior ambient was overwritten — #782 regression"
        );
        assert_eq!(
            cell_lit.directional_color,
            [0.3, 0.3, 0.3],
            "interior directional_color was overwritten — #782 regression"
        );
    }

    /// Exterior path still works — weather_system MUST update fog on
    /// exterior cells (otherwise sky-tinted fog never reaches the
    /// composite UBO at all). Negative test that pins the gate's
    /// `!is_interior` polarity.
    #[test]
    fn exterior_cell_fog_is_updated_by_weather() {
        let world = build_world(false);
        weather_system(&world, 0.016);

        let cell_lit = world.try_resource::<CellLightingRes>().unwrap();
        // Mid-day with the sky-blue fog at every slot — interpolator
        // returns the slot value unchanged.
        assert!(
            (cell_lit.fog_color[0] - 0.65).abs() < 1e-3,
            "exterior fog_color was not updated by weather_system: {:?}",
            cell_lit.fog_color
        );
        assert!(
            (cell_lit.fog_color[2] - 0.8).abs() < 1e-3,
            "exterior fog_color was not updated by weather_system: {:?}",
            cell_lit.fog_color
        );
    }
}
