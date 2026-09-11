# CHAR-2026-09-11-D5-01: the spawn tail resolves the `Use Traits` chain for `Background` and the Skyrim pools, and ignores it for every mesh that depends on race

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4092
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4092 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: MEDIUM
- **Dimension**: 5 — Population boundary
- **Game**: all (measurable on Skyrim / FNV / FO3)
- **Location**: `byroredux/src/npc_spawn.rs:836`, `:853`, `:969`; `byroredux/src/cell_loader/references/mod.rs:636`; `byroredux/src/npc_spawn/resumable.rs:434`, `:616`, `:1168`
- **Source**: `crates/plugin/src/equip.rs:429-438` — `resolve_inherited_traits` is defined as "the NPC record that should supply **race** (and other 'traits' fields)", gated on `TEMPLATE_FLAG_USE_TRAITS`. `crates/plugin/src/esm/records/actor_value_derive.rs:196-207` (`0dcb5cf0` / #3480) establishes that reading race off anything but the traits chain is a defect.

## Description

`0dcb5cf0` fixed *one* of the two race readers in the spawn tail. `stamp_character_components` writes `Background { race_form_id: traits_npc.race_form_id }` (`npc_spawn.rs:204-207`) and `derive_skyrim_actor_values` keys the race lookup on the traits chain — but four sibling sites in the same spawn still read the shell's raw `npc.race_form_id`: the `RACE.WNAM` default-skin lookup (`:836`), both `resolve_armor_meshes` race-match arguments (`:853`, `:969`), and the `races.get(&npc.race_form_id)` that supplies the head/body `RaceRecord` handed to `NpcSpawnJob::runtime` (`references/mod.rs:636`). `Gender::from_acbs_flags(npc.acbs_flags)` at `resumable.rs:434/616/1168` is the same shape for the ACBS Female bit — flagged rather than asserted, because no in-repo source states which template flag governs gender.

## Impact

an `NPC_` with `Use Traits` set whose own `RNAM` differs from its template's gets the wrong race's default-skin `ARMO` and fails the `ARMA.RNAM` race match. `resolve_armor_meshes` returns `Vec::new()` on a race miss — the exact indistinguishable-from-unshipped-mesh failure `1ee804c2`/#3714 describes — so the symptom is an invisible or wrong-race body, plus a head/body `RaceRecord` from a race the actor is not. Measured divergence for this comparison on FNV/FO3 is small and already in-repo: 2/744 (FNV) and 19/337 (FO3) of `Use Traits` actors with a resolvable direct template have an own race that disagrees (`equip.rs:436-438`). The Skyrim count is **not measured** — #3480's 1,180/5,118 figure is a *different* comparison (traits-race vs stats-race), so it is an upper bound, not this finding's number. Severity is MEDIUM rather than HIGH for that reason.

## Related

#3480 (CLOSED, `0dcb5cf0`) fixed the actor-value half; #3714 (CLOSED) is the identical "race miss ⇒ empty mesh list" failure mode; `CHAR-2026-09-11-D1-01` is the same class on the stats chain.

## Suggested Fix

resolve `traits` once in `spawn_placement_root`/`NpcSpawnJob` construction and thread `traits.race_form_id` into `build_npc_equip_state`, `resolve_armor_meshes` and the `races.get` at `references/mod.rs:636`, exactly as `stamp_character_components` already does. Separately, source which flag governs the ACBS Female bit before touching gender.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix