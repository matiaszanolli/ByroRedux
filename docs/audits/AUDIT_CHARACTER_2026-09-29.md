**HEAD**: `9fcfdc3fc` · **Baseline**: [`AUDIT_CHARACTER_2026-09-21.md`](AUDIT_CHARACTER_2026-09-21.md) (HEAD `f97775ca8`) · **Audited**: Dim 1 (Ruleset Seam), Dim 3 (Progression & Pools), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2 (Derived Formulas; constants byte-identical, the only commit is a doc edit)

# Character / CHARAL Audit — 2026-09-29

This run is `/audit-character` with the default scope and `--depth deep`, as one leg of `/audit-suite --preset
comprehensive`. The five dimensions were analysed one at a time, with no sub-agents. The delta is `f97775ca8..HEAD`: 312 commits, about 45 in scope. The in-scope commits that matter:
- The fix wave for the baseline's eight findings:
  - `91fad8be1` #4674
  - `65717ab58` #4675
  - `d55041d5e` #4676
  - `facd9964d` #4677
  - `7b0b84c2c` #4678
  - `e6ab4afe2` #4679
  - `ddd0f7f4f` #4680
  - `7778528e2` #4681
- The 09-19 carry-overs:
  - `c4f30cbde` #4453
  - `dfb5b57e8` #4456
  - `3748f4cb1` #4457
  - `a35778d2e` #4454/#4455/#4459–#4463
- `cd4fc019a` #4415: the magic runtime. Constant-spell permanent modifiers are applied at spawn and on `AddSpell`.
- `ed52fa5e4` #4699: player factions.
- `e52d4a8a1` #4812: TPLT outfit and level.
- `a070baaad`, `0182fc5e8` and `db8351587`: the player body assembled through `NpcSpawnJob`.

## Tests recorded (read-only; nothing launched, no `--ignored`, no GPU)

| Command | Result |
|---|---|
| `cargo test -j 4 -p byroredux-core --features inspect character` | 124 passed, 0 failed. That is +3 vs the baseline's 121: the #4456, #4453 and #4459 pins. |
| `cargo test -j 4 -p byroredux-core --features inspect -- character combat stealth` | 156 passed (124 + 32 combat/stealth, the same 32 as the baseline) |
| `cargo test -j 4 -p byroredux-plugin --lib -- actor_value_derive consumables::tests::fn586 equip` | 103 passed, 1 ignored |
| `cargo test -j 4 -p byroredux --bin byroredux -- resolve_inherited obscript_dialect player_character_template attach_to_player vitals_snapshot vital melee_damage player_npc_form_id hud` | 19 passed. Includes `resolve_inherited_call_sites_are_enumerated_and_pinned`. |
| Real-master probe | Read-only. A scratch crate outside the repo (`/mnt/data/tmp/char_probe`, path-dep on `byroredux-plugin`) calls `parse_esm`, `ResolvedNpc::resolve`, `resolve_actor_spells` and `derive_resolved_actor_values`. It ran on `FalloutNV.esm`, `Fallout3.esm`, `Fallout4.esm` and `Skyrim.esm` (SE). Separately, a Python AVIF EDID scan covered `Starfield.esm` and `SeventySix.esm`. |

## Executive Summary

**5 new findings: 0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW. 0 regressions.**
- All 8 baseline findings are closed and verified fixed on HEAD: #4674–#4681.
- All 12 carry-overs from the 09-19 report are closed and verified: #4452–#4457 and #4459–#4463.
- 3 still-open issues were re-confirmed and cited, not re-filed: #4137, #4232 and #4415.

