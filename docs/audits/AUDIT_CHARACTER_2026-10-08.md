**HEAD**: 00f580e09 · **Baseline**: [`AUDIT_CHARACTER_2026-10-05.md`](AUDIT_CHARACTER_2026-10-05.md) (HEAD `a2c24b16e`) · **Audited**: Dim 1 (Ruleset Seam), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2 (Derived Formulas), Dim 3 (Progression & Pools)

# Character / CHARAL Audit — 2026-10-08

This is `/audit-character` with the default scope (`--depth deep`), run solo as part of `/audit-suite --preset comprehensive`. No sub-agents were used and no engine was launched.

The delta `a2c24b16e..00f580e09` is 116 commits. Eight of them touch CHARAL paths:
- `2464a52d7` (#5239): a player `setav` or SDK `SetBase` on Health/AP now goes into the modifier layer.
- `9813af435` (#4415 scope 4): the racial spell pair `RaceSpells` and the `AddRaceSpells`/`RemoveRaceSpells` lowering.
- `7c7711cff` (#5079): the shared `is_child_race` helper.
- `8c925ec54` (#5095): a PNAM head-part fallback when a pre-baked FaceGen mesh is missing.
- `bd7aa2d13` (#5093): `actor/mod.rs` split into one file per record.
- `cff4d8934` (#5242): feature-matrix prose.
- `81fbf9188` (#5241): the scheduler comment.
- `00f580e09`: Eat/Sleep.

The other population-path commits do not write actor values: the corpse-set commits `d4e8c31be`/`2d47bf7f1`, the gear reconcile #5266/#5268/#5269, and Story Manager.

## Tests recorded (read-only)

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect -- character combat stealth` | 166 passed, 0 failed. That is 2 more than the baseline's 164; the 2 new tests are #5239's in `ruleset.rs`. |
| `cargo test -p byroredux-plugin --lib -- actor_value_derive consumables::tests::fn586 equip` | 103 passed, 1 ignored |
| `byroredux` bin (rustc 1.96.0) with filters `resolve_inherited obscript_dialect player_character_template attach_to_player vitals_snapshot vital melee_damage player_npc_form_id modav setav player_derived reset_player_factions npc_spawn is_child_race` | 141 passed, 9 ignored |
| `byroredux` bin `extensions::tests` (the SDK `SetBase` path) | 79 passed |
| Real-master probe outside the tree: `/mnt/data/tmp/char_probe_magic`, which uses `byroredux_scripting::magic::canonical_spell` on `FalloutNV.esm` and `Fallout3.esm` | Counts the constant spells that modify Health or ActionPoints, and the Player record's own spell set. Numbers are under D4-01. |

## Executive Summary

**2 new findings: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 2 LOW. 0 regressions.**

- **Fixed since the baseline, verified on HEAD:** #5239, #5241, #5242, #4415 and #5079.
- **D4-01:** the #5239 fix works, but the semantics it picked have two problems:
  - `set_permanent` *replaces* the permanent-modifier layer. That is the same layer constant spells (abilities) write into, so a player `setav Health` throws away any active ability's contribution, and a later `RemoveSpell` subtracts it a second time.
  - Its only cited source is an **Oblivion** CS-wiki note copied into the GECK wiki. It uses `fPCBaseHealthMult` and "40 Endurance", which is TES4 attribute scale. The fix also applies to FO4, where the CK documents `SetValue` as "Sets the base value … Any modifiers are left intact."
  - On FO3/FNV this changes no value today: nothing applies a Health/AP ability to the player yet.
- **D4-02:** the #4415 `AddRaceSpells`/`RemoveRaceSpells` pair always targets the player, but the player never carries the `RaceSpells` component it reads. Both are silent no-ops in production; only the unit test, which inserts the component by hand, exercises them. ROADMAP says MQ101 `Fragment_11` "now lowers onto the player's RACE SPLO set".
- **Constants:** no Dim 2/3 code moved. This is the sixth run in a row with no constant drift. The three open baseline findings are unchanged and cited, not re-filed: #5338, #5340, #5342.
- **Eat/Sleep (`00f580e09`)** touches no CHARAL surface: no needs, afflictions, regen or `ActorValues` writes. The only new condition function is `GetButtonPressed` (a constant −1). The mover and lock-order bugs in that commit belong to the ECS and concurrency audits.

## Constant Verification Table

Dim 2 and Dim 3 have no code changes since 2026-10-05, so every verdict in the baseline's tables carries forward unchanged, including the re-graded FO3/FNV NPC Health rows. Two rows stay open:
- The END input is FAIL for actors with auto-calc off (#5338).
- The level-term binding is UNSOURCED (#5340).

No new numeric row was introduced this run.

| Item | Code | Document | Verdict |
|---|---|---|---|
| Player-pool `SetBase` semantics, FO3/FNV (#5239, behaviour rather than a coefficient) | Total = formula base + value; `set_permanent` replaces the permanent layer | The GECK *SetActorValue* note is a verbatim copy of the CS wiki's Oblivion note: "Base health is determined by (Endurance * fPCBaseHealthMult) … 40 Endurance". No Fallout-specific source. | **Oblivion-sourced only** (D4-01) |
| Player-pool `SetBase` semantics, FO4 | Same as above | falloutck *SetValue - ObjectReference*: "Sets the base value … Any modifiers are left intact." | **Contradicted** (D4-01) |

## Coverage Matrix

Re-derived from the `profile.rs` arms, workspace greps and scheduler registrations. It is unchanged from the baseline.

| Family | Ruleset impl | Wired | Derived rows | Leveling model | NPC seed | Player seed | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | 8 | ✓ (model only) | ✓ auto-calc; non-auto-calc NPCs get the class SPECIAL (#5338) | ✓ HP/AP refreshed per frame; `setav` routed to the modifier layer (#5239; D4-01) | ✗ registered, inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | 8 | ✓ | as FO3 | as FO3 | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | 3 | ✓ uncapped | ✓ stored `PRPS` + `DNAM` | ✓ HP/AP refreshed per frame (`SetBase` semantics contradicted, D4-01) | ✗ | ✗ |
| Skyrim SE | ✓ `skyrim_ruleset` | ✓ `RulesetBuilder::Skyrim` | 2 | ✓ + skill XP | ✓ race + offset (class/level term deferred, disclosed) | ✓ race + offset; `RaceSpells` absent on the player (D4-02) | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` (deliberate, pinned) | tests only | ✓ | ✗ | ✗ | ✗ | ✗ |
| FO76 | ✗ | ✗ | ✗ | ✗ | ✗ `NpcStatModel::None` (#4453) | ✗ | ✗ | ✗ |
| Starfield | ✗ | ✗ | ✗ | ✗ | ✗ `NpcStatModel::None` (#4453) | ✗ | ✗ | ✗ |

## Findings

**LOW (2)**
- D4-01: #5239's player-pool `SetBase` redirect wipes the constant-spell layer, and its semantics rest on an Oblivion note (contradicted for FO4).
- D4-02: `AddRaceSpells`/`RemoveRaceSpells` target the player, who never carries `RaceSpells`, so both are silent no-ops.

---

### CHAR-2026-10-08-D4-01: #5239 sends a player `SetBase` on Health/AP to `set_permanent`, which **replaces** the permanent-modifier layer that constant spells also write to. The only source behind the semantics is an Oblivion CS-wiki note, and the FO4 CK says `SetValue` sets the base and leaves modifiers intact
- **Severity**: LOW. On FO3/FNV it changes no value today, because nothing applies a Health/AP ability to the player yet. It is a latent semantic defect plus a sourcing gap.
- **Dimension**: Population Boundary (setav/SDK write contract), with a doc-sourcing part in Coverage & Doctrine
- **Game**: fo3, fnv, fo4
- **Location**:
  - `byroredux/src/commands/actor_value.rs:82-107`: `edit_av`, where `player_pool` leads to `avs.set_permanent(av, value)`.
  - `byroredux/src/extensions/commands.rs:414-455`: `apply_pending_actor_value_writes`, the `ActorValueOperation::SetBase` arm.
  - `crates/core/src/ecs/components/actor_values.rs:152`: `set_permanent` does `permanent_mod = value`.
  - `crates/core/src/character/ruleset.rs:174-186`: the `is_player_derived_pool` doc.
  - `docs/engine/charal.md` §6, around line 414: "the GECK documents that a player `SetActorValue` never modifies base health".
- **Status**: NEW. It was introduced by the #5239 fix (`2464a52d7`); #5239 itself is closed and its stated defect is fixed.
- **Source**:
  - (1) `/mnt/data/src/reference/geck-uesp-wiki/(main)/SetActorValue.wiki` carries `CSWikiPage = SetActorValue`. Its note ("Base health is determined by (Endurance * fPCBaseHealthMult). For example, a level 2 player with 40 Endurance will have 80 base health … 180 total health - 80 from base health, and 100 for the rest") is word-for-word the note in `/mnt/data/src/reference/cs-uesp-wiki/(main)/SetActorValue.wiki`. A 40 Endurance and an END×2 base are Oblivion figures; FO3/FNV END runs 1–10, and their player Health is `fAVDHealth*`-driven.
  - (2) `/mnt/data/src/reference/falloutck-uesp-wiki/(main)/SetValue - ObjectReference.wiki`: "Sets the base value specified Actor Value on the Object Reference to the passed-in value. **Any modifiers are left intact.**" Its own example is `Game.GetPlayer().SetValue(HealthAV, 50)`.
  - (3) `crates/scripting/src/magic.rs` module doc, citing the CK *Actor Value* page: the permanent modifier is "adjusted when Abilities or Enchantments change the value". `apply_modifiers` writes constant spells there with `mod_permanent(±amount)`.
- **Description**:
  - The per-frame `refresh_player_only_bases` owns the player's Health/AP **base**. #5239 therefore redirects a deliberate `SetBase` into the permanent-modifier layer.
  - That layer is not reserved for the `SetBase` value: the magic runtime adds and removes every constant spell's Health/AP amount there. `set_permanent` overwrites the layer outright. A player who carries a +30 Health ability (permanent = 30) and runs `setav Health 100` ends with permanent = 100; the ability's 30 is gone.
  - When that ability is later removed, `remove_spell` applies `mod_permanent(-30)`, leaving 70. The result is a net loss of 30 that no source describes.
  - The FO4 CK names "modifiers left intact" as the defining property of `SetValue`, and this implementation violates it on every game. The only citation for "base untouched, value on top" is the TES4 note; FO3/FNV and FO4 have no source of their own.
- **Evidence**:
  - `actor_values.rs:152`: `self.values.entry(avif_form_id).or_default().permanent_mod = value;`, versus `magic.rs::apply_modifiers`, which does `values.mod_permanent(modifier.actor_value, modifier.amount * direction)`.
  - The out-of-tree probe found that vanilla authors constant spells that modify exactly these pools: **FNV 14, FO3 9**. Examples: `PerkLifeGiver1/2/3` Health +30/+60/+90, `PerkActionBoy*`/`PerkActionGirl*` ActionPoints +15/+25/+30, `MS03PerkSurvival*` Health +5/+10/+15, `PerkMathWrath` ActionPoints +200, `DLC03PuppiesEffect` Health +500, `CalmHeartEffect` Health +50.
  - The FNV and FO3 Player `NPC_` records author 0 spells. No perk-ability runtime exists, and FO3/FNV `SCPT` is not executed (ROADMAP). On vanilla FO3/FNV, then, the overwritten layer is 0 today. The defect becomes live once perk abilities or FO3/FNV scripted `AddSpell` reach the player, or now through an FO4 Papyrus `AddSpell`; the FO4 reach was not measured.
- **Impact**:
  - A console `setav` or an SDK mod's `SetBase` on the player's Health/AP silently discards active ability bonuses.
  - Removing the ability afterwards lowers the pool below the value that was set.
  - FO4 behaviour is the opposite of its documentation.
- **Related**: #5239 (closed; this is its follow-up), #4415 (closed; the magic layer), #5039 (the per-frame refresh).
- **Suggested Fix**:
  - Give the redirect its own layer instead of reusing `permanent_mod`. For example, add a `set_override`/`base_delta` field on `ActorValue` that `current()` adds and that `refresh_player_only_bases` never touches. Constant spells then keep `permanent_mod` to themselves.
  - Alternatively, keep `permanent_mod` but write `value + Σ(active constant amounts)`.
  - Either way, re-source the semantics per game: mark the FO3/FNV behaviour as Oblivion-derived in `charal.md` §6, and for FO4 either follow the CK (base set, refresh suspended for that key) or record the conflict.

### CHAR-2026-10-08-D4-02: The #4415 racial spell pair always targets the player, but the player never receives `RaceSpells`, so `AddRaceSpells`/`RemoveRaceSpells` are silent no-ops in production
- **Severity**: LOW. On vanilla MQ101 the visible state is the same today: the player's `SpellList` already holds the racial spells from the template, so the `AddRaceSpells` call would have been idempotent anyway. It becomes wrong for any `RemoveRaceSpells`, or once a race change lands. This is the "writers vs stamps" class of #4458 (MEDIUM), graded lower because no value changes yet.
- **Dimension**: Population Boundary (writers vs stamps)
- **Game**: skyrim (MQ101); all families by construction
- **Location**:
  - `crates/scripting/src/translate/effects.rs:1444-1468`: `lower_race_spells` always emits `ActorRef::Player`.
  - `crates/scripting/src/magic.rs:213-240`: `add_race_spells`/`remove_race_spells` read `world.get::<RaceSpells>(actor)…unwrap_or_default()`.
  - `byroredux/src/inventory.rs:742`: `attach_to_player` inserts `SpellList(character.spells)` but no `RaceSpells`.
  - `byroredux/src/npc_spawn/resumable/mod.rs:369-371`: `player_body` placement roots return before `stamp_spell_list`, the only `RaceSpells` insert (`npc_spawn.rs:156`).
- **Status**: NEW. It was introduced by `9813af435` (#4415 scope 4, closed).
- **Description**:
  - `RaceSpells` is stamped only by `stamp_spell_list`, which runs for NPC placement roots and is skipped for the player body.
  - The player's own seed (`build_player_character_template` → `attach_to_player`) resolves `resolve_actor_spells` (own + racial, merged) and never computes the racial subset separately.
  - The lowering sends every `AddRaceSpells()`/`RemoveRaceSpells()` to the player. Both functions find no `RaceSpells` on the player, return `false`, and the fragment arm logs only at `debug!` ("changed nothing").
  - `race_spell_pair_reapplies_and_clears_only_the_racial_set` inserts `RaceSpells` by hand on a bare entity, which hides the gap. That is the #4458 pattern.
- **Evidence**:
  - `grep -rn RaceSpells byroredux/src` finds a single insert, at `npc_spawn.rs:156`, inside `stamp_spell_list`. `spawn_placement_root` returns at `if player_body { return (placement_root, resolved); }` before that call.
  - ROADMAP ("MQ101 residue"): "`Fragment_11`'s `AddRaceSpells()` now lowers onto the player's RACE `SPLO` set (#4415, `9813af435`); no live MQ101 replay has re-checked it."
- **Impact**:
  - `RemoveRaceSpells` on the player never removes racial abilities.
  - `AddRaceSpells` never re-applies them after a removal.
  - The #4415 closure claim and the ROADMAP line describe behaviour that production never performs.
- **Related**: #4415 (closed), #4458 (the same writers-vs-stamps class). Cross-audit: `/audit-scripting` owns the lowering.
- **Suggested Fix**:
  1. In `build_player_character_template`, also compute `resolve_racial_spells(&resolved, index)`.
  2. Have `attach_to_player` insert `RaceSpells` beside `SpellList`.
  3. Add a test that runs `AddRaceSpells`/`RemoveRaceSpells` against a player built by `attach_to_player`, not one with hand-inserted components.

## Observations (not findings)

- **`#5095` head-part fallback reads the shell's PNAM.** `prebaked_head_fallback_paths(npc, …)` (`npc_spawn/resumable/prebaked.rs:163,174`) reads `npc.face_morphs` from the spawning shell, while race, gender and skeleton two lines above read `resolved.r#traits`. I found no source that `Use Traits` governs PNAM: xEdit `wbDefinitionsTES5.pas` puts no template gate on the `NPC_` `PNAM` array (fetched this run). It is recorded here, not filed.
- **Runtime-path gender still reads the shell** (`runtime.rs`: `Gender::from_acbs_flags(npc.acbs_flags)`), while the prebaked path reads `traits.acbs_flags`. #4092 flagged this and deferred it until a source settles which flag governs the ACBS Female bit.
- **`is_child_race` (#5079)** keeps a `GameKind` gate. It is an appearance translation (body and walk variants), not a stat seam, so it is outside CHARAL's doctrine.
- **`docs/engine/debug-cli.md:616`** still lists `setav` among "Future Bethesda-console additions". The command exists. This is tooling doc rot (`/audit-tooling`).
- **#5239 also changes kill semantics.** An SDK `SetBase(Health, 0)` on the player used to trip `commit_actor_value_deaths` through a base of 0. It now composes to the formula base plus 0, so the player lives. That matches the cited Oblivion note and is consistent with D4-01's framing.

## Regression checks (closed since baseline)

| Issue | Verified |
|---|---|
| #5239 | Both write sites route through `is_player_derived_pool`. The redirect facts are read before the `ActorValues` write in canonical order. Console test `setav_on_player_derived_pool_survives_the_refresh` and the core tests are green. The semantics are D4-01. |
| #5241 | `81fbf9188`: the "#2153 3-deep hold stack" comment heads `pool_regen_tick_system` again, and `player_derived_stats_system` has its own comment (`boot/schedule/update.rs`). |
| #5242 | The `feature-matrix.md` prose says the stored mechanism populates FO4 only, and that FO76/Starfield pin `NpcStatModel::None` (#4453). It matches the table. |
| #4415 | `9813af435`: the racial pair resolves through the `Use Traits` race (`racial_spells`). `apply_constant_modifiers` is still `mod_permanent(±amount)`, symmetric and never touching base. The player-side gap is D4-02. |
| #5079 | One `is_child_race`, called from `runtime.rs` and `player_body.rs`, and pinned. |
| #4092/#4457/#5047 class | The production `resolve_inherited_*` calls are still exactly 2 (`cell_loader/references/mod.rs`, `player_body.rs`). `effective_actor_level` still has one definition (`actor/mod.rs:149`, `.max(0)`) after the #5093 split. |

## Known-Open Register (re-confirmed on HEAD; none re-filed)

1. FNV/FO3 tag-skill per-level formula: still undocumented and absent. CLAS SPECIAL is still read from `ATTR`.
2. FO3/FNV AP NPC scope (#2937, closed as documented): still `.player_only()`. Melee is still CONFLICTED and deliberately kept actor-general.
3. VATS runtime: absent; only the AP formulas exist.
4. Regen, affliction and level-up are unchanged:
   - `PoolRegenConfig` has 4 inserts, all in `regen.rs` tests.
   - `pool_regen_tick_system` is registered but inert.
   - `affliction_tick_system` is unregistered.
   - `level_cap()` has test consumers only.
   - `FactionReputation` has no production insert.
5. Oblivion `RulesetBuilder::None` is deliberate. FO76/Starfield have captures but no builders.
6. #4137 (OPEN): six `template_flags` bits have no consumer.
7. #4232 (OPEN): `effective_actor_level` returns `npc.level.max(0)` verbatim.
8. #5338 (OPEN, baseline D4-01): FO3/FNV non-auto-calc NPCs still get the class SPECIAL. The `NPC_ DATA` arm (`actor/npc.rs:609-617`) decodes only the i32.
9. #5340 (OPEN, baseline D1-01): `NpcHealthCurve::evaluate` still applies `self.offset` to the level term.
10. #5342 (OPEN, baseline D5-01): the `actor_value_derive.rs` header still cites the player Health formulas, "15/15" is still claimed, and the ROADMAP "62 constants … zero mismatch" sentence is unchanged.
11. `CharacterLevel` is still unsaved.

## Summary

| Severity | NEW | Already tracked (cited) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 2 (#5338, #4232) |
| LOW | 2 | 3 (#5340, #5342, #4137) |

## Cross-Audit Routing

- D4-01: CHARAL, plus `/audit-tooling` for the SDK `SetBase` contract.
- D4-02: CHARAL, plus `/audit-scripting`, which owns the `lower_race_spells` lowering.
- The Eat/Sleep mover and lock-order bugs belong to `/audit-ecs` and `/audit-concurrency`, which have already filed them.

Suggested next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-08.md`. Use the domain label `character`; add `game:fo3`, `game:fnv` and `game:fo4` on D4-01, and `game:skyrim` plus `scripting` on D4-02.
