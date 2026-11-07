**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-07.md` (INC-2).

- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` — test `open_sky_sun_dust_rides_the_sun_term_only_and_fades_with_a_high_sun` (last assertion)
- **Changed in**: commit d79c92813

## Description
The test ends with `assert_eq!(open_sky_sun_dust_per_meter(), 0.0, "off unless BYRO_SUN_DUST is set")`. `open_sky_sun_dust_per_meter` reads the live environment through a `OnceLock`.

## Impact
Anyone with `BYRO_SUN_DUST=<n>` exported (the documented recipe for the experiment) gets a failing `cargo test -p byroredux-renderer` with a misleading message.

## Suggested Fix
Split the parse into a pure `fn(Option<&str>) -> f32` and test that; leave the `OnceLock` wrapper untested, or drop the assertion.

## Completeness Checks
- [ ] **SIBLING**: `sun_alpha_trace_enabled` and `bfecc_error_correction_multiplier` checked for the same env-dependent test pattern
- [ ] **TESTS**: The new pure parser has a test covering unset, invalid, negative, non-finite and valid values
