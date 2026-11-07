**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-07.md` (INC-3).

- **Severity**: LOW
- **Location**: (a) `crates/renderer/src/vulkan/volumetrics.rs` — `VolumetricsParams::wind_gust` doc; (b) same file, the doc block above `open_sky_sun_dust_rides_the_sun_term_only_and_fades_with_a_high_sun`; (c) the ABI-pin comment near `crates/renderer/src/vulkan/reflect.rs:771`
- **Changed in**: fdcd02c0a / 684bc9161 (a), d79c92813 (b)

## Description
(a) The `wind_gust` doc was updated for lane `z` but still says "The remaining lane is reserved", while lane `w` has carried `open_sky_sun_dust_per_meter()` since d79c92813. The `reflect.rs` ABI-pin comment names only `wind_gust.y`.
(b) The `#4793 / #4785 ... every sun-visibility ray ...` doc paragraph that belonged to `sun_visibility_rays_are_gated_on_a_live_sun_term` now sits directly above the new `BYRO_SUN_DUST` test's own doc, so it documents the wrong test and the gate test lost its doc.

## Impact
A later author reads `wind_gust.w` as free and reuses it, or reads the wrong test as the #4793 gate.

## Suggested Fix
Document lane `w` in the `wind_gust` doc and the reflect pin comment; move the #4793 paragraph back above `sun_visibility_rays_are_gated_on_a_live_sun_term`.

## Completeness Checks
- [ ] **SIBLING**: Other `VolumetricsParams` lane docs checked against the shader's actual use
- [ ] **TESTS**: `_audit-validate.sh` still passes after the edits