**Formula constants: no drift, third run running.**
- `derived.rs`, `tes.rs`, `skyrim.rs`, `resistance.rs`, `combat.rs`, `stealth.rs`, `leveling.rs`, `reputation.rs` and `components.rs` have an empty diff since the baseline.
- The `fallout.rs` diff is 4 lines of module doc (#4681).
- So the baseline's Constant Verification Table stands for FO3, FNV, FO4, Oblivion and Skyrim.
- FO76 and Starfield still have captures but no builders.

**The defects are in how values are composed after population, not in the formulas.** Since the baseline, two writers put
values into `ActorValues` that CHARAL's design says should be computed on demand:

1. **#4674's fix materialises the player's `PlayerOnly` derived stats as base values at stamping (D1-01).**
   - FO4 85/70 and FNV AP 80 are now correct at spawn.
   - But the values are frozen. Any later Endurance, Agility or level change leaves HP/AP stale, and so does a constant spell applied at the same attach. This contradicts `charal.md` §6 and the FO4 capture's "rescales dynamically".
2. **The magic runtime's constant modifiers (#4415) create carried `{base 0, +mod}` entries for formula-derived AVs (D4-01).**
   - `GetActorValue` then returns only the modifier.
   - `melee_damage_charal_bonus` returns only the formula.
   - The probe finds it on vanilla content: 66 FNV NPCs with Finesse (CritChance), 12 FNV and 74 FO3 ghouls (RadResist), FNV's Motor-Runner (MeleeDamage) and FO4's Strong (CarryWeight).

**Wiring state.** Regen and affliction are unchanged:
- `PoolRegenConfig` has zero production inserts.
- `affliction_tick_system` is unregistered.
- `level_cap()` has no production consumer.

New player columns: the player now carries `CharacterLevel`, `Background`, `FactionRanks`, `SpellList` and permanent spell modifiers. After #4453, FO76 and Starfield have **no** player seed.

## Constant Verification Table

Doc keys: **FO4** = `charal-fo4-ruleset.md`, **FNV/FO3** = `charal-fnv-fo3-ruleset.md`, **OBL** =
`charal-oblivion-ruleset.md`, **SKY** = `charal-skyrim-ruleset.md`, **CORE** = `charal.md`, **F76** = `charal-fo76-ruleset.md`,
**SF** = `charal-starfield-ruleset.md`.

### Derived-stat formulas (Dim 2): code unchanged, baseline verdicts carried forward

| Formula / constant | Code | Document | Verdict |
|---|---|---|---|
| FO4 Health `floor(77.5+4.5·END+2.5·L+0.5·L·END)` player-only | `fallout4_ruleset` | FO4:72,87 | PASS (the only cross-term row) |
| FO4 AP `60+10·AGI` player-only · CW `200+10·STR` | `fallout4_ruleset` | FO4:132; FNV/FO3 CW table | PASS ×2 |
| FO3 Health `90+20·END+10·L` / AP `65+2·AGI` cap 85 (player-only) | `fallout3_ruleset` | FNV/FO3 derived table | PASS ×2 |
| FNV Health `95+20·END+5·L` / AP `65+3·AGI` cap 95 (player-only) | `falloutnv_ruleset` | FNV/FO3 derived table | PASS ×2 |
| FO3/FNV CW `150+10·STR` · Melee `0.5·STR` · Crit `1.0·Luck` cap 10 · Unarmed `ceil(0.5+0.05·U)` | `fallout3_ruleset` shared rows | FNV/FO3 derived table | PASS ×4 (scope unsourced, pinned, #4450) |
| RadResist `(END−1)·2` cap 85 · PoisonResist `(END−1)·5` uncapped · `damage_multiplier` | `resistance.rs` | FNV/FO3 derived table | PASS ×3 |
| Oblivion Health `2·END` · Magicka `2·INT` · Fatigue 4×1.0 · Armor `0.35+0.0065·skill` | `tes.rs` | CORE §5; OBL damage formula | PASS ×4 |
| Skyrim Light Armor `1+0.004·skill` (player-only mult) · CW `250+0.5·base Stamina` | `skyrim_ruleset` | SKY | PASS ×2 |
| `combat.rs` / `stealth.rs` coefficients | unchanged | OBL damage formula; FNV/FO3 Sneak Detection | PASS (32 tests green) |

### Progression / pools / reputation (Dim 3)
The baseline's 60 rows (33 leveling and 27 pools/afflictions/resistance/reputation) are unchanged. The only code change is the #4456
tie-break, which is behavioural, not a constant.

### Player seed and profile rows (new this run)

| Row | Stamped / claimed value | Document value | Verdict |
|---|---|---|---|
| FO4 player Health / AP at spawn (END 1, AGI 1, L1) | 85 / 70 (real-master `#[ignore]` leg) | FO4:72,87 → 85; FO4:132 → 70 | PASS at spawn |
| FNV player AP (AGI 5) | 80 | FNV/FO3: `65+3·5` = 80 | PASS at spawn |
| FO4 player HP after an END change | unchanged (stamped base) | FO4:77-78 "rescales **dynamically** with any Endurance / level change" | **FAIL → D1-01** |
| Starfield `vital_pools` `("O2","O2")` | editor id `O2` | SF: no pool/oxygen line; `Starfield.esm` AVIF `Oxygen` = 0x2D5, no `O2` | **UNSOURCED + wrong → D1-02** |
| FO76 `vital_pools` `Health`/`ActionPoints` | resolves 0x2D4 / 0x2D5 (`SeventySix.esm`) | F76:54 (AP `60+10·AGI`); FO4 cross-game Health table | PASS |
| FO3/FNV body-condition base 100 | `body_condition_base: Some(100.0)` | FNV/FO3 body-condition row (#4680) | PASS |

## Coverage Matrix

Re-derived from the `profile.rs` arms, workspace greps and the real-master probe (Dim 5):

| Family | Ruleset impl | Wired | Derived rows | Leveling model | Player seed | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | 8 | ✓ `150·L+50`, cap 20 (model only) | ✓ SPECIAL/skills + PlayerOnly HP/AP evaluated at stamp (static → D1-01) | ✗ registered-inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | 8 | ✓ `150·L+50`, cap 30 | ✓ as FO3 (AP 80 real-master) | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | 3 | ✓ `75·L+125`, uncapped | ✓ HP 85 / AP 70 (static → D1-01) | ✗ | ✗ |
| Skyrim SE | ✓ `skyrim_ruleset` | ✓ `RulesetBuilder::Skyrim` | 2 | ✓ `25·L+75` + skill-XP curve | ✓ race+offset 100/100/100 | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` (deliberate, pinned) | tests only | ✓ 10 major-skill-ups | ✗ `NpcStatModel::None` | ✗ | ✗ |
| FO76 | ✗ | ✗ | ✗ | ✗ curve LOCKED, uncoded | ✗ `NpcStatModel::None` (#4453) → empty seed | ✗ | ✗ |
| Starfield | ✗ | ✗ | ✗ | ✗ PENDING | ✗ `NpcStatModel::None` (#4453) → empty seed | ✗ | ✗ |

What the regen and affliction columns mean:
- **Registered-inert**: `pool_regen_tick_system` is registered in `boot/schedule/update.rs` (`add_exclusive_with_access`), but all four `PoolRegenConfig` inserts are inside `regen.rs` `mod tests`.
- `affliction_tick_system` has no registration anywhere.
- The "Leveling model" column is a model, not a runtime: `level_cap()` is consumed only by `profile.rs` tests.

On every seeded family the player also carries `CharacterLevel`, `Background` (#4678), `FactionRanks` (#4699) and `SpellList`, plus constant-spell permanent modifiers (#4415).

## Findings

**MEDIUM (2)**

### CHAR-2026-09-29-D1-01: #4674 materialises the player's `PlayerOnly` derived stats (Health/AP) into `ActorValues` base at stamping — any later Endurance/Agility/level change, and constant spells applied at the same attach, leave them stale
- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam
- **Game**: FO4 (capture explicitly requires dynamic rescale); FO3 / FNV (same mechanism)
- **Source**: charal-fo4-ruleset.md:77-78 ("health rescales **dynamically** with any Endurance / level change — there is no permanent/temporary split for player HP"). charal.md §6 (decision: "derived stats … are **computed on demand** … **not** materialised into `ActorValues` at spawn … so an attribute change can't leave a stale derived value behind").
- **Location**:
  - `byroredux/src/inventory.rs:340-436` (`build_player_character_template`; the write is `values.set_base(key, value)` at `:400`)
  - `byroredux/src/inventory.rs:698-735` (`attach_to_player`; `apply_modifiers` runs at `:730`, *after* evaluation)
  - `docs/engine/charal.md:377-392` (§6)
  - `docs/engine/charal-fo4-ruleset.md:96-106` (caveat 2, "Player application LANDED")
- **Status**: NEW. #4674's fix is correct for spawn values. Its own Impact line ("A temporary END change cannot rescale HP, which the capture requires") was not addressed.
- **Description**: The skill's single-sink rule says no site besides the ruleset writes derived stats straight into `ActorValues`, and charal.md §6 records that as a design decision. The #4674 fix does it anyway. It evaluates the player's `PlayerOnly` Absolute rows once, against the seed, and writes the results into the **base** layer, so they become ordinary carried values. What follows:
  - Every consumer takes the carried fast path: `GetActorValue`, `vitals_snapshot`, combat damage and drowning.
  - No formula is ever re-evaluated.
  - `attach_to_player` applies the record's constant-spell modifiers *after* the evaluation. An Endurance/Agility ability on the Player record therefore shifts END/AGI but never reaches HP/AP.

  Production writers that can change the inputs today:
  - `magic::add_spell` / `remove_spell` (a scripted `AddSpell` of an attribute ability)
  - The SDK `ActorValueOperation::{SetBase, ModifyPermanent, ModifyTemporary}` in `byroredux/src/extensions/commands.rs`
  - Console `setav` / `modav`

  The stale base is what the save serialises. The first level-up or XP writer, when it lands, inherits the same problem.
- **Evidence**:
  ```rust
  // inventory.rs:398-401 — evaluated once, written as base
  if let Some(value) = ruleset.derived_value(key, &values, level) {
      values.set_base(key, value);
  }
  // inventory.rs:728-731 — modifiers land afterwards
  if let Some(mut values) = character.values {
      byroredux_scripting::magic::apply_modifiers(&mut values, &character.spell_modifiers, 1.0);
  ```
  charal.md:381 still says derived stats are "**not** materialised into `ActorValues` at spawn".

  The vanilla spawn value is right. The probe shows the FO3/FNV Player and its race carry no SPLO, and the FO4 race spell 0x1CCDA3 touches none of END/AGI/Health/AP. The defect is the first attribute change after spawn.
- **Impact**:
  - FO4 player HP does not follow END/level. The capture calls that the defining property of the stat.
  - FO3/FNV HP and AP do not follow END/AGI/level.
  - The HUD, combat damage pool, drowning and CTDA reads all see the frozen value.
  - The design doc and the code now disagree on a stated decision, and the FO4 caveat reports the application as "LANDED".
- **Related**: #4674 (closed; its Impact line named this), CHAR-2026-09-29-D4-01 (same composition gap on the NPC side), #4415 (open; modifier writer), #2947 (level/XP persistence)
- **Suggested Fix**: Pick one of two:
  - Keep the player's `PlayerOnly` stats derived (`base` = formula output recomputed on read, or a re-evaluation system keyed on END/AGI/level change events), with modifiers layered on top.
  - Or amend §6 and the FO4 caveat to document stamping as a deliberate interim, and re-evaluate on every attribute writer, including `attach_to_player`'s own modifier application (evaluate *after* `apply_modifiers`, using `current`).

  Either way, add a test that `modav Endurance` on the player changes FO4 Health.

### CHAR-2026-09-29-D4-01: Constant-spell modifiers on formula-derived AVs create a carried `{base 0, +mod}` entry — `GetActorValue` returns the bare modifier, `melee_damage_charal_bonus` returns the bare formula; neither composes them (66 FNV Finesse NPCs, 86 FO3/FNV ghouls, FO4 Strong)
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: FNV, FO3, FO4 (Skyrim: only via scripted `AddSpell`, e.g. `doomSteedAbility` CarryWeight +100)
- **Source**: charal.md §6 (derived AVs computed from the formula; modifiers layer on the composed value, per the `ActorValues` base/permanent/temporary/damage model). Formula rows: FNV/FO3 derived table (CritChance `1.0·Luck`, RadResist `(END−1)·2`, MeleeDamage `0.5·STR`); FO4 CW `200+10·STR`. The spell data comes from the real-master probe (see Evidence).
- **Location**:
  - `crates/core/src/ecs/components/actor_values.rs:132-134` (`mod_permanent` → `entry().or_default()`)
  - `byroredux/src/npc_spawn.rs:145-167` (`stamp_spell_list`)
  - `crates/scripting/src/magic.rs:202-226` (`apply_constant_modifiers` / `apply_modifiers`)
  - `crates/scripting/src/condition.rs:582-590` (carried fast path) and `:604-632` (derived fall-through)
  - `byroredux/src/combat.rs:444-488` (`melee_damage_charal_bonus`)
- **Status**: NEW. It became production-reachable with `cd4fc019a` (#4415). The `modav` shape predates that.
- **Description**: For most derived stats, the NPC derivation carries no key. Examples are FO3/FNV CritChance, MeleeDamage and RadResist, and FO4 CarryWeight for some actors. The value comes from `CharacterRuleset::derived_value` on read. When `stamp_spell_list` applies an ability that modifies such a stat, `mod_permanent` creates a new entry with base 0. From then on the consumers split:
  - `GetActorValue` sees the key as carried and returns `0 + mod`; the formula is never reached.
  - `melee_damage_charal_bonus` reads only `derived_value`; the modifier is never reached.

  The correct value, formula output plus permanent modifier, comes out of neither. The same entry is also created by `magic::add_spell`, by `modav`, and by the SDK `ActorValueOperation::ModifyPermanent/ModifyTemporary` on any derived key.
- **Evidence**: The probe (`ResolvedNpc::resolve` → `resolve_actor_spells` → `derive_resolved_actor_values`) counted NPCs that carry the ability and whose derivation lacks the key:

  | Master | Spell | Effect | NPCs | Derivation carries the key |
  |---|---|---|---|---|
  | FNV | `PerkFinesse` 0x94EC0 | CritChance +5 | 66 | 0 |
  | FNV | `RadResistGhoul` 0x617BD | RadResist +85 | 12 | 0 |
  | FNV | `MotorRunnerBuff` 0xFE409 | MeleeDamage +10 | 1 (V03MotorRunner) | 0 |
  | FO3 | `RadResistGhoul` | RadResist +85 | 74 | 0 |
  | FO4 | `AbStrongStats` 0x225F7D | STR/AGI +10, CarryWeight +140 | 1 (CompanionStrong) | 0 |

  FO4's `AbMagLiveLoveCompanionPerks` (CW +10) is on 8 companions; 7 of them carry CW via PRPS and compose correctly.

  What a consumer reads today:
  - A Finesse NPC with Luck *L*: `GetActorValue CritChance` returns 5, not `L + 5`.
  - CompanionStrong: `GetActorValue CarryWeight` returns 150, not `200 + 10·STR + 150`.
  - V03MotorRunner's melee bonus ignores the ability's +10.
- **Impact**:
  - CTDA conditions on these AVs silently read wrong values for about 150 vanilla actors across three games.
  - Combat ignores the melee ability.
  - The two CHARAL consumers disagree on the same actor and stat.
  - Nothing warns or fails.
- **Related**: #4415 (open, magic runtime), #2933 (GetActorValue contract), #4452 (melee consumer contract), CHAR-2026-09-29-D1-01 (player-side twin), GAME-D5-2026-09-24-02 / #4819 (spell-delta persistence)
- **Suggested Fix**: Compose in one place with one accessor, `CharacterRuleset::actor_value(avif, &avs, level)`: for a derived Absolute row, `derived_value + permanent_mod + temporary_mod − damage` of the carried entry; otherwise `current`. Route `GetActorValue` and `melee_damage_charal_bonus` through it. The "carried wins" fast path must not treat a base-0 modifier-only entry as authored. Pin with the FNV Finesse and FO4 Strong shapes.

**LOW (3)**

### CHAR-2026-09-29-D1-02: Starfield's `vital_pools` row names AVIF editor id `O2`; `Starfield.esm`'s oxygen AVIF is `Oxygen` (0x2D5) and no capture line sources the row
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Starfield
- **Source**: the read-only AVIF EDID scan of `Starfield.esm` (1,107 AVIFs) gives `Health` = 0x2D4, `Oxygen` = 0x2D5, and no `O2`. `charal-starfield-ruleset.md` has no pool or oxygen line at all.
- **Location**: `crates/core/src/character/profile.rs` (`CharacterRulesProfile::STARFIELD`, `vital_pools: &[("HP", "Health"), ("O2", "O2")]`)
- **Status**: NEW. `e6ab4afe2` (#4679) moved the value verbatim from the old consumer match; #4679 was about where the roster lives, not whether its values are right.
- **Description**: The #4453 rule says a profile row for an unwired family may not claim data without a capture line, and this skill extends it to `vital_pools`. The FO76 row satisfies it: `Health`/`ActionPoints` resolve to 0x2D4/0x2D5 in `SeventySix.esm`, and F76:54 sources AP. The Starfield row is both unsourced and wrong: `actor_value_form_id("O2")` resolves nothing, so the bar would silently drop out.
- **Evidence**: The scan hits `('Oxygen', '0x2d5')`, `('OxygenUseMult', …)`, `('Player_Sprint_O2_DrainRate', …)`; there is no `O2` EDID.
- **Impact**: Latent. Starfield has `NpcStatModel::None`, so there is no player `ActorValues` and no bars. The wrong key surfaces the day a Starfield seed lands.
- **Related**: #4453, #4679
- **Suggested Fix**: Replace the second entry with `("O2", "Oxygen")`, citing the AVIF scan in `charal-starfield-ruleset.md`. Or leave the Starfield roster empty until the capture sources it, with the "blocked, not forgotten" comment.

### CHAR-2026-09-29-D4-02: Player population reads TPLT-governed fields three different ways, and `player_body.rs` adds a production `resolve_inherited_traits` call that the #4457 guard does not scan
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: all seeded families (latent on vanilla)
- **Source**: not numeric. The rule is the #4457 contract: consumers read the `ResolvedNpc` terminals, and a raw `npc.<field>` read or a new direct call is the recurrence.
- **Location**:
  - `byroredux/src/inventory.rs:368-372`: `effective_actor_level(player)`, `player.race_form_id` and `player.class_form_id` read from the shell.
  - `byroredux/src/inventory.rs:577,588,607`: `build_player_template_for` reads `effective_actor_level(player)` and `player.default_outfit`.
  - `byroredux/src/player_body.rs:160-164`: `resolve_inherited_traits(&npc, …)`.
  - `byroredux/src/npc_spawn/tests.rs:2612-2696`: the guard's six-file list.
- **Status**: NEW (`a070baaad` 2026-09-28; `7b0b84c2c` #4678)
- **Description**: The NPC path takes each field from its template terminal:
  - `Background` from `resolved.r#traits` / `resolved.stats`, and `CharacterLevel` from `resolved.stats` (`stamp_character_components`).
  - Outfit and gear level from the Use-Inventory / Use-Stats terminals (#4812).

  The player path builds a `ResolvedNpc`, then reads level, race, class and outfit from the raw shell. Separately, the player body resolves its race through `resolve_inherited_traits`. So one templated Player record would get a body race from the terminal and a `Background` race from the shell.

  `player_body.rs` is not among the files `resolve_inherited_call_sites_are_enumerated_and_pinned` counts. The enumerated production count moved from 1 to 2 with the guard still green.
- **Evidence**: The probe finds Player `NPC_` 0x7 with `tplt=0x0 tflags=0x0` on FNV, FO3, FO4 and Skyrim, so every read agrees on vanilla data today.
- **Impact**: None on vanilla content. A mod that templates the Player record would get inconsistent level, background and outfit. The guard gives false assurance about the site count.
- **Related**: #4457, #4812, #4678, #4137
- **Suggested Fix**: Read `resolved.stats` / `resolved.r#traits` / the inventory terminal in both player templates. Add `player_body.rs` to the guard (expected 1, as the pre-spawn race boundary, like `cell_loader/references/mod.rs`), or pass it the `ResolvedNpc` the job already builds.

### CHAR-2026-09-29-D5-01: `feature-matrix.md` says the FO76/Starfield player actor-value seed is "partial"; #4453 made it empty the next day
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: FO76, Starfield
- **Source**: not numeric. The code is the authority: `actor_value_derive.rs` maps `NpcStatModel::None => Vec::new()`, and `CharacterRulesProfile::FALLOUT76` / `STARFIELD` both carry `npc_stats: NpcStatModel::None` (#4453).
- **Location**: `docs/feature-matrix.md:341` ("CHARAL: player actor-value seed … FO76/Starfield partial")
- **Status**: NEW. The row was written by `d55041d5e` (#4676, 2026-09-22) and went stale with `c4f30cbde` (#4453, 2026-09-23).
- **Description**: This is a wiring event rotting prose. With an empty derivation, `build_player_character_template` returns only `factions`, `spells` and `spell_modifiers`: no `ActorValues`, `ActorVitals`, `CharacterLevel` or `Background`. The FO76/Starfield player seed is absent, not partial.
- **Evidence**: See Location and Source. `inventory.rs:360-366` is the `pairs.is_empty()` early return.
- **Impact**: The feature matrix overstates coverage for two games.
- **Related**: #4676, #4453
- **Suggested Fix**: Change the row to "FO76/Starfield no (NPC stat wire layout uncaptured, #4453)".

## Regression checks (closed since baseline: all fixed on HEAD)

| Issue | Verified |
|---|---|
| #4674 | The player's PlayerOnly rows are evaluated at stamping (`build_player_character_template`). The real-master `#[ignore]` leg asserts FO4 85/70 and FNV AP 80; the synthetic FNV fixture asserts AP 83. The static-value design is filed separately → D1-01. |
| #4675 | HUD tables carry editor ids resolved through `PlayerVitals::resolved`. `fraction()` reads `PlayerEntity` only. No literal 0x2C9/0x2D0 is left outside doc comments. |
| #4676 | Fixed at all four sites: charal.md §7, the charal-fo4 caveats, the condition.rs comment and the feature-matrix player row. The row's FO76/SF cell went stale afterwards → D5-01. |
| #4677 | `derive_stored_actor_values` drops a PRPS pair when a baked DNAM value exists (`baked_pairs`), so precedence is explicit. |
| #4678 | `attach_to_player` stamps `CharacterLevel` + `Background` from the Player record. Shell-vs-terminal reads → D4-02. |
| #4679 | `CharacterRulesProfile::vital_pools()`; `build_player_vitals` reads it; there is no `GameKind` match. The Starfield value → D1-02. |
| #4680 | The FNV/FO3 capture has a body-condition row: base 100, GECK *Stats List*, naming `BODY_CONDITION_VALUES`. |
| #4681 | The `fallout.rs` module doc calls Crit/Melee/Unarmed an "explicit, UNsourced" choice and names the pin. |
| #4452 | `melee_damage_charal_bonus` checks scope + kind. |
| #4453 | FO76/SF use `NpcStatModel::None`; pin `fo76_and_starfield_claim_no_npc_stat_model_until_captured`. |
| #4454 | The Skyrim profile and derivation disclose the 2-of-3 pool terms in place. |
| #4455 | charal.md cites `CharacterRulesProfile::OBLIVION` with no line range. |
| #4456 | `band_for` picks the first maximal band, agreeing with `band_by_key`; the stateful pin nets to zero. |
| #4457 | `ResolvedNpc::resolve` sits at `spawn_placement_root`; the guard is green at 1 site. It is blind to `player_body.rs` → D4-02. |
| #4459 / #4460 / #4461 / #4462 / #4463 | The docstring ban-test is present. Regen prose is past-tense only. The ROADMAP bullet is current. All 8 FNV/FO3 derived rows are BUILT. The feature-matrix regen row says "Fatigue/Magicka". |

## Known-Open Register (re-confirmed on HEAD; none re-filed)

1. **FNV/FO3 tag-skill per-level formula**: still undocumented and absent, not guessed. CLAS SPECIAL is still read from `ATTR`.
2. **FO3/FNV AP (and Crit/Melee/Unarmed) NPC scope (#2937, closed as documented)**: AP still ships `.player_only()`; Crit/Melee/Unarmed ship an explicit unsourced `ActorGeneral`, pinned.
3. **VATS runtime**: absent; only the AP formulas exist.
4. **Regen / affliction / level-up**:
   - `PoolRegenConfig` has 4 inserts, all in `regen.rs` `mod tests`.
   - `pool_regen_tick_system` is registered-inert.
   - `affliction_tick_system` is unregistered.
   - `level_cap()` has test consumers only.
5. **Oblivion `RulesetBuilder::None`**: deliberate and pinned. FO76/Starfield have captures but no builders, and since #4453 no NPC stat model.
6. **#4137** (OPEN): six `template_flags` bits still have no consumer.
7. **#4232** (OPEN): `effective_actor_level` still returns 0 verbatim on the non-multiplier branch. There is still a single definition (`actor/mod.rs`); `attach.rs`'s `.max(1)` is loot policy, not a copy.
8. **#4415** (OPEN): the magic runtime is partial. Constant Recover-flagged modifiers are applied; per-second effects and the cast runtime are not. D4-01 is a bug in the landed slice, not the partiality.
9. **`CharacterLevel` unsaved**: the player now carries it (#4678). `validate_progression_state` refuses a save once `xp != 0`. This belongs to `/audit-save`.
10. **`FactionReputation`**: no production insert. Unchanged.

## Cross-Audit Routing

- D4-01 fix sites span `/audit-scripting` (`GetActorValue`), `/audit-gameplay` (combat melee bonus, the spell stamp) and CHARAL (the accessor on `CharacterRuleset`).
- D1-01 touches `inventory.rs` (`/audit-gameplay`). Its persistence aspect, the stale base serialised, belongs to `/audit-save`.
- D4-02's `player_body.rs` → `/audit-gameplay` / `/audit-physics` (player body owners). The guard lives in `npc_spawn/tests.rs`.
- AVIF identity for Starfield (D1-02) → `/audit-starfield` when a Starfield seed is wired.
- Component shape → `/audit-ecs`. Scheduler access for `pool_regen_tick_system` → `/audit-concurrency` Dim 4 (unchanged).

Suggested next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-09-29.md`. Use the domain label `character`, plus `game:fnv`/`game:fo3`/`game:fo4` on D4-01, `game:fo4` on D1-01, and `game:starfield` on D1-02 and D5-01.
