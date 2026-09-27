# EXT-D2-2026-09-27-03: `EsmCellIndex::merge_from` never merges `landscape_grasses` — every production load has an empty LTEX→GRAS map

**Issue**: #4904
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,esm-plugin

**Severity**: MEDIUM. It is latent because nothing consumes the map yet (EXT-D3-01); it becomes HIGH when the authored-card wiring lands.
**Dimension**: Terrain, splatting (ESM→EXAL handoff)
**Tier Violated**: single-boundary (the authored value is dropped before the translate)
**Game Affected**: FO3, FNV, Skyrim, FO4
**Status**: NEW (gap in #4642)
**Location**:
`crates/plugin/src/esm/cell/mod.rs:1587-1597`; `byroredux/src/cell_loader/load_order.rs:560,630`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `merge_from` extends 13 of the 14 `EsmCellIndex` maps; `landscape_grasses` is the only one missing.
- The production path starts from `EsmIndex::default()` and merges every plugin, master included, so the map is empty even with a single ESM.
- Main-context re-check: the only writer is `crates/plugin/src/esm/records/parse.rs:158`.

## Evidence
A scratch probe on `FalloutNV.esm`:
- `parse_esm` alone gives `landscape_grasses=20`.
- After `EsmIndex::default().merge_from(..)` it is 0, while `landscape_textures` stays 88.
- No test goes through the merge.

## Impact
#4642's decode is dead at runtime, so `authored_grass` is empty everywhere.

## Suggested Fix
- Add `self.landscape_grasses.extend(other.landscape_grasses)`; the last writer per LTEX wins.
- Destructure `other` exhaustively so the next new field is a compile error.
- Add a load-order merge test on a synthetic LTEX+GNAM plugin.

## Related
#4642, #4413, EXT-D3-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
