//! Weather + time-of-day system.
//!
//! Advances the game clock, samples the climate-driven TOD colour and
//! fog tables on each frame, cross-fades through any in-flight WTHR
//! transition, and writes the result into `SkyParamsRes` /
//! `CloudSimState` / `CellLightingRes`.

use byroredux_core::ecs::components::groundcover::WindField;
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
///  - `sunset_end + 2h` (clamped against `sunset_begin` and against the
///    `keys[0] + 24` wrap point) → `TOD_NIGHT`
///
/// Kept `pub(crate)` so the unit test in this module can pin the
/// formula independently of a full World setup.
pub(crate) fn build_tod_keys(tod_hours: [f32; 4]) -> [(f32, usize); 7] {
    use byroredux_plugin::esm::records::weather::*;
    let [sunrise_begin, sunrise_end, sunset_begin, sunset_end] = tod_hours;
    let afternoon_peak = (sunrise_end + sunset_begin) * 0.5;
    // #2473 (REN-D18-NEW-01) — clamp against the true predecessor key
    // (`afternoon_peak`), not `sunrise_end` (which is two keys back, key
    // 2, not key 3). The old clamp let `afternoon_cool` land BEFORE
    // `afternoon_peak` on any climate with `sunset_begin - sunrise_end <
    // 4h` — solving `afternoon_cool < afternoon_peak` under the old
    // clamp gives exactly that trigger range. `pick_tod_pair`'s `h >=
    // h0 && h < h1` scan can never satisfy a decreasing `(h0, h1)` pair,
    // so the HIGH_NOON→DAY ease-out segment went unreachable and hour
    // `afternoon_peak` snapped straight into the next segment — a
    // single-frame discontinuous jump in every WTHR-driven colour and
    // fog distance, once per in-game day, on any such climate.
    // `sunset_begin - 1e-3` keeps `afternoon_cool` from ever reaching
    // (or passing) key 5 (`sunset_begin`) itself when the two clamp
    // floors above already pin it there.
    let afternoon_cool = (sunset_begin - 2.0)
        .max(afternoon_peak + 0.1)
        .min(sunset_begin - 1e-3);
    let midnight = 1.0f32;
    // #2820 (REN-D18-03) — clamp against the true predecessor key
    // (`sunset_begin`), not an absolute `23.0`. The old absolute clamp
    // fired on every vanilla FNV/FO3 climate (`sunset_end == 22.0` →
    // `22 + 2 = 24` clamped down to `23`, compressing the documented
    // `sunset_end + 2h` ease from 6h to 5h with no cited source for the
    // extra hour of slack) and could go non-monotonic against
    // `sunset_begin` on any climate with `sunset_begin > 23.0` (TNAM
    // bytes 139-144, which pass `climate_tod_hours`'s `1..=144` range
    // check). `24.9` keeps the same margin under the `keys[0] + 24 =
    // 25.0` wrap point the old `23.0` clamp was guarding — any value
    // under 25.0 satisfies that invariant, so this is the least-strict
    // bound that still holds it. Mirrors #2473's predecessor-relative
    // treatment of key 4 (`afternoon_cool`).
    let night = (sunset_end + 2.0).max(sunset_begin + 0.1).min(24.9);
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
/// Derive sun direction + intensity from the climate's `tod_hours`.
///
/// `tod_hours = [sunrise_begin, sunrise_end, sunset_begin, sunset_end]`
/// (same quad `build_tod_keys` consumes). The visible-sun arc spans
/// `[sunrise_begin, sunset_end]` so the directional light stays in
/// lockstep with the sky palette across the entire dawn → day → dusk
/// transition. Outside this window the sun direction is the below-
/// horizon sentinel `[0, -1, 0]` and intensity is 0.
///
/// Intensity envelope:
///   - ramp 0 → 4.0 across `[sunrise_begin, sunrise_end]`
///   - full 4.0 across `[sunrise_end, sunset_begin]`
///   - ramp 4.0 → 0 across `[sunset_begin, sunset_end]`
///   - 0.0 outside `[sunrise_begin, sunset_end]`
///
/// Pre-#1012 the arc was hardcoded `(hour - 6.0) / 12.0 * π` and the
/// intensity window was `[7, 17]`, which produced a ~40 min "below-
/// horizon sun under sunrise-tinted sky" window on FO3 Capital
/// Wasteland (`tod_hours = [5.333, 10.0, 17.0, 22.0]`).
/// South-tilt of the sun arc (engine +Z = Bethesda −Y = south). The arc is
/// otherwise a pure east → zenith → west semicircle; this tilts it slightly
/// south so the sun is not dead-overhead at solar noon (#802 / SUN-N2).
///
/// EXAL Q1 (docs/engine/exal.md §9): there is **no authored latitude / sun-angle
/// field** anywhere in CLMT or WRLD, and the Gamebryo engine lineage has no
/// astronomical model — the sun-path is engine-defined. So this is a deliberate
/// engine constant, not a value read from data. #1019 ("per-worldspace latitude
/// tilt") is therefore "pick a defensible engine value", not "find the field".
pub(crate) const SUN_SOUTH_TILT: f32 = 0.15;

pub(crate) fn compute_sun_arc(hour: f32, tod_hours: [f32; 4]) -> ([f32; 3], f32) {
    let [sunrise_begin, sunrise_end, sunset_begin, sunset_end] = tod_hours;
    let day_span = (sunset_end - sunrise_begin).max(1e-3);

    // Sun direction: semicircular arc east → zenith → west, with a
    // slight south tilt ([`SUN_SOUTH_TILT`]; engine +Z = Bethesda -Y =
    // south) per #802 / SUN-N2.
    let sun_dir = if hour >= sunrise_begin && hour <= sunset_end {
        let solar_hour = (hour - sunrise_begin).clamp(0.0, day_span);
        let angle = solar_hour / day_span * std::f32::consts::PI;
        let x = angle.cos();
        let y = angle.sin();
        let z = SUN_SOUTH_TILT;
        let len = (x * x + y * y + z * z).sqrt();
        [x / len, y / len, z / len]
    } else {
        [0.0, -1.0, 0.0]
    };

    let sun_intensity = if hour >= sunrise_end && hour <= sunset_begin {
        4.0
    } else if hour >= sunrise_begin && hour < sunrise_end {
        let ramp_span = (sunrise_end - sunrise_begin).max(1e-3);
        ((hour - sunrise_begin) / ramp_span * 4.0).clamp(0.0, 4.0)
    } else if hour > sunset_begin && hour <= sunset_end {
        let ramp_span = (sunset_end - sunset_begin).max(1e-3);
        ((sunset_end - hour) / ramp_span * 4.0).clamp(0.0, 4.0)
    } else {
        0.0
    };

    (sun_dir, sun_intensity)
}

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

/// Sunrise/sunset/etc. breakpoints used when no climate record drives
/// `WeatherDataRes::tod_hours`. Matches the pre-#463 hardcoded
/// defaults the synthetic-fallback path in `scene::world_setup` ships.
pub(crate) const DEFAULT_TOD_HOURS: [f32; 4] = [6.0, 10.0, 18.0, 22.0];

/// Write the exterior no-weather fallback into `cell_lit` when no WTHR
/// record is loaded (#1034 / REN-D15-NEW-15). Pre-#1034 the no-WTHR
/// branch in `weather_system` returned without writing the resource, so
/// the prior cell's stale values leaked through — if the prior cell was a
/// dark interior, the exterior rendered pitch-black.
///
/// The values come from the **one** canonical EXAL boundary fallback
/// ([`procedural_fallback_cell_lighting`](crate::env_translate::procedural_fallback_cell_lighting)
/// — the same `FB_*` set `scene::world_setup` installs when no climate
/// record is present), not a private `NEUTRAL_*` set. Pre-#1722 this
/// system carried its own divergent constants (ambient ~2.6× brighter,
/// fog distances 10–15× off), so the same logical state ("exterior, no
/// authored weather") rendered two different looks depending on which
/// producer ran — an EXAL "no render-time fallback" violation (exal.md §3).
///
/// Interior cells are skipped so XCLL/LGTM-authored values survive —
/// mirrors the interior gate in the main update path (#782).
fn apply_neutral_exterior_fallback(cell_lit: &mut CellLightingRes) {
    if cell_lit.is_interior {
        return;
    }
    let (sun_dir, _intensity) = compute_sun_arc(6.0, DEFAULT_TOD_HOURS);
    *cell_lit = crate::env_translate::procedural_fallback_cell_lighting(sun_dir);
}

/// Per-unit `wind_speed` (u8 0..=255) contribution to the cloud-layer 0
/// scroll rate (UV/sec). Calibrated so `wind_speed = 32` (typical
/// vanilla mid-range across FNV/FO3/Oblivion/Skyrim WTHR DATA bytes
/// — fixtures in `crates/plugin/src/esm/records/weather.rs::tests`
/// ship 16/25/30/50, mean ≈ 30) reproduces the pre-#1033 `0.018`
/// baseline that `cloud_scroll_rate_from_wind(32)` returns.
///
/// `wind_speed = 0` (calm WTHR) zeroes the rate so clouds halt;
/// `wind_speed = 255` (storm) reaches ≈0.143 UV/sec — visibly
/// streaking clouds matching the perceptual range of Bethesda's
/// storm-weather content. Replace with a bench-captured calibration
/// when one becomes available.
const WIND_TO_SCROLL_RATE: f32 = 0.018 / 32.0;

/// Pure helper for the cloud-scroll-rate derivation so the unit test
/// can pin the calm-vs-storm contract without a live `World`. See the
/// `WIND_TO_SCROLL_RATE` doc for the calibration rationale.
#[inline]
pub(crate) fn cloud_scroll_rate_from_wind(wind_speed: u8) -> f32 {
    wind_speed as f32 * WIND_TO_SCROLL_RATE
}

/// Seven blended WTHR sky fields, in order:
/// zenith, horizon, lower, sun_col, ambient, sunlight, fog_col.
type WthrColors = (
    [f32; 3],
    [f32; 3],
    [f32; 3],
    [f32; 3],
    [f32; 3],
    [f32; 3],
    [f32; 3],
);

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
) -> WthrColors {
    use byroredux_plugin::esm::records::weather::*;
    (
        lerp3(
            sky_colors[SKY_UPPER][slot_a],
            sky_colors[SKY_UPPER][slot_b],
            t,
        ),
        lerp3(
            sky_colors[SKY_HORIZON][slot_a],
            sky_colors[SKY_HORIZON][slot_b],
            t,
        ),
        lerp3(
            sky_colors[SKY_LOWER][slot_a],
            sky_colors[SKY_LOWER][slot_b],
            t,
        ),
        lerp3(sky_colors[SKY_SUN][slot_a], sky_colors[SKY_SUN][slot_b], t),
        lerp3(
            sky_colors[SKY_AMBIENT][slot_a],
            sky_colors[SKY_AMBIENT][slot_b],
            t,
        ),
        lerp3(
            sky_colors[SKY_SUNLIGHT][slot_a],
            sky_colors[SKY_SUNLIGHT][slot_b],
            t,
        ),
        lerp3(sky_colors[SKY_FOG][slot_a], sky_colors[SKY_FOG][slot_b], t),
    )
}

