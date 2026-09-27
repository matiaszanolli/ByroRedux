# EXT-D4-2026-09-27-01: Interior Show-Sky and aperture backgrounds render with no stars, moon or aurora — `weather_sky_details` gates on the room's `depth_params.x`

**Issue**: #4908
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,shaders,renderer

**Severity**: MEDIUM (visual)
**Dimension**: Sky, weather, sun
**Tier Violated**: n/a
**Game Affected**: all (interiors with Show Sky / Behave Like Exterior, or authored LightShaft apertures, at night)
**Status**: NEW
**Location**:
- `crates/renderer/shaders/include/sky.glsl:84-87`.
- `crates/renderer/shaders/composite.frag:422`.
- `crates/renderer/src/vulkan/context/draw.rs` (`build_composite_params` sets `depth_params[0]` = room `is_exterior` = 0; the interior cube arm sets 1).
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `0572bfd5a` made interior composites paint the outdoor palette through Show Sky and bounded apertures. It deliberately kept `depth_params.x = 0` so rain and height fog stay off.
- The pre-existing `weather_sky_details` returns early on `depth_params.x <= 0.5`, dropping the stars, the moon and the aurora.
- The interior cube bake packs 1, so the same room's window-portal escape does show them.

## Evidence
- Main-context re-read of `sky.glsl:84`.
- `interior_portal_sky_preserves_room_weather_gate` asserts composite 0 and cube 1.

## Impact
At night an open-roof interior shows a starless, moonless sky, while the glass in the same room shows stars.

## Suggested Fix
- Gate the details on "an outdoor palette is drawn" (`depth_params.x > 0.5 || sky_lower.w > 0.5`), and keep `depth_params.x` for weather and fog.
- Extend the existing test.

## Related
#4861, EXT-D4-05

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix
