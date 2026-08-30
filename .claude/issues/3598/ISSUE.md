# #3598: OBL-D3-01: Oblivion has zero navigation data — 8,228 PGRD records are dropped and the title authors no NAVI/NAVM

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 3 (ESM Record Coverage)
**Severity**: HIGH
**Location**: `crates/plugin/src/record.rs` (`RecordType`), `crates/plugin/src/esm/records/mod.rs` (dispatch), `crates/plugin/src/esm/cell/` (CELL-child walker)

## Description

Oblivion has **zero** navigation data in the engine: all 8,228 PGRD (pathgrid) records in
`Oblivion.esm` are dropped, and Oblivion authors no NAVI/NAVM — the formats the engine does
parse.

## Evidence

Full walk of `Oblivion.esm` (277,504,985 B, HEDR 1.0, `GameKind::Oblivion`, 1,252,095
records, 63 distinct record types, **0 walker errors**, peak RSS 273 MB):

```
PGRD = 8,228        NAVI = 0        NAVM = 0
PACK = 7,209        REFR = 1,025,617   CELL = 35,494
```

Code side, verified 2026-08-30:

```
$ grep -rn "PGRD" crates/plugin/src byroredux/src
(no hits)
```

PGRD is not in `RecordType`, not in any dispatch table, not in the CELL walker.
`NaviRecord` / `NavmRecord` **are** parsed (`records/index.rs`) — which is exactly why
FO3/FNV/Skyrim/FO4 have navigation and Oblivion does not.

## Impact

Not theoretical: **7,209 PACK records parse successfully** and the sandbox/travel procedures
have no graph to path on. Oblivion is the only supported title in this state — every other
game's nav format is handled. Any AI-movement work on Oblivion is blocked at the data layer,
not the behaviour layer.

## Suggested Fix

PGRD is a flat record — `DATA` (u16 point count), `PGRP` (16-byte points), `PGRR`/`PGRI`/`PGRL`
link tables — attached to the CELL group the same way LAND is. It slots into the same
CELL-child walker that already handles LAND.

## Related

Sibling nav formats: `NaviRecord` / `NavmRecord` in `crates/plugin/src/esm/records/index.rs`.

## Completeness Checks
- [ ] **SIBLING**: the CELL-child walker's LAND arm is the model — confirm PGRD lands in the same GRUP-type position for both interior and exterior children
- [ ] **TESTS**: a regression test pins PGRD point + link decode against a real `Oblivion.esm` cell (the record count 8,228 is the corpus-level assertion)
