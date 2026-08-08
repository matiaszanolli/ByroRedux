# REN-D18-NEW-01: build_tod_keys afternoon re-anchor clamps against the wrong neighbour -- TOD key table goes non-monotonic on short-day climates

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2473
**Finding ID**: REN-D18-NEW-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 18 — Sky/Weather
**Location**: `byroredux/src/systems/weather.rs::build_tod_keys` (line ~37), test `tod_keys_clamp_afternoon_cool_on_compressed_days` (line ~823)
**Status**: NEW

## Description
`build_tod_keys` emits 7 `(hour, slot)` pairs that `pick_tod_pair` walks as a *strictly increasing* piecewise-linear table. Key 3 is the synthetic `afternoon_peak = (sunrise_end + sunset_begin) * 0.5` (HIGH_NOON) and key 4 is the `afternoon_cool` DAY re-anchor, clamped as `(sunset_begin - 2.0).max(sunrise_end + 0.1)`. The clamp is anchored to **key 2** (`sunrise_end`), not to its actual predecessor **key 3** (`afternoon_peak`). Solving `afternoon_cool < afternoon_peak` gives the trigger condition `0.2 < (sunset_begin - sunrise_end) < 4.0` — every climate whose clear-day window is under 4 hours produces a table where key 4 sits *before* key 3. The dedicated regression test asserts `keys[4].0 > keys[2].0` (against `sunrise_end`) rather than `keys[4].0 >= keys[3].0`, so it passes on inputs that already violate the invariant; the sibling monotonicity test's corpus never crosses the 4h threshold either. Both tests give false assurance.

## Evidence
```rust
let afternoon_peak = (sunrise_end + sunset_begin) * 0.5;          // key 3
let afternoon_cool = (sunset_begin - 2.0).max(sunrise_end + 0.1); // key 4 — clamped vs key 2, not key 3
```
Worked example with the exact input the existing "clamp" test already uses, `tod_hours = [5.0, 10.0, 11.0, 20.0]`: `afternoon_peak = 10.5`, `afternoon_cool = max(9.0, 10.1) = 10.1` → keys = `[1, 5, 10, 10.5, 10.1, 11, 22]`, with `keys[4] (10.1) < keys[3] (10.5)`. Downstream in `pick_tod_pair`, the `h >= h0 && h < h1` scan can never satisfy `h >= 10.5 && h < 10.1`, so the HIGH_NOON→DAY ease-out segment is unreachable and hour `10.5` snaps straight to `mix(DAY, SUNSET, 0.555)`. No NaN — the failure is a hard discontinuity.

## Impact
A single-frame discontinuous jump in zenith/horizon/lower/sun/ambient/sunlight/fog colour **and** fog near/far distance, occurring once per in-game day at `hour == afternoon_peak`, on any worldspace whose CLMT ships `sunset_begin - sunrise_end < 4h`. Vanilla FNV (gap 8h) and FO3 Capital Wasteland (gap 7h) are safe, so shipped content is unaffected today; reachable on modded/authored CLMTs and on any synthetic climate. `climate_tod_hours` accepts any TNAM byte in `1..=144`, so nothing upstream filters short-day climates out. Blast radius is the whole exterior frame, but visual-only and self-corrects on the next segment.

## Related
Same key table as #463 / #530 / #897 (fog-palette lockstep).

## Suggested Fix
Clamp against the true predecessor: `let afternoon_cool = (sunset_begin - 2.0).max(afternoon_peak + 0.1).min(sunset_begin - 1e-3);` and tighten `tod_keys_clamp_afternoon_cool_on_compressed_days` to assert full `windows(2)` monotonicity (or add `[5.0, 10.0, 11.0, 20.0]` to `tod_keys_are_monotonic_on_realistic_climates`'s corpus, which fails today).

## Completeness Checks
- [ ] **TESTS**: `tod_keys_clamp_afternoon_cool_on_compressed_days` re-tightened to assert full monotonicity; `[5.0, 10.0, 11.0, 20.0]` added to the monotonicity corpus
