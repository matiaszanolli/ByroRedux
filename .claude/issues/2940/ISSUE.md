# CHAR-D3-01: Perks is stamped at spawn but HasPerk reads PerkList — two perk components with an empty intersection

- **Issue**: [#2940](https://github.com/matiaszanolli/ByroRedux/issues/2940)
- **Finding ID**: `CHAR-D3-01`
- **Labels**: `medium,ecs,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2940 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: FO4 (parse side), all (component side)
- **Location**: `crates/core/src/character/components.rs:29-84`; `crates/core/src/ecs/components/perk_list.rs:19-68`; writer at `byroredux/src/npc_spawn.rs:132-144`; reader at `crates/scripting/src/condition.rs:673-693`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §4.3 — "`pub struct Perks { entries: Vec<(u32 /* PERK FormID */, u8 /* rank */)> }` … The component the perk entry-point modifier pipeline iterates."
- **Description**: Two ECS components model "the perks an actor holds", and each docstring claims to be *the* perk surface. `Perks` (CHARAL, ranked) is the canonical type per `charal.md` §4.3, and is the one the spawn path writes: `spawn_npc_entity` builds it from the NPC's parsed `PRKR` pairs. `PerkList` (`Vec<FormId>`, rankless) is what the only runtime perk reader — `ConditionFunction::HasPerk`, CTDA index 449 (FO3/FNV) / 448 (Skyrim) — actually queries. A repo-wide grep confirms `PerkList` has **zero** production write sites; the codebase already knows this, and `byroredux/src/save_io/registry_completeness_tests.rs:134` records it as an accepted state with the note "do not confuse with the unrelated, already-tracked `Perks` character component" — an assertion of unrelatedness that both components' own docstrings contradict. The net effect is that the writer and the reader never meet.
- **Evidence**:
  ```rust
  // byroredux/src/npc_spawn.rs:135-143 — the only production perk writer
  world.insert(placement_root, Perks { entries: npc.perks.iter()
      .map(|&(perk_form_id, rank)| PerkRank { perk_form_id, rank }).collect() });

  // crates/scripting/src/condition.rs:679 — the only production perk reader
  let Some(perks) = world.get::<PerkList>(entity) else { return 0.0; };
  ```
  `grep -rn --include="*.rs" "PerkList" .` outside `crates/core` returns only
  `condition.rs` (read + one `#[cfg(test)]` insert) and the registry-completeness
  note. `crates/core/src/ecs/components/perk_list.rs:3-5` claims `PerkList` "is the
  ECS surface the perk system (`PERK` records, perk-grant/revoke) writes to"; nothing
  writes it.
- **Impact**: Every `HasPerk` condition takes its `return 0.0` fallback on every actor, including FO4 NPCs whose `PRKR` perks were correctly parsed and stamped. Perk-gated dialogue, quest and package CTDAs silently evaluate false. Structurally it is also a CHARAL/NIFAL canonical-type violation (`charal.md` §2 "Introduce a new canonical type only where none exists"), so the divergence will widen as either component grows a consumer.
- **Related**: #1667 (CLOSED — implemented `HasPerk` against `PerkList`), ECS-2026-08-13-04 in `docs/audits/AUDIT_ECS_2026-08-13.md` (the same "built component, no producer" shape for `FactionReputation`), #1835 (CLOSED — the save-registry guard that first documented `PerkList`'s zero write sites)
- **Suggested Fix**: Collapse to one component. `Perks` is the canonical type per `charal.md` §4.3 and already carries rank, so repoint `HasPerk` at `Perks` (`Perks::rank(id) > 0`) and delete `PerkList`, or make `PerkList` a projection the spawn path also writes. Do not leave both with a live docstring claiming ownership.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
