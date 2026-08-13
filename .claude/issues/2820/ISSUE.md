# REN-D18-03: `build_tod_keys`'s night anchor is clamped by an unsourced `23.0` that fires on vanilla FNV/FO3 and can go non-monotonic

- **Severity**: MEDIUM
- **Dimension**: 18 — Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` — `build_tod_keys`, the `let night = (sunset_end + 2.0).min(23.0);` binding (key 6). Guard corpus: `tod_keys_are_monotonic_on_realistic_climates`.
- **Status**: NEW (sibling of OPEN #2473, which covers key 4's `afternoon_cool` clamp only — different key, different trigger)
- **Description**: Two problems with one literal. (a) The documented model in the function's own doc comment is "`sunset_end + 2h` (clamped to 23h) → `TOD_NIGHT`". Every shipped Fallout climate has `sunset_end = 22.0` (FNV `[6, 10, 18, 22]`, FO3 Capital Wasteland `[5.333, 10, 17, 22]`), so `22 + 2 = 24` is clamped to `23` on vanilla content: the interpolator reaches full `TOD_NIGHT` an hour earlier than the `+2h` rule states, compressing the `SUNSET → NIGHT` ease from 6 h to 5 h. The clamp's stated purpose (staying below the `keys[0] + 24 = 25.0` wrap point) is satisfied by anything under 25.0, so `23.0` is 2 hours stricter than the constraint requires with no source cited. (b) Because the clamp is absolute rather than relative to its predecessor key 5 (`sunset_begin`), any climate with `sunset_begin > 23.0` produces `keys[5] > keys[6]`. `climate_tod_hours` validates TNAM bytes only against `1..=144`, so bytes 139–144 (`23.17h`–`24.0h`) pass validation and yield a non-monotonic table — exactly the invariant `pick_tod_pair` assumes and #2473 documents the consequences of.
- **Evidence**: `let afternoon_cool = (sunset_begin - 2.0).max(sunrise_end + 0.1); // key 4 — #2473` sits directly above `let night = (sunset_end + 2.0).min(23.0); // key 6`. `tod_keys_are_monotonic_on_realistic_climates`'s four-entry corpus tops out at `sunset_begin = 19.5`, so it cannot catch (b); no test asserts (a) at all.
- **Impact**: (a) affects every FNV/FO3 exterior every in-game evening. (b) is modded/authored-CLMT only, visual, self-correcting on the next segment.
- **Related**: #2473 (key 4, same table, same invariant), #463 / #530 (`climate_tod_hours` validation range), #897.
- **Suggested Fix**: Fold into #2473's fix — clamp each key against its true predecessor (`night = (sunset_end + 2.0).max(sunset_begin + 0.1).min(24.9)`) and extend `tod_keys_are_monotonic_on_realistic_climates` to a full `windows(2)` assertion over a corpus that includes a late-sunset climate (`[6.0, 10.0, 23.5, 24.0]`). If the 1-hour vanilla compression in (a) is intentional, record why in the doc comment instead of leaving the literal unexplained.

## Completeness Checks
- [ ] **SIBLING**: Same predecessor-relative clamp treatment applied to key 6 (`night`) as #2473 applies to key 4 (`afternoon_cool`)
- [ ] **TESTS**: `tod_keys_are_monotonic_on_realistic_climates` extended to a full `windows(2)` assertion over a corpus including a late-sunset climate

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding `REN-D18-03`)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2820
