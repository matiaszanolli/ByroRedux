# GAME-D2-2026-09-21-04: The theft rule compares ownership to PlayerRef 0x14 (never authored), ignores cell ownership, and the player carries no base factions

**Issue**: #4699
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 2 — Containers & Loot
**Location**: `byroredux/src/inventory.rs` (`transfer_is_theft`, `attach_to_player`); `byroredux/src/cell_loader/references/synth_child.rs` (Owned stamped from REFR XOWN only); `crates/core/src/ecs/components/owned.rs`

## Description
Theft rule compares owner to `SceneAliasCandidate.reference_form_id` (0x14), but vanilla authors player ownership as NPC_ 0x7. `CellData.ownership` (cell-level fallback) is never stamped. `attach_to_player` never seeds `FactionRanks`.

## Evidence
Census: zero XOWN=0x14 refs across 5 masters; dozens of XOWN=0x7. Thousands of only-cell-owned refs per master never flagged. Player 0x7 authors 22 factions on FNV, none stamped.

## Impact
Looting the player's own property says "Stolen"; looting a cell-owned shop says "Took" (never theft).

## Related
#692; GAME-D5-2026-09-21-01 (#4694, same 0x14 confusion); CHAR-D4-2026-09-21-03 (concurrent /audit-character).

## Suggested Fix
Compare against player base 0x7. Stamp `Owned` from `CellData.ownership` when REFR has none. Seed `FactionRanks` in `attach_to_player`.
