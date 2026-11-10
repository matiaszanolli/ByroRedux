# REN-D8-08: host `height_fog_optical_depth` no longer mirrors the shader after #6728, and the new piecewise tail has no numeric test

Labels: low,renderer,test-gap,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW (test-gap / stale mirror). **Status**: NEW.

**Location**: `pub fn height_fog_optical_depth` (`crates/renderer/src/vulkan/volumetrics.rs`), its tests `horizontal_height_fog_ray_is_finite_and_matches_constant_density` and `steep_downward_height_fog_ray_stays_finite`; `the_analytic_tail_and_the_grid_share_one_height_profile` (`crates/renderer/src/vulkan/sky_dome.rs`).

## Description
After #6728 the shader integrates `sigma0 * exp(-max(y - y0, 0) / H)` piecewise (constant below the reference, exponential above). The host function still integrates the unclamped `sigma0 * exp(-(y - y0)/H)` and is used only by those two tests. The only test of the new shader code is a substring scan (`crossing`, `expRunOpticalDepth(`, `exp(-max(aboveStart, 0.0)`), so REN-D8-07 passes every test; "steep downward ray stays finite" certifies a function nothing ships.

## Evidence
`grep -rn height_fog_optical_depth crates byroredux` finds the definition (`volumetrics.rs:356`) and the two tests only; the doc comment still reads `sigma_t(y) = sigma0 * exp(-(y - base_height) / scale_height)`.

## Impact
A regression in the tail's math (REN-D8-07's class) is invisible to `cargo test`; the stale mirror reads as the contract.

## Suggested Fix
Port the piecewise function to the host, delete the unclamped one, and test it against a numeric integral (horizontal, up, down, crossing both ways, start 100+ scale heights up) — the same shape as `sky_dome.rs`'s `chapman_column_matches_numerical_integration`.

Related: REN-D8-07, #6728. Report: `docs/audits/AUDIT_RENDERER_2026-11-09.md`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

