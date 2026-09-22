# CHAR-2026-09-21-D5-01: Doc rot — four CHARAL sites still say no stat-bearing player actor exists, after #4458 made one

**Severity**: MEDIUM
**Dimension**: Coverage & Doctrine
**Game**: all (FO4 + FO3/FNV most directly)

## Description

`eb3784309` (#4458) wired a stat-bearing player, but none of the CHARAL prose that states the opposite was updated. The two FO4 capture caveats are exactly what a contributor fixing CHAR-2026-09-21-D4-01 would read: they say the player formulas have nowhere to apply and application is deferred, while the code has already applied the NPC path instead. `mod_docstring_indexes_every_sub_module` checks module names only, so nothing catches this.

## Evidence

Verified at HEAD `ee6d3fb39`, all four sites still present, unchanged since `eb3784309` (2026-09-19):
- `docs/engine/charal.md` §7: "No player chargen yet. There is still no stat-bearing player-actor entity (`scene.rs`'s `player_entity` is an `AnimationPlayer`)".
- `docs/engine/charal-fo4-ruleset.md`, Health caveat 2: "No player-actor entity yet ... application deferred"; and the AP "Application caveat" making the same claim.
- `crates/scripting/src/condition.rs`, `GetActorValue`: "NPCs bake them, the player isn't modelled yet".
- `docs/feature-matrix.md`'s CHARAL table has an NPC-population row and no player-seed row.

The player seed has existed since 2026-09-19 (`eb3784309`); all four sites predate it and were not touched by it.

## Impact

- The capture documents, the authority for this layer, misstate the player's state in a way that steers the next change away from the real defect (CHAR-2026-09-21-D4-01).
- The feature matrix gives no signal that a player seed exists, or which games get it.

## Related

CHAR-2026-09-21-D4-01, #4458; sibling doc-rot clusters #4459, #4460 (still open, same class of drift).

## Suggested Fix

Rewrite the three doc sites to say the player carries a seed from the base Player `NPC_` via `derive_npc_actor_values` (#4458), and that the player-only formulas are not yet evaluated for it (CHAR-2026-09-21-D4-01). Fix the `condition.rs` comment. Add a "Player actor-value seed" row to feature-matrix's CHARAL table (FO3/FNV/FO4/Skyrim yes, Oblivion no, FO76/Starfield partial).

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D5-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
