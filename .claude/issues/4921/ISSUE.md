# EXT-D3-2026-09-27-05: The authored-model tier is rigid — no `WindField` or interaction-field consumer, and §12.12 does not say so

**Issue**: #4921
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,enhancement

**Severity**: LOW (feature gap)
**Dimension**: Ground-cover pipeline
**Tier Violated**: n/a
**Game Affected**: all with authored cover
**Status**: NEW
**Location**:
`groundcover_models.comp:398-472`; `docs/engine/exal-groundcover.md:1601-1633`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
Authored clumps are static next to swaying blades and SpeedTree crowns. Motion vectors are consistent with that.

## Suggested Fix
Record the decision in §12.12. If sway is wanted, apply the §8 bend per instance, using previous-frame wind for motion vectors.

## Related
#4729

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
