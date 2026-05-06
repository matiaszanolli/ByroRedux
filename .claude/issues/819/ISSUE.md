# #819 — FO4-D4-NEW-07: No FO4 ESM parse-rate harness — FNV/FO3/Oblivion covered, FO4 silent

**Severity**: HIGH
**Location**: `crates/plugin/tests/parse_real_esm.rs` (FNV/FO3/Oblivion harnesses; no FO4 equivalent)
**Source**: `docs/audits/AUDIT_FO4_2026-05-04_DIM4.md`
**Created**: 2026-05-04

## Summary

No `parse_rate_fo4_esm` test. Parser handles `Fallout4.esm` cleanly
(964 cells, 31,989 statics, 2,617 SCOLs, 872 PKINs, 2,537 MSWPs,
379 TXSTs, GameKind::Fallout4) but no regression floor exists.

## Sequencing

- Land AFTER #817 (so the 5 FO4-architecture maps are visible to
  `category_breakdown()` and can be floored).
- Land BEFORE #813/#814/#815/#816 (so those land with floors).

## Floor sketch

Per-category floors (live measurements with 5% buffer):

```
cells.cells          >= 900
cells.statics        >= 30000
scols                >= 2500
packins              >= 850
material_swaps       >= 2400
texture_sets         >= 370
items                >= 3800
npcs                 >= 2800
factions             >= 660
globals              >= 1200
game_settings        >= 1900
```

Pin `idx.game == GameKind::Fallout4`.

## How to fix

```
/fix-issue 819
```
