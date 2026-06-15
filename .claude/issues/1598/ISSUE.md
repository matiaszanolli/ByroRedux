# #1598 — FO4-D6-LOW-01: MOVS (movable static) records parsed into index.movables but never queried

**Severity**: LOW · **Dimension**: ESM Architecture Records
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D6-LOW-01)
**Location**: `crates/plugin/src/esm/cell/mod.rs:896-912` (`movables` map); `byroredux/src/cell_loader/` (no consumer)

## Description
`parse_movs` decodes EDID/MODL/LNAM/ZNAM/DEST/VMAD into `MovableStaticRecord`; records land in `EsmCellIndex.movables`, merged across plugins and surfaced in `categories()`. No production code reads `index.movables`. MOVS REFRs still render via the MODL-only catch-all into `index.statics`, so the mesh appears, but the loop/activate-sound IDs, destruction flag, and script flag are inert.

## Evidence
`grep -rn '\.movables\|MovableStatic' byroredux/src` outside tests → nothing. Real-data: `movables=0` for vanilla `Fallout4.esm` — zero vanilla records.

## Impact
None on vanilla FO4 (count 0). On DLC/mod content, movable statics render as immobile statics — the documented design state (MOVS is "parse-only; physics runtime not wired"). Known-gap / code-quality, not a bug.

## Related
#1359 (CONT — same pattern); #588 (MOVS parser); FO4-D9-MEDIUM-02 (BSConnectPoint, same parsed-but-unconsumed shape); M28 Rapier bridge (eventual consumer).

## Suggested Fix
No urgent action. Optionally fold into #1359 so the unqueried-typed-record set (CONT, MOVS, activators/doors/npcs) is tracked as one categorised-spawn work item.

## Completeness Checks
- [ ] **SIBLING**: Folded into the categorised non-STAT spawn work item alongside CONT (#1359) and BSConnectPoint (FO4-D9-MEDIUM-02)
- [ ] **TESTS**: When consumed, a regression test pins MOVS routing through `index.movables`
