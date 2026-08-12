//! Regression tests for [`climate_tod_hours`] — #530 / FNV-CELL-8.
//!
//! Pre-fix the filter was `OR-of-four-bytes != 0`, so a corrupt
//! modded CLMT shipping `[0, 0, 0, 0xFF]` slipped through and
//! produced a `sunset_end` of 42.5h, breaking the TOD interpolator.
//! Post-fix every byte must fall in `1..=144` (1 = 0:10, 144 = 24:00)
//! before the authored values are accepted; any out-of-range byte
//! falls back to the pre-#463 hardcoded breakpoints.
use super::*;
use byroredux_plugin::esm::records::ClimateRecord;

fn climate_with_tnam(bytes: [u8; 4]) -> ClimateRecord {
    ClimateRecord {
        sunrise_begin: bytes[0],
        sunrise_end: bytes[1],
        sunset_begin: bytes[2],
        sunset_end: bytes[3],
        ..ClimateRecord::default()
    }
}

const FALLBACK: [f32; 4] = [6.0, 10.0, 18.0, 22.0];

#[test]
fn no_climate_returns_fallback() {
    assert_eq!(climate_tod_hours(None), FALLBACK);
}

#[test]
fn vanilla_fnv_climate_returns_authored_hours() {
    // Vanilla FNV ClimateMojave TNAM:
    //   sunrise_begin = 0x24 (36 = 6:00),
    //   sunrise_end   = 0x3C (60 = 10:00),
    //   sunset_begin  = 0x6C (108 = 18:00),
    //   sunset_end    = 0x84 (132 = 22:00).
    let c = climate_with_tnam([36, 60, 108, 132]);
    let h = climate_tod_hours(Some(&c));
    assert!((h[0] - 6.0).abs() < 1e-6);
    assert!((h[1] - 10.0).abs() < 1e-6);
    assert!((h[2] - 18.0).abs() < 1e-6);
    assert!((h[3] - 22.0).abs() < 1e-6);
}

#[test]
fn all_zero_tnam_returns_fallback() {
    // Stub CLMT with no TNAM data — every byte is zero. Pre-fix
    // path already handled this; the regression keeps it pinned.
    let c = climate_with_tnam([0, 0, 0, 0]);
    assert_eq!(climate_tod_hours(Some(&c)), FALLBACK);
}

#[test]
fn out_of_range_byte_falls_back() {
    // 0xFF / 6 = 42.5h — clearly corrupt. Pre-#530 the filter
    // would have admitted this CLMT because `OR-of-bytes != 0`
    // is true, and the TOD interpolator would have received a
    // sunset_end past the end of the day.
    let c = climate_with_tnam([36, 60, 108, 0xFF]);
    assert_eq!(climate_tod_hours(Some(&c)), FALLBACK);
}

#[test]
fn audit_example_single_byte_set_falls_back() {
    // Audit example: `[0, 0, 0, 0x80]`. Three zero bytes (out of
    // range 1..=144) means at least one breakpoint would land at
    // hour 0, which doesn't make physical sense — fall back. The
    // pre-#530 OR filter would have admitted this record.
    let c = climate_with_tnam([0, 0, 0, 0x80]);
    assert_eq!(climate_tod_hours(Some(&c)), FALLBACK);
}

#[test]
fn boundary_bytes_one_and_one_forty_four_are_accepted() {
    // 1 = 0:10, 144 = 24:00 — the inclusive endpoints of the
    // authored range. A future tightening that drops the
    // boundaries would silently reject midnight wraparound
    // CLMTs (real-world: Aurora-Borealis-style polar climates).
    let c = climate_with_tnam([1, 144, 1, 144]);
    let h = climate_tod_hours(Some(&c));
    assert!((h[0] - (1.0 / 6.0)).abs() < 1e-6);
    assert!((h[1] - 24.0).abs() < 1e-6);
    assert!((h[2] - (1.0 / 6.0)).abs() < 1e-6);
    assert!((h[3] - 24.0).abs() < 1e-6);
}

#[test]
fn one_forty_five_just_past_the_boundary_falls_back() {
    // 145 = 24:10 — one tick past midnight. Beyond the authored
    // range, must fall back rather than producing a >24h hour.
    let c = climate_with_tnam([36, 60, 108, 145]);
    assert_eq!(climate_tod_hours(Some(&c)), FALLBACK);
}