/// #993 — Skyrim DALC ambient cube TOD interpolation. The DALC array has
/// 4 TOD slots (sunrise / day / sunset / night) while `sky_colors` has 6
/// (4 + high_noon + midnight); fold high_noon→day and midnight→night per
/// the WTHR parser's on-disk padding rule
/// (`crates/plugin/src/esm/records/weather.rs:312-314`) so the same
/// `(slot_a, slot_b, t)` the colour interpolator picked applies cleanly.
/// `None` when the snapshot carries no DALC bytes (FNV / FO3 / Oblivion,
/// always).
///
/// Pulled out so #2816's cross-fade fix can sample the source and target
/// snapshots identically — mirrors why [`sample_wthr_colors`] exists.
fn sample_dalc_cube(
    cubes: Option<&[crate::components::DalcCubeYup; 4]>,
    slot_a: usize,
    slot_b: usize,
    t: f32,
) -> Option<crate::components::DalcCubeYup> {
    use byroredux_plugin::esm::records::weather::*;
    let cubes = cubes?;
    let fold = |slot: usize| match slot {
        TOD_HIGH_NOON => TOD_DAY,
        TOD_MIDNIGHT => TOD_NIGHT,
        s => s,
    };
    Some(crate::components::DalcCubeYup::lerp(
        &cubes[fold(slot_a)],
        &cubes[fold(slot_b)],
        t,
    ))
}

