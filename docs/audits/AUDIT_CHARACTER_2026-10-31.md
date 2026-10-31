**HEAD**: 670f81917 · **Baseline**: [`AUDIT_CHARACTER_2026-10-27.md`](AUDIT_CHARACTER_2026-10-27.md) (HEAD `fa6be4a6b`) · **Audited**: Dim 1 (Ruleset Seam), Dim 3 (crime/jail delta), Dim 4 (Population Boundary delta), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2 (Derived Formulas). `derived/fallout/tes/skyrim/resistance.rs` and `crates/core/src/combat.rs` have no commits since `fa6be4a6b`; `stealth.rs` changed only by the #6433 doc and pin.

# Character / CHARAL Audit, 2026-10-31

`/audit-character` at default scope, run inside `/audit-suite --preset comprehensive`. Every dimension was analysed synchronously without sub-agents. Nothing was edited or committed and the engine was not launched. Builds went into the repo `target/` with `CARGO_INCREMENTAL=0`. No private target dir or probe crate was created.

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect character` | 165 passed, 0 failed, 1 ignored (`vats::tests::to_hit_chance_matches_recorded_in_game_percentages`, #6403) |
| `cargo test -p byroredux --bin byroredux -- resolve_inherited_call_sites_are_enumerated_and_pinned vats build_vats_model install_tests install_catalog npc_spawn:: player_derived commands::actor_value regen armor smithing` | 216 passed, 0 failed, 14 ignored (real-master) |

A read-only byte-scan of `Skyrim.esm` (`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`) was used for the armor GMSTs (M-01).

## Delta since the baseline

136 commits landed since `fa6be4a6b`. The ones that touch CHARAL or its population boundary:

| Commit | Change | Verdict |
|---|---|---|
| `3a0b6be0a` (#6427) | `trespass_gold` / `pickpocket_gold` become `CrimeModel` fields read from `iCrimeGoldTresspass` / `iCrimeGoldPickpocket` | PASS |
| `cf58dec5a` (#6428) | `AfflictionTable::pool_value` reads `ActorValues::current` | PASS |
| `7e9678fd8` (#6433) | `detection_score` range precondition documented and pinned | PASS |
| `e97707b14` (#6449/#6450) | WEAP VATS base to-hit is `DNAM`@40; a regular attack charges `DNAM`@68 | PASS (`resolve_weapon` no longer reads the sub-record's AP) |
| `9d82dc616` (#6469) | `vats.enter` shares the key's gates; scan/queue/confirm refuse outside VATS | PASS |
| `26850aa5f` (#6515) | Skyrim Use Keywords template bit gets a terminal | PASS (the `resolve_inherited_*` pin still passes) |
| `613c7f46e` / `258c6d9b2` | `npc_spawn.rs` and `inventory.rs` file splits | PASS (single ruleset sink unchanged) |
| `31aca901d` (M43) | NEW `byroredux/src/armor.rs`: Skyrim armor rating and physical damage reduction, applied in `combat_damage_system` | **M-01, L-01** |
| `2c55546f4`, `b54bc65a8`, `96d050795`, `851636a13` (M43) | `smithing.rs` quality table, improvement XP, forge XP, crafting | PASS: 44-row table matches UESP *Smithing Qualities*; both XP formulas match UESP (iron dagger 38.4). `improve_plan` lock order is ECS-2026-10-31-01. |
| `36d7bfbd6` (M43) | `jail::send_player_to_jail` / `detect_escape` | **L-02** |

## Executive Summary

**3 findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 2 LOW.** All three are NEW. All of them come from the M43 gameplay wave, not from the CHARAL tier itself.

- The three 10-27 findings (#6440, #6441, #6442) are still present and open (Known-Open Register).
- All five baseline-adjacent fixes landed correctly: #6427, #6428, #6433, #6449/#6450 and #6469.
- No `GameKind` or master-name test was added to `crates/core/src/character/`. The one new consumer-side branch is `armor.rs` (M-01).

**Constants verified this run:** the Skyrim armor figures (0.12 per rating point, 3% per piece, 80% cap, 0.4 PC / 1.5 NPC skill coefficient, ceil per item) match UESP *Skyrim:Armor* and the CK *Settings* page. They match `Skyrim.esm`'s authored GMSTs too (M-01 table). The Smithing quality table (44 rows), improvement XP `3.8·Δvalue^0.5·ΔQ^0.5` and craft XP `3·value^0.65 + 25` match UESP *Skyrim:Smithing*. `CrimeModel` trespass 5 / pickpocket 25 are confirmed as fields. Dim 2 rows are unchanged and were PASS on 2026-10-26.

## Constant Verification Table (delta rows only)

| Item | Code | Source | Verdict |
|---|---|---|---|
| Armor reduction per rating point | `REDUCTION_PER_RATING` 0.12 (`armor.rs`) | UESP *Skyrim:Armor* line 141; `Skyrim.esm` `fArmorScalingFactor` 0.12 | VALUE PASS, **hardcoded, GMST ignored** (M-01) |
| Hidden per-piece reduction | `HIDDEN_PER_PIECE` 3.0 | UESP line 148; `fArmorBaseFactor` 0.03 | VALUE PASS, hardcoded (M-01) |
| Armor cap | `MAX_REDUCTION` 80.0 | UESP line 192; `fMaxArmorRating` 80 | VALUE PASS, hardcoded (M-01) |
| Player skill coefficient | `SKILL_COEFF_PLAYER` 0.4 | UESP line 144; `fArmorRatingPCMax` 1.4 / `fArmorRatingPCBase` 1.0 | VALUE PASS, hardcoded (M-01); duplicates `LIGHT_ARMOR_RATING_COEFF` (L-01) |
| NPC skill coefficient | `SKILL_COEFF_NPC` 1.5 | UESP line 154; `fArmorRatingMax` 2.5 / `fArmorRatingBase` 1.0 | VALUE PASS, hardcoded (M-01) |
| Biped body slot / armor-type enum | `BODY_SLOT_BIT` 0x4; 0 light, 1 heavy, 2 clothing | UESP slot list; xEdit `wbArmorTypeEnum` | PASS |
| Smithing quality table | `smithing::QUALITIES` ×44 | UESP *Smithing Qualities* (rows 1-44 compared) | PASS |
| Improvement XP / craft XP | `improvement_xp`, `smithing_xp` | UESP lines 349 and 396 | PASS |
| Skyrim sentence days | `CrimeModel::SKYRIM.prison_days_divisor` 100 | 150 gold gives 1.5 days (pinned) | PASS |

## Coverage Matrix

Unchanged from 2026-10-27. It was re-derived from the `profile.rs` arms and workspace greps. The only change is that Skyrim now has a non-CHARAL armor-mitigation consumer (`armor.rs`) beside its CHARAL Light Armor derived row.

| Family | Ruleset | Wired | Derived stats | Leveling model | Leveling runtime | Regen | Affliction | Other rows live |
|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ | ✓ | ✓ | `XpCurve` + GMST | ✓ kill XP, `levelup` | ~ ActionPoints lane only | ✗ unregistered | `vats()` = `FALLOUT3`, radiation gauge |
| FNV | ✓ | ✓ | ✓ | `XpCurve` + GMST + `level_cap_raises` | ✓ | ~ ActionPoints lane only | ✗ | `vats()` = `FALLOUT_NV`, radiation gauge |
| FO4 | ✓ | ✓ | ✓ | `XpCurve` uncapped | ~ no kill XP | ✗ | ✗ | barter |
| Skyrim | ✓ | ✓ | ✓ | `SkillXp` + GMST | ~ `AdvanceSkill` roll (forge and improve XP now feed it) | ✗ | ✗ | `CrimeModel::SKYRIM`, barter; armor mitigation lives **outside** CHARAL (M-01) |
| Oblivion | ✓ built | ✗ `RulesetBuilder::None` (deliberate, #3768) | n/a | `SkillUse` | ✗ | ✗ | ✗ | `CrimeModel::OBLIVION` |
| FO76 / Starfield | ✗ captures only | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – |

## Findings

### MEDIUM

### CHAR-2026-10-31-M-01: Skyrim armor mitigation hardcodes five authored GMSTs and gates on `GameKind::Skyrim` in the consumer
- **Severity**: MEDIUM (mod-compatibility: vanilla values are right, but a retuned load order is silently ignored)
- **Dimension**: Ruleset Seam & Doctrine
- **Game**: skyrim
- **Location**: `byroredux/src/armor.rs`: consts at lines 28-37 and the `index.game == GameKind::Skyrim` test in `install_armor_book` (line 66). Consumer: `byroredux/src/combat.rs:404` (`crate::armor::mitigate`).
- **Status**: NEW (introduced by `31aca901d`, 2026-10-31)
- **Source**: `Skyrim.esm` authors these (byte-scan this run): `fArmorScalingFactor` 0.12, `fArmorBaseFactor` 0.03, `fMaxArmorRating` 80, `fArmorRatingPCMax` 1.4, `fArmorRatingMax` 2.5, `fArmorRatingBase` 1.0. The CK *Settings* page (`/mnt/data/src/reference/ck-uesp-wiki/Category/Settings.wiki` lines 317-327, 2791) describes them: base factor "each piece of armor worn provides a hidden 3% damage reduction"; PC/NPC Max "multiplied by this number … at 100 skill".
- **Description**: The per-game seam is data in `CharacterRulesProfile`, and every number a load order can author resolves load-order-merged GMST then a compiled-in default (`CrimeModel::with_gmst`, `VatsModel::with_gmst`, `BarterModel`; the #6424/#6427 class). `armor.rs` instead holds `REDUCTION_PER_RATING`, `HIDDEN_PER_PIECE`, `MAX_REDUCTION`, `SKILL_COEFF_PLAYER` and `SKILL_COEFF_NPC` as bare consts. None is read from `EsmIndex::game_setting_float`. A repo grep for `fArmor|fMaxArmor|ArmorScaling` finds nothing outside this audit. Whether the model applies at all is decided by `index.game == GameKind::Skyrim` inside the consumer. That is the doctrine regression #5921 (`3162b21b8`) retired for the encumbrance, barter, experience, bleedout and perk-entry tables, and the profile rows already carry `barter`, `crime` and `vats` rows for exactly this.
- **Impact**: Armor-rebalance mods and the Creation Club armor changes retune these settings. With them ignored, the reduction is wrong on those load orders. A future game that shares the armor model (Skyrim VR and other Creation-era titles) needs another consumer branch instead of a profile row.
- **Suggested Fix**: Add an `ArmorModel` (or similar) row to `CharacterRulesProfile`, `Some(...)` on `SKYRIM` only. It would carry scaling factor, base factor, cap, and PC/NPC skill ranges, with a `with_gmst` overlay. Install it as a resource the way `build_vats_model` and `install_crime_model` do, and make `ArmorBook` populate when the row exists. Pin it in `crime_and_radiation_gauge_rows_are_stated_on_every_profile`'s successor and add a `with_gmst` test.

### LOW

### CHAR-2026-10-31-L-01: The new armor module duplicates the Light Armor skill-scaling constants CHARAL already owns, and the Skyrim capture still says no such module exists
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim
- **Location**: `byroredux/src/armor.rs:34-37` vs `crates/core/src/character/skyrim.rs` (`LIGHT_ARMOR_RATING_COEFF` 0.004, `skyrim_ruleset` DamageResist row, lines 120-131). Docs: `docs/engine/charal-skyrim-ruleset.md:211-239` and `:241-286`; `docs/engine/charal.md:344-346`.
- **Status**: NEW
- **Source**: UESP *Skyrim:Armor* line 154 ("For NPCs the skill coefficient is 1.5 instead of 0.4"). The capture itself (charal-skyrim-ruleset.md, "Armor Rating / Damage Reduction") says `0.4/100 = 0.004` and `1.5/100 = 0.015` "are the exact same constants" as `LIGHT_ARMOR_RATING_COEFF`.
- **Description**: Two independent implementations of the same skill-scaling term now exist. CHARAL's is a `player_only` multiplier row on `DamageResist` (Light Armor only), and its doc says NPCs use 0.015 and are "not modelled". `armor.rs` models both skills, player 0.4 and NPC 1.5, and never reads the `DamageResist` row. Nothing writes the displayed rating into `DamageResist` either. The CHARAL row therefore feeds only a `GetActorValue DamageResist` read, and the two can disagree. The capture's closing paragraph ("belongs in a combat/equipment module … no such module exists yet") is now stale, and `charal.md` still presents the row as the Skyrim armor story. The project rule is to improve existing code instead of duplicating logic.
- **Impact**: None today (both give the same numbers where they overlap). A coefficient correction made in one place will not reach the other, and the capture misleads a reader about where armor lives.
- **Suggested Fix**: When the M-01 row lands, derive both from it, or retire the `DamageResist` multiplier row if nothing reads it. Update the two capture paragraphs and the `charal.md` Skyrim line to name `armor.rs`.

### CHAR-2026-10-31-L-02: `send_player_to_jail` falls back to `CrimeModel::SKYRIM` when no model is installed, unlike every sibling in `jail.rs`
- **Severity**: LOW
- **Dimension**: Progression & Pools (Crime)
- **Game**: skyrim (reachable only if a non-Skyrim load order runs the native)
- **Location**: `crates/scripting/src/jail.rs:371`
- **Status**: NEW (introduced by `36d7bfbd6`)
- **Source**: `CharacterRulesProfile::crime()` is `None` on FO3/FNV/FO4/FO76/Starfield, and `install_crime_model` removes the resource there (pinned by `crime.rs` `install_tests`). `pay_fine`, `pay_fine_thief` and `go_to_jail` (`jail.rs:237, 260, 275`) decline when the resource is absent.
- **Description**: `world.try_resource::<CrimeModel>().map_or(CrimeModel::SKYRIM, |model| *model)` substitutes a Skyrim ruleset for "no CHARAL crime model for this game", which `character/mod.rs` doctrine forbids ("`None` means no CHARAL for this game, never use the default"). The path is gated by `JailTable` and `CrimeFactionData` matches, so it is unlikely to fire outside Skyrim (not verified for FO4's Papyrus surface).
- **Impact**: Latent: a sentence length computed from Skyrim's divisor on a game that models no crime.
- **Suggested Fix**: `let Some(model) = world.try_resource::<CrimeModel>().map(|m| *m) else { return false };` after raising `ARRT`, as the siblings do.

## Known-Open Register

Confirmed still true in code; none re-filed.
- **#6440** (M-01 of 10-27): `vats_confirm` still folds same-target shots into one `HitEvent` (`systems/vats.rs:679-720`), so FNV Damage Threshold applies once to the sum.
- **#6441** (L-01 of 10-27): the `NEUTRAL_SKILL` comment at `systems/vats.rs:359-361` is unchanged.
- **#6442** (L-02 of 10-27): `profile.rs` still carries the FO4 2.5/1.5 and Skyrim 2.4/1.4 barter ends with no capture line.
- **FNV/FO3 tag-skill per-level formula** undocumented. **FO3/FNV AP, Crit, Melee and Unarmed NPC scope** unsourced, shipped `.player_only()` (#2937, #6436).
- **Oblivion `RulesetBuilder::None`** deliberate (#3768). FO76/Starfield are captures only.
- **`affliction_tick_system`** has no scheduler registration. Regen is live for FO3/FNV ActionPoints only (`PoolRegenConfig` inserted through `build_vats_model`).
- **VATS (#6350)**: no menu, no per-limb routing, no playback or camera, Concentrated Fire unplumbed (#6422). To-hit order reconstructed (#6403).
- **Open issues re-verified in code**: #4137 (Script 0x0200, Def Pack List 0x0400), #4232, #6429, #6430, #6431, #6432, #6434, #6435.
- **Suite cross-references**: the crafting `improve_plan` lock order is ECS-2026-10-31-01 and REG-2026-10-31-01 (not CHARAL-owned). The lock-order lane red is that finding. The `GetCrimeGold` caller-scope bug is #6510.

## Cross-audit routing

- Armor mitigation in `combat_damage_system`, crafting and improving: `/audit-gameplay`. The CHARAL parts are M-01 and L-01.
- Lock order in `improve_plan`: `/audit-ecs` (ECS-2026-10-31-01).
- Skyrim `Faction.SendPlayerToJail` native and story events: `/audit-scripting`.

Next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-31.md` with domain label `character` plus `game:skyrim` for all three.
