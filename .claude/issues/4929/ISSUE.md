# EXT-D5-2026-09-27-04: The Rapids third normal layer uses `WaterFlow.speed` (BU/s) directly as a UV/s scroll

**Issue**: #4929
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,water,shaders

**Severity**: LOW (latent in vanilla)
**Dimension**: WATAL contract × renderer Dim 8
**Tier Violated**: no-leak
**Game Affected**: modded rapids
**Status**: NEW (present since M38)
**Location**:
`crates/renderer/shaders/water.frag:808-810`; `byroredux/src/render/water.rs:198-200,259`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
The layer runs 44× too fast (8 BU/s gives 16 UV/s where the translate gives 0.365) and ignores `scroll_c`.

## Suggested Fix
Use `normalScrollC` on this arm.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix
