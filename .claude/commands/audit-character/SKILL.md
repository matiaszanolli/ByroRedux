---
description: "Deep audit of CHARAL — the per-game character ruleset → canonical ActorValues/Level/Perks layer: derived-stat formulas, leveling models, skill/attribute rosters, regen + affliction + reputation systems, population boundary"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# Character / CHARAL Audit

Audit `crates/core/src/character/` — the per-game **character ruleset** translation tier. CHARAL
translates *rules*, not data: each game's attribute / skill / perk / leveling model resolves into one
canonical representation over `ActorValues`, so the gameplay runtime never branches on the source game.
`/audit-ecs` checks the *shape* of its components; nothing else checks its *numbers*. A wrong constant
changes gameplay silently — no crash, no validation error, no failing test unless someone wrote one.

**Architecture**: Orchestrator; each dimension runs as a Task agent (max 3 concurrent). Read
`.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and
`.claude/commands/_audit-severity.md` first; do not duplicate them here.

## Scope

- `crates/core/src/character/` (26 modules, `mod_docstring_indexes_every_sub_module` pins the count):
  `ruleset.rs` (`CharacterRuleset`, the per-game `Resource` seam), `profile.rs` (`CharacterRulesProfile` —
  the single per-game policy row: `NpcStatModel`, `creature_stats`, `body_condition_base`, `vital_pools`,
  `RulesetBuilder` — `Oblivion` since #7010, `None` only for FO76/Starfield — and since #5921 `3162b21b8` `OverEncumberedRule`, `awards_kill_experience`,
  `essential_death_time_secs`, `BarterModel`, `PerkEntryPointDialect`; `combat_distance` (the `fCombatDistance` melee-reach default, LMC-SET-002 `c4425799a`: FO3/FNV 128, Skyrim/FO4/FO76 141, Oblivion 180 kept with no source, `None` on Starfield; the load order's GMST wins; and `Game.AdvanceSkill`'s `fSkillUseCurve` is likewise the load order's, `SKYRIM_SKILL_USE_CURVE` only the default); later `radiation_gauge` `e9b7bcead`,
  `level_cap_raises` #6227 `cafd3ccab`, `crime`, `vats`, `armor`; the Oblivion/Skyrim rule rows `spell_cast`, `bow`, `block`, `fall`, `armor_rating` (each `Option`, `Some` only where the game's rules are sourced and built); M65's `stance`, `stealth`, `indicator` rows and Oblivion's `NpcStatModel::OblivionNpc`/`OblivionCreature`, `build_tes_pool_regen` and `build_disposition` (#7010/#7011); `PerkEntryPointDialect` has three variants — `Fallout3NV`, `Skyrim`, `Fallout4` — plus `None`, and `add_perk_raises_rank` #6619), `derived.rs` (`DerivedStatFormula`/`DerivedInput`/`DerivedOutput`/`DerivedScope`/
  `RoundMode`), `leveling.rs`, `skill_use.rs` (`SkillUseCosts`/`SkillClass`/`OBLIVION_SKILL_USE_VALUES`/`ArmorHitSlot` — classic-TES skill-use cost + banking, #7050/#7051), `attribute.rs`/`skill.rs`, `disposition.rs` (`DispositionModel`, Oblivion starting disposition from `Oblivion.exe` `0x546260`, #7011), `stance.rs` (`StanceModel`/`SneakMovement`, M65 sneak-stance speed), `fallout.rs`/`tes.rs`/`skyrim.rs` (per-family
  ruleset builders), `regen.rs` (`PoolRegenConfig`, `TesPoolRegen` — Oblivion's Fatigue/Magicka rates, #7197), `affliction.rs`/`resistance.rs`, `reputation.rs`, `crime.rs` (`CrimeModel` — Oblivion and Skyrim rows; `CrimeKind`, `BountyScope`), `vats.rs` (`VatsModel`/`VatsHitInput` — the `fVATS*`/`fActionPoints*` tuning family + the to-hit chain, FO3/FNV rows, #6350), `limb_effects.rs` (`LimbEffectModel` — crippled-leg speed, crippled-arm melee/gun-spread terms, FO3/FNV; VATS-LIMB-006/-010/-011), `armor.rs` (`ArmorModel` — Skyrim's `fArmor*`/`fMaxArmorRating` row with a GMST overlay, #6567 `cb9faca7d`), `spell_cast.rs` (`SpellCastModel`/`EffectCostInput` — Oblivion's effect/actual cost, mastery tiers, primary school and cast Fatigue; `fMagic*`/`iSkill*Min`/`fFatigueCast*` overlay; player-spellcasting PRD, #7077), `bow.rs` (`BowModel` — Oblivion's draw timer/speed/damage rating, Skyrim's draw time/tilt/range cap/Archery bonus, `fArrow*` overlay; player-archery PRD, #7055), `block.rs` (`BlockModel`/`BlockGuard`, Oblivion Block share, #7054), `fall.rs` (`FallModel`/`FallTracker`, Oblivion `fJumpFall*` fall damage, #7056), `armor_rating.rs` (`ArmorRatingModel`/`weapon_resistance_factor` — Oblivion armor rating + Resist Normal Weapons, `fMaxArmorRating` overlay held to `0..=100`, `fist_mastery_skill` = `iSkillJourneymanMin`; Skyrim's sibling is `armor.rs`), `components.rs` (incl. `DismemberRecord`, VATS-LIMB-008/-009, and `CharacterIdentity` — the saved name/sex/race chargen chooses).
  Substrate: `crates/core/src/ecs/components/{actor_values,faction_ranks}.rs`.
- CHARAL-adjacent (not submodules): `crates/core/src/combat.rs` (Oblivion damage math: `OblivionDamageSettings`, the weapon / Hand-to-Hand / Luck / condition / Fatigue settings read from the load order, #7199 `12189f685`),
  `crates/core/src/stealth.rs` + `crates/core/src/stealth/{fallout,fallout4,indicator,oblivion,settings,skyrim,starfield}.rs`
  (M65 detection: six `StealthKind` formulas — FO3, FNV, Oblivion, Skyrim, FO4, Starfield — plus the indicator table; consumed by
  `byroredux/src/detection/` and `systems/stance.rs`, so no longer thin). Oblivion skill-use leveling runtime:
  `crates/scripting/src/oblivion_skill_use.rs` (the one entry point: `bank_skill_use`, `raise_skill_ranks`, `pick_attribute`,
  `apply_pending_level`; `apply_level_menu_picks` for the sleep/level-up menu) fed by `byroredux/src/skill_use_emit.rs` (hit / armor-hit / block / stance / jump emitters; the stance emitter banks Athletics/Sneak only while movement input drives the body, `player_movement_input_drives_body`, #7281 `6e6fdb58b`, and Sneak only with a detector in range, UESP *Oblivion:Sneak*, #7294 `c56f83650` — pinned by `a_held_key_banks_no_stance_use_while_the_body_cannot_move` / `sneaking_with_nobody_in_range_banks_no_sneak`) plus the cast (`byroredux/src/player_cast.rs`) and bow (`byroredux/src/bow.rs`) actions and the sleep/wait route (`byroredux/src/oblivion_rest.rs`, which follows the installed leveling model, not the game, #7212 `188bb4535`) — #7050-#7056. The five rule rows above are installed as resources by `install_{armor_rating,block,fall}_model` (`byroredux/src/combat.rs`), `install_spell_cast_model` (`byroredux/src/player_cast.rs`) and `install_bow_model` (`byroredux/src/bow.rs`), each reading the profile row and never the game.
- **Population boundary**: `byroredux/src/npc_spawn/` (`build_character_ruleset`, the only ruleset
  construction site) + `npc_spawn/resumable/`, `npc_spawn/ai_package.rs`, `crates/plugin/src/esm/records/
  actor_value_derive.rs` + `oblivion_population.rs` (Oblivion's NPC/creature stat translation) + `oblivion_actor_values.rs` (the engine-supplied 72-name roster), `crates/plugin/src/esm/records/actor/mod.rs` (`effective_actor_level`),
  `crates/plugin/src/equip.rs` (`resolve_inherited_*`), `byroredux/src/cell_loader/references/`,
  `byroredux/src/inventory/` (`attach_to_player` stamps the player's `ActorValues`/`ActorVitals`/`CharacterLevel`/
  `Background`/`SpellList`/`RaceSpells` and reads `CharacterRulesProfile::vital_pools`), `byroredux/src/combat.rs` (`melee_damage_charal_bonus`, `grant_experience`), `byroredux/src/{armor,limbs}.rs` (`install_armor_book` reads the profile's `armor` row; `build_limb_effects` — FO3/FNV only, gated on `body_condition_base`),
  `byroredux/src/commands/{actor_value,condition}.rs`, `byroredux/src/chargen.rs` (FNV new-game character creation, Session 111: the one UI-driven writer of the player's SPECIAL / tag skills / traits / `CharacterIdentity`) + `byroredux/src/systems/character.rs` (`player_derived_stats_system`, `player_identity_sync_system`), VATS runtime `byroredux/src/systems/vats.rs` + `components/vats.rs` (`VatsState`) +
  `commands/vats.rs` (`vats.*`; `build_vats_model` in `npc_spawn/stamps.rs` is the one installer), `byroredux/src/crime.rs` (installs the `CrimeModel` resource from
  `index.character_rules.crime()`), `crates/scripting/src/{crime_faction,jail}.rs` (crime-faction fines, Oblivion jail).
- Handoff: gameplay writers of `ActorValues` (consumables, restoration, combat — `byroredux/src/
  {inventory,combat}.rs`, `systems/restoration.rs`) belong to `/audit-gameplay`; CHARAL owns only whether
  they respect the layer contract (Dim 4). Save schema of `ActorValues`/`Perks` → `/audit-save`.

**Ground truth — read the captures BEFORE the Rust constants** (reading code first turns verification
into confirmation): `docs/engine/charal.md`, then `charal-{fnv-fo3,fo4,oblivion,skyrim,fo76,starfield}-
ruleset.md` — **the authority for every constant**; a coefficient no capture supports is a finding
(*feedback_no_guessing*). Memory: *actor_value_population*, *class_system*, *tes_character_rules*,
*vats_system*, `perk_system`.

**Known-open — do NOT re-file** (confirm still true):
- FNV/FO3 **tag-skill per-level** formula undocumented, deferred; CLAS SPECIAL lives in `ATTR`, not `DATA`.
- FO3/FNV AP (and Crit/Melee/Unarmed) NPC scope is unsourced: those rows ship `.player_only()` as the
  documented conservative choice (#2937, closed as documented), disclosed in place and pinned; the GECK's per-actor wording for `fAVDActionPointsBase/Mult` is a recorded weak lead only, scope stays `player_only` (#6436 `87c9b9eb2`).
- VATS (#6350, open umbrella; phases #6644 captures, #6645 numbers, #6646 limbs): the AP pool, its regen, the mode machine with world freeze, the targeting scan and the AP-gated
  queue/confirm (die roll per shot through the combat funnel) are live for FO3/FNV (`3dc16208e`..`1fb7e6f56`). Since the last sync: Concentrated Fire prices repeat shots at the same part when the player holds the perk (#6422 `5502c2f97`, `has_concentrated_fire` composes perk entry point 33; the scan still prices a part's first shot); VATS ends on a cell transition and on the player's death through the shared `end_vats_for_world_change` (#6625 `6186ab77e`); a confirmed volley reaches the funnel as a `HitEvent` plus transient `HitShots` and `HitParts` (one `PartStrike` per landed shot at the struck part's `BPND` damage multiplier, DT/armor applied per strike, VATS-LIMB-002/-003 `83e9c6c92`); a killing blow writes the saved `DismemberRecord` that `IsLimbGone`/`GetKillingBlowLimb` read (`d50fc1153`, nothing marks a part gone yet — `mark_gone` has no caller, so `IsLimbGone` answers 0); crippled legs slow the player and every NPC stride and a crippled arm weakens a melee swing (`byroredux/src/limbs.rs`, `bda7fab73`); `vats.limbs` reports a limb table (`532cc486f`); a VATS kill restores the Player Kill AP Reward (`9e88a183b`). Still absent, so not findings: a VATS menu, playback + camera, **routing a hit into the limb-condition actor values** (VATS-LIMB-004 — nothing lowers a limb below its seed, so no limb becomes crippled by play; it waits on its Phase 0 formula), the gun-spread consumer for the crippled-arm term (the engine has no spread computation), FO4 (`vats: None`). `vats.enter` shares the key's entry gates through `vats_entry_refusal` and `vats.scan/queue/confirm` refuse outside VATS (#6469 `9d82dc616`); a weapon's base to-hit is the `u8` at `WEAP` `DNAM`@40 and the VATS sub-record's AP is the *special* attack's (`WeaponVats::special_attack_ap`), a regular attack charges `DNAM`@68 or the engine default (#6449/#6450 `e97707b14`). The skill term reads the player's value for the weapon's own `DNAM`@104 actor-value code through its AVIF, a weapon or player without one (a carried 0 stays 0) pricing at `NEUTRAL_SKILL` 50 (`a9748e54c`, #6441 `bb60b15cf`); FNV Damage Threshold applies once per shot (#6440 `e669918ad`). The to-hit operation order is **reconstructed, not captured**; its ground-truth test is
  `#[ignore]`d and with an empty `GROUND_TRUTH` pins nothing — a placeholder, not coverage. The captures are an acceptance item of #6350 (#6403 closed that way, `2d6af0aeb`; the Phase 0 capture issue is #6644) — never let the reconstruction arbitrate itself.
- Regen: `pool_regen_tick_system` is live for two lane sets — FO3/FNV **ActionPoints** (`build_vats_model` returns a `PoolRegenConfig` carrying
  the `ActionPointsRegen` lane, #6374 `68f200ec4`) and Oblivion **Fatigue/Magicka** (`CharacterRulesProfile::build_tes_pool_regen` resolves
  Fatigue/Magicka/Willpower/Endurance/`StuntedMagicka` and takes the load order's settings: the rates are `PoolRegenConfig::tes_rates`, a `TesPoolRegen` overlaying
  `fFatigueReturnBase`/`fFatigueReturnMult` (Fatigue restores `base + mult × Endurance`) and `fMagickaReturnBase`/`fMagickaReturnMult` over the documented defaults — all four
  authored by `Oblivion.esm`, a non-finite or negative value keeps its default; the old "literal rates" reading is closed, #7197 `2fc1f447c`; the `FATIGUE_REGEN_PER_SEC`/`MAGICKA_REGEN_*` consts survive only as those defaults.
  A TES lane without `tes_rates` is off, and the Fallout AP config carries none. A positive Stunted Magicka value zeroes only Magicka regen, pin
  `stunted_magicka_stops_only_magicka_regeneration`; Magicka's ceiling is `undamaged()`), both installed idempotently on the first cell load
  (`cell_loader/references/mod.rs`). `PoolRegenConfig`'s ids are `Option<u32>` (#6430 `a10bb9764`): each lane is `None` on a game that does not resolve it, FO4/Skyrim/FO76/Starfield never insert the config,
  and `affliction_tick_system` still has no scheduler registration (`AfflictionStatus` is a `registry_completeness_tests` forward-latent row).
  Verify both, and that prose names the lanes per game rather than "inert"/"live" wholesale (Dim 5 doc sweep). Level-up is no longer absent: M47.2's
  `grant_experience` (`byroredux/src/combat.rs`, shared by Fallout kill experience, `levelup xp` and Skyrim
  `Game.AdvanceSkill` rank-ups via `xp_from_skill_rank`) rolls levels through `LevelingModel::roll_levels` up to
  `level_cap()` and banks the rewards into the saved `LevelUpPoints` (Dim 3/4) — Fallout `skill_point_reward`,
  Skyrim a perk point + a pool pick (#5916 `612bd179e`); classic-TES `SkillUse` has no XP-driven roll but a skill-use runtime since #7050-#7052 (`9ea611dec`, `7b11ebe07`, Dim 3).
- Oblivion has a runtime ruleset (`RulesetBuilder::Oblivion`, `tes::oblivion_ruleset`; engine-supplied actor values, #7008–#7010 all closed), pinned by
  `oblivion_builds_its_runtime_ruleset`; its player Health/Magicka/Fatigue are formulas (#7037/#7041, the record's authored pools are zeroed as `Oblivion.exe` `0x5240b2` does). Only FO76/Starfield: captures, no builders (`RulesetBuilder::None`).
- Closed 2026-11-22 — cite, do not re-file: #7077 (player-spellcasting tracking, closed when the PRD shipped; its residuals issue #7082 closed in `c343c141f`: a Touch cast follows the authored `iActivatePickLength` camera ray (`SpellCastModel::touch_pick_length`, default 150, GMST-retuned) to the first solid actor collider, established by original-game trials; MGEF flag bit 22 (`MGEF_COST_SKIPPED`) and the equal-Magicka gate boundary stay marked executable-derived where they appear — the owner ruled out decoding executables for rules, so do not "source" them from a disassembly).
- Open (verified 2026-11-22) — cite, do not re-file: #4137 (`template_flags` Def Pack List 0x0400 has no consumer, its `DPLT` list
  not decoded; Script 0x0200 is consumed by the VMAD attach through `scripts_terminal`, own plus template scripts, Skyrim; the other four listed gained `ResolvedNpc` terminals, `eae8fd740`), #6350 (VATS umbrella, carries the to-hit ground-truth captures; #6403 itself is closed, `2d6af0aeb`). #4232 (`effective_actor_level` returns 0 verbatim) is closed (`4fbff2c6c`, see Dim 4). #4415 (magic runtime) is closed (`9813af435`). The 2026-09-19 report's
  #4452-#4457/#4459-#4463 are all closed — regression checks now. So are all 28 findings of the 2026-11-21 report, #7193-#7220 (`cc907aa92`..`91e91aeac`) — see Dim 1-5 for each regression check. Verify with `gh issue view` before citing.

## Parameters

`--focus <dims>` (default all 5) · `--game fnv|fo3|fo4|oblivion|skyrim` (restrict constant checks) ·
`--depth shallow|deep` (`shallow` = structure + doctrine; `deep` = every coefficient vs its capture; default deep).

## Extra Per-Finding Fields

- **Dimension**: Ruleset Seam | Derived Formulas | Progression & Pools | Population Boundary | Coverage & Doctrine
- **Game**: family the finding concerns (or `all`). **Source**: the capture-document line the correct value
  comes from — **required for any numeric finding**; a numeric finding with no source is not reportable.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/character`; dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/character/issues.json`.
2. Read the newest `docs/audits/AUDIT_CHARACTER_*.md`; its fixed findings are regression checks.
3. `cargo test -p byroredux-core character` (record counts). Load the capture documents before any Rust constant.

## Phase 2: Dimensions

Delta-first: for each dimension run `git log --since=<last report date> --format='%h %cs %s' -- <Paths>`;
skim dimensions whose Paths did not change (constants only move when a capture or formula commit lands).

### Dimension 1: Ruleset Seam & Doctrine
Paths: `crates/core/src/character/{ruleset,profile,mod}.rs`, `byroredux/src/npc_spawn/`
First step: `grep -rn 'GameKind\|\.game ==\|game_kind\|ScriptGame' crates/core/src/character byroredux/src/npc_spawn* crates/plugin/src/esm/records/actor_value_derive.rs byroredux/src/{encumbrance,barter,combat,armor,limbs,crime,bow,player_cast,skill_use_emit,oblivion_rest}* byroredux/src/detection/ crates/scripting/src/{bleedout,perk_entry_points,oblivion_skill_use}*`
- **Doctrine**: the per-game seam is *data in the tables*, never a branch in a consumer. Report every
  `game ==` / `GameKind` / master-name test in CHARAL population or consumers (weight = `/audit-nifal`'s
  render-time per-game branch). Precedents: FO3/FNV body-condition seeding was a raw `GameKind` gate until
  it became `CharacterRulesProfile.body_condition_base` (#4447); five M47.2 consumers (`install_encumbrance_rules`,
  `BarterRules::from_index`, `install_experience_rules`, bleedout's `engine_defaults`, `PerkEntryTable`) matched on
  `GameKind` until #5921 `3162b21b8` moved each table onto a profile row. Every row is an exhaustive struct
  literal (a new profile cannot compile without stating each); pin `per_game_rule_rows_are_stated_on_every_profile`. Each consumer also has a test whose index has a `GameKind` and a `CharacterRulesProfile` that disagree, proving the profile wins (#6760 `573f3ced8`: encumbrance, bleedout defaults, perk dialect, barter, `install_experience_rules`) — a new row-reading consumer adds one. The same move retired `armor.rs`'s five bare consts and its `index.game == Skyrim` test (#6567). The rows added since follow the rule: `the_player_rule_row_installers_follow_the_profile_not_the_game` (`byroredux/src/combat.rs`) drives all five player-rule installers (spell cast, armor rating, fall, block, bow) with disagreeing game/profile pairs, the crime install tests pair each profile with a game that disagrees, and `player_and_stealth_rule_rows_are_stated_on_every_profile` value-pins the eight rows on every profile (#7203 `afaacfba1`). The two consumer-side game tests the 2026-11-21 audit found on the Oblivion level-up route are gone — `install_skill_use_classes` holds every class that carries a specialization, and `oblivion_rest::supports_rest` follows the installed leveling model (`skill_use_costs`), each with a test pairing a game and a ruleset that disagree (#7212 `188bb4535`); a new `GameKind`/`ScriptGame` test on that route is the regression. The Player `NPC_` FormID (7) has one home beside `NpcRecord` in `crates/plugin` (`byroredux/src/inventory/` re-exports it; scripting's `PLAYER_BASE` aliases it) — a restated `== 7` / `get(&7)` in a population path is the regression (#7211 `3fd30c4f1`).
- **Profile-as-oracle**: `CharacterRulesProfile` is the only FO3-vs-FNV discriminator (both are
  `GameKind::Fallout3NV`); non-character consumers (`cell_loader/references/attach.rs::obscript_dialect_for`,
  `crates/plugin/src/consumables.rs` fn-586 gate, and the #5921 rule-row readers above) borrow it. The duty is documented and pinned (#4448) —
  verify each consumer still has a test proving the discriminator is the *profile row*, and that a new
  borrower adds one.
- **Single sink**: `build_character_ruleset` is the only construction site; no second site builds a
  ruleset or writes derived stats straight into `ActorValues`.
- Derived table is a flat `Vec<(u32, DerivedStatFormula)>` scanned linearly (N ≈ 3–10): report growth to
  dozens as a documented trade-off unless on a hot path. Output keys are **remapped global AVIF FormIDs**
  (`/audit-esm` Dim 3), not plugin-local ids.
- `attributes`/`skills` *membership* is engine-supplied, the FormIDs they resolve to are authored: a
  hardcoded FormID in a roster, or an engine count derived from parsed data, is a finding. Guard:
  `ROSTER_CASES` + `assert_rosters_resolve` (`crates/plugin/tests/parse_real_esm.rs`, `#[ignore]`d
  real-master corpus gate) — a new roster/derived output without a `ROSTER_CASES` entry is untested by
  construction. The Oblivion case installs the engine-supplied roster at a free slot and asserts the six derived rows like every other game (#7202 `6aae19a63`), and each case records its master's *unauthored* leveling GMSTs, asserted as an exact set so the list cannot go stale either way.
- **Profile rows must be sourced**: `BarterModel`'s FO4 2.5/1.5 and Skyrim 2.4/1.4 curve ends are the *executables'* setting defaults, not the masters' 3.5/2.0 and 3.3/2.0 — do not "correct" them toward the master values (#6442 `e41fb357c`). A row for an unwired family may not claim a wire format without a
  capture line (FO76/Starfield were reset to `NpcStatModel::None`, #4453); the same holds for `vital_pools`
  (per-profile roster since #4679 — a consumer-side `GameKind` pool list is the regression); the "blocked, not forgotten" comment pattern
  (it was Oblivion's arm until #7010 wired it; the FO76/Starfield `RulesetBuilder::None` rows carry it now) is the template; Starfield's pool roster is `("O2", "Oxygen")`, the AVIF EDID its master
  authors (#5044 `9bac2d87b` — an unresolvable `"O2"` EDID once silently dropped the bar). Pins: `fallout_profiles_keep_roster_health_and_ruleset_in_lockstep`,
  `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst`.
**Output**: `/tmp/audit/character/dim_1.md`

### Dimension 2: Derived-Stat Formulas
Paths: `crates/core/src/character/{derived,fallout,tes,skyrim,resistance,armor}.rs`, `crates/core/src/{combat,stealth}.rs`, `crates/core/src/stealth/`
First step: `cargo test -p byroredux-core character::derived character::fallout character::tes character::skyrim character::armor`
- **Every coefficient vs its capture** (bias, `c_a`, `c_b`, cross term, cap, `RoundMode`), citing the
  document line. FO4 Health `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` is the only cross-term row; a
  non-zero cross term elsewhere is a red flag. Rounding modes are per-source (FO4 sourced — the Endurance page's `floor` is what the code implements, the Health page's 93.5 is the unrounded example, the 0.5 HP difference is unadjudicated, #6432 `6b637eacb`; FO3/FNV ship
  `None`, exact on their integer domain) — "obviously round/floor" is the guess this audit catches (#4451).
- **GMST overlay**: the Fallout rows are `*_ruleset_with_gmst` builders (the no-GMST ones delegate); Health `fAVDHealthEnduranceMult` + `fAVDHealthLevelMult`, Action Points `fAVDActionPointsBase`/`Mult`, Carry Weight `fAVDCarryWeightsBase` (FO3/FNV spelling, with an `s`) / `fAVDCarryWeightBase` (FO4) + `fAVDCarryWeightMult`, Melee `fAVDMeleeDamageStrengthMult`, Unarmed `fAVDUnarmedDamageBase`/`Mult` read the load order over the vanilla numbers (#6756 `6dbf69d65`); the engine-supplied 100 in Health and the `fAVDSkill*` skill constants stay compiled in. A coefficient restated as a literal beside its GMST is the regression. The overlay helper drops a non-finite authored value like every sibling overlay (#7200 `a3d886fc8`). The **Oblivion** rows follow the same rule: `oblivion_ruleset_with_gmst` (`tes.rs`; `oblivion_ruleset` delegates with no settings) overlays `fPCBaseHealthMult` (Health = `trunc(End × mult)`, default 2.0) and `fPCBaseMagickaMult` (Magicka = `trunc(Int × (1 + mult))`, `Oblivion.esm` authors 1.0), a non-finite or negative value keeping the default (#7198 `7d4770a57`); the capture's second Magicka factor (actor value 40, Magicka Multiplier / 10) is a *declared* gap in the ruleset's Known gaps, not a derived row. `oblivion_ruleset` registers six rows (Health, Magicka, four Fatigue) — the two armor-rating `Multiplier` rows and `ARMOR_RATING_SKILL_*` are gone because the 72-name roster has no such value and `ArmorRatingModel` is the one home of `0.35 + 0.0065·skill` (#7201 `987b8d266`; the `ROSTER_CASES` Oblivion case now installs the engine roster and asserts those six, #7202 `6aae19a63`): a derived row or a restated coefficient duplicating `ArmorRatingModel` is the regression.
- **Sentinels**: uncapped cap is `f32::INFINITY` (no table may rely on cap-0 = uncapped);
  `DerivedInput` `0` = unused, `u32::MAX` = level — no path may build a real FormID equal to either.
- **Scope contract**: `DerivedScope` (player-only vs actor-general) is what contains the #2937
  deferral. A row with no scope annotation in its capture is *unsourced* (#4450: documented + pinned);
  every consumer that evaluates for an arbitrary entity must check scope
  (`crates/scripting/src/condition/eval.rs` and, since #4452,
  `melee_damage_charal_bonus` do). Oblivion's Health/Magicka/Fatigue rows are `player_only` by analogy to the Fallout baked-pool convention — the capture states no pool scope, so UNsourced (#6431 `576dbe973`, `oblivion_pool_rows_are_player_only_pending_a_source`). `PlayerOnly` rows are evaluated for the
  player only (stamped #4674, refreshed per frame since #5039) — verify no NPC path reaches them. FO3/FNV
  shared-row scopes cite the GECK *Stats Tab — NPC* page (#5240 `3826c2792`,
  `fo3_fnv_geck_npc_page_scopes_the_shared_derived_rows`); Melee Damage is annotated CONFLICTED and kept
  actor-general deliberately — do not re-file the flip without the settle bar the capture names.
- **Chaining**: SPECIAL + skills must be in `ActorValues` before dependents evaluate (order is the
  mechanism, no dependency graph); an unpopulated input reads a *documented* default, not accidental 0.
- **Oblivion rule rows** (`spell_cast`, `bow`, `block`, `fall`, `armor_rating`): pure data + functions, no ECS, no game branch; every number cites a CS wiki / UESP line in the module docstring and `docs/engine/charal-oblivion-ruleset.md` ("Spell casting", "Acrobatics", "The Complete Damage Formula"); a coefficient with no source stays a *declared* gap, labelled (fall's "what counts as falling", Oblivion's arrow range, Skyrim's partial-draw bow). Check the cost chain `0.1·base·D·A·M^1.28·T` floors each effect (≥ 1), the mastery tier tests the *bare* base cost — `fMagicSpellLevelCostMult`/`Base` are CS-wiki "Not used" settings, so `SpellCastModel` carries and reads neither (#7208 `db8b59c14`; the tier test pins that authoring them changes nothing) — Manual Spell Cost floors `stored·modifier(primary school)` with no minimum (`2d9bfe7e9`, `a_manual_spell_costs_its_stored_cost_times_the_primary_schools_modifier_rounded_down`), shield block caps at 75% and bare hands block only an unarmed attacker (`the_documented_reductions`), fall damage is `80·t·(fJumpFallSkillBase + Acrobatics·fJumpFallSkillMult/100)` = `80·t·(1.25 − Acrobatics/10000)` and nothing below 0.2 s (`the_documented_damage`; not `0.01·Acrobatics`, which is 100× too steep), armor sums pieces capped at `fMaxArmorRating` 85 and Resist Normal Weapons scales separately (`the_documented_rating`, `the_documented_resistance`; Oblivion only, Skyrim's row is `armor`). `ArmorRatingModel::with_gmst` ignores a non-finite *or negative* `fMaxArmorRating` and holds the cap to 100 — a negative one panicked (`clamp` min > max) and one over 100 made armor heal (#7194 `f8120077a`, the guard `ArmorModel::with_gmst` already had). `weapon_resistance_factor` floors at 0 only — a *weakness* to Normal Weapons is a negative resistance (50% weakness: a 20-point hit does 30) — creature natural attacks ignore the resistance, and a blow from an aggressor with no `EquippedWeapon` that is not the player's arrow is a normal weapon only below Hand to Hand `fist_mastery_skill` (`iSkillJourneymanMin`, 50); an armed creature's blow stays its weapon's, a declared capture gap (#7195 `2c5c264dc`). The unarmed test rests on those two alone, not on the projectile id (every shot lands through the `TrapHit` flush with `projectile: 0`); a script's `UseWeapon` strike carries its fired weapon's flag through `item_ignores_resistance`, the one reading `weapon_ignores_resistance` shares, pinned by `a_use_weapon_strike_carries_the_weapons_resistance_flag` (#7234 `71f31e0aa`). The bow's Fatigue term is `current / base view` of Fatigue, floored at 0 and *not* clamped at 1, so Fortify Fatigue keeps paying off (#7207 `eaff8c5ca`; the floor is a declared choice). Consumers in `byroredux/src/combat.rs` read the installed resource, never the game.
- `eval` stays allocation-free, no game branch; `DerivedStatFormula` `Copy` + 36 B
  (`formula_is_thirty_six_bytes_and_copy`).
- **Siblings**: `combat.rs` (`OblivionDamageSettings` since #7199 `12189f685`: nineteen settings — the weapon `fDamageWeaponMult`/`fDamageStrength*`/`fDamageSkill*` family, the `fHand*` Hand-to-Hand family, the Luck term (`fActorLuckSkillMult`/`iActorLuckSkillBase`, shared with `SpellCastModel`, which takes its defaults from it) and the Fatigue factor — overlaid by `with_gmst`, with `modified_skill`, `weapon_multiplier`, `hand_to_hand`, `condition_factor`, `fatigue_factor` as methods; the bow's Oblivion `DamageRule::Rating` carries it. The condition factor is `fDamageWeaponConditionBase + ratio·fDamageWeaponConditionMult` (0.5 + 0.5·ratio), not the bare ratio, #6762 `7f5e549ec`; a restated literal beside any of those settings is the regression; Hand to Hand keeps its cross-term) vs `charal-oblivion-ruleset.md` "Complete Damage Formula";
  `stealth.rs` + `stealth/` (`StealthRules::value` / `level`, M65 #6911; one `StealthKind` per formula) vs `charal-fnv-fo3-ruleset.md` "Sneak Detection" and `docs/engine/sneak-detection.md` §4.2; Skyrim armor skill scaling lives in `byroredux/src/armor.rs`, which reads the installed `ArmorModel` (`crates/core/src/character/armor.rs`, the profile's `armor` row + the load order's `fArmor*`/`fMaxArmorRating` GMST overlay, #6567 `cb9faca7d`; six of its seven settings are master-authored, `fArmorRatingPCBase` is a CK Settings-page default no master authors, #6761 `fc41d4818`), whose skill multipliers derive from `LIGHT_ARMOR_RATING_COEFF`/`ARMOR_SKILL_RATING_COEFF_NPC` (`skyrim.rs`, the only home of UESP's 0.4/1.5, #6568 `a858df844`) — the `DamageResist` derived row is not what feeds it, and a restated literal or a `GameKind` test in `armor.rs` is the regression (`the_consumer_holds_no_game_gate_or_armor_constants`). Perspective
  traps: UESP's "Opponent…" names are attacker-relative; the code reads the *wearer's* own skill (#4449).
  Since M65 (#6911) every coefficient is a game setting read from the load order (`DetectionSettings` in `stealth/settings.rs`, each row's own
  executable defaults behind them: `Fallout3.exe`, `Oblivion.exe`, `SkyrimSE.exe`, #6914; `Fallout4.exe`, #6996 `4870d7772`; Starfield is FO4's model re-based to metres, `stealth/starfield.rs`, `f256fd771` — #7046 closed with that row, formula coverage 6/6; the ambient sound class, sky factor, combat falloff set and the ambush/observed/flying range multipliers stay neutral, tracked in ROADMAP's M65 follow-ups; Starfield has no perk entry-point dialect, so its sneak hooks get unmodified values), and the attenuation clamps at
  the range as `Fallout3.exe` `0x57e300` does (`max(0, …)`), so the #6433 unclamped-attenuation precondition is gone; a
  literal coefficient in `stealth/`, or a game test in the detection tick, is the regression
  (`the_fnv_row_matches_the_captured_formula`, `the_fo3_row_drops_the_level_and_armor_terms` — the dropped armor term is sourced to the GECK *Category:Detection (Target Skill)*: "In FO3, the Target Skill is simply Sneak * IsSneaking", #7213 `f3b99dd9c`,
  `the_oblivion_row_matches_the_cs_formula`, `the_skyrim_row_matches_the_ck_formula`, `the_fallout4_row_matches_the_decoded_formula`, `the_starfield_row_matches_the_decoded_formula` — the last two are transcriptions from the disassembly, so read them for what they pin, not as captures).
**Output**: `/tmp/audit/character/dim_2.md`

### Dimension 3: Progression, Pools, Afflictions & Reputation
Paths: `crates/core/src/character/{leveling,regen,affliction,reputation,crime,vats,components,skyrim,tes}.rs`
First step: `cargo test -p byroredux-core character::leveling character::regen character::affliction character::reputation character::vats`
- **Leveling**: three models (Fallout `XpCurve`, classic-TES `SkillUse`, Skyrim `SkillXp`) are enum *data*
  variants with one consumer-side match; `level_cap()==0` = uncapped identically in all three. The roll is
  `LevelingModel::roll_levels` (#5918 `118edb1e6`): `capped = level_cap() > 0 && level >= level_cap()`, each
  level costs `xp_to_next` rounded UP to ≥ 1 whole XP (a `0 < cost < 1` curve once truncated to a zero-XP level
  and spun the main thread forever), and it stops at `u16::MAX` (`roll_levels_is_bounded_and_exact_for_odd_curves`);
  the cap off-by-one has a consumer pin, `experience_roll_stops_at_the_fallout_new_vegas_level_cap`
  (#5925 `dc259eef9`). Fallout rewards: `LevelingModel::skill_point_reward` is the one reward query
  (`skill_points` and `grants_perk_at` delegate to it since #5925; FO3 `10 + INT`, perk every level; FNV
  `10 + INT/2`, perk every other level; FO4 `SpecialOrPerk` → `(0, perk)`; `None` for the TES models, Skyrim's
  per-level perk + `pool_pick_gain` pick banked by `grant_experience` into `LevelUpPoints::pool_picks`, #5916
  `612bd179e`) banks into `LevelUpPoints` (`character/components.rs`; the half point rides in
  `skill_point_fraction` via `grant_skill_points`; pin `skill_point_rewards_follow_each_fallout`), with INT read
  through the *Base* view (base + `set_override`, not current — #5917 `9c087f15e`) and perk hook 10 (Adjust
  Gained Skill Points) composed at the consumer, not in the ruleset. `levelup` spends skill points on the AV
  *base* and perk points only under the PERK's authored `num_ranks` (`PerkEntryTable::num_ranks`; #5924
  `c3239fd55`). `with_gmst` requests exactly `["fXPLevelUpBase","fXPLevelUpMult"]` on Skyrim and
  `["iXPBumpBase","iXPBase","iMaxCharacterLevel"]` on the Fallout curves (#5915 `8791be5fe` — the cap is
  overlaid only when authored and ≥ 1, so `BrokenSteel.esm` lifts FO3's 20 to 30; pins
  `skyrim_gmst_overlay_reads_only_authored_curve_settings`, `fallout_gmst_overlay_reads_curve_and_cap`);
  `xp_per_skill_rank` is a fixed engine coefficient *by design* (do not re-flag). **Cap raises** (#6227
  `cafd3ccab`): `CharacterRulesProfile::build_ruleset(resolve, gmst, plugin_files)` applies `LevelingModel::raise_cap` *after*
  the GMST overlay, summing `level_cap_raises` (FNV only: `DeadMoney`/`HonestHearts`/`OldWorldBlues`/`LonesomeRoad`, +5 each, matched
  on the loaded file's stem from `EsmIndex::plugin_files`; no master authors it — `iMaxCharacterLevel` is in `FalloutNV.esm` alone);
  an uncapped model stays uncapped (`raise_cap_lifts_only_a_capped_curve`). **Oblivion skill-use** (#7049-#7052, all closed; `docs/engine/skill-use.md`, `docs/prd/oblivion-skill-use-leveling.md`):
  `LevelingModel::SkillUse` now carries `costs: SkillUseCosts` (`LevelingModel::OBLIVION`: Factor 0.35 / Exp 1.5 authored by `Oblivion.esm`, Minor 1.25 / Major 0.75 / Spec 0.75 the CS `fSkillUse*` defaults, UESP *Increasing Skills* the same five, #7331 `c196ea037`;
  `with_gmst` overlays the five `fSkillUse*` settings, `skill_use_costs_overlay_the_load_orders_settings`). The variant also carries `attribute_bonuses: [u8; 10]` (`iLevelUp01Mult`..`iLevelUp10Mult`) and `health_per_endurance` (`fStatsHealthLevelMult`), and `with_gmst` overlays those plus `iLevelUpSkillCount` (the major-increase threshold; a non-finite/negative value, or a count below 1, is ignored) — none is authored by `Oblivion.esm`, so a leveling mod retuning them must reach every reader (`PendingLevel::bonus`, `pick_attribute`, `apply_level`, the `levelup` report, the rest menu's offer), and a tally of 0 stays the engine's +1 (#7196 `4e91d26c6`). The size pin `leveling_model_is_copy_and_small` is ≤ 48 B since that (was 32). The rank cost is
  `(factor·L)^exp · spec · (major ? major_mult : minor_mult)` (the major multiplier *replaces* the minor one), `SkillUseCosts::bank` ranks only when the
  cost is *strictly below* the banked XP and never past `SKILL_USE_CAP` 100 (`the_cost_table_matches_uesp`, `an_equal_cost_does_not_rank`, `a_skill_never_exceeds_100_by_use`);
  `OBLIVION_SKILL_USE_VALUES` (21 rows, Armorer 12 … Speechcraft 32) is the `SKIL` `DATA` use-value table. The saved `SkillUseLevel` counts each rank-up toward its governing attribute's tally and a
  *major* one toward the level; the tenth major increase freezes the tallies into a `PendingLevel` (`the_tenth_major_increase_freezes_its_tallies_into_a_pending_level`), `oblivion_attribute_bonus`
  reads that frozen tally (0 → +1, 1-4 → +2, 5-7 → +3, 8-9 → +4, 10+ → +5), the player picks three attributes (`levelup attr`, `pick_attribute`) and
  `apply_pending_level` applies the level at a completed sleep (`oblivion_level_system`): attributes capped at 100, Health's per-level gain into `ActorValue::stored`
  (`apply_oblivion_level_up_health`, `floor(fStatsHealthLevelMult·Endurance)` read after the picks, #7044). Every level-up message carries flavour text: levels 2-20 read the load order's `sLevelUp{N}` (the master authors only nine: 4, 6, 11, 12, 15, 17, 18, 19, 20) and otherwise UESP *Oblivion:Leveling*'s documented text (`LEVEL_UP_MESSAGES`), level 21+ its own documented message (#7209 `c9b684eaf`).
  **Banking gate**: `bank_skill_use` banks nothing once the skill's *current* value (Fortify included) is ≥ 100 — the recorded rule is `GetAV(skill) >= 100` — while the rank-up still writes the base layer; the scripted raisers (`AdvancePCSkill`, `ModPCSkill`) are unchanged by it (#7210 `723c5281c`, owner-decided 2026-11-22). Both raisers are one `raise_skill_ranks(world, player, code, ranks)` that sets the skill's banked experience to 0.0 even when no rank rises (PRD Q5, the Beta Patch 1.1 behaviour CS *ModPCSkill* documents and CS *AdvancePCSkill* says it shares; the former *keep_surplus* flag, the "banked − cost" carry rested only on the executable, #7310 `bc67d1c2f`, pinned by `the_script_calls_raise_ranks`) — a surplus-keeping raiser is the regression. The armor-hit roll (which of Light/Heavy Armor a hit on the player feeds) has only its slot weights documented (CS *iArmorDamageXXChance*, shield 30 included); the roll order, the shield taking the remainder (`iArmorDamageShieldChance` is not read; it equals the remainder at the defaults) and `ARMOR_HIT_ATTEMPTS` = 7 are a *declared engine choice* (#7310 `bc67d1c2f`, `docs/engine/skill-use.md` §3), not the original's — do not file them as deviations and do not "source" them from a disassembly. The emitters (`byroredux/src/skill_use_emit.rs`) feed only `FED_SKILLS` (16 of 21; #7053-#7056 all closed) — Athletics, Blade, Block (a blocked hit on the player, #7054), Blunt, Hand to Hand, Heavy Armor, the six magic schools (the player's cast action, one use for the spell's primary school; `magic.cast` and a script `Cast` feed nothing), Acrobatics (jump; fall damage #7056), Light Armor, Marksman (the player's own arrow hit, #7055), Sneak;
  Armorer, Alchemy, Mercantile, Security and Speechcraft are unfed (the `levelup` report says so) — do not file those as bugs, and a skill joins `FED_SKILLS` only in the commit that adds its emitter. The writers hard-code the engine roster's order (cast: Luck 7, Magicka 9, Fatigue 10, schools from 20; emitters: Blade 14 … Block 15, Marksman 28; `FED_SKILLS`, `OBLIVION_SKILL_AV_FIRST/LAST`): two tests pin each constant against `OBLIVION_ACTOR_VALUES[code]` (one beside the cast's private codes, one in `skill_use_emit.rs`), so a roster correction cannot silently retarget a writer (#7204 `531e4f831`). No game branch: the arm is the model's own,
  and a ruleset with another leveling model drops queued uses. Skyrim pool base 100, +10/level
  (`SKYRIM_POOL_BASE`, `pool_pick_gain`); `Perks::try_set_rank` rejects out-of-range rather than clamping. A scripted `AddPerk` on a held perk raises its rank (capped at `num_ranks`, never lowered) only where `CharacterRulesProfile::add_perk_raises_rank` says so — FO3/FNV (GECK) and FO4 (CK), `false` on Skyrim (sources silent, an open question) and FO76/Starfield — read once into `PerkEntryTable` (#6619 `2d9c5802c`, `add_perk_rank_row_is_stated_on_every_profile`); a consumer-side game test is the regression.
- **Regen tick**: fixed-60 Hz (`POOL_REGEN_DT`), backlog clamped (`MAX_REGEN_SUBSTEPS`), zero-dt cannot
  spin; the scheduler access declaration matches the body (`/audit-concurrency` Dim 4 owns the rule).
  AP lane unit: `fActionPointsRestoreRate` is per second on FO3 (4) but per *frame* on FNV (0.06); `VatsModel::ap_regen_per_second`
  divides by `POOL_REGEN_DT` (not a hard-coded 60, #6389 `f8a1b34bd`) so the installed rate is 3.6/s on FNV — a raw read of the GMST is the regression.
- **VATS model** (`vats.rs`): every number resolves load-order-merged GMST -> compiled-in default (`with_gmst`; a bounded family rejects
  out-of-range authored values); `FALLOUT_NV` carries the six re-tunes FalloutNV.esm authors (#6424), FO3 the wiki defaults; the model is the
  profile's `vats()` row (#6423) — `build_vats_model` must not test the game. Check each default against `charal-fnv-fo3-ruleset.md`
  (*VATS tuning, Action Points regen and the radiation gauge*); steps in `to_hit_chance` marked RECONSTRUCTED stay marked until the #6350 captures land (#6403 closed as a documented placeholder). `VatsHitInput::part_to_hit` is the `BPND`@8 0-100 chance, not a 255 sentinel (#6764 `c100b400c`). **Limb effects** (`limb_effects.rs`): `LimbEffectModel::FALLOUT.with_gmst` — leg speed, arm melee/gun-spread terms from the load order's settings; the crippled test is `condition <= 0` on a *present* value (an absent AV is never a limb at 0); the un-cripple threshold is unverified (VATS-LIMB-005).
- **Affliction pool** = the *composed* value (`AfflictionTable::pool_value` reads `ActorValues::current`, the layer `GetAV` and the HUD rad gauge read; writers accumulate on `mod_temporary`/`mod_permanent`, the `damage` layer is not the pool — #6428 `cf58dec5a`, `the_rad_gauge_and_the_affliction_pool_read_one_write`); rad accumulation and RADS-stage penalties are unbuilt.
- **Affliction** = diff-and-reapply (remove previous band's penalty before applying the new one — a
  missing removal compounds); bands half-open and consistent; `band_for` vs `band_by_key` tie-break (#4456).
- **Resistance/reputation**: `damage_multiplier` clamps `[0, cap]`, never negative (no healing);
  `STANDING_GRID` axes `[infamy][fame]` (transpose = plausible wrong standing); karma/affinity clamp both
  ends; per-faction `FactionRepThresholds` rows match the capture row-for-row. `FactionReputation` is a saved player column (#6759 `8fc1a9ef9`; `AfflictionStatus` still has no production mutator; `Background`'s writers are `SetClass` on the player and `player_identity_sync_system`, both keeping `CharacterIdentity.class_form_id` the source of truth, #7348).
- **Crime** (`crime.rs`): `CrimeModel::OBLIVION` / `SKYRIM` are profile rows (`crime()`; `None` on FO3/FNV/FO4/FO76/Starfield — FO3/FNV
  crime is faction-based and unmodelled), installed as a resource by `byroredux/src/crime.rs`, never a consumer `GameKind` match. Check each
  default against its cited CS/UESP line (`fCrimeGoldSteal` 0.5, attack 40, murder 1000, prison divisor 100, `rush_bounty` 500 =
  Oblivion.esm's authored `iCrimeGoldAttackMin`); `theft_rounds_up` (Oblivion up, min 1 septim; Skyrim down) and the saturating
  `as i32` casts; `with_gmst` ignores non-finite/negative values; Skyrim's jail sentence is capped by `max_sentence_days` = `Some(7.0)` (engine rule, not a GMST; Oblivion `None`, #6757 `a28b85810`). Pins `a_theft_fines_half_the_value_rounded_up_and_never_less_than_one`,
  `skyrim_rounds_a_theft_down_and_keeps_the_bounty_per_crime_faction`. Trespass 5 / pickpocket 25 are `CrimeModel` fields (`trespass_gold`/`pickpocket_gold`, read from `iCrimeGoldTresspass`/`iCrimeGoldPickpocket` in `with_gmst`, #6427 `3a0b6be0a`,
  `the_trespass_and_pickpocket_fines_are_authored_settings`), read by `crates/scripting/src/crime_faction.rs::faction_fine` only where a faction's Crime tab does not author them; a fixed const there is the regression.
- Every constant here needs a **Source**; regen rates and band thresholds are exactly the numbers typed
  from memory.
**Output**: `/tmp/audit/character/dim_3.md`

### Dimension 4: Population Boundary (parse → ruleset → actor) — highest yield
Paths: `byroredux/src/npc_spawn*`, `crates/plugin/src/esm/records/{actor_value_derive,actor/mod}.rs`, `crates/plugin/src/equip.rs`, `byroredux/src/inventory/`, `byroredux/src/cell_loader/references/`, `byroredux/src/chargen.rs`
First step: `cargo test -p byroredux --bin byroredux resolve_inherited_call_sites_are_enumerated_and_pinned; git log --since=<last report> --format='%h %cs %s' -- <Paths>`
- **One derivation, one implementation** (the class that keeps recurring): `effective_actor_level` lives
  once in `actor/mod.rs` and is `.max(0)` on the non-multiplier branch — a second copy or a `.max(1)` (also
  in `examples/`) is the regression (#3171/#4095); `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` calls it through the plugin crate. It stays unfloored on purpose; the level-0 hole is closed one tier up (#4232 `4fbff2c6c`): `ResolvedNpc::level()` backfills a terminal level of 0 from `stats_pick_level` — the authored entry level of the deepest LVLN pick the Use-Stats walk went through — so gear/spell expansion no longer resolves every list at level 0; an authored level or a list-less chain is unchanged (a `.max(1)` in `effective_actor_level` itself is still the regression). **Oblivion is not routed through it** (#7193 `cc907aa92`): ACBS bit 7 is FO3/FNV's *PC Level Mult* but Oblivion's *PC Level Offset*, so `ResolvedNpc::level` answers an Oblivion record (one carrying `oblivion_pools`) from `actor_level_at_spawn` and holds an NPC's level to the player's (UESP *Leveled Lists*: the lowest of the NPC's and the player's; a creature keeps its own). `ResolvedNpc::with_player_level` is set once at the spawn boundary (`npc_spawn/resumable/`) with the same `player_level_for_spawn` the stats stamp reads — a second player-level source, or Oblivion gear/spells resolving through the bare `effective_actor_level`, is the regression (`oblivion_leveled_lists_resolve_at_the_actor_level_held_to_the_players`, `oblivion_offset_creature_uses_the_live_player_level_for_stats_and_level`). A PC-offset creature's three skills and attack damage are stamped as authored, a gap the capture now lists as its Known gaps item 5 (#7206 `239600469`); choosing a coefficient waits on a recorded observation from the original game. TPLT template inheritance: FO4 authors a per-flag `TPTA`
  Template Actors array (`NpcRecord::template_actors: [u32; 13]`, 0 = use the `TPLT` default) and
  `walk_inherited_records` picks the flag's own entry when non-zero (#5498 `99c22a8db`; FO4-only data, no game
  branch — pinned by `crates/plugin/src/equip.rs` tests). Since #4457 the population boundary resolves
  every category once through `ResolvedNpc::resolve` (`crates/plugin/src/equip.rs`; M47.2 added the
  `base_data` terminal for "Use Base Data", `TEMPLATE_FLAG_USE_BASE_DATA`, which carries Essential/Protected —
  decoded per game in `parse_npc` into `NpcRecord::essential`/`protected` since #5921, so `ActorProtection::from_record`
  takes no game)
  and consumers read the resolved type; the guard pins production `resolve_inherited_*` calls per file (one left, the pre-spawn
  race boundary `cell_loader/references/mod.rs`; `player_body.rs` joined in #5047
  `c20dcdb04`, which also moved the player's level/Background/outfit reads onto the Use-Stats/Use-Traits/
  Use-Inventory terminals, and left again when #6435 `218948853` routed its race and walk-clip gender through `traits_terminal`, expected count 0) — a new direct call or a raw `npc.<field>` read is the recurrence
  (fix-by-addition made it worse in #4086). Verify the enumerated count moved only deliberately, and that new reads honor `template_flags` rather than overwriting inherited values (only Def Pack List 0x0400 still lacks a consumer: #4137; Script 0x0200 is the VMAD attach's `scripts_terminal`; Skyrim's Use Keywords 0x1000 gained a `keywords_terminal`, #6515 `26850aa5f`).
- **Base-FormID readers of Use-Traits fields** (#6228 `b76a74e45`): sex/voice/race for readers that hold a base FormID rather than a
  spawn's `ResolvedNpc` go through `equip::traits_terminal` — the dialogue voice key (`systems/dialogue_voice.rs`), `ActorBaseTraits`
  (`GetIsSex`/`GetSex`/`GetVoiceType`/`GetLeveledActorBase`, which is its own arm now, not an alias of `GetActorBase`), the gendered
  animation subgraph (`crates/scripting/src/anim_subgraph.rs`), the `HasKeyword` base table (`crates/scripting/src/keywords.rs`) and the player body's race/walk-clip gender (`byroredux/src/player_body.rs`, #6435). `GetActorBase().GetSex()` deliberately stays the shell's. The
  `resolve_inherited_call_sites_are_enumerated_and_pinned` test also asserts all five call `traits_terminal(`; a new reader of
  `NpcRecord` sex/voice/race that reads the placed shell is the recurrence.
- **Writers vs stamps (silent no-op class)**: for every writer that gates on a component
  (`consume_item`/`restoration_system`/drowning gate on player `ActorValues`+`ActorVitals`), find a
  *production* insert of that component on the entity class the writer targets; unit tests hand-insert and
  hide the gap (#4458: fixed by `attach_to_player`; recurred as #5413 `212a71dba` — the player body path skips
  `stamp_spell_list`, so `RaceSpells` was absent and the scripted `AddRaceSpells`/`RemoveRaceSpells` pair no-oped
  until `attach_to_player` stamped it from `resolve_racial_spells`). The player now carries `CharacterLevel`/`Background`
  from the resolved Player record (#4678); `CharacterLevel` is saved since M47.2's kill experience (v46),
  which retired #2947's `validate_progression_state` guard; `LevelUpPoints` is saved too (`38539eee8`), as are `FactionReputation` (#6759 `8fc1a9ef9`) and the per-actor `DismemberRecord` (`d50fc1153`, parked by the per-placement `ReferenceState` so a corpse keeps its killing blow across a cell unload). The
  player's `FactionRanks` is re-derived from the record (Use-Factions terminal) on load, not saved (#5058
  `be3cd9468`); scripted edits (`FactionRanks::set_rank` / `remove`, Papyrus `SetFactionRank` /
  `RemoveFromFaction`) are instead recorded by reference FormID in the saved `ScriptedFactionChanges` ledger
  (`b3556ef6c`) and replayed on spawn and load — a direct write to `FactionRanks` that bypasses the ledger is
  lost on load.
- **Magic modifiers (#4415, closed)**: `byroredux_scripting::magic::apply_constant_modifiers` adds constant-spell
  amounts to an `ActorValues` *permanent modifier* at spawn and on `AddSpell`/`RemoveSpell`. Check it never
  writes base, that add/remove are symmetric, and that no other writer *overwrites* `permanent_mod`
  (`set_permanent` replaces the whole constant-spell sum — the pre-#5412 `SetBase` reroute did exactly that).
  Composition is `current = base + set_override + stored + permanent + temporary − damage` (`stored` is Oblivion's level-up Health term, #7044, a required saved field — save v136). The views stack on one another: `ActorValue::base_view()` = base + set_override + stored, `permanent()` = + permanent_mod, `undamaged()` builds on it — `actor_value_view`'s authored fast path and `GetPermanentActorValue` read the accessors, never a hand-sum that omits `stored` (#7205 `c781ad41d`; the SDK projection folds through `undamaged()` too, #7184). **Derived-value composition** (#5042 `c9254beb8`):
  `CharacterRuleset::actor_value` is the one composer — an authored carried base (`ActorValue::base_authored`,
  set only by `set_base`) wins; otherwise an actor-general `Absolute` row supplies the base and the entry's
  modifier + damage layers compose on top (a modifier-only placeholder entry once read as the bare modifier:
  Finesse CritChance 5). `GetActorValue` (`crates/scripting/src/condition/eval.rs`) and
  `melee_damage_charal_bonus` must both call it; a consumer reading `derived_value` or the raw entry alone is
  the regression. The composer derives its layer term as `value.current() - value.base`, never a hand-summed
  subset, so a new layer in `ActorValue::current` cannot be missed (#5706 `ad26650ec` — the derived branch had
  dropped `set_override`; pin `set_override_composes_onto_the_formula`). **Player pools** (#5039): the player's `PlayerOnly` Health/AP base is a *cache* of the
  formula, refreshed by `CharacterRuleset::refresh_player_only_bases` (allocation-free) at stamping, after
  `attach_to_player`'s constant spells, and every frame by `player_derived_stats_system`
  (`byroredux/src/systems/character.rs`, Update) — the one documented exception to computed-on-demand
  (`docs/engine/charal.md` §6); a refresh must preserve the damage, `set_override` and modifier layers.
- **Oblivion population** (#7009/#7010; `docs/prd/oblivion-actor-values.md`): there are no AVIF records, so `EsmIndex::install_oblivion_actor_values`
  synthesises the 72-name roster (`OBLIVION_ACTOR_VALUES`, xEdit `wbActorValueEnum` order; indices 38/39 are excluded from script commands) into a free regular
  slot — atomic and idempotent, a conflicting slot is an `Err` — and every resolver (`actor_value_form_id`, `oblivion_actor_value`) sees them as ordinary AVIFs; check no second roster or hard-coded index→FormID table appears
  (`obscript_actor_values::actor_value_form` is the script-side lookup). `NpcStatModel::OblivionNpc`/`OblivionCreature` route to
  `oblivion_population::derive`: stored NPCs (neither auto-calc nor level-offset, and the player's `Player` record) seed their authored `DATA`; auto-calc NPCs derive from race/class/level
  (`fAttributeClassPrimaryBonus` and `fAttributeClassSecondaryBonus`, Personality left at its `DATA` byte as `Oblivion.exe` `0x522345` does), and `actor_level_at_spawn` takes the player level for a
  level-offset NPC clamped by the record's calc min/max. `ActorValues` aliases (`set_alias` / `get_base`: a logical AV reading its current and base from different canonical entries) are how
  Oblivion's paired values resolve; a base-view reader (`DerivedInput::a_from_base`, `chargen`'s base read) must call `get_base`, not `get`
  (`base_derived_formula_uses_the_base_view_of_an_alias`). The player's Health/Magicka/Fatigue are `PlayerOnly` formulas refreshed per frame, so the record's authored pools are replaced
  (`oblivion_profile_refreshes_all_three_player_pools`; the profile comment at `build_ruleset`'s Oblivion arm now says the level-up is wired, #7067 `3854f4c53`).
- `build_character_ruleset` returns `None` for FO76/Starfield only (Oblivion builds since #7010); every caller must treat `None` as
  "no CHARAL for this game", never "use the default ruleset" (Fallout formulas on a TES actor).
- `resolve` = `index.actor_value_form_id(editor_id)`: an unresolved EditorID skips its formula
  (`push_derived` resolve-or-skip), never registers one keyed on `0`.
- Ordering: base attributes + skills written before dependent derived stats; FO3/FNV class auto-calc
  `skill = 2 + 2×SPECIAL + ceil(Luck/2)` implemented, tag-skill part still *absent* not guessed.
  FO3/FNV NPC Health is the **NPC** curve, not the player row (#5238 `c71eeed80`): `NpcHealthCurve` in
  `NpcStatModel::ClassAutoCalc` = `NPC_` `DATA` Base Health + `fAVDNPCHealthEnduranceMult`·(END + offset) +
  `fAVDNPCHealthLevelMult`·(L + `level_anchor`), measured 5 / 5 / −1 / −1. `with_gmst` overlays the Endurance
  offset and the two mults only: the level anchor is an engine-hardcoded −1 (the player formula's `Level − 1`;
  no level-offset GMST exists in either exe), so a retuned Endurance offset moves only the Endurance term
  (#5340 `5ed49ed5c`, `the_level_anchor_is_engine_hardcoded_and_never_overlaid`). Pins
  `fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples` — all 15 infobox samples since #5342 `274ba13c5`, which
  also restored the player Health row beside the NPC row in the capture — and the `#[ignore]`d
  `fo3_fnv_named_npc_health_matches_the_authored_curve`. The player-curve constants (bias 90/95, END·20)
  on an NPC path are the regression. Non-auto-calc NPCs (ACBS auto-calculate bit clear, ~40% of the FO3/FNV
  census) seed their authored `NPC_` `DATA` SPECIAL (`NpcRecord::data_attributes`) instead of the class `ATTR`,
  falling back to the class for a `DATA`-less shell; skills still derive from the SPECIAL, the DNAM per-skill
  half is unparsed (#5338 `4957536ba`).
- Skyrim `derive_skyrim_actor_values`: Health/Magicka/Stamina resolve **independently** (race base +
  signed ACBS offset, own AVIF; `RaceRecord.starting_*` are `Option<f32>`) — a missing pool suppresses
  only itself. For an auto-calc NPC the class/level term is implemented
  (#5914 `b9e60846e`: `skyrim_class_pool_points` spreads `10·(level−1)` points over the three pools by the class weights, plus `5·(level−1)` Health);
  a non-auto-calc NPC gets none, and prose still saying "race base + ACBS offset only" is doc rot (#6285 `27f022dfd`). It also
  emits the 18 skills (`7c740eca0`, `derive_skyrim_skills`): auto-calc NPCs = `SKYRIM_BASE_SKILL` 15 + race
  bonus + the class's `skyrim_skill_weights` share of 8 points/level (`skyrim_class_skill_points`, UESP *CLAS*;
  capped at 100; skills and pools share one `distribute_by_weight::<N>` routine since #6157 `f520d8d77` — equal weights break by
  descending actor-value index in both, so a divergent second copy is the regression); a non-auto-calc NPC uses its `DNAM` `skill_values` as written (race bonus not added — the
  combination is undocumented, so absent, not guessed). `SKYRIM_SKILLS` names the AVIF editor ids; Illusion is
  `AVMysticism` (`a8dc9e8e6`) — an unresolved skill name drops silently, so the all-18 check
  `vanilla_skyrim_skill_names_all_resolve_and_a_vanilla_npc_derives_them` (`#[ignore]`d) is the guard.
  Morality is seeded from the `AIDT` Responsibility byte (`ActorAiData::responsibility`, Skyrim-only decode) by
  `derive_resolved_actor_values` unless the stat model already supplied it (#5927 `a1eece82a` — `GetBribeAmount`
  once priced every NPC at Morality 0; `skyrim_spawn_seeds_morality_from_inherited_aidt_responsibility`).
- **Chargen answers** (`byroredux/src/chargen.rs::apply`, FNV `VCG01`): SPECIAL lands as `set_base` on codes 5..=11,
  clamped to the GECK 1..=10 (`SPECIAL_MIN`/`SPECIAL_MAX`), and the derived pools follow through
  `player_derived_stats_system` — a chargen write into a derived output or a `set_override` is the regression. Tag skills go
  through `byroredux_scripting::obscript_more::set_player_tag_skill` (the `SetPlayerTagSkill` path, `fAVDTagSkillBonus`,
  three slots) — a second tag-bonus implementation is the regression; traits are `Perks::set_rank(…, 1)` / `remove` over
  the playable `is_trait` PERKs. Name/sex/race and the script-set class (`class_form_id`, written by `SetClass`, 0 = the record's own; #7348 `19daf084e`, a stable-FormID column like the race, `df5db4b46`) live in the saved `CharacterIdentity`; `Background` is not saved, so
  `player_identity_sync_system` re-seeds its race and class from the identity (each only when set, writes only on a change; a class a script gives another actor is still unsaved). The SPECIAL/skill code ranges
  (`SPECIAL_CODES` 5..=11, `SKILL_CODES` 32..=45) are Fallout actor-value codes hard-wired in the module — the one
  non-table per-game constant set in the population path; verify they stay chargen-only and that retired skills
  (`OBSOLETE_MARKER`, FNV's Big Guns) are never offered.
- **One ceiling composition**: `ActorValue::undamaged()` (base + set_override + stored + permanent + temporary) is the
  single ceiling; `current()` is `undamaged() − damage`. HUD bar fractions, debug vitals and the SDK projection
  read through it (the SDK folds `set_override` into base — `ActorValueState` has no override slot); a reader that
  re-sums layers by hand is the regression (#5508 `627149b62`, `undamaged_includes_every_layer_but_damage`).
- `setav`/`modav` write the *base* component, not a derived output the next tick recomputes — except the
  player's `PlayerOnly` derived pools: `setav`/SDK `SetBase` there route through
  `CharacterRuleset::is_player_derived_pool` into the `set_override` layer (`ActorValues::set_override`, composed
  total = formula base + value + any constant-spell modifier), because `refresh_player_only_bases` would revert a
  base write in one tick (#5239 `2464a52d7`; re-layered by #5412 `a614eb273` — the first destination,
  `set_permanent`, erased carried ability bonuses and made their removal subtract twice;
  `set_override_survives_the_player_refresh_and_keeps_permanent_mod`). Sourcing per `charal.md` §6: the Oblivion
  CS SetActorValue note (FO3/FNV inherit it unverified) plus the FO4 CK `SetValue` "modifiers are left intact".
  Non-player targets and actor-general stats keep `set_base`.
**Output**: `/tmp/audit/character/dim_4.md`

### Dimension 5: Coverage, Documentation & Doctrine Drift — second-highest yield
Paths: `docs/engine/charal*.md`, `crates/core/src/character/mod.rs` docstring, `docs/feature-matrix.md`, `ROADMAP.md`
First step: `grep -rn 'not yet reachable\|oblivion_pool_regen_config\|when a live .CharacterRuleset' crates byroredux docs ROADMAP.md`
- **Coverage matrix** (publish even with zero findings): family × {ruleset implemented, wired, derived
  stats, leveling model, regen wired, affliction wired}. Re-derive wiring from `profile.rs` arms + workspace
  greps, never from prose. Today: FO3/FNV/FO4/Skyrim/Oblivion wired, FO76/Starfield absent; regen
  live for FO3/FNV AP and Oblivion Fatigue/Magicka (the Oblivion rates read from the load order, #7197), affliction inert everywhere; Oblivion also has the skill-use leveling runtime (16 of 21 skills fed), the spell-cast / bow / block / fall / armor-rating rows (Skyrim: `bow` + `armor`) and the starting-disposition row; M65 detection rows on FO3/FNV/Oblivion/Skyrim/FO4/Starfield; VATS runtime FO3/FNV only, limb damage routing and crippling-by-play absent (known-open list); Skyrim armor row wired (`armor`), FO4 `vats`/`armor` rows `None`; crime on
  Oblivion/Skyrim rows (FO3/FNV crime is faction-based and unmodelled); "leveling model" is a model, not a runtime.
- **Wiring events rot prose**: after any commit that wires/unwires a builder or resource, grep for the old
  claim in the `mod.rs` docstring, the capture Status columns (code ahead of capture is doc rot too),
  `docs/engine/charal.md`, `docs/feature-matrix.md`, `ROADMAP.md`, and code comments describing an
  *intended* insertion site. `mod_docstring_indexes_every_sub_module` checks module *names* only.
- **Regression checks from the 2026-11-21 doc sweep** (#7214-#7220, all closed): the captures carry the Oblivion block / fall / bow, Skyrim archery and FO3/FNV crippled-limb rows (#7214 `b8fb962a9`); `docs/feature-matrix.md` and ROADMAP rate Oblivion's character runtime, M65, crime and the rule rows (#7215 `a8e07797e`); `charal.md` §4.7/§5, the Oblivion capture and `skill-use.md` describe the wired ruleset, live regen lanes and built level-up (#7216 `293037aed`, #7217 `4aa677be8`); `CharacterRulesProfile`'s accessor and field docs match their rows (#7219 `76f826970`); the M65 captures, `sneak-detection.md` and its index row report the built stealth rows (#7220 `91e91aeac`). No production source may name the closed #3768 "pre-AVIF resolver" as a blocker — `no_production_source_names_the_closed_pre_avif_resolver_as_a_blocker` (`byroredux/src/workspace_hygiene_tests.rs`, #7218 `9765d4b1f`) scans every crate and fails if the wording returns; comments naming Oblivion's actor values, ruleset or regen state a live mechanism.
- Cite symbols, not line ranges, in docs (`CharacterRulesProfile::OBLIVION`, not `profile.rs:82-87`).
- Every capture document has an implementation or an explicit "not implemented" note; a capture with
  neither is silent scope loss. Vocabulary stays `translate`/`canonical`/`resolve` (NIFAL/EXAL/PHYSAL/WATAL).
**Output**: `/tmp/audit/character/dim_5.md`

## Phase 3: Merge

Combine `/tmp/audit/character/dim_*.md` into `docs/audits/AUDIT_CHARACTER_<TODAY>.md`: **Executive
Summary** (severity counts; which families' constants were verified against captures) · **Constant
Verification Table** (formula × code × document × verdict; a row with no document value is `UNSOURCED` and
itself a finding) · **Coverage Matrix** (Dim 5) · **Findings** by severity, deduplicated · **Known-Open
Register** (restate the items above; confirm none re-filed). Cross-audit routing: component shape →
`/audit-ecs`; AVIF/CLAS/NPC_ parsing → `/audit-esm` Dim 4; CTDA evaluation → `/audit-scripting`;
scheduler access → `/audit-concurrency` Dim 4; gameplay writers → `/audit-gameplay`.

## Phase 4: Cleanup

`rm -rf /tmp/audit/character`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_CHARACTER_<TODAY>.md` (domain label `character`; add the matching
`game:*` — CHARAL findings are almost always one title's ruleset).
