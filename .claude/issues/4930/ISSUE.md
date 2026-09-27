# EXT-D5-2026-09-27-05: The BGSM flow-map UV offset kept the pre-#4728 sign and grows without bound with uptime

**Issue**: #4930
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,water,shaders

**Severity**: LOW (not measured on data)
**Dimension**: WATAL contract × renderer Dim 8
**Tier Violated**: no-leak
**Game Affected**: FO4/Starfield unplaced mesh water with a flow texture
**Status**: NEW
**Location**:
`crates/renderer/shaders/water.frag:697-701,790`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Suggested Fix
Subtract the offset, switch to a dual-phase flow-map blend, and extend the #4728 source guard.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix
