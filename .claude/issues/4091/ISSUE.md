# CHAR-2026-09-11-D1-01: `stamp_creature_attack` reads the shell record, bypassing the `Use Stats` TPLT resolution its three sibling stamps all perform

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4091
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4091 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: MEDIUM (downgraded from HIGH at merge — see the Impact correction below)
- **Dimension**: Ruleset Seam & CHARAL Doctrine (population boundary)
- **Game**: FO3 / FNV (the only families with a `CREA` stat model)
- **Location**: `byroredux/src/npc_spawn.rs:131-145` (definition);
  `byroredux/src/npc_spawn/resumable.rs:1424-1427` (call site)
- **Source**: Not a numeric finding — no capture-document constant is involved.
  The rule it violates is CHARAL's own, `actor_value_derive.rs:154-160`: "`TPLT`
  template inheritance is resolved first for **every** stat model (#2956, #3381,
  #3382)".

## Description

`spawn_placement_root` calls four stamps with the same raw
  shell `&NpcRecord`. Three of them resolve the TPLT chain internally before
  reading anything — `stamp_actor_values` → `derive_npc_actor_values` resolves
  both `Use Stats` and `Use Traits` (`actor_value_derive.rs:194-207`), and
  `stamp_character_components` resolves both again (`npc_spawn.rs:179-181`).
  `stamp_creature_attack` does not: it reads `npc.creature_stats` off the
  unresolved shell. `CREA.DATA` is the creature's stat block, so it rides the
  same `Use Stats` (`0x0002`) bit that already gates `derive_creature_actor_values`
  — which *is* handed the resolved record (`actor_value_derive.rs:222`). The
  result is that one entity gets its SPECIAL and Health from the resolved
  template and its attack damage from the shell.

## Impact

**Corrected at merge.** This dimension originally rated the finding
  HIGH on the strength of "a live consumer (`combat_damage_system`)". Dimension 5
  falsified that half independently (`CHAR-2026-09-11-D5-03`), and the merge
  re-verified it directly: `attack_damage` has exactly two callers, one of which
  is a test fixture (`byroredux/src/combat.rs:636`, inside `mod tests`), and the
  sole production producer is `combat_input_system` (`combat.rs:250`), whose
  `aggressor` is `world.try_resource::<PlayerEntity>()` (`combat.rs:129-131`)
  gated on `InputAction::Attack` + `PlayerMode::Character`. **Creatures never
  produce a `HitEvent`**, so no wrong number reaches gameplay today.

  What remains is real and unchanged: the stamp writes a **wrong value** into
  `CreatureAttack` for templated creatures (815/1578 FNV, 399/533 FO3 per the
  `equip.rs:473-491` census), there is **no test for `stamp_creature_attack` at
  all** (every `CreatureAttack` test in `combat.rs:743-805` inserts the component
  by hand; `save_io/round_trip_tests.rs:1105` only checks registry membership),
  and the component is save-serialised — so the wrong value persists. It becomes
  wrong gameplay the instant creature-initiated attacks are wired, which is the
  express purpose of the commit that introduced it. MEDIUM: incorrect stored
  state with no current consumer, not incorrect behaviour.

## Related

#3762 (the fix this incompletes), #3390 (the creature stat model),
  #2956 / #3381 / #3382 / #3480 (the four prior "a stamp forgot to resolve its
  TPLT chain" defects, each measured in the hundreds-to-thousands of records),
  #4086 (OPEN — the adjacent intermediate-template sentinel gap).

## Suggested Fix

Resolve the chain before reading, exactly as the siblings do
  — `let stats = resolve_inherited_stats(npc, effective_actor_level(npc), index);`
  then read `stats.creature_stats` — or, better, hoist one
  `resolve_inherited_stats` / `resolve_inherited_traits` pair into
  `spawn_placement_root` and hand every stamp the record it needs, so a fifth
  stamp cannot make this mistake a fifth time. Add the census (templated creatures
  whose shell `DATA` differs from the resolved record's) before claiming a
  magnitude, and add the missing direct test for the stamp.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix