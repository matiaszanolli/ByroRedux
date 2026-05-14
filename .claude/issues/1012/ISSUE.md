# Issue #1012

**Title**: REN-D15-NEW-08: Sun arc ignores CLMT tod_hours for direction — only fades intensity at hardcoded 6h/18h

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D15-NEW-08
**Severity**: HIGH
**File**: `byroredux/src/systems/weather.rs:294-330`

## Premise verified (current `main`)

`weather.rs:227` correctly drives the colour interpolator from `build_tod_keys(wd.tod_hours)`, so palettes track CLMT TNAM hours per worldspace (FO3 Capital Wasteland sunrise 5.333h, FNV Mojave 6.0h). But the sun **direction** and **intensity** at lines 294-330 are computed against hardcoded constants:

```rust
let solar_hour = (hour - 6.0).clamp(0.0, 12.0);   // sunrise hardcoded to 6h
let angle = solar_hour / 12.0 * std::f32::consts::PI;   // arc spans hardcoded 12h
let sun_intensity = if (7.0..=17.0).contains(&hour) { 4.0 }
    else if (6.0..7.0).contains(&hour) { (hour - 6.0) * 4.0 }
    else if hour > 17.0 && hour <= 18.0 { (18.0 - hour) * 4.0 }
    else { 0.0 };
if (6.0..=18.0).contains(&hour) { [x/len, y/len, z/len] }
else { [0.0, -1.0, 0.0] }     // night sentinel hardcoded at 6h/18h
```

## Issue

On FO3's canonical climate (sunrise 5.333h), the sun direction stays at the below-horizon sentinel `[0, -1, 0]` for ~40 minutes of in-game *sunrise* while the sky gradient is sunrise-tinted — sky paints dawn while the world goes pitch-dark under a "below-horizon" sun. Symmetric ~1h dead window at sunset. #463 migrated the colour path off hardcoded values but left the arc literals.

## Fix

Drive `solar_hour`, the arc-span denominator, and the intensity envelope from `wd.tod_hours.{sunrise_begin, sunrise_end, sunset_begin, sunset_end}`. Same shape as the colour interpolator at line 227.

## Test

Synthetic CLMT with sunrise=5.0h, sunset=19.0h; assert `sun_dir.y > 0` across [5.5h, 18.5h] (vs current [6h, 18h]). FO3 Capital Wasteland integration smoke test (M40 sunrise/sunset boundary).

## Completeness Checks

- [ ] **UNSAFE**: N/A — pure-Rust math
- [ ] **SIBLING**: Cross-check intensity envelope, direction vector, and night sentinel all read the same TOD source
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test pinning sun_dir.y sign across non-default CLMT TOD bands

