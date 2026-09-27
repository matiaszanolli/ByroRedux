# EXT-D5-2026-09-27-01: NAM0-filled WATR layers are rotated 90° off the current — #4727's conversion is applied to angles the parser already wrote in the engine frame

**Issue**: #4900
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: high,terrain-exterior,bug,water,esm-plugin

**Severity**: HIGH. The skill rule applies: this is a wrong canonical value out of the WATAL translate. The vanilla reach is one FO4 record, but any NAM0 water with a zero-speed layer is affected.
**Dimension**: Water translation (WATAL)
**Tier Violated**: single-boundary + no-fabrication
**Game Affected**: Skyrim, FO4, FO76, Starfield (all NAM0 games). Vanilla case: FO4 `IntOldGulletWaterSlow` (0x1E214D).
**Status**: NEW (introduced by #4727's fix, `a6a210eb9`)
**Location**:
- `crates/plugin/src/esm/records/misc/water.rs:1502-1515`: the NAM0 fill writes `(-y).atan2(x)`, which is already in engine XZ, into `noise_wind_directions`.
- `byroredux/src/env_translate.rs:859-871` (`resolve_water_layer_motion` → `watr_angle_to_engine_xz`, +90° on every layer).
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `noise_wind_directions` now holds two frames: DNAM layers in record-frame bearings, and NAM0-filled layers already in the engine frame.
- The translate cannot tell them apart and rotates both, so a filled layer runs perpendicular to the current it was copied from.
- The fill also puts a BU/s speed into a UV/s slot.

## Evidence
- `IntOldGulletWaterSlow` has NAM0 (0.08, 0, 0) and layer speeds (0.0072, 0.0, 0.0145), so layer 1 is filled along +X.
- The record classifies as River, with a flow term of 0.0228 UV/s. The emitted `scroll_b` is about (0, 0.091) UV/s: perpendicular to the current, at 4× the flow term. Before #4727 it ran along the flow.
- Every translate test builds `WatrRecord` directly, so the parse→translate path is unpinned.

## Impact
The normal layer slides across the current on affected water. The same applies to every mod with a NAM0 water and an unset layer.

## Suggested Fix
- Store the filled layers in the record frame (φ − 90°), or move the fill into the translate after the conversion.
- Convert the filled speed to UV/s.
- Add a parse→translate test asserting that a filled layer is parallel to `WaterFlow.direction`.

## Related
#4727, EXT-D5-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
