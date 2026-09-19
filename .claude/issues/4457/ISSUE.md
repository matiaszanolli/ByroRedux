# D5-06: D5-06: TPLT resolve_inherited_* hoist still not done — ten chain-walk sites; the #4086 fix added two more instead of consolidating

- **Labels**: medium,character,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4457

---

**Source**: Not numeric — docs/audits/AUDIT_CHARACTER_2026-09-11.md's structural remedy ("hoist one `resolve_inherited_stats`/`resolve_inherited_traits` pair into `spawn_placement_root`… so a sixth site cannot repeat it"), re-verified against HEAD. Filed now because the 2026-09-11b report's D5-06 never became a GitHub issue.

**Description**

`spawn_placement_root` (`byroredux/src/npc_spawn/resumable.rs:1592-1618`) still takes the raw `&NpcRecord` and hands it untouched to all four stamps. All nine previously-counted independent `resolve_inherited_*` sites remain (renumbered); since the last audit the count has GROWN: #4086's fix (`702b0b020`, 2026-09-15) added a new `resolve_inherited_field` helper with two more independent chain-walks inside `derive_stored_actor_values` (`actor_value_derive.rs:347-366`) — the fix-by-addition recurrence D5-06 predicted — and a tenth site exists on the player-inventory path (`byroredux/src/inventory.rs:289`). Full site table: npc_spawn.rs:89,168,219-220,1184; ai_package.rs:534-538; resumable.rs:461-463,673-675,1305-1306; actor_value_derive.rs:197,209,+347-366; cell_loader/references/mod.rs:683; inventory.rs:289.

**Evidence**

grep of all `resolve_inherited_{stats,traits,factions,inventory,record,ai_packages,field}` production call sites; `spawn_placement_root` body read — four stamps, one raw `npc`, zero pre-resolution. A runtime-path NPC now walks the TPLT chain ~6× per spawn, plus the new per-field walks per FO4 spawn.

**Impact**

Eighth episode of the class (#2956, #3381, #3382, #3480, #4091, #4092, #4093, #4086), each individually correct on inspection (all 27 actor_value_derive tests green, including the four regressions) — but the type system still lets a new population-boundary read take `npc.<field>` directly and compile, and contributors demonstrably keep reaching for a fresh `resolve_inherited_*` call.

**Related**

#2956, #3381, #3382, #3480, #4091, #4092, #4093, #4086 (all CLOSED, each an instance of the class); #4137 (OPEN, D5-07).

**Suggested Fix**

Resolve once in `spawn_placement_root` (stats/traits/factions via the existing helpers) and change the four stamps' signatures to accept resolved records instead of `(npc, index)`; hand `apply_ai_package_behavior` and the `build_npc_equip_state` callers the same. `resolve_inherited_field` can fold behind one resolved-record type, making "which record shape do I have" a compile-time question.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D5-06, /audit-character 2026-09-19, HEAD `479163836`).*