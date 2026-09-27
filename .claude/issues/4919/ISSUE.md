# EXT-D3-2026-09-27-03: The model candidate grid restarts every 512-unit chunk with `ceil()` — density stripes along chunk borders at 80-unit spacing

**Issue**: #4919
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,shaders

**Severity**: LOW (visual)
**Dimension**: Ground-cover pipeline
**Tier Violated**: n/a
**Game Affected**: Oblivion, FO3, FNV (Skyrim/FO4 marginally)
**Status**: NEW
**Location**:
`crates/renderer/shaders/groundcover_models.comp:266-294`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `perSide = ceil(512/80) = 7`, which gives 49 candidates against 40.96 implied (+19.6%).
- Column 6's centre, 520, reflects to 504. Gaps across a border run 80, 80, 64, 48, 80.
- The result is a band of about 1.4× density every 512 units.
- `model_slab_holds_every_vanilla_candidate_grid` checks capacity only.

## Suggested Fix
Anchor candidates to a world-space lattice, and pin continuity across a chunk border.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **SPIR-V**: Edited shaders recompiled and `scripts/check-shader-artifacts.sh` is clean
- [ ] **TESTS**: A regression test pins this specific fix
