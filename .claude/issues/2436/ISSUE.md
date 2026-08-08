# COORD-3: RENDER_ORIGIN_SNAP is a second, uncoupled 4096.0 exterior-cell literal

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2436
**Finding ID**: COORD-3 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — Coordinate-system correctness
**Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:339-352` (const), `:404-412` (test)
**Status**: NEW

## Description
`RENDER_ORIGIN_SNAP: f32 = 4096.0` is a bare literal whose own doc comment names it as the exterior cell edge length, but its pin test asserts `== 4096.0` rather than `== EXTERIOR_CELL_UNITS`, even though `byroredux-renderer` already depends on `byroredux-core`. Residue of #1112/TD3-202's literal collapse — six sites unified, this seventh (added later, #1494) reintroduced the pattern.

## Impact
Latent — the value is spec-fixed today so the two constants cannot realistically disagree. Risk: the render-origin rebase and the cell-streaming grid must snap on the same boundary for the #1489 `prev_view_proj` origin correction to stay valid; an isolated retune of either breaks motion vectors across grid crossings with no test failure.

## Related
#1112 (CLOSED, TD3-202, prior literal collapse), #1494 (CLOSED, REN2-09, introduced this specific constant).

## Suggested Fix
`RENDER_ORIGIN_SNAP = byroredux_core::math::coord::EXTERIOR_CELL_UNITS` and update the pin test to assert against the SoT constant.

## Completeness Checks
- [ ] **TESTS**: Pin test asserts against `EXTERIOR_CELL_UNITS`, not a bare literal
