# EXT-D1-2026-09-27-02: `render/sky.rs` builds the procedural exterior sky and sun arc inside the render loop on direct `--cell` interior boots

**Issue**: #4902
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,renderer

**Severity**: MEDIUM. The severity table's "EXAL translation done at render time" row would make this HIGH. It is downgraded because the render-side values equal the canonical fallback today: the same `procedural_fallback_sky`, `compute_sun_arc` and `FB_SUN_COLOR`.
**Dimension**: EXAL boundary discipline
**Tier Violated**: no-render-time-fallback (and a single-boundary dent)
**Game Affected**: all interior-only `--cell` sessions (window portals, Show-Sky interiors, apertures, godrays)
**Status**: NEW (introduced by `0572bfd5a`, extended by `a4a68fa92`)
**Location**:
- `byroredux/src/render/sky.rs:121-140`: the else-arm of `build_sky_params` calls `procedural_fallback_sky` and `compute_sun_arc` every frame.
- `:73-98`: `portal_sun` has its own copy of the arc.
- `env_translate.rs:1554`: `FB_SUN_COLOR` was made `pub(crate)` by `0572bfd5a`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `exal.md` §3 contract 2 requires the no-climate case to be an explicit canonical default, not a render-loop branch.
- There are now three render-side derivations that must be kept in step with `weather_system` by hand.

## Evidence
Main-context re-read of `build_sky_params` and `portal_sun`. `git log -S'procedural_fallback_sky(direction)'` points to `0572bfd5a`.

## Impact
Latent divergence. A change to the fallback palette or sun model would silently split the interior-portal sky from the exterior fallback.

## Suggested Fix
- On an interior-only boot, install the canonical defaults once (`procedural_fallback_sky`, `procedural_fallback_weather`, `GameTimeRes`) and let `weather_system` advance them.
- Delete the render-side arms, and make `FB_SUN_COLOR` private again.
- Land together with EXT-D1-04, or env.health FAILs every interior.

## Related
#3323, #4839, EXT-D1-04, EXT-D4-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **LOCK_ORDER**: `WeatherDataRes` / `WeatherTransitionRes` / `SkyParamsRes` / `CellLightingRes` acquisition order preserved (#3263, #1410)
- [ ] **TESTS**: A regression test pins this specific fix
