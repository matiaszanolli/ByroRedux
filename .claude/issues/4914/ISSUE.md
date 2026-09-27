# EXT-D1-2026-09-27-03: `translate_weather` emits an identity `image_space` that the orchestration caller patches; the IMGS boundary is undocumented

**Issue**: #4914
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug

**Severity**: LOW
**Dimension**: EXAL boundary discipline
**Tier Violated**: single-boundary, no-leak
**Game Affected**: FO3, FNV, Skyrim, FO4
**Status**: NEW
**Location**:
`byroredux/src/env_translate.rs:1493-1495`; `byroredux/src/scene/world_setup.rs:366-374`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
Identity is also the legitimate "no IMGS" value, so a second caller would silently render ungraded exteriors. `exal.md` and `skyal.md` never mention `exterior_image_spaces`, IMSP or INAM.

## Suggested Fix
Pass the IMGS inputs into `translate_weather`, and list `exterior_image_spaces` in `exal.md` §3.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