/// #2816 — a flat, isotropic stand-in cube for the side of a WTHR
/// cross-fade that authored no DALC bytes at all. Every face equals the
/// same TOD-sampled `ambient` scalar that side's own (non-DALC) sky
/// palette already contributes — the six-axis directional variation the
/// other side's real cube carries is exactly what's absent, not an
/// invented color — `specular` off and `fresnel_power` at the documented
/// vanilla-Skyrim default (see [`DalcCubeYup::fresnel_power`]). Lets a
/// source-with-DALC → target-without-DALC (or the reverse) transition
/// ease the directional cube out toward plain isotropic ambient instead
/// of holding it at full weight until it snaps to `None` on completion.
fn flat_dalc_cube(ambient: [f32; 3]) -> crate::components::DalcCubeYup {
    crate::components::DalcCubeYup {
        pos_x: ambient,
        neg_x: ambient,
        pos_y: ambient,
        neg_y: ambient,
        pos_z: ambient,
        neg_z: ambient,
        specular: [0.0; 3],
        fresnel_power: 1.0,
    }
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
        game_time.tick(dt);
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
        // #1034 / REN-D15-NEW-15 — no WTHR record loaded for this
        // exterior cell. Without this branch, `CellLightingRes`
        // keeps the stale interior values from the prior cell;
        // if those were neutral / dark, the exterior renders
        // pitch-black. Write a documented neutral default so the
        // exterior is at least lit (audit-checklist item 10).
        // Interior cells are skipped by `apply_neutral_exterior_fallback`'s
        // internal gate (sibling of the main path's `!is_interior`
        // check at line ~525).
        if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
            apply_neutral_exterior_fallback(&mut cell_lit);
        }
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
    // #2816 — sampled here, while `wd` is still borrowed, so the
    // cross-fade block below can blend it against the target's own
    // sample instead of re-reading the (by-then-replaced) live
    // `WeatherDataRes` after the fact.
    let dalc_source = sample_dalc_cube(wd.skyrim_dalc_per_tod.as_ref(), slot_a, slot_b, t);

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
    let fog_medium = wd.fog_media[0].lerp(wd.fog_media[1], night_factor);

    // M33.1 — if a WTHR cross-fade is in flight, run the same TOD-slot
    // pick + per-group sampling on the target snapshot and blend each
    // colour channel by `transition_t`. The TOD slots are independent
    // per-side (target may use the same `keys` table since `tod_hours`
    // is on WeatherDataRes; we re-derive it from the target's own
    // breakpoints to stay correct if the target ships a different CLMT).
    let (
        zenith,
        horizon,
        lower,
        sun_col,
        ambient,
        sunlight,
        fog_col,
        fog_near,
        fog_far,
        fog_medium,
        dalc_cube,
    ) = if transition_t > 0.0 {
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
        // #1018 / REN-D15-NEW-09 — `night_factor` above was derived
        // from the SOURCE weather's `(slot_a, slot_b, t)`. The
        // target's fog distance must use the target's own TOD
        // slot pair (already computed for `sample_wthr_colors`)
        // so colour and distance stay in lockstep when the source
        // and target ship different CLMTs (rare today: typically
        // both weathers share a CLMT, which makes the two
        // night_factors equal; matters as soon as a WTHR cross-
        // fade spans worldspace boundaries or mod content).
        let target_night_a = tod_slot_night_factor(b_a);
        let target_night_b = tod_slot_night_factor(b_b);
        let target_night_factor = target_night_a + (target_night_b - target_night_a) * b_t;
        let target_fog_near = target.fog[0] + (target.fog[2] - target.fog[0]) * target_night_factor;
        let target_fog_far = target.fog[1] + (target.fog[3] - target.fog[1]) * target_night_factor;
        let target_fog_medium = target.fog_media[0].lerp(target.fog_media[1], target_night_factor);
        // #2816 / REN-D18-02 — the DALC cube is the eleventh per-weather
        // quantity this block blends; it used to be computed entirely
        // outside this branch from the source only, so the target's cube
        // arrived as a single-frame snap on completion instead of
        // easing in. Sampled at the target's own TOD pair, same as
        // every other target_* quantity above. When one side authored
        // no DALC bytes at all (FNV/FO3/Oblivion, or a Skyrim WTHR that
        // simply has none), `flat_dalc_cube` stands in with that side's
        // own isotropic ambient instead of holding the real cube at full
        // weight until it snaps to/from `None`.
        let target_dalc = sample_dalc_cube(target.skyrim_dalc_per_tod.as_ref(), b_a, b_b, b_t);
        let dalc_cube = match (dalc_source, target_dalc) {
            (Some(a), Some(b)) => Some(crate::components::DalcCubeYup::lerp(&a, &b, transition_t)),
            (Some(a), None) => Some(crate::components::DalcCubeYup::lerp(
                &a,
                &flat_dalc_cube(target_ambient),
                transition_t,
            )),
            (None, Some(b)) => Some(crate::components::DalcCubeYup::lerp(
                &flat_dalc_cube(ambient),
                &b,
                transition_t,
            )),
            (None, None) => None,
        };

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
            fog_medium.lerp(target_fog_medium, transition_t),
            dalc_cube,
        )
    } else {
        (
            zenith,
            horizon,
            lower,
            sun_col,
            ambient,
            sunlight,
            fog_col,
            fog_near,
            fog_far,
            fog_medium,
            dalc_source,
        )
    };

    // Sun direction + intensity — derived from this WTHR's
    // `tod_hours` via `compute_sun_arc`, so the sun stays in lockstep
    // with the climate-driven palette. Pre-#1012 these were hardcoded
    // to a 6h/18h arc + 7h/17h intensity window that disagreed with
    // non-default CLMTs — FO3 Capital Wasteland (sunrise 5.333 h) had
    // ~40 min where the palette was sunrise-tinted but the sun
    // direction was the below-horizon sentinel `[0, -1, 0]` (sky
    // painted dawn while N·L = 0).
    let (sun_dir, sun_intensity) = compute_sun_arc(hour, wd.tod_hours);

    // Cloud layer 0 base scroll rate, driven by the WTHR DATA
    // `wind_speed` byte (#1033 / REN-D15-NEW-12).
    //
    // Pre-#1033 a hardcoded `0.018 UV/sec` literal sat here. The
    // record's `wind_speed` byte was parsed (`weather.rs::WeatherRecord`)
    // but never reached the cloud-scroll path, so calm-WTHR vs
    // storm-WTHR animated identically. Earlier-fix attempt at #535
    // sourced from `cloud_speeds[0]`, which was actually the first
    // byte of a DNAM cloud-path zstring (`'s'` = 0x73 = 115 →
    // factor 0.898 → ≈0.018 UV/sec) — explains why the previous
    // visual matched the hardcoded constant.
    //
    // The scale is calibrated to reproduce the existing `0.018`
    // baseline at a typical mid-range vanilla `wind_speed` of 32
    // (seen across the FNV/FO3/Oblivion/Skyrim WTHR fixtures in
    // `weather.rs` tests: 16, 25, 30, 50 → mean ≈ 30, median 27.5).
    // `wind_speed = 0` (calm WTHR) → static clouds; `wind_speed =
    // 255` (storm) → ≈0.143 UV/sec — ~8× the mid-range, which
    // matches the Bethesda-content perceptual range of "completely
    // still" to "visibly streaking storm clouds." Replace with a
    // bench-captured calibration when one becomes available.
    let cloud_scroll_rate = cloud_scroll_rate_from_wind(wd.wind_speed);
    let weather_wind_speed = if transition_t > 0.0 {
        world
            .try_resource::<WeatherTransitionRes>()
            .map(|tr| {
                wd.wind_speed as f32
                    + (tr.target.wind_speed as f32 - wd.wind_speed as f32) * transition_t
            })
            .unwrap_or(wd.wind_speed as f32) as u8
    } else {
        wd.wind_speed
    };
    let weather_precipitation = if transition_t > 0.0 {
        world
            .try_resource::<WeatherTransitionRes>()
            .map(|tr| {
                wd.precipitation
                    + (tr.target.precipitation - wd.precipitation) * transition_t
            })
            .unwrap_or(wd.precipitation)
    } else {
        wd.precipitation
    };

    drop(wd);

    // Keep rainfall/snowfall disturbance continuous during a WTHR cross-fade;
    // the completion path below still promotes the exact target snapshot.
    if !transition_done {
        if let Some(mut wd) = world.try_resource_mut::<WeatherDataRes>() {
            wd.precipitation = weather_precipitation.clamp(0.0, 1.0);
        }
    }

    // Keep the shared atmospheric wind live during weather transitions. The
    // ground-cover install path seeds this resource when entering a
    // worldspace, but WTHR changes can occur without a worldspace reload;
    // SpeedTree sway and water-normal motion must follow those changes too.
    if let Some(mut wind) = world.try_resource_mut::<WindField>() {
        *wind = WindField::from_weather_byte(weather_wind_speed, wind.direction);
    }

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
        // #993 — DALC cube write-through. `None` on every non-Skyrim
        // cell, so the renderer's future consumer can branch on
        // `current_dalc_cube.is_some()` to gate the 6-axis sample.
        sky.current_dalc_cube = dalc_cube;
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
            cell_lit.fog_medium = fog_medium;
        }
    }

    // M33.1 — promote the in-flight transition target into the live
    // WeatherDataRes once the cross-fade completes. Uses in-place
    // mutation via try_resource_mut (interior mutability, &World safe).
    // elapsed_secs is saturated at duration_secs so subsequent frames
    // skip the blend path without removing the resource (remove_resource
    // needs &mut World which systems do not have).
    if transition_done {
        if world.try_resource::<WeatherTransitionRes>().is_some() {
            // Ordering invariant (#1103 / REN-D15-003): the promotion
            // below reads `tr.target.*` and writes into `wd.*` on the
            // *same* weather_system invocation that computed the blend
            // ratio (in the early part of this function). If
            // weather_system is ever split into a timer-advance pass +
            // a blend-apply pass, the promotion must move to the
            // blend-apply pass to preserve lerp(src,tgt,1.0)=tgt
            // semantics on the completion frame.
            promote_weather_transition_target(world);
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

/// Copy the in-flight transition's `target` snapshot into the live
/// `WeatherDataRes`, unconditionally. Shared by `weather_system`'s
/// natural-completion path (above, once `t >= 1.0`) and
/// `scene::world_setup::collapse_weather_transition`'s early-collapse
/// path (a second worldspace change landing mid-fade), so both apply the
/// exact same field copy. No-op if either resource is absent.
pub(crate) fn promote_weather_transition_target(world: &World) {
    let Some(tr) = world.try_resource::<WeatherTransitionRes>() else {
        return;
    };
    let new_sky = tr.target.sky_colors;
    let new_fog = tr.target.fog;
    let new_fog_media = tr.target.fog_media;
    let new_tod = tr.target.tod_hours;
    let tr_target_wind = tr.target.wind_speed;
    let tr_target_precipitation = tr.target.precipitation;
    let tr_target_dalc = tr.target.skyrim_dalc_per_tod;
    drop(tr);
    if let Some(mut wd) = world.try_resource_mut::<WeatherDataRes>() {
        wd.sky_colors = new_sky;
        wd.fog = new_fog;
        wd.fog_media = new_fog_media;
        wd.tod_hours = new_tod;
        // #1101 / REN-D15-001 — promote wind_speed so cloud scroll uses
        // the target weather after cross-fade completion. Without this
        // the source weather's wind speed persists.
        wd.wind_speed = tr_target_wind;
        wd.precipitation = tr_target_precipitation;
        // #1102 / REN-D15-002 — promote DALC ambient cube so the Skyrim
        // 6-axis directional ambient uses the target weather.
        wd.skyrim_dalc_per_tod = tr_target_dalc;
    }
}

#[cfg(test)]
mod cloud_scroll_rate_tests {
    //! Regression tests for #1033 / REN-D15-NEW-12. Pre-fix the
    //! cloud-scroll rate was a hardcoded `0.018` literal; the parsed
    //! WTHR `wind_speed` byte never reached the cloud animation, so
    //! calm vs storm WTHR records produced identical visual cloud
    //! motion. These tests pin the new wind-driven derivation against
    //! its calibration contract.
    use super::*;

    /// Calibration pin: `wind_speed = 32` reproduces the pre-fix
    /// hardcoded baseline `0.018 UV/sec`. Any future re-calibration
    /// of `WIND_TO_SCROLL_RATE` that moves this anchor without an
    /// intentional decision should trip this assertion.
    #[test]
    fn baseline_wind_speed_reproduces_pre_fix_018() {
        let rate = cloud_scroll_rate_from_wind(32);
        assert!(
            (rate - 0.018).abs() < 1e-6,
            "wind_speed=32 must reproduce the pre-#1033 baseline 0.018 UV/sec; got {rate}"
        );
    }

    /// Calm WTHR (`wind_speed = 0`) halts the cloud scroll — visible
    /// "still air" lookalike. Pre-fix this case still emitted the
    /// hardcoded 0.018 rate, so static-sky weather still drifted.
    #[test]
    fn calm_weather_halts_scroll() {
        assert_eq!(
            cloud_scroll_rate_from_wind(0),
            0.0,
            "wind_speed=0 (calm WTHR) must produce zero scroll rate"
        );
    }

    /// Storm WTHR (`wind_speed = 255`) scrolls visibly faster than
    /// the mid-range baseline — the core regression the audit caught.
    /// 255 × (0.018/32) ≈ 0.143 UV/sec, ~8× the baseline.
    #[test]
    fn storm_weather_scrolls_faster_than_baseline() {
        let baseline = cloud_scroll_rate_from_wind(32);
        let storm = cloud_scroll_rate_from_wind(255);
        assert!(
            storm > baseline * 4.0,
            "storm (wind_speed=255) must scroll noticeably faster than baseline ({baseline} → {storm})"
        );
        let expected = 255.0 * WIND_TO_SCROLL_RATE;
        assert!((storm - expected).abs() < 1e-5);
    }

    /// Monotonic across the whole byte range — clouds must scroll
    /// faster as wind_speed increases, full stop. Pins the linearity
    /// of the derivation against future re-calibrations that might
    /// introduce a non-monotonic curve (e.g. logarithmic or
    /// piecewise) without a behavioural test catching the
    /// regression.
    #[test]
    fn rate_is_monotonic_in_wind_speed() {
        let mut prev = cloud_scroll_rate_from_wind(0);
        for speed in 1u16..=255 {
            let current = cloud_scroll_rate_from_wind(speed as u8);
            assert!(
                current >= prev,
                "scroll rate must be non-decreasing in wind_speed; speed={speed} broke monotonicity ({prev} → {current})"
            );
            prev = current;
        }
    }
}

#[cfg(test)]
mod tod_keys_tests {
    //! Regression tests for #463 — climate-driven TOD breakpoints on
    //! `WeatherDataRes.tod_hours` flow through `build_tod_keys` so the
    //! time-of-day interpolator runs on the right schedule per worldspace.

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
            (24.0, TOD_NIGHT), // (22+2).max(18+0.1).min(24.9) = 24 — #2820
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
            // #2820 — late-sunset climate (TNAM bytes 141/144 → hours
            // 23.5/24.0). Pre-fix, `night = min(sunset_end + 2, 23.0)`
            // clamped to an absolute `23.0` regardless of `sunset_begin`,
            // producing `keys[5] = 23.5 > keys[6] = 23.0` — non-monotonic.
            [6.0, 10.0, 23.5, 24.0],
            // #2473 — short clear-day climate (`sunset_begin -
            // sunrise_end = 1h < 4h`). Pre-fix, `afternoon_cool` clamped
            // against `sunrise_end` instead of `afternoon_peak`, landing
            // BEFORE it (10.1 < afternoon_peak=10.5) — this was the
            // exact input the (then too-weak) dedicated regression test
            // used without catching it.
            [5.0, 10.0, 11.0, 20.0],
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

    /// #2820 (REN-D18-03) — the `night` anchor must stay strictly after
    /// its true predecessor key (`sunset_begin`), not merely under an
    /// absolute `23.0`. A late-sunset climate whose `sunset_begin`
    /// exceeds the old clamp reproduces the non-monotonic table the
    /// absolute clamp allowed.
    #[test]
    fn tod_keys_clamp_night_relative_to_sunset_begin_on_late_sunset_climates() {
        let keys = build_tod_keys([6.0, 10.0, 23.5, 24.0]);
        let sunset_begin = keys[5].0;
        let night = keys[6].0;
        assert!(
            night > sunset_begin,
            "night ({night:.2}) must be strictly after sunset_begin \
             ({sunset_begin:.2}) to keep keys monotonic"
        );
    }

    /// #2820 (REN-D18-03) — on vanilla FNV/FO3 climates (`sunset_end ==
    /// 22.0`) the night anchor must get the documented full `+2h` ease,
    /// not be compressed to `23.0` by an unsourced absolute clamp.
    #[test]
    fn tod_keys_night_anchor_gets_full_two_hour_ease_on_vanilla_climates() {
        for tod_hours in [
            [6.0, 10.0, 18.0, 22.0],   // FNV
            [5.333, 10.0, 17.0, 22.0], // FO3 Capital Wasteland
        ] {
            let keys = build_tod_keys(tod_hours);
            let sunset_end = tod_hours[3];
            let night = keys[6].0;
            assert!(
                (night - (sunset_end + 2.0)).abs() < 1e-5,
                "night ({night:.2}) must equal sunset_end + 2h ({:.2}) on vanilla \
                 content, not be compressed by the clamp",
                sunset_end + 2.0
            );
        }
    }

    /// #2473 (REN-D18-NEW-01) — afternoon_cool clamp, re-tightened
    /// against its TRUE predecessor. Pre-fix the clamp anchored
    /// `afternoon_cool` (key 4) against `sunrise_end` (key 2) instead of
    /// `afternoon_peak` (key 3), so this exact test's own input —
    /// `sunrise_end=10, sunset_begin=11` — produced
    /// `afternoon_peak=10.5, afternoon_cool=max(9.0,10.1)=10.1`: strictly
    /// after key 2 (10 > passes the old, too-weak assertion) but
    /// strictly BEFORE key 3 (10.1 < 10.5), non-monotonic. The old
    /// assertion (`afternoon_cool > keys[2].0`) passed on that broken
    /// table — this is the exact false-assurance gap the issue reports.
    /// Re-tightened to the real invariant: every key must be
    /// non-decreasing relative to its immediate predecessor, walked via
    /// `windows(2)` like `tod_keys_are_monotonic_on_realistic_climates`.
    #[test]
    fn tod_keys_clamp_afternoon_cool_on_compressed_days() {
        // sunrise_end=10, sunset_begin=11 — only 1h of clear "day".
        let keys = build_tod_keys([5.0, 10.0, 11.0, 20.0]);
        for w in keys.windows(2) {
            assert!(
                w[0].0 <= w[1].0 + 1e-5,
                "TOD keys must be monotonic on a compressed-day climate: \
                 {:?} → {:?} (full table {:?})",
                w[0],
                w[1],
                keys,
            );
        }
        // The specific relation the pre-fix clamp got wrong: key 4 must
        // stay at/after key 3 (afternoon_peak), not merely after key 2.
        let afternoon_peak = keys[3].0;
        let afternoon_cool = keys[4].0;
        assert!(
            afternoon_cool >= afternoon_peak,
            "afternoon_cool ({afternoon_cool:.2}) must be at/after \
             afternoon_peak ({afternoon_peak:.2}), its true predecessor \
             key — not merely after sunrise_end"
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
    /// SUNRISE (0.5) toward DAY (0.0) at
    /// `t = (5.7 - 5.333) / (10.0 - 5.333) ≈ 0.0786` produces
    /// `night_factor ≈ 0.461` —
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
        // Hour 0.5 wraps to 24.5; falls inside NIGHT (24, per #2820's
        // predecessor-relative clamp — was 23 pre-fix) → MIDNIGHT (25).
        let (a, b, t) = pick_tod_pair(&keys, 0.5);
        assert_eq!(a, TOD_NIGHT, "pre-midnight hour 0.5 wraps into NIGHT");
        assert_eq!(b, TOD_MIDNIGHT);
        // t = (24.5 - 24) / (25 - 24) = 0.5.
        assert!((t - 0.5).abs() < 1e-5, "expected t≈0.5, got {t}");
    }

    /// `pick_tod_pair` post-last-key branch — hour after the last
    /// authored key (NIGHT, now `24.0` on this climate per #2820)
    /// interpolates NIGHT → MIDNIGHT through the same wrap segment as
    /// the pre-midnight case.
    #[test]
    fn pick_tod_pair_post_night_anchor_returns_night_to_midnight() {
        let keys = build_tod_keys([6.0, 10.0, 18.0, 22.0]);
        // Hour 24.5 — after NIGHT (24.0) and before the MIDNIGHT wrap
        // key (25.0); hits the >= keys[last] branch directly.
        let (a, b, t) = pick_tod_pair(&keys, 24.5);
        assert_eq!(a, TOD_NIGHT);
        assert_eq!(b, TOD_MIDNIGHT);
        assert!(t > 0.0 && t <= 1.0);
    }

    /// Regression for #1012 / REN-D15-NEW-08.
    ///
    /// Pre-fix: sun direction used a hardcoded `[6h, 18h]` gate. On
    /// FO3 Capital Wasteland (`tod_hours = [5.333, 10.0, 17.0, 22.0]`)
    /// the palette interpolator entered the SUNRISE band at hour 5.333
    /// while the sun direction stayed at the below-horizon sentinel
    /// `[0, -1, 0]` until hour 6.0 — a ~40 min window where the sky
    /// painted dawn but `sun_dir.y < 0` killed N·L on every surface.
    /// Symmetric ~1h dead window at sunset between 17h and 18h.
    ///
    /// Post-fix: `compute_sun_arc` derives the visible-sun window from
    /// `[sunrise_begin, sunset_end]`. At hour 5.5 on FO3 the sun is
    /// just above horizon (positive y) with low elevation. At hour
    /// 17.5 (within FO3's sunset band) the sun is still above horizon.
    #[test]
    fn fo3_wasteland_sun_above_horizon_during_sunrise_palette_band() {
        let fo3_tod = [5.333, 10.0, 17.0, 22.0];

        // Hour 5.5: 0.167 h past sunrise_begin, sky is sunrise-tinted.
        // Pre-fix the sun was at [0, -1, 0] (below horizon). Post-fix
        // the sun is just above horizon with low positive elevation.
        let (dir, _) = compute_sun_arc(5.5, fo3_tod);
        assert!(
            dir[1] > 0.0,
            "sun must be above horizon at hour 5.5 on FO3 (sunrise_begin=5.333). \
             Pre-#1012: dir=[0,-1,0] sentinel; got dir=[{:.3},{:.3},{:.3}]",
            dir[0],
            dir[1],
            dir[2],
        );
        assert!(
            dir[0] > 0.5,
            "sun should still be in the eastern half (cos(angle) > 0.5) at hour 5.5; \
             got dir.x={:.3}",
            dir[0],
        );

        // Hour 17.5: in FO3's sunset band [17, 22]. Pre-fix the sun
        // was at [0,-1,0] because hour > 18.0 hardcoded gate. Post-fix
        // the sun is still above horizon, ramping toward west.
        let (dir, intensity) = compute_sun_arc(17.5, fo3_tod);
        assert!(
            dir[1] > 0.0,
            "sun must still be above horizon at hour 17.5 on FO3 (sunset_end=22). \
             Got dir=[{:.3},{:.3},{:.3}]",
            dir[0],
            dir[1],
            dir[2],
        );
        assert!(
            dir[0] < 0.0,
            "sun should be in the western half at hour 17.5; got dir.x={:.3}",
            dir[0],
        );
        // Hour 17.5 is 0.5h past sunset_begin (17.0) of a 5h sunset
        // band → intensity ≈ 4.0 * (22.0 - 17.5)/5.0 = 3.6.
        assert!(
            (intensity - 3.6).abs() < 0.05,
            "FO3 sunset_begin=17, sunset_end=22 → intensity at 17.5h ≈ 3.6; got {intensity:.3}",
        );
    }

    /// Default FNV-style climate retains a sane sun arc + intensity
    /// envelope post-#1012. The arc-span widens from the pre-fix
    /// 12 h hardcoded window to 16 h (`sunset_end - sunrise_begin`),
    /// which matches the authored TOD bands.
    #[test]
    fn fnv_default_sun_arc_matches_tod_bands() {
        let fnv_tod = [6.0, 10.0, 18.0, 22.0];

        // Pre-sunrise: sentinel below horizon.
        let (dir, intensity) = compute_sun_arc(5.5, fnv_tod);
        assert_eq!(dir, [0.0, -1.0, 0.0]);
        assert_eq!(intensity, 0.0);

        // Sunrise band [6, 10]: ramping intensity. At hour 8 the
        // ramp is half-way → intensity = 2.0.
        let (dir, intensity) = compute_sun_arc(8.0, fnv_tod);
        assert!(
            dir[1] > 0.0,
            "sun should be above horizon at hour 8 on FNV; got y={:.3}",
            dir[1],
        );
        assert!(
            (intensity - 2.0).abs() < 0.05,
            "FNV sunrise band hour 8 → intensity ≈ 2.0; got {intensity:.3}",
        );

        // Day band [10, 18]: full intensity.
        let (_, intensity) = compute_sun_arc(14.0, fnv_tod);
        assert!(
            (intensity - 4.0).abs() < 1e-5,
            "FNV day band → intensity 4.0; got {intensity:.3}",
        );

        // Post-sunset: sentinel.
        let (dir, intensity) = compute_sun_arc(22.5, fnv_tod);
        assert_eq!(dir, [0.0, -1.0, 0.0]);
        assert_eq!(intensity, 0.0);
    }

    /// Regression for #1018 / REN-D15-NEW-09.
    ///
    /// Pre-fix: during a WTHR cross-fade, `target_fog_near/far` were
    /// computed using the *source* weather's `night_factor` (derived
    /// from source's `(slot_a, slot_b, t)`). When source and target
    /// shipped DIFFERENT CLMTs (rare today, but possible across
    /// worldspace boundaries / mod content), the target's fog
    /// distance disagreed with its own colour table — visible as
    /// fog colour shifting at a different rate than fog distance
    /// during the 8s cross-fade.
    ///
    /// Post-fix: the cross-fade path derives `target_night_factor`
    /// from the target's own `(b_a, b_b, b_t)` so colour and
    /// distance share the same TOD source.
    ///
    /// Two assertions pin the bug:
    ///
    /// 1. Even between ship-realistic CLMTs (FNV vs FO3, both shipping
    ///    sunrise/sunset within ~0.7 hours of each other), the
    ///    night_factors diverge in some hour windows — proving the
    ///    target's lookup must be independent.
    /// 2. With a more compressed-daylight target CLMT (worldspace mod
    ///    content), the divergence at sunrise hours is dramatic —
    ///    proving the bug magnitude scales with CLMT difference.
    #[test]
    fn cross_fade_uses_per_weather_night_factor() {
        // Helper: replicate the pure-function path that
        // `weather_system` walks for both source and target.
        fn nf(tod_hours: [f32; 4], hour: f32) -> f32 {
            let keys = build_tod_keys(tod_hours);
            let (a, b, t) = pick_tod_pair(&keys, hour);
            let na = tod_slot_night_factor(a);
            let nb = tod_slot_night_factor(b);
            na + (nb - na) * t
        }

        // ── Assertion 1: ship-realistic divergence (FNV vs FO3) ──
        //
        // At hour 17.5, FNV (`sunset_begin = 18`) is still in the
        // (DAY-reanchor → SUNSET) bracket while FO3 (`sunset_begin =
        // 17`) has already crossed into (SUNSET → NIGHT). The two
        // night_factors differ by enough that pulling the source's
        // value into the target's fog-distance lookup would shift
        // the target's fog distance by ~17% of the day↔night
        // amplitude — visible during the 8 s cross-fade.
        let fnv_nf = nf([6.0, 10.0, 18.0, 22.0], 17.5);
        let fo3_nf = nf([5.333, 10.0, 17.0, 22.0], 17.5);
        assert!(
            (fnv_nf - fo3_nf).abs() > 0.1,
            "Realistic FNV/FO3 cross-fade at hour 17.5 must show some night_factor \
             divergence to justify per-weather lookup; got FNV={fnv_nf:.3}, \
             FO3={fo3_nf:.3}, diff={:.3}",
            (fnv_nf - fo3_nf).abs(),
        );

        // ── Assertion 2: dramatic divergence (mod / compressed CLMT) ──
        //
        // A compressed-daylight CLMT (sunrise 4h, sunset 20h vs
        // FNV's 6h/22h) crossed at hour 5.5 lands in a different
        // TOD bracket than FNV. Pre-#1018, the target's fog distance
        // would be lerp'd at the source's night_factor — a 0.2+ error.
        let compressed_nf = nf([4.0, 8.0, 16.0, 20.0], 5.5);
        let fnv_at_55 = nf([6.0, 10.0, 18.0, 22.0], 5.5);
        let dramatic_div = (compressed_nf - fnv_at_55).abs();
        assert!(
            dramatic_div > 0.2,
            "FNV vs compressed-CLMT cross-fade at hour 5.5 must show >0.2 \
             night_factor divergence (mod-content scenario); got FNV={fnv_at_55:.3}, \
             compressed={compressed_nf:.3}, diff={dramatic_div:.3}",
        );
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
            fog_medium: crate::fog::FogMedium::from_legacy_ramp(
                INTERIOR_FOG_NEAR,
                INTERIOR_FOG_FAR,
                None,
            ),
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
            inheritance_flags: None,
        });

        // Mid-day so the TOD slot is unambiguous; freeze the clock so dt
        // advances are no-ops.
        world.insert_resource(GameTimeRes::frozen_at(12.0));

        // Build a WTHR snapshot with sky-blue fog at every TOD slot so
        // any unconditional write would clobber the interior fog with
        // (0.65, 0.7, 0.8) — the symptom from #782.
        let bright_sky_blue = [0.65_f32, 0.7, 0.8];
        let mut sky_colors = [[[0.0_f32; 3]; 6]; 10];
        sky_colors[byroredux_plugin::esm::records::weather::SKY_FOG].fill(bright_sky_blue);
        sky_colors[byroredux_plugin::esm::records::weather::SKY_AMBIENT].fill([0.5, 0.5, 0.5]);
        sky_colors[byroredux_plugin::esm::records::weather::SKY_SUNLIGHT].fill([1.0, 1.0, 1.0]);
        world.insert_resource(WeatherDataRes {
            sky_colors,
            fog: [100.0, 60000.0, 200.0, 30000.0],
            fog_media: [
                crate::fog::FogMedium::from_legacy_ramp(100.0, 60000.0, None),
                crate::fog::FogMedium::from_legacy_ramp(200.0, 30000.0, None),
            ],
            tod_hours: [6.0, 10.0, 18.0, 22.0],
            skyrim_dalc_per_tod: None,
            wind_speed: 0,
            precipitation: 0.0,
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
        let authored_medium = world.try_resource::<CellLightingRes>().unwrap().fog_medium;
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
        assert_eq!(
            cell_lit.fog_medium, authored_medium,
            "interior canonical fog medium was overwritten by weather_system"
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
        assert_eq!(
            cell_lit.fog_medium,
            crate::fog::FogMedium::from_legacy_ramp(100.0, 60000.0, None),
            "midday exterior must consume the canonical daytime medium"
        );
    }
}

/// Regression tests for #1034 / REN-D15-NEW-15 — the no-WTHR exterior
/// fallback. Without this branch, an exterior cell load that lands
/// before `WeatherDataRes` resolves leaks the prior cell's stale
/// (often dark interior) lighting values into `CellLightingRes`,
/// producing pitch-black exteriors. The fix writes documented
/// neutral defaults at the early-return site; these tests pin the
/// happy path (ambient non-zero, sun lit) AND the interior gate
/// (interior values preserved even when the system early-returns).
#[cfg(test)]
mod no_wthr_fallback_tests {
    use super::*;
    use byroredux_core::ecs::World;

    /// Same fixture shape as `interior_gate_tests::build_world` but
    /// deliberately omits `WeatherDataRes` so `weather_system` takes
    /// the no-WTHR branch.
    fn build_world_no_wthr(is_interior: bool) -> World {
        let mut world = World::new();
        // Stale interior values — pre-#1034 these would leak unchanged
        // into the exterior render. Pure black so an assertion on
        // "ambient != black" reliably catches the bug.
        world.insert_resource(CellLightingRes {
            ambient: [0.0, 0.0, 0.0],
            directional_color: [0.0, 0.0, 0.0],
            directional_dir: [0.0, 1.0, 0.0],
            is_interior,
            fog_color: [0.0, 0.0, 0.0],
            fog_near: 0.0,
            fog_far: 0.0,
            fog_medium: crate::fog::FogMedium::DISABLED,
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
            inheritance_flags: None,
        });
        world.insert_resource(GameTimeRes::frozen_at(12.0));
        // NB: no WeatherDataRes — that's the case under test.
        world
    }

    /// Core regression: exterior cell + no WTHR must produce non-zero
    /// ambient + non-zero directional light + non-zero fog_far. Pre-fix
    /// every field stayed at zero (pitch-black exterior).
    #[test]
    fn no_wthr_exterior_writes_neutral_defaults() {
        let world = build_world_no_wthr(false);
        weather_system(&world, 0.016);

        let cell_lit = world.try_resource::<CellLightingRes>().unwrap();
        assert!(
            cell_lit.ambient.iter().any(|c| *c > 0.0),
            "no-WTHR exterior must produce non-zero ambient (#1034); got {:?}",
            cell_lit.ambient
        );
        assert!(
            cell_lit.directional_color.iter().any(|c| *c > 0.0),
            "no-WTHR exterior must produce non-zero directional color (#1034); got {:?}",
            cell_lit.directional_color
        );
        assert!(
            cell_lit.fog_far > 0.0,
            "no-WTHR exterior must produce non-zero fog_far (#1034); got {}",
            cell_lit.fog_far
        );
        // #1722 — the no-WTHR fallback must match the single canonical
        // EXAL boundary default, not a private NEUTRAL_* set. Pin against
        // `procedural_fallback_cell_lighting` so any future divergence in
        // either producer fails here.
        let (sun_dir, _) = compute_sun_arc(6.0, DEFAULT_TOD_HOURS);
        let canonical = crate::env_translate::procedural_fallback_cell_lighting(sun_dir);
        assert_eq!(cell_lit.ambient, canonical.ambient);
        assert_eq!(cell_lit.directional_color, canonical.directional_color);
        assert_eq!(cell_lit.fog_color, canonical.fog_color);
        assert_eq!(cell_lit.fog_near, canonical.fog_near);
        assert_eq!(cell_lit.fog_far, canonical.fog_far);
        assert_eq!(cell_lit.fog_medium, canonical.fog_medium);
    }

    /// Interior gate survives the fallback path — `is_interior=true`
    /// must NOT be clobbered by the no-WTHR branch, mirroring the
    /// main-path interior gate (#782).
    #[test]
    fn no_wthr_interior_preserves_xcll_values() {
        let world = build_world_no_wthr(true);
        weather_system(&world, 0.016);

        let cell_lit = world.try_resource::<CellLightingRes>().unwrap();
        // Original zero values must survive — interior cells own their
        // XCLL/LGTM-authored lighting and the weather fallback must not
        // clobber them.
        assert_eq!(
            cell_lit.ambient,
            [0.0, 0.0, 0.0],
            "interior ambient was clobbered by the no-WTHR fallback — \
             same #782-shape regression on the new branch"
        );
        assert_eq!(cell_lit.directional_color, [0.0, 0.0, 0.0]);
        assert_eq!(cell_lit.fog_color, [0.0, 0.0, 0.0]);
    }
}

/// Regression tests for #2827 / REN-D18-01 — a mid-session worldspace
/// transition used to seed `SkyParamsRes` with the correct live-hour sun
/// *direction* (`7a851ab9`) but a fixed TOD_DAY palette and the constant
/// `SUN_INTENSITY` peak, because `world_setup.rs::apply_environment`'s
/// `translate_sky`/`translate_exterior_cell_lighting` seed always samples
/// the TOD_DAY slot. At boot the scheduler's own `weather_system` call
/// corrected this before the first render; a mid-session transition
/// rendered one frame with that mismatch. The fix appends a
/// `weather_system(world, 0.0)` resample to the end of `apply_environment`
/// — `apply_environment` itself needs a live `VulkanContext` and is out of
/// unit-test reach, so this pins the mechanism it now relies on directly:
/// starting from exactly the seeded-at-noon, live-hour-is-night state the
/// bug describes, a `dt = 0.0` resample must fully replace both the
/// palette AND the intensity with the correct night sample — not just
/// leave the (already-correct) direction alone.
#[cfg(test)]
mod seeded_at_wrong_tod_resample_tests {
    use super::*;
    use byroredux_core::ecs::World;

    /// Reproduces the exact mismatched seed: `sun_direction` already the
    /// correct night sentinel (the half `7a851ab9` fixed), but
    /// `zenith_color`/`sun_intensity` still the constants a TOD_DAY-only
    /// seed would have installed (matching `env_translate::translate_sky`'s
    /// hardcoded `TOD_DAY` read and `SUN_INTENSITY = 4.0`).
    #[test]
    fn resample_replaces_a_noon_seeded_sky_with_the_live_night_sample() {
        let mut world = World::new();

        const NOON_ZENITH: [f32; 3] = [0.3, 0.5, 0.9];
        const NIGHT_ZENITH: [f32; 3] = [0.0, 0.0, 0.0];
        const SUN_INTENSITY_PEAK_CONSTANT: f32 = 4.0;

        // Game clock at 01:00 — well inside the night window for the
        // tod_hours below (sunrise 6h, sunset 22h). Frozen so `dt = 0.0`
        // isn't the only thing keeping the hour from advancing.
        world.insert_resource(GameTimeRes::frozen_at(1.0));

        let mut sky_colors = [[[0.0_f32; 3]; 6]; 10];
        use byroredux_plugin::esm::records::weather::{SKY_UPPER, TOD_DAY, TOD_MIDNIGHT};
        sky_colors[SKY_UPPER][TOD_DAY] = NOON_ZENITH;
        sky_colors[SKY_UPPER][TOD_MIDNIGHT] = NIGHT_ZENITH;
        world.insert_resource(WeatherDataRes {
            sky_colors,
            fog: [100.0, 60000.0, 200.0, 30000.0],
            fog_media: [
                crate::fog::FogMedium::from_legacy_ramp(100.0, 60000.0, None),
                crate::fog::FogMedium::from_legacy_ramp(200.0, 30000.0, None),
            ],
            tod_hours: [6.0, 10.0, 18.0, 22.0],
            skyrim_dalc_per_tod: None,
            wind_speed: 0,
            precipitation: 0.0,
        });

        // The buggy seed: direction already correct (below-horizon
        // sentinel, what `compute_sun_arc(1.0, …)` itself returns), but
        // palette/intensity still at the TOD_DAY constants.
        world.insert_resource(SkyParamsRes {
            zenith_color: NOON_ZENITH,
            horizon_color: [0.0; 3],
            lower_color: [0.0; 3],
            sun_direction: [0.0, -1.0, 0.0],
            sun_color: [0.0; 3],
            sun_size: 1.0,
            sun_intensity: SUN_INTENSITY_PEAK_CONSTANT,
            sun_angular_radius: 0.020,
            is_exterior: true,
            cloud_tile_scale: 0.0,
            cloud_texture_index: 0,
            sun_texture_index: 0,
            cloud_tile_scale_1: 0.0,
            cloud_texture_index_1: 0,
            cloud_tile_scale_2: 0.0,
            cloud_texture_index_2: 0,
            cloud_tile_scale_3: 0.0,
            cloud_texture_index_3: 0,
            current_dalc_cube: None,
        });

        weather_system(&world, 0.0);

        let sky = world.try_resource::<SkyParamsRes>().unwrap();
        assert_eq!(
            sky.zenith_color, NIGHT_ZENITH,
            "a dt=0.0 resample must replace the seeded TOD_DAY zenith \
             colour with the live-hour (night) sample, not leave it at noon"
        );
        assert_eq!(
            sky.sun_intensity, 0.0,
            "a dt=0.0 resample must replace the seeded SUN_INTENSITY peak \
             constant with the live-hour intensity — 0.0 at 01:00, matching \
             the below-horizon direction the seed already had correct"
        );
        assert_eq!(
            sky.sun_direction,
            [0.0, -1.0, 0.0],
            "the direction half of the seed was already correct \
             (7a851ab9) and must still match after resample"
        );
    }
}

/// Regression tests for #2816 / REN-D18-02 — the Skyrim DALC ambient cube
/// used to be sampled from the SOURCE `WeatherDataRes` only, entirely
/// outside the `if transition_t > 0.0` cross-fade block every other
/// per-weather quantity blends through. A WTHR transition between two
/// Skyrim weathers with differing DALC cubes therefore held the source
/// cube for the whole 8-second fade and popped to the target's on the
/// completion frame, instead of easing like zenith/horizon/ambient/etc.
#[cfg(test)]
mod dalc_cube_crossfade_tests {
    use super::*;
    use byroredux_core::ecs::World;
    use byroredux_plugin::esm::records::weather::SKY_AMBIENT;

    const RED: [f32; 3] = [1.0, 0.0, 0.0];
    const BLUE: [f32; 3] = [0.0, 0.0, 1.0];
    const NEUTRAL_AMBIENT: [f32; 3] = [0.2, 0.2, 0.2];

    fn dalc_cube(pos_y: [f32; 3]) -> crate::components::DalcCubeYup {
        crate::components::DalcCubeYup {
            pos_x: [0.0; 3],
            neg_x: [0.0; 3],
            pos_y,
            neg_y: [0.0; 3],
            pos_z: [0.0; 3],
            neg_z: [0.0; 3],
            specular: [0.0; 3],
            fresnel_power: 1.0,
        }
    }

    /// Midday with symmetric `tod_hours` on both sides folds
    /// `(slot_a, slot_b, t)` and `(b_a, b_b, b_t)` to a pure `TOD_DAY`
    /// sample on each side (`TOD_HIGH_NOON` folds to `TOD_DAY` for DALC,
    /// and the two DAY-side keys bracketing hour 12 are DAY/HIGH_NOON),
    /// so each side's DALC cube is unambiguously `cubes[TOD_DAY]`
    /// regardless of the TOD blend weight — isolates the assertions to
    /// the crossfade weight alone.
    fn weather(dalc: Option<[crate::components::DalcCubeYup; 4]>) -> WeatherDataRes {
        let mut sky_colors = [[[0.0_f32; 3]; 6]; 10];
        sky_colors[SKY_AMBIENT].fill(NEUTRAL_AMBIENT);
        WeatherDataRes {
            sky_colors,
            fog: [100.0, 60000.0, 200.0, 30000.0],
            fog_media: [
                crate::fog::FogMedium::from_legacy_ramp(100.0, 60000.0, None),
                crate::fog::FogMedium::from_legacy_ramp(200.0, 30000.0, None),
            ],
            tod_hours: [6.0, 10.0, 18.0, 22.0],
            skyrim_dalc_per_tod: dalc,
            wind_speed: 0,
            precipitation: 0.0,
        }
    }

    fn build_world(
        source_dalc: Option<[crate::components::DalcCubeYup; 4]>,
        target_dalc: Option<[crate::components::DalcCubeYup; 4]>,
    ) -> World {
        let mut world = World::new();
        world.insert_resource(GameTimeRes::frozen_at(12.0));
        world.insert_resource(weather(source_dalc));
        world.insert_resource(WeatherTransitionRes {
            target: weather(target_dalc),
            elapsed_secs: 4.0,
            duration_secs: 8.0,
            done: false,
        });
        world.insert_resource(SkyParamsRes {
            zenith_color: [0.0; 3],
            horizon_color: [0.0; 3],
            lower_color: [0.0; 3],
            sun_direction: [0.0; 3],
            sun_color: [0.0; 3],
            sun_size: 1.0,
            sun_intensity: 0.0,
            sun_angular_radius: 0.020,
            is_exterior: true,
            cloud_tile_scale: 0.0,
            cloud_texture_index: 0,
            sun_texture_index: 0,
            cloud_tile_scale_1: 0.0,
            cloud_texture_index_1: 0,
            cloud_tile_scale_2: 0.0,
            cloud_texture_index_2: 0,
            cloud_tile_scale_3: 0.0,
            cloud_texture_index_3: 0,
            current_dalc_cube: None,
        });
        world
    }

    /// Both sides carry DALC data — the core fix. Halfway through the
    /// fade (`elapsed=4.0 / duration=8.0` → `t=0.5`) the cube must be the
    /// halfway blend of source and target, not the source cube held at
    /// full weight.
    #[test]
    fn both_sides_with_dalc_ease_between_the_two_cubes() {
        let source = [dalc_cube(RED); 4];
        let target = [dalc_cube(BLUE); 4];
        let world = build_world(Some(source), Some(target));

        weather_system(&world, 0.0);

        let sky = world.try_resource::<SkyParamsRes>().unwrap();
        let cube = sky
            .current_dalc_cube
            .expect("both sides carry DALC data — the blend must be Some");
        assert!(
            (cube.pos_y[0] - 0.5).abs() < 1e-4 && (cube.pos_y[2] - 0.5).abs() < 1e-4,
            "halfway through an 8s fade the DALC cube must be the halfway \
             blend of source (red) and target (blue), got {:?} — pre-fix \
             this held the source cube unblended for the whole fade",
            cube.pos_y
        );
    }

    /// Source has DALC, target doesn't (a Skyrim → non-Skyrim-flavoured
    /// WTHR, or simply a target with no authored DALC bytes). The
    /// directional cube must ease OUT toward the target's own isotropic
    /// ambient, not hold at full weight until it snaps to `None`.
    #[test]
    fn source_with_dalc_eases_out_when_target_has_none() {
        let source = [dalc_cube(RED); 4];
        let world = build_world(Some(source), None);

        weather_system(&world, 0.0);

        let sky = world.try_resource::<SkyParamsRes>().unwrap();
        let cube = sky
            .current_dalc_cube
            .expect("source's DALC data must still contribute mid-fade, not vanish");
        let expected = 1.0 * 0.5 + NEUTRAL_AMBIENT[0] * 0.5;
        assert!(
            (cube.pos_y[0] - expected).abs() < 1e-4,
            "expected the red source cube eased halfway toward the \
             target's flat ambient ({NEUTRAL_AMBIENT:?}), got {:?}",
            cube.pos_y
        );
    }

    /// Neither side carries DALC data — the overwhelming common case
    /// (every non-Skyrim game, and most Skyrim WTHRs too). Must stay
    /// `None`, not manufacture a cube out of nothing.
    #[test]
    fn neither_side_with_dalc_stays_none() {
        let world = build_world(None, None);
        weather_system(&world, 0.0);
        let sky = world.try_resource::<SkyParamsRes>().unwrap();
        assert!(
            sky.current_dalc_cube.is_none(),
            "no DALC data on either side of the fade must not manufacture a cube"
        );
    }
}
