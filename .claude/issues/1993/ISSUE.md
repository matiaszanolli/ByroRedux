**Severity**: low (INFO-tier)
**Dimension**: Denoiser/Composite — test coverage (renderer audit 2026-07-14, DIM8)
**Location**: `crates/renderer/shaders/svgf_temporal.comp` firefly-clamp block (immediately before `if (hasHistory)`); missing test belongs in `crates/renderer/src/vulkan/svgf.rs`
**Status**: NEW (CONFIRMED against HEAD)

## Description
Commit `48906670` hoisted the spatial firefly clamp ahead of the `hasHistory` branch so it also clamps the no-history / disocclusion path. This is correct in the current source, but is protected only by an in-shader comment (`INVARIANT (REG-07 / #1639, #1481)`), not a test — unlike the sibling TAA α-floor invariant, which IS guarded by a source-scanning unit test (`taa.rs::taa_comp_floors_alpha_for_moving_pixels_under_parked_camera`, which `include_str!`s `taa.comp`). A future edit re-scoping the clamp inside `hasHistory` would compile clean and pass `cargo test`, silently re-opening firefly leaks on disocclusion.

## Evidence
- Firefly-clamp block ends at `currLum2 = maxL * maxL;`, then `if (hasHistory) {` follows; the no-history else-branch writes the (now-clamped) `currInd`.
- `grep -c 'include_str.*svgf_temporal' crates/renderer/src/vulkan/svgf.rs` → 0 (no source-scanning test exists).

## Impact
None today; a latent regression-guard gap.

## Suggested Fix
Add a `#[test]` in `svgf.rs` that `include_str!`s `svgf_temporal.comp` and asserts the firefly-clamp `imageStore` / `currInd *=` site precedes the `if (hasHistory)` token (mirroring the TAA α-floor test).

## Completeness Checks
- [ ] **SIBLING**: Confirm the à-trous pass has no equivalent order-dependent clamp that also wants pinning.
- [ ] **TESTS**: This finding *is* the test — the added `#[test]` pins the REG-07 hoist.
