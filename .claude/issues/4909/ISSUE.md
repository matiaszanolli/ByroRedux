# EXT-D4-2026-09-27-02: The froxel medium's sun ignores cloud cover and the HNAM sunlight dimmer, and uses the sun-disc colour at the raw 0–4 scale

**Issue**: #4909
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,renderer,shaders

**Severity**: MEDIUM (visual; a third derivation of the sun)
**Dimension**: Sky, weather, sun (volumetrics consumer, co-owned by `/audit-renderer`)
**Tier Violated**: single-boundary
**Game Affected**: all exteriors; interiors via `portal_sun`
**Status**: NEW. `volumetric_sun` arrived in `0572bfd5a`, and no prior report or issue covers it.
**Location**:
- `crates/renderer/src/vulkan/context/post_passes.rs:53-78` (`volumetric_sun`).
- `byroredux/src/render/sky.rs:73-85` (`portal_sun`).
- `crates/renderer/shaders/volumetrics_inject.comp:3022`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
There are three "sun" radiances.
- **Surface key:** `SKY_SUNLIGHT × sunlight_dimmer × (sun_intensity/4) × exp(-2.5·coverage)`, from `compute_directional_upload`.
- **Cloud march:** the same `sun_illuminance`. skyal.md explicitly rejects `sun_color * sun_intensity` as "the sun disc colour and its raw 0-4 scale".
- **Volumetric in-scatter:** still `sky.sun_color * sky.sun_intensity`, with no dimmer and no cloud transmittance.

## Evidence
Main-context re-read of `volumetric_sun`'s exterior arm.

## Impact
- Under full overcast the terrain key drops to about 8% of clear sky, but sunlit haze and godrays stay at clear-sky strength. That is about 12× too bright relative to the key.
- HNAM-dimmed worldspaces get undimmed shafts.

## Suggested Fix
- Feed the medium from `sun_illuminance`: the exterior's own value, or for interiors the portal palette's value from #4839.
- Make `portal_sun` return only a direction.
- Add a test that a coverage=1 weather dims `volumetric_sun`.

## Related
#4785, #4839, EXT-D1-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix
