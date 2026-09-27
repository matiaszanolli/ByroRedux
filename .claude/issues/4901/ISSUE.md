# EXT-D1-2026-09-27-01: A WTHR cross-fade never promotes `image_space` — the exterior grade snaps back to the source weather once the transition completes

**Issue**: #4901
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug

**Severity**: MEDIUM (visual; affects the whole session after any weather transition)
**Dimension**: EXAL boundary discipline × Sky, weather, sun
**Tier Violated**: n/a. A canonical lane is dropped at the promotion step; this is the fifth recurrence of the #4481 class.
**Game Affected**: FO3/FNV (worldspace INAM changes, XCCM crossings); Skyrim/FO4 (weathers with differing IMSP)
**Status**: NEW (incomplete fix of #4416 and #4733)
**Location**:
- `byroredux/src/systems/weather.rs:1286-1302` (`image_space: _` in `promote_weather_transition_target`).
- Consumer: `:854`, `:1213-1234`.
- Also reached via `scene/world_setup.rs:723-728` (`collapse_weather_transition`).
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- #4733's exhaustive destructure fired on `image_space`, and the field was bound `_`. The rationale given was "the completion frame lerp-samples the target's image space".
- That holds only on the completion frame. Afterwards `tr.done` latches and `transition_t` is 0.0 (`:781-782`).
- From then on the grade is sampled from the unpromoted `wd.image_space` (`:854`) and published to `ImageSpaceBase`.

## Evidence
- The promotion writes 14 fields but not `image_space` (re-read in the main context).
- No test drives a transition past completion and then checks `ImageSpaceBase`.

## Impact
The grade fades to the target over 8 s, then reverts to the old weather's grade for the rest of the session.

## Suggested Fix
- Bind `image_space: tr_target_image_space` and assign `wd.image_space = tr_target_image_space`.
- Add a test that checks `ImageSpaceBase` equals the target's sample on the frame after `done`.

## Related
#4416, #4733, #4481

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **LOCK_ORDER**: `WeatherDataRes` / `WeatherTransitionRes` / `SkyParamsRes` / `CellLightingRes` acquisition order preserved (#3263, #1410)
- [ ] **TESTS**: A regression test pins this specific fix
