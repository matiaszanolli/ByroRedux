//! Time-of-day key table and day/night blend (#6714: lifted out of `systems::weather` so the
//! render collector can read the climate-authored blend without naming `systems`).
//!
//! Pure functions of a climate's `tod_hours`; the fallback quad they default to is
//! [`super::FB_TOD_HOURS`].

/// Build the time-of-day key table used by the `weather_system`
/// interpolator from a climate's `tod_hours`.
///
/// `tod_hours = [sunrise_begin, sunrise_end, sunset_begin, sunset_end]`
/// in floating-point game hours (CLMT TNAM bytes divided by 6). The
/// returned 8-entry table is `(hour, TOD slot index)` pairs the
/// interpolator walks in increasing-hour order:
///
///  - `midnight` (synthetic — TNAM doesn't encode it; anchored at 1h)
///  - `sunrise_begin` → `TOD_SUNRISE`
///  - `sunrise_end`   → `TOD_DAY`
///  - midpoint(sunrise_end, sunset_begin) → `TOD_HIGH_NOON`
///  - `sunset_begin - 2h` (clamped) → `TOD_DAY` re-anchor — preserves
///    the `day → sunset` ease-in the pre-#463 hardcoded path had
///  - `sunset_begin` → `TOD_SUNSET`
///  - `sunset_end` → `TOD_SUNSET` (the sunset hold — see below)
///  - `sunset_end + 2h` (clamped against `sunset_begin` and against the
///    `keys[0] + 24` wrap point) → `TOD_NIGHT`
///
/// # The sunset hold
///
/// `compute_sun_arc` puts the sun **on the horizon exactly at
/// `sunset_end`** — the visible arc spans `[sunrise_begin, sunset_end]`,
/// so through the `[sunset_begin, sunset_end]` window the sun is still
/// visibly descending and only touches the horizon at the window's end.
/// The palette must match: the authored `TOD_SUNSET` colours hold
/// through the whole descent, and the slide to `TOD_NIGHT` eases in only
/// after the sun is gone (the existing `+2h` tail). Pre-fix the single
/// `(sunset_begin, SUNSET) → (night, NIGHT)` span started the night
/// slide at the top of the window, so by the golden hour — sun near the
/// horizon, the moment a sunset *is* — the palette had already blended
/// `window/(window+2)` toward night (Skyrim's 4.5 h window: 69% night;
/// FO3/FNV's 4 h: 67%): a slate-grey dusk with the warm band gone, in
/// every game, exactly when the sun was lowest. The same lockstep keeps
/// fog on the `TOD_SUNSET` half-day distance through the descent instead
/// of clamping to night fog while the sun still lights the terrain.
///
/// Kept `pub(crate)` so the unit test in this module can pin the
/// formula independently of a full World setup.
pub(crate) fn build_tod_keys(tod_hours: [f32; 4]) -> [(f32, usize); 8] {
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
    // The sunset hold's end (see the function doc): the sun touches the
    // horizon at `sunset_end`, clamped inside `(sunset_begin, night)` so
    // the key chain stays strictly increasing on the same broken-data
    // extremes the two clamps above already defend.
    let sunset_hold_end = sunset_end.max(sunset_begin + 0.05).min(night - 0.05);
    [
        (midnight, TOD_MIDNIGHT),
        (sunrise_begin, TOD_SUNRISE),
        (sunrise_end, TOD_DAY),
        (afternoon_peak, TOD_HIGH_NOON),
        (afternoon_cool, TOD_DAY),
        (sunset_begin, TOD_SUNSET),
        (sunset_hold_end, TOD_SUNSET),
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
pub(crate) fn pick_tod_pair(keys: &[(f32, usize); 8], hour: f32) -> (usize, usize, f32) {
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

/// Return the climate-authored day/night blend for a game-clock hour.
///
/// Water's GNAM surface variants use the same TOD keys as the sky and fog;
/// keeping this calculation here prevents water from reintroducing fixed
/// 06:00/18:00 breakpoints that disagree with FO3/FO76 climate schedules.
pub(crate) fn night_factor_for_hour(hour: f32, tod_hours: [f32; 4]) -> f32 {
    let hour = if hour.is_finite() {
        hour.rem_euclid(24.0)
    } else {
        12.0
    };
    let keys = build_tod_keys(tod_hours);
    let (slot_a, slot_b, t) = pick_tod_pair(&keys, hour);
    (tod_slot_night_factor(slot_a)
        + (tod_slot_night_factor(slot_b) - tod_slot_night_factor(slot_a)) * t)
        .clamp(0.0, 1.0)
}
