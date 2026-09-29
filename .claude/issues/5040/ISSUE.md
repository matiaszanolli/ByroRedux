# #5040 — GAME-D1-2026-09-29-03: Module docs still say mid-life gear import is open

**Labels**: low,gameplay,inventory,documentation,doc-rot

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: LOW
**Dimension**: 1 — Inventory & Equipment Model

## Location
- `byroredux/src/player_body.rs` — module doc: "Two halves stay open … no new-gear import for mid-life equips"
- `byroredux/src/npc_spawn/loot_appearance.rs` — `equipment_appearance_system` doc: "Newly acquired gear … applied to mid-life equips later"

## Description
Both docs contradict code in the same file: `equipment_appearance_system` ends by calling `queue_midlife_imports`, and `0182fc5e8` landed the `GearImportLoader` path. The walk/idle half of the `player_body.rs` doc was updated; the mid-life half was not.

## Evidence
`player_body.rs` module doc still reads "Two halves stay open and are tracked in the slice doc: no new-gear import for mid-life equips …"; `loot_appearance.rs` still lists "Newly acquired gear" as deliberately out of scope.

## Impact
Doc rot only; misleads contributors about which half of P3 remains open.

## Related
GAME-D1-2026-09-29-01 (#5031), GAME-D1-2026-09-29-02 (#5034).

## Suggested Fix
Rewrite both paragraphs to describe the queue → `GearImportLoader` path, leaving player FaceGen as the one remaining open item.

Validated at HEAD 9fcfdc3fc: both doc paragraphs present verbatim in `player_body.rs` and `loot_appearance.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
