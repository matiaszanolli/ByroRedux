# EXT-D5-2026-09-27-07: Frame and contract doc rot after #4727/#4735

**Issue**: #4932
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,documentation,water,doc-rot

**Severity**: LOW
**Dimension**: Water translation (WATAL)
**Tier Violated**: n/a
**Game Affected**: all
**Status**: NEW
**Location**:
- `byroredux/src/env_translate.rs:539-541,863,2866-2868`.
- `docs/engine/watal.md:433`.
- `crates/core/src/ecs/components/water.rs:272`.
- `crates/renderer/shaders/water.frag:75`.
- `crates/plugin/src/esm/records/misc/water.rs:289-292`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
1. "Compass-style bearing" mis-describes φ = β + 90°, which is a wind-FROM bearing.
2. A test comment says a +Z current "runs north"; +Z is game south.
3. The scroll doc says "layer-3" and omits the #4734 no-flow arm.
4. The `water.frag` comment still gives scroll units as world units/s; they are UV/s.
5. The `noise_wind_directions` doc states no frame, although the field now holds two.

## Suggested Fix
Correct all five, and state the frame once on the field and per game in watal.md §2.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: Doc text matches the code it describes (re-grep the cited symbols)
