**HEAD**: 09940503b · **Baseline**: [`AUDIT_CHARACTER_2026-10-31.md`](AUDIT_CHARACTER_2026-10-31.md) (HEAD `670f81917`) · **Audited**: Dim 1 (Ruleset Seam), Dim 2 (Derived Formulas), Dim 3 (Progression/Pools/Crime/VATS), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: none. All five dimensions had commits in `670f81917..HEAD` (403 commits repo-wide).

# Character / CHARAL Audit, 2026-11-07

`/audit-character` at default scope (`--depth deep`, all five dimensions). One Task agent per dimension (at most three concurrent), each told to read the capture documents before the Rust constants. Nothing was edited, committed or filed, and the engine was not launched. Builds used `CARGO_INCREMENTAL=0` in the repo `target/`. Real-master reads were byte scans (read-only). The orchestrator re-read the source for the four MEDIUM findings and confirmed each (M-01 no GMST read in `fallout.rs`; M-02 `sentence_days` has no ceiling; M-03 `remove_from_all_factions` clears the component with no ledger write; M-04 the exemption row still says "no production insertion or mutation site" while `adjust_standing` inserts and mutates).

## Executive Summary

**15 findings: 0 CRITICAL, 0 HIGH, 4 MEDIUM, 11 LOW.** All are NEW. None is a wrong number on a vanilla load order; no coefficient in the constant table disagrees with its capture.

- **Constants verified**: FO3, FNV, FO4, Skyrim and Oblivion derived rows, Oblivion/Skyrim armor and combat formulas, FNV/FO3 sneak detection, VATS/AP defaults (every `fVATS*` and `fActionPoints*`), leveling curves and caps, crime defaults, reputation tables, and FO76/Starfield pool rosters. The master-authored GMSTs were re-measured against the shipped masters (Fallout 3, New Vegas, Fallout 4, Skyrim SE and LE, FO76, Starfield). 40 derived-stat rows plus about 60 VATS/regen/leveling/crime/reputation rows: no value FAIL, one behavioural FAIL (M-02, the Skyrim jail cap), and a group of UNSOURCED-in-capture rows (L-07).
- **Baseline fixes confirmed**: #6567 (armor profile row `ArmorModel` plus GMST overlay), #6568 (one home for 0.004/0.015), #6569 (jail declines with no model), #6440, #6441, #6442, #6430, #6431, #6432, #6433, #6435, #6619, #6625, #6422 and #4232's mechanism. No `GameKind` was added in `npc_spawn/` or `crates/core/src/character/` (per-file counts identical to baseline).
- **The four MEDIUMs**:
  - **M-01**: the FO3/FNV/FO4 derived rows still bake in GMSTs the masters author, the one CHARAL family `#6567` did not reach.
  - **M-02**: Skyrim jail sentences have no 7-day cap.
  - **M-03**: ObScript `RemoveFromAllFactions` bypasses the `ScriptedFactionChanges` ledger.
  - **M-04**: the save registry exempts `FactionReputation` as having no mutation site, though ObScript and the SDK mutate it.
- **The audit brief is stale**: the SKILL says 16 modules (now 17, `armor.rs`), lists #6403, #6440, #6441, #6442 and #6422 as open or absent (all CLOSED), says Concentrated Fire is absent (live since `5502c2f97`), and does not list the `armor` or `add_perk_raises_rank` profile rows. Route to `/audit-sync`.

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect character` | 173 passed, 0 failed, 1 ignored (`vats::tests::to_hit_chance_matches_recorded_in_game_percentages`; its ignore reason now names #6350). Baseline was 165 passed |
| core `character::{leveling,regen,affliction,reputation,vats,crime}` | 75 passed, 1 ignored |
| core `character::{derived,fallout,tes,skyrim,armor,resistance}`; `stealth::`, `combat::` | 49 passed; 33 passed |
| core `character::{profile,ruleset}` | 27 passed |
| `mod_docstring_indexes_every_sub_module` | passed (asserts 17 modules; real count 17) |
| `cargo test -p byroredux --bin byroredux resolve_inherited_call_sites_are_enumerated_and_pinned` | 1 passed (only `cell_loader/references/mod.rs` has a direct production call; every other file 0) |
| bin `npc_spawn:: inventory:: player_body stamps perk_entry add_perk character level` | 336 passed, 21 ignored (real-master) |
| bin `npc_spawn::` (Dim 1 run); `profile`; `armor::`; `systems::vats` + DT | 179 passed / 14 ignored; 16 passed; 6 passed; 24 passed |
| bin `armor:: smithing player_derived profile` | 29 passed |
| `byroredux-plugin` `equip actor_value_derive esm::records::actor`; `fn586` | 205 passed / 10 ignored; 1 passed |
| `byroredux-scripting` `perk fragment::tests`; `add_perk` | 145 passed; 4 passed |

Float-exactness probes (scratch `rustc`, outside the repo): `ceil(0.05f32·U + 0.5f32)` equals the integer `ceil((10+U)/20)` for U in 0..=255; the FO4 Health `floor` expression equals the integer form for END 1..=20, L 1..=255. Read-only probes: `crates/plugin/examples/probe_gmst_vats` on FalloutNV.esm and Fallout3.esm; byte scans of Oblivion/Skyrim crime GMSTs, FNV/FO3 `BPTD` `BPND`@8 and FNV `WEAP` `DNAM`@40.

## Delta since the baseline

Commits in `670f81917..HEAD` that touch CHARAL or its population boundary, verdicts merged from the five dimensions:

| Commit | Change | Verdict |
|---|---|---|
| `cb9faca7d` (#6567) | `ArmorModel` profile row (Skyrim only) + `with_gmst`; `byroredux/src/armor.rs` reads the installed resource | PASS. No `GameKind` or old consts in the consumer; pin `the_consumer_holds_no_game_gate_or_armor_constants`; row on all 8 profiles. Provenance wording: L-02; prose: L-04 |
| `a858df844` (#6568) | `ArmorModel::SKYRIM` derives from `LIGHT_ARMOR_RATING_COEFF` / `ARMOR_SKILL_RATING_COEFF_NPC` | PASS. Residual prose: L-04 |
| `dcc3dc681`, `81332fc38` (#6569, #6573) | `send_player_to_jail` declines with no model; deposits before taking | PASS on those defects. New problem on the path: M-02 |
| `2d9c5802c` (#6619) | `add_perk_raises_rank` profile row | PASS (pinned, varies profile only). Capture line missing: L-11 |
| `4fbff2c6c` (#4232) | `ResolvedNpc::level()` backfills a level-0 terminal from the LVLN pick | Mechanism PASS (`effective_actor_level` still `.max(0)`); partial adoption: L-09 |
| `5502c2f97` (#6422) | Concentrated Fire prices repeat shots at one part | PASS in code (63/68/73 wiki example). Prose stale: L-06 |
| `6186ab77e` (#6625) | VATS ends on cell transition, death, load | PASS |
| `a9748e54c`, `bb60b15cf`, `e669918ad` (#6441, #6440) | VATS skill pricing at the weapon's own AV (neutral 50 if uncarried); DT once per shot via `HitShots` | PASS |
| `7350efc5c` (#6484) | VATS cover fan owned-body sets from `ActorColliderOwner` | PASS (no two locks held) |
| `2d6af0aeb` (#6403) | Ground-truth placeholder names tracker #6350 | PASS. #6403 now CLOSED; test stays `#[ignore]` |
| `a10bb9764` (#6430) | `PoolRegenConfig` TES lanes `Option<u32>`, `None` | PASS in code; `charal.md` still shows `u32`/`0` (L-06) |
| `e41fb357c` (#6442), `576dbe973` (#6431), `6b637eacb` (#6432), `87c9b9eb2` (#6436), `7e9678fd8` (#6433) | barter exe citations, Oblivion pool scope unsourced, FO4 floor, GECK AP lead, `detection_score` precondition | PASS |
| `00d53e659` (#6579) | one living-actor roster + `PassClock` for periodic systems | PASS for CHARAL (crime paths keep `Dead` filtering) |
| `a53173780` (#6490), `afd260aeb` (#6610), `375d48c6d` (#6574), `40cebd21d` (#6621), `039ff243c`, `1110f1d54`, `13647f400` | corpse pose no `GameKind`; effect-local CTDAs; load discards crafting session; RACE roles by game; decoupling moves | PASS (parse-boundary `GameKind` in #6621 is legitimate; verified against xEdit defs) |
| `3c9dca757` (#6561) | `improve_plan` reads the item pool after dropping the `Inventory` guard | PASS (coefficients untouched) |

## Constant Verification Table

Verdicts: PASS (value matches the capture or master) · FAIL · UNSOURCED (no capture value; itself a finding). Document paths are under `docs/engine/` unless stated. Part 1 is Dim 2 (derived stats, combat, stealth, armor); part 2 is Dim 3 (VATS, regen, leveling, crime, reputation, affliction).

### Part 1: derived stats, combat, stealth, armor (Dim 2)

| # | Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|---|
| 1 | FO3 Health `90 + 20·END + 10·L`, player-only, RoundMode None, uncapped | `fallout3_ruleset` `bilinear(END,20,LEVEL,10,0,90).player_only()` | `charal-fnv-fo3-ruleset.md:93`; masters: `fAVDHealthEnduranceMult` 20, `fAVDHealthLevelMult` 10, `fAVDHealthEnduranceOffset` 0 (Fallout3.esm) | PASS (worked END 5 L1 = 200) |
| 2 | FNV Health `100 + 20·END + 5·(L−1)` = bias 95 | `falloutnv_ruleset` bias 95, coeffs 20 / 5 | `charal-fnv-fo3-ruleset.md:93,123-124`; FalloutNV.esm `fAVDHealthLevelMult` 5 | PASS (END 10 L30 = 445) |
| 3 | FO3 AP `65 + 2·AGI` cap 85, player-only | `affine(AGI,2,65).capped(85).player_only()` | `:95`; Fallout3.esm `fAVDActionPointsBase` 65 / `Mult` 2 | PASS (value). Scope UNSOURCED, disclosed (#2937/#6436), pinned |
| 4 | FNV AP `65 + 3·AGI` cap 95, player-only | `affine(AGI,3,65).capped(95).player_only()` | `:95`; FalloutNV.esm 65 / 3 | PASS (value). Same scope note. The 85/95 caps are in the capture table only, no GMST |
| 5 | FO3/FNV Carry Weight `150 + 10·STR`, actor-general | `add_fnv_fo3_shared` `affine(STR,10,150)` | `:96,131`; masters `fAVDCarryWeightsBase` 150 (sic, with an `s`), `fAVDCarryWeightMult` 10 | PASS (value). Capture spells the GMST `fAVDCarryWeight{Base...}`, the master authors `fAVDCarryWeightsBase` (see M-01) |
| 6 | FO3/FNV Melee Damage `0.5·STR`, actor-general (scope CONFLICTED, kept) | `affine(STR,0.5,0)` | `:98,328-330`; masters `fAVDMeleeDamageStrengthMult` 0.5, `Offset` 0 | PASS. Scope conflict is the documented one (#5240); not re-filed |
| 7 | FO3/FNV Crit Chance `Luck·1` capped 10 (0-100 scale), actor-general | `affine(Luck,1,0).capped(10)` | `:97`; `derived.rs` 0-100 convention (#2936); master `fAVDCritLuckBase` 0 | PASS. Scope SOURCED (GECK Stats Tab, #5240), pinned |
| 8 | FO3/FNV Unarmed Damage `ceil((10+Unarmed)/20)` | `affine(Unarmed,0.05,0.5).ceiled()` | `:99,152-156`; masters `fAVDUnarmedDamageBase` 0.5, `fAVDUnarmedDamageMult` 0.05 | PASS. f32-exact on 0..=255 (probe) |
| 9 | FO3/FNV Radiation Resist `(END−1)·2`, cap 85, floor 0 | `Affliction::RADIATION` (k 2, cap 85) | `:100,386-392` | PASS. The floor 0 is a disclosed defensive domain floor (#2939), not a sourced number |
| 10 | FO3/FNV Poison Resist `(END−1)·5`, uncapped | `Affliction::POISON` (k 5, cap `f32::INFINITY`) | `:101,398-400` | PASS (no cap documented, none shipped) |
| 11 | FO4 Health `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)`, player-only | `bilinear(END,4.5,LEVEL,2.5,0.5,77.5).floored().player_only()` | `charal-fo4-ruleset.md:39,72,90-96` | PASS. Only non-zero cross term in the table; floor per the Endurance page (0.5 HP unadjudicated, #6432); integer-equivalent on END 1..=20, L 1..=255 (probe) |
| 12 | FO4 AP `60 + 10·AGI`, player-only | `affine(AGI,10,60).player_only()` | `:46,145-147,193-199`; Fallout4.esm `fAVDActionPointsBase` 60 / `Mult` 10 | PASS (AGI 5 = 110) |
| 13 | FO4 Carry Weight `200 + 10·STR`, actor-general | `affine(STR,10,200)` | `:36,263-266`; Fallout4.esm `fAVDCarryWeightBase` 200 / `Mult` 10 | PASS |
| 14 | FO4 Melee Damage `1 + STR/10` multiplier: no `derived` row (vanilla authors no `MeleeDamage` AVIF) | none registered; `melee_damage_config` resolves `None` | `:37,283-300`; Fallout4.esm `fAVDMeleeDamageMult` 0.1 | PASS. The coefficient the capture calls "presumably a GMST (not named)" (`:301`) is `fAVDMeleeDamageMult` = 0.1 on Fallout4.esm. No FO4 consumer applies it: known (#2933/#3092, `byroredux/src/combat.rs:971`) |
| 15 | FO4 Radiation / Poison Resist: not SPECIAL-derived | no row | `:317-350` | PASS |
| 16 | Oblivion Health `2·END`, player-only (UNsourced scope) | `oblivion_health_formula` | `charal-oblivion-ruleset.md:476,460-470` | PASS (value), scope UNSOURCED, #6431 pin |
| 17 | Oblivion Magicka `2·INT`, player-only (UNsourced) | `oblivion_magicka_formula` | `:532,460-470` | PASS (value) |
| 18 | Oblivion Fatigue `STR+WIL+AGI+END`, four uncapped affine rows | `oblivion_fatigue_formulas` | `:380,460-470` | PASS (value) |
| 19 | Oblivion Armor Rating multiplier `0.35 + 0.0065·ArmorSkill`, Light and Heavy, `Multiplier`, actor-general | `ARMOR_RATING_SKILL_COEFF` 0.0065, `_BIAS` 0.35 | `:326`; UESP *The Complete Damage Formula* `.wiki` line "BaseArmorRating * ( 0.35 + 0.0065 * OpponentArmorSkill)" | PASS (0.675 @ 50, 0.48 @ 20). Perspective: wearer's own skill, #4449 |
| 20 | Oblivion `ModifiedSkill = Skill + 0.4·(Luck−50)` | `combat::modified_skill` | `:286` + UESP `.wiki` "ModifiedSkill = Skill + 0.4 * (Luck - 50)" | PASS |
| 21 | Oblivion weapon multiplier `0.5·(0.75+0.005·A)·(0.2+0.015·MS)`, A and MS clamped [0,100] | `oblivion_weapon_damage_multiplier` | `:209,226-236,287-289`; UESP "constrained between 0 and 100" | PASS for the stated formula. The condition factor contract is wrong in the rustdoc: see L-01 |
| 22 | Oblivion Hand-to-Hand `Health = 1 + 10.5·(STR/100)·(MS/100)`, `Fatigue = 1 + 0.5·Health`, no clamp | `oblivion_hand_to_hand_damage` | `:304-305,370`; UESP Hand to Hand section | PASS. The `fHandHealthMax 15` GMST is the range end, not a clamp: `(15−1)·fHandDamageStrengthMult 0.75 = 10.5` |
| 23 | Skyrim Light Armor multiplier `1 + 0.004·LightArmor`, player-only | `LIGHT_ARMOR_RATING_COEFF` 0.004 | `charal-skyrim-ruleset.md:221`; UESP *Skyrim:Armor* line 144 "(1 + 0.4 × skill/100)" | PASS |
| 24 | Skyrim NPC coefficient 1.5 (Light and Heavy) | `ARMOR_SKILL_RATING_COEFF_NPC` 0.015 | `:222,268-273`; UESP line 154 | PASS |
| 25 | Skyrim Carry Weight `250 + 0.5·BaseStamina`, base-layer read, actor-general | `CARRY_WEIGHT_BIAS` 250, `_STAMINA_COEFF` 0.5, `.a_from_base()` | `:684-690` | PASS (300 / 305 pinned; Fortify Stamina ignored) |
| 26 | Skyrim armor: 0.12 % per rating point | `ArmorModel::reduction_per_rating` 0.12 | UESP line 141 and 148; CK *Settings* line 327; Skyrim.esm `fArmorScalingFactor` 0.12 (SE and LE) | PASS |
| 27 | Skyrim hidden 3 % per non-clothing piece | `hidden_percent_per_piece` 3.0 (= `fArmorBaseFactor` 0.03 × 100) | UESP lines 147-148, 152; CK *Settings* line 317; Skyrim.esm 0.03 | PASS |
| 28 | Skyrim cap 80 % (held ≤ 100) | `max_reduction` 80 | UESP line 192; CK line 2791; Skyrim.esm `fMaxArmorRating` 80 | PASS |
| 29 | Skyrim player multiplier 1.0 → 1.4, NPC 1.0 → 2.5 (linear at skill/100) | `player_rating_base/max`, `npc_rating_base/max` | CK lines 319-325; Skyrim.esm: `fArmorRatingPCMax` 1.4, `fArmorRatingBase` 1.0, `fArmorRatingMax` 2.5; `fArmorRatingPCBase` NOT authored (default 1.0 per CK line 323) | PASS (the unauthored PCBase falls back to the 1.0 default) |
| 30 | Skyrim per item `CEILING[(base + quality)·mult]`; shield is a piece; clothing gets no hidden rating | `item_rating` `.ceil()`; `ArmorKind::Clothing` adds no piece | UESP lines 144-152 | PASS |
| 31 | Sneak `Detection = Att·(Sound+Visual+DetectorSkill/2) − TargetSkill/2 − 35` | `detection_score` | `charal-fnv-fo3-ruleset.md:256` | PASS |
| 32 | `TargetSkill = Sneak + 5·(TL−DL) + max(50−10·TL,0) − Armor`, 0 if not sneaking | `target_skill` | `:257-258,271-273` | PASS |
| 33 | `DetectorSkill = (10+8·PER)·state`, 0.8 / 1.2 / 1 | `DetectorState::multiplier` | `:259,263-269` | PASS |
| 34 | `Attenuation = ((MaxDist−d)/MaxDist)²`, 2500 in / 5000 out | `Locale::max_distance` | `:260` | PASS (past-range behaviour is the pinned caller precondition, #6433; no caller exists) |
| 35 | Sound: `SM` 1.6 / 0.16; `MovementSound=(12+W/2)·MM`; MM 0 (stationary or Silent Running) / 1.5 / 1; `Sound = SM·(MS + 2·Action)` | `detection_score` | `:284-293` | PASS |
| 36 | Action sound Loud 100 / Normal 50 / Silent 10 / None 0; armor Heavy 20 / Medium 10 / Light 0 | `ActionSound::value`, `ArmorClass::penalty` | `:304-312` | PASS |
| 37 | Visual: 0 without LOS or if invisible; `1.4·min(100, Light·nighteye)`, nighteye 3 / 1; VisualMovement 0 / 0.01 / 0.21 | `detection_score` | `:294-300` | PASS. Silent Running → 0.01 is the disclosed modelling choice (`:326-336`) |
| 38 | Bands: `< −20` undetected, `−20..0` suspicious, `> 0` detected | `classify` | `:271` | PASS (boundary −20 → Suspicious, 0 → Suspicious) |
| 39 | (cross-ref, Dim 4) FNV/FO3 skill base `2 + 2·gov + ceil(Luck/2)` | `actor_value_derive.rs` `SKILL_BASE` 2, `_ATTR_MULT` 2, `_LUCK_MULT` 0.5 | `:46-48`; masters `fAVDSkill*Base` 2.0 for all 13 | PASS |
| 40 | (cross-ref, Dim 4) FO3/FNV NPC Health `base + 5·(END−1) + 5·(L−1)` | `NpcHealthCurve` 5 / 5 / −1 / −1 (both profiles) | `:94,111-127`; masters `fAVDNPCHealthEnduranceMult` 5, `LevelMult` 5 | PASS |

- **Rounding modes:** None on every FO3/FNV Health/AP/CW/Melee/Crit/resist row; `Floor` only on FO4 Health; `Ceil` only on FO3/FNV Unarmed and `ArmorModel::item_rating`. No row carries a mode its capture does not state.
- **Sentinels:** every shipped `cap` is 10, 85, 95 or `f32::INFINITY` (Poison, and the constructor default); no `capped(0.0)` and no struct literal outside `derived.rs`. `EsmIndex::actor_value_form_id` filters `form_id != 0 && != u32::MAX` before any `DerivedInput::actor_value` is built, so no resolved FormID can alias `UNUSED` / `LEVEL` (the `debug_assert` in `actor_value` is the second line, debug only). `DerivedStatFormula` is `Copy`, 36 B (pin passes), no `GameKind` anywhere in the Dim 2 files, no allocation in `eval`.
- **Scope contract:** the only consumers that evaluate a row for an arbitrary entity are `CharacterRuleset::actor_value` (it composes `derived_value` only for `ActorGeneral` + `Absolute`; used by `GetActorValue` in `condition/eval.rs:511`), `melee_damage_charal_bonus` (checks both fields), and `pool_regen_tick_system` (filters `ActorGeneral`). The `PlayerOnly` writers are `refresh_player_only_bases` and `player_only_output_avifs`, reached only from `attach_to_player` / `player_derived_stats_system` (player entity only) and the `setav` / SDK routing predicate `is_player_derived_pool`. No NPC path reaches a `PlayerOnly` row. Level default differs by caller (`GetActorValue` uses 0, the melee bonus 1) but no `ActorGeneral` row reads `LEVEL`, so it is inert.

### Part 2: VATS, regen, leveling, crime, reputation, affliction (Dim 3)

Document column: `fnv-fo3` = `charal-fnv-fo3-ruleset.md`; `obl` = `charal-oblivion-ruleset.md`; `sky` = `charal-skyrim-ruleset.md`. "Wiki" is the local GECK/CS/UESP dump under `/mnt/data/src/reference`.

### VATS / AP (`vats.rs`)

| Formula / setting | Code | Document + line | Verdict |
|---|---|---|---|
| `fActionPointsAttackPistol/Rifle/1H/2H/Unarmed/Grenade/Mine/Launcher/Handle` | 20/25/15/30/22/35/10/40/20 | fnv-fo3 :527-529; probe: both masters identical | PASS |
| `fActionPointsReload`, `Heal`, `PowerAttackMult`, `VATSMeleeMult` | 10, 10, 1.5, 1 | fnv-fo3 :527-529; probe both masters | PASS |
| `fActionPointsRestoreRate` FO3 | 4 per s (`restore_rate_per_frame=false`) | fnv-fo3 :552-559; Fallout3.esm = 4 | VALUE PASS; unit stated as unverified by the doc itself (not re-filed) |
| `fActionPointsRestoreRate` FNV | 0.06 per frame, installed 0.06/`POOL_REGEN_DT` = 3.6/s | same; FalloutNV.esm = 0.06 | VALUE PASS; unit documented UNVERIFIED |
| FNV `fVATSPlayerDamageMult` / FO3 | 0.75 / 0.1 | fnv-fo3 :535-543 table; probe | PASS |
| FNV `fVATSDamageToWeaponMult` / FO3 | 1 / 5 | same table; probe | PASS |
| FNV `fVATSSpreadMult` / FO3 | 1.1 / 0.5 | same table; probe | PASS |
| FNV `fVATSRangeSpreadMax` / FO3 | 100 / 3 | same table; probe | PASS |
| FNV `fVATSRangeSpreadUncertainty` / FO3 | 0.01 / 0.7 | same table; probe | PASS |
| `fVATSCriticalChanceBonus` FNV / FO3 | 5 / 15 | same table (not authored in FO3, wiki 15); probe: FNV 5, FO3 absent | PASS |
| `fVATSHitChanceMult`, `MaxChance` | 1, 95 | fnv-fo3 :545-546; wiki *VATS Settings* | PASS (wiki-only; no master authors them, confirmed) |
| `fVATSMaxEngageDistance`, `MeleeMaxDistance`, `MeleeReachMult` | 5000, 300, 2 | fnv-fo3 :545-548; wiki | PASS |
| `fVATSStealthMult`, `AutomaticMeleeDamageMult`, `DestructibleMult`, `ShotBurstTime`, `SkillFactor` | 0.5, 2, 1.25, 0.43, 1 | fnv-fo3 :545-549; wiki | PASS |
| `iVATSConcentratedFireBonus` | 5 (percentage points per prior shot) | fnv-fo3 :549-551; wiki "63 to 68 to 73" | PASS (prose on the same line is stale: L-02) |
| `fVATSDistanceFactor` | 2 | **no capture-doc value**; wiki *VATS Settings*; both masters author 2 | **UNSOURCED in capture** (value verified, **L-03**) |
| `fVATSScreenPercentFactor` | 1 | doc cites the page but gives no value; wiki *FVATSScreenPercentFactor*; both masters 1 | UNSOURCED in capture (**L-03**) |
| `fVATSShotLongBurstTime` | 1.75 | no capture-doc value; wiki; both masters 1.75 | UNSOURCED in capture (**L-03**) |
| `fVATSMeleeWarpDistanceMult` / `H2HWarpDistanceMult` | 0.32 / 0.27 | no capture-doc value; wiki; both masters | UNSOURCED in capture (**L-03**) |
| `fVATSParalyzePalmChance`, `PlaybackDelay` | 0.3, 0.17 | no capture-doc value; wiki; both masters | UNSOURCED in capture (**L-03**) |
| `fVATSStrangerOdds`, `iVATSStrangerMaxHP`, `fVATSStrangerDistance` | 0.1, 150, 164 | no capture-doc value; wiki; both masters | UNSOURCED in capture (**L-03**) |
| `stealth_mult` bound `0.0..=1.0` (`weak_bounded`) | engine choice | none | Engine guard, not a game number; documented in code |
| `fVATSFortuneOdds` | not carried | fnv-fo3 :539-542 "not modelled" | PASS |
| To-hit chain, steps 1-4 (part share, skill 50-neutral, `range^distance_factor`, screen%) | RECONSTRUCTED, marked | fnv-fo3 :560-563 "reconstructed" | Known-open (#6350/#6645); not re-filed. See observation O-1. |
| To-hit step 5 (Concentrated) and step 6 (cap) | additive `bonus x (shot-1)`, clamp to `max_chance` | wiki worked example | PASS |
| Weapon base to-hit `WEAP DNAM`@40 | u8, percent | #6449; FNV scan: 0..100 | PASS |
| `part_to_hit` source | `BPND`@8 as raw `f32` | `equipment.rs:196-199` (#6448); FNV/FO3 scan: max 100 | PASS in production (the "255 = always" claim in the type doc is stale: **L-01**) |

### Regen (`regen.rs`)

| Formula | Code | Document + line | Verdict |
|---|---|---|---|
| Fatigue regen | flat 10/s (`FATIGUE_REGEN_PER_SEC`) | obl :391-393 (`fFatigueReturnBase` 10, `fFatigureReturnMult` 0) | PASS (lane `None` everywhere) |
| Magicka regen | `(W*0.02 + 0.75)*(Max/100)` | obl :544 | PASS (lane `None` everywhere) |
| Health regen | none | obl :473-492 | PASS |
| `POOL_REGEN_DT` | 1/60 s | engine choice, `charal.md` §4.7 | Engine tuning, documented |
| `MAX_REGEN_SUBSTEPS` | 8 | none ("no capture document specifies it", `regen.rs` const doc) | Documented-unsourced engine tuning (#2953 closed as documented), not re-filed |
| AP lane rate | `ap_regen_per_second()`: FO3 4.0, FNV 3.6 | see AP rows | PASS (installed rate pinned) |
| Regen under VATS freeze | accumulator sees `dt x 0` | `regen.rs` comment | PASS: `app_events.rs:1570` scales `DeltaTime` by `world_time_scale()` before the scheduler. |

### Leveling (`leveling.rs`, `profile.rs`, `tes.rs`, `skyrim.rs`)

| Formula | Code | Document + line | Verdict |
|---|---|---|---|
| FO3/FNV XP curve | `150*L + 50`; `with_gmst` derives it as `iXPBumpBase*L + (iXPBase - iXPBumpBase)` | fnv-fo3 :462; GECK *Experience Settings* (`iXPBase` 200, `iXPBumpBase` 150; derivation re-checked by differencing) | PASS |
| FO3 cap | 20 (`iMaxCharacterLevel` overlay, `BrokenSteel` 30) | fnv-fo3 :468-469 | PASS |
| FNV cap + raises | 30; +5 x {DeadMoney, HonestHearts, OldWorldBlues, LonesomeRoad} | fnv-fo3 :468-473 (#6227) | PASS (`profile.rs:415`, `raise_cap_lifts_only_a_capped_curve`) |
| FO3 / FNV skill points | `10 + INT` / `10 + INT/2`, half carried | fnv-fo3 :482-490 | PASS |
| Perk cadence | FO3 1, FNV 2 | fnv-fo3 :470-473 | PASS |
| FO4 curve | `75*L + 125`, no cap, `SpecialOrPerk` | fo4 :499-513 | PASS |
| Skyrim curve | `25*L + 75`, 1 XP per skill rank, +10 pool, perk each level | sky :771-778 | PASS (`fXPLevelUpBase/Mult` GMSTs overlay) |
| Skyrim skill use curve | 1.95 | sky :793 | PASS |
| Oblivion level-up | 10 major-skill ups; attribute bonus 0 -> +1, 1-4 -> +2, 5-7 -> +3, 8-9 -> +4, 10+ -> +5 | obl :808-815 | PASS (`attribute_bonus_tiers_match_uesp`) |
| FNV/FO3 kill XP | 0/10/20/30/50 by band, share threshold from GMST | fnv-fo3 :495-501 | PASS (consumer in `combat.rs`, not CHARAL; share is GMST-read) |
| `roll_levels` bounds | ceil cost >= 1, stops at cap and `u16::MAX` | engine invariants | PASS |
| `xp_per_skill_rank` 1.0 | fixed coefficient | sky :773-775 "an engine rule" | PASS (by design) |

### Crime (`crime.rs`)

| Formula | Code | Document + line | Verdict |
|---|---|---|---|
| `fCrimeGoldSteal` | 0.5; Oblivion rounds up, min 1 | obl Crime table (:833+); CS *FCrimeGoldSteal* (0.5000); UESP *Oblivion:Crime* | PASS |
| Skyrim theft | 0.5 rounded down | sky :836-838; UESP *Skyrim:Crime* :14 | PASS |
| `iCrimeGoldAttack` | 40 | obl Crime table; CS *ICrimeGoldXX* 40; UESP Skyrim assault 40 | PASS |
| `iCrimeGoldMurder` | 1000 | obl Crime table; CS 1000; UESP Skyrim 1000 | PASS |
| `iCrimeGoldTresspass` / `iCrimeGoldPickpocket` | 5 / 25, GMST-read | obl Crime table; CS 5/25; UESP Skyrim 5/25 | PASS (#6427, pinned) |
| `iCrimeAlarmRecDistance` | 4000 | CS 4000; Oblivion.esm authors 4000 (scan) | PASS |
| `iNumberGuardsCrimeResponse` | 4 | CS default 4 | PASS |
| `iCrimeGoldAttackMin` (`rush_bounty`) | 500 | obl Crime section; Oblivion.esm authors 500 (scan) | PASS |
| `iCrimeDaysInPrisonMod` (Oblivion) | 100, `days = bounty/100`, no cap | CS *ICrimeDaysInPrisonMod* 100; UESP *Oblivion:Crime* :72 "one day for every 100 gold" | PASS |
| Skyrim `prison_days_divisor` | 100 (inherited from `OBLIVION`), **no 7-day cap** | **sky has no jail-sentence row**; UESP *Skyrim:Crime* :103 "maximum sentence is seven days ... any bounty 700 or higher" | **FAIL: M-01**; divisor itself is UNSOURCED in the Skyrim capture (**L-03**) |
| `infamy_bounty_step` | 1000 | obl Crime table (infamy note); UESP *Oblivion:Infamy* :25 | PASS |
| Skyrim `CrimeModel` guard/alarm/rush fields | inherited from Oblivion, consumers gated `scope == Player` | `crime.rs:150,201,283,814` | PASS (inert on Skyrim) |

### Reputation (`reputation.rs`)

| Formula | Code | Document + line | Verdict |
|---|---|---|---|
| Karma range and bands | +-1000; >=750 / >=250 / >=-249 / >=-749 | fnv-fo3 :598-612 | PASS (boundary test covers -249/-250/-749/-750) |
| `REPUTATION_AXIS_MAX` | 100 | fnv-fo3 :674 | PASS |
| Bump table | `[_,1,2,4,7,12]` | fnv-fo3 :692-694 | PASS |
| 13 per-faction thresholds + REPU FormIDs | rows match line for line | fnv-fo3 :707-724 | PASS (all 13 compared) |
| 4x4 standing grid, `[infamy][fame]` | matches, no transpose | fnv-fo3 :737-745 | PASS |
| `sentiment()` per-cell colours | internal heuristic | none, flagged UNSOURCED in code (#2949) | Known, documented |
| FO4 affinity: bounds -1000/+1100; bands -500/0/250/500/750/1000; reactions +-15/+-35 x 0.5/1/1.5; passive `40 - 0.033*a` | matches | `charal.md` :530-545 | PASS |

### Affliction (`affliction.rs`)

| Item | Code | Document | Verdict |
|---|---|---|---|
| Thresholds / penalties | none shipped; mechanism only | fnv-fo3 :388-393 "pending" | PASS (no invented numbers) |
| Pool read | `ActorValues::current` | #6428 | PASS (`the_rad_gauge_and_the_affliction_pool_read_one_write`) |
| Diff-and-reapply, `band_for`/`band_by_key` tie-break | unchanged | #4456 | PASS |
| Scheduler registration | none | known-open | Confirmed still unregistered; `AfflictionStatus` registry entry "forward-latent". |

## Coverage Matrix

Re-derived from `CharacterRulesProfile` consts and `build_ruleset` arms in `profile.rs`, `build_character_ruleset` / `build_vats_model` (`byroredux/src/npc_spawn/stamps.rs`) and workspace greps, not from prose.

| Family | Ruleset implemented | Wired (`RulesetBuilder` arm, live) | Derived stats | Leveling model | Leveling runtime | Regen wired | Affliction wired | VATS runtime | Crime row |
|---|---|---|---|---|---|---|---|---|---|
| FO3 | `fallout3_ruleset` | Yes, `Fallout3` | Yes (Health/AP player-only, Carry, Melee, Unarmed, Crit, resistances) | `XpCurve` FO3, GMST overlay | Yes: kill XP (`awards_kill_experience` true), `levelup xp`, `RewardPlayerXP`; skill-point rewards | AP lane only (`PoolRegenConfig.action_points`); Fatigue/Magicka/Willpower `None` | No (mechanism built, no table, no stamp, no registration) | Yes, `vats() = FALLOUT3` (mode, freeze, scan, AP-gated queue/confirm, Concentrated Fire); no menu/limb damage/playback | `None` (faction-based, unmodelled) |
| FNV | `falloutnv_ruleset` | Yes, `FalloutNewVegas` | Yes (same family, Health bias 95) | `XpCurve` FNV, GMST overlay + `level_cap_raises` (+5 x 4 DLC) | Yes (as FO3) | AP lane only | No | Yes, `vats() = FALLOUT_NV` (own `fVATS*` re-tunes) | `None` |
| FO4 | `fallout4_ruleset` | Yes, `Fallout4` | Yes (Health floored, AP, Carry, Melee) | `XpCurve` FO4 (uncapped) | Partial: no kill XP (`kill_experience: false`, formula uncaptured); `levelup xp`/`RewardPlayerXP` reach it | No (config never inserted) | No | No (`vats: None`) | `None` |
| Skyrim (SE + LE) | `skyrim_ruleset` | Yes, `Skyrim` (#3848) | Yes (Carry Weight; `DamageResist` multiplier row, no reader; pools underived by design) | `SkillXp` + GMST overlay | Partial: `Game.AdvanceSkill` roll via `xp_from_skill_rank`, forge/improve XP, perk point + pool pick | No | No | No | `Some(CrimeModel::SKYRIM)` (per-crime-faction fines; `SendPlayerToJail` native built); also `armor: Some(ArmorModel::SKYRIM)` and `barter` |
| Oblivion | `oblivion_ruleset` (full) | **No**: `RulesetBuilder::None` (deliberate, no AVIF; pinned by `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate`) | n/a at runtime | `SkillUse` OBLIVION (model only) | No | No (the `oblivion_pool_regen_config` builder is deleted; TES lanes `None` everywhere) | No | No | `Some(CrimeModel::OBLIVION)` |
| FO76 | No (capture only) | No (`None`) | No | No | No | No | No | No | `None` |
| Starfield | No (capture only; XP curve / category thresholds unpublished) | No (`None`) | No | No | No | No | No | No | `None` |

Cross-checks against `docs/feature-matrix.md` (CHARAL section): every cell of its table agrees with this matrix except the VATS row wording (L-05). The matrix has no crime/armor/barter rows (coverage gap, not rot).

## Findings

IDs are `CHAR-2026-11-07-<severity letter>-NN`. Where two dimensions reported the same defect the findings are merged and the contributing dimension noted. Full per-dimension detail (evidence snippets, long-form impact) was in the scratch reports and is condensed here. Every numeric finding carries a Source.

### MEDIUM

### CHAR-2026-11-07-M-01: The FO3/FNV/FO4 derived-stat rows hardcode coefficients the masters author as GMSTs, so a retuned load order is ignored
- **Severity**: MEDIUM (same class and rating as #6567, closed for armor this week)
- **Dimension**: Derived Formulas (Dim 2)
- **Game**: fo3, fnv, fo4
- **Location**: `crates/core/src/character/fallout.rs:61-101` (`add_fnv_fo3_shared`: Carry Weight, Melee, Unarmed), `:142-169` (`fallout4_ruleset`), `:174-243` (`fallout3_ruleset`, `falloutnv_ruleset`: Health, AP). Build site that already holds a GMST closure: `byroredux/src/npc_spawn/stamps.rs:252-258` (`build_ruleset` passes `gmst`, used for leveling only).
- **Status**: NEW. Earlier mentions were never filed (`AUDIT_CHARACTER_2026-10-05.md:200`, `-10-16.md:346` "Observation, not filed"; `-08-20.md` D3-01 suggested it, #3170 closed after fixing leveling only). No open or closed issue covers the derived table.
- **Source**: `charal-fo4-ruleset.md:149-150` ("AUTHORED GMSTs ... read not hardcoded") and `:262-268`; `charal.md:143` and `:665-670` (names only `fAVDSkill*` as the remaining seam). Byte scan this run, FalloutNV/Fallout3: `fAVDActionPointsBase` 65, `fAVDActionPointsMult` 3/2, `fAVDCarryWeightsBase` 150 (sic), `fAVDCarryWeightMult` 10, `fAVDHealthEnduranceMult` 20, `fAVDHealthLevelMult` 5/10, `fAVDMeleeDamageStrengthMult` 0.5, `fAVDUnarmedDamageBase` 0.5, `fAVDUnarmedDamageMult` 0.05; Fallout4: `fAVDCarryWeightBase` 200, `Mult` 10, `fAVDActionPointsBase` 60, `Mult` 10, `fAVDMeleeDamageMult` 0.1.
- **Description**: Since #5915, #5238, #6424, #6427 and #6567 a number a load order can author resolves load-order-merged GMST, then a compiled-in default (`LevelingModel`, `NpcHealthCurve`, `VatsModel`, `CrimeModel`, `ArmorModel` all have `with_gmst`). The derived-stat table is the one family left out: each row is `DerivedStatFormula::affine/bilinear` with the vanilla number baked in, and the builders take only the EditorID resolver. Every baked value equals its vanilla master value, so nothing is wrong on a vanilla load order.
- **Evidence**: `fn fallout3_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F)` has no `gmst` parameter; `fallout.rs:65` `affine(av(s), 10.0, 150.0)`, `:187` `affine(av(a), 2.0, 65.0)`, `:181` `bilinear(av(e), 20.0, LEVEL, 10.0, 0.0, 90.0)`; no `game_setting_float` use in `fallout.rs`.
- **Impact**: A carry-weight, AP or health-curve rebalance mod (a common category) changes nothing for the player's Health/AP (re-stamped every frame, #5039), Carry Weight (any actor via `GetActorValue`) or Melee/Unarmed bonuses. No crash, log line or test. Note FO3/FNV Health folds `100 - levelMult` into the bias (90/95) and no master authors the 100, so the overlay must keep the engine-supplied 100.
- **Related**: #6567, #5915, #5238, #6424, #6427, #3170.
- **Suggested Fix**: Give the builders a `gmst` closure and overlay the named settings with today's numbers as fallbacks (a `DerivedCoefficients` row on the profile or `with_gmst` on the per-game constants). Read the FO3/FNV Carry Weight base as `fAVDCarryWeightsBase` (the master's spelling). Fix `charal-fnv-fo3-ruleset.md:131` and `charal.md:665-670` in the same commit, name the five GMSTs the capture omits, and pin with a retuned-GMST test like `a_retuned_scaling_factor_reaches_the_damage_pipeline`.

### CHAR-2026-11-07-M-02: Skyrim jail sentences have no 7-day cap, and the 100-gold-per-day divisor is not in the Skyrim capture
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools (Crime, Dim 3)
- **Game**: skyrim
- **Location**: `crates/core/src/character/crime.rs` (`CrimeModel::sentence_days` at `:174-175`; `SKYRIM` inherits `prison_days_divisor: 100` via `..Self::OBLIVION`, `:121`). Callers `crates/scripting/src/jail.rs:397` (`send_player_to_jail`) and `:293`; consumer `serve_sentence` (`jail.rs:340`, `sleep.0 += sentence.days * 24.0`).
- **Status**: NEW (searched "jail sentence", "maximum sentence", "seven days", "prison days", "SendPlayerToJail": only closed #6569, #6573, #6340, #6388, other defects).
- **Source**: UESP *Skyrim:Crime* (`/mnt/data/src/reference/uesp-wiki/Skyrim/Crime.wiki:103`): "The maximum sentence is seven days ... You will serve the maximum sentence for any bounty 700 or higher." UESP *Oblivion:Crime* `:72` gives "one day for every 100 gold" with no cap, so the cap is Skyrim-specific. Skyrim.esm authors no `iCrimeDaysInPrisonMod` (byte scan).
- **Description**: `CrimeModel::SKYRIM` is `OBLIVION` with three fields flipped, so `sentence_days` returns `bounty/100` with no ceiling. Since #6569/#6573 and `36d7bfbd6`, `Faction.SendPlayerToJail` is a live Skyrim path that advances the game clock by `days x 24` hours. The `SKYRIM` doc comment ("the guards and jail settings below are Oblivion's and unused here") is no longer true for `prison_days_divisor`.
- **Impact**: A 5000-gold Skyrim bounty serves 50 days instead of 7; schedules, timers, weather and quest clocks advance 43 extra days per arrest.
- **Related**: #6569, #6573; L-07 (the divisor is also absent from the capture).
- **Suggested Fix**: Add `max_sentence_days: Option<f32>` to `CrimeModel` (`Some(7.0)` on `SKYRIM`, `None` on `OBLIVION`), clamp in `sentence_days`, pin 700 and 5000 gold, and add the divisor and cap to the Skyrim capture's Crime section.

### CHAR-2026-11-07-M-03: ObScript `RemoveFromAllFactions` (FO3/FNV, 0x1214) clears `FactionRanks` directly and never records the `ScriptedFactionChanges` ledger
- **Severity**: MEDIUM (a scripted state change is silently lost on cell reload or save-load; vanilla call frequency not measured)
- **Dimension**: Population Boundary (Dim 4)
- **Game**: fo3, fnv
- **Location**: `crates/scripting/src/obscript_more/tail.rs:396-408` (handler), `:822` (registration `Command { id: 0x1214, games: Games::Fallout, .. }`)
- **Status**: NEW (no issue for `RemoveFromAllFactions` / ledger bypass; #5919 is the closed ledger-reload fix)
- **Source**: not numeric. Contract at `crates/scripting/src/faction_changes.rs:3-13`, `:55-98` (scripted edits are recorded by reference FormID and replayed at spawn and load) and the SKILL Dim 4 bullet; behaviour per GECK *RemoveFromAllFactions*.
- **Description**: Papyrus goes through `Effect::ChangeFaction`, which `flush_faction_changes` (`fragment/effects/deferred.rs:1027-1075`) applies and records, including a `FactionChange::RemoveAll` arm. ObScript `AddToFaction`, `RemoveFromFaction` and `SetFactionRank` also route through it (`obscript_effects/actor_state.rs:10-12`, `:143-151`). The one exception is this handler: `world.query_mut::<FactionRanks>()` then `factions.0.clear()`, return `done(..)`.
- **Evidence**: `tail.rs:397-407` as above; the only test (`obscript_more/tests.rs:662`) asserts the live component is empty, and `ScriptedFactionChanges::record` appears nowhere in `obscript_more/`.
- **Impact**: On the next cell eviction/respawn or save-load the actor is re-derived from its record and the removed memberships return (ambient hostility, theft exemption, `GetFactionRank`/`GetInFaction` conditions). The player is affected through `reset_player_factions_to_record`.
- **Related**: #5919, #5058, `b3556ef6c`.
- **Suggested Fix**: Route the command through `Effect::ChangeFaction` with `FactionChange::RemoveAll` as the Papyrus path does, and add a ledger assertion beside the `:662` test.

### CHAR-2026-11-07-M-04: `FactionReputation` is mutated in production by ObScript and the SDK, but the save registry exempts it as "no production insertion or mutation site exists yet"
- **Severity**: MEDIUM (silent state loss on save/load; the exemption guard's premise is false)
- **Dimension**: Coverage & Doctrine (Dim 5). Route the registration fix to `/audit-save`.
- **Game**: fnv
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:476` (the `NOT_SAVED_BY_DESIGN` row); mutators `crates/scripting/src/obscript_effects/mod.rs:229-243` (`adjust_standing`; callers `obscript_effects/actor_state.rs:187`, `obscript_more/tail.rs:522`, `obscript_more/actions.rs:185`) and `byroredux/src/extensions/commands.rs:678-686` (SDK `AddFame`/`AddInfamy`).
- **Status**: NEW (#6151 created `adjust_standing` and is not a save fix; `AUDIT_SAVE_2026-08-20.md:420-425` recorded "zero production insert or mutate sites", true then, overtaken).
- **Source**: not numeric. The guard's own doc (`registry_completeness_tests.rs:440-444`: "forward-latent ... Register the moment a real system starts inserting it"); `charal.md` §4.5 and `components.rs:195-197` claim the component rides the save machinery.
- **Description**: `adjust_standing` does `standings.insert(player, Default::default())` on first use then `add_fame/add_infamy/set_*` for `AddReputation`, `AddReputationExact`, `SetReputation`; the SDK does the same. `FactionReputation` is nowhere in `build_save_registry`; the debug-server projection states it is "not part of the save serialization surface".
- **Evidence**: `registry_completeness_tests.rs:476`: `("FactionReputation", "forward-latent: no production insertion or mutation site exists yet")` against `obscript_effects/mod.rs:230-233`.
- **Impact**: On FNV, reputation earned through quest-script `AddReputation` resets to Neutral on save/load, and `GetReputation`/`GetReputationThreshold` CTDAs (573/575, wired) then read 0, flipping dialogue and quest branches. The guard that exists to catch this is satisfied by a stale reason string.
- **Related**: #4107 (same class), #6151.
- **Suggested Fix**: Register `FactionReputation` for save (serde under the `save` feature, FormId-keyed REPU entries like `FactionRanks`), delete the exemption row and add a round-trip test; or, if deliberately transient, correct the reason and the `components.rs` doc.

### LOW

### CHAR-2026-11-07-L-01: Most #5921 rule-row readers have no test that varies the profile independently of the game; kill-XP install has no test at all
- **Severity**: LOW · **Dimension**: Ruleset Seam (Dim 1) · **Game**: all · **Status**: NEW
- **Location**: `byroredux/src/combat.rs:558-607` (`install_experience_rules`), `byroredux/src/encumbrance/tests.rs:6-20`, `crates/scripting/src/bleedout/tests.rs:171-205`, `crates/scripting/src/perk_entry_points/tests.rs:44-55`, `byroredux/src/barter/tests.rs:121-124`
- **Source**: SKILL Dim 1 "Profile-as-oracle". Contrast `obscript_dialect_follows_the_profile_not_the_game_kind`, `fn586_condition_gate_is_profile_scoped_not_game_scoped`, `no_armor_row_means_no_model_and_no_book`, `rank_after_scripted_adds`, which build a pair where game and profile disagree.
- **Description**: Each of the four consumers' tests builds an `EsmIndex` whose `game` and `character_rules` are a matching pair, so a regression to `match index.game` would still pass. `git grep 'install_experience_rules('` finds only the definition and `scene/scene_runtime.rs:226`: no test.
- **Impact**: None today (the code reads the rows); the doctrine regression #5921 fixed is the one most likely to return.
- **Suggested Fix**: One mismatched-index install test per consumer (e.g. `game: Skyrim` + `FALLOUT_NEW_VEGAS` profile), a direct `install_experience_rules` test, optionally the `production_text` `GameKind::` scan from `armor.rs`.

### CHAR-2026-11-07-L-02: `ArmorModel` documents `fArmorRatingPCBase` as a `Skyrim.esm` / executable value, but no Skyrim master authors it
- **Severity**: LOW · **Dimension**: Ruleset Seam, profile-row sourcing (Dim 1) · **Game**: skyrim · **Status**: NEW
- **Location**: `crates/core/src/character/armor.rs:3-19`, `:57`, `:70-77`; `docs/engine/charal-skyrim-ruleset.md:297-302`
- **Source**: byte scan of Skyrim.esm (SE and LE), Update, Dawnguard, Dragonborn, HearthFires and cc masters: six of seven settings authored (0.12, 0.03, 80, 1.4, 1.0, 2.5); `fArmorRatingPCBase` has no `GMST` record. Its 1.0 comes only from the CK *Settings* page (`/mnt/data/src/reference/ck-uesp-wiki/Category/Settings.wiki:323`).
- **Description**: The value is right; the stated provenance is not (the #6442 class: compiled-in fallback and authored value are different things). The header says "executable defaults of six game settings" while seven are read.
- **Impact**: None on gameplay; a future audit looking for the master value finds none.
- **Suggested Fix**: State that `fArmorRatingPCBase` is a CK-page default no master authors (overlay kept for mods) and say "master-authored, byte-scanned" for the other six.

### CHAR-2026-11-07-L-03: `combat.rs` rustdoc tells callers to multiply by the weapon-condition "ratio", which is not the sourced factor
- **Severity**: LOW (no caller exists) · **Dimension**: Derived Formulas (Dim 2) · **Game**: oblivion · **Status**: NEW (`oblivion_weapon_damage_multiplier` has zero callers)
- **Location**: `crates/core/src/combat.rs:45-60`
- **Source**: `charal-oblivion-ruleset.md:357-358` ("`(0.5 + WeaponHealth/BaseWeaponHealth × 0.5)`") and `:278`; UESP *The Complete Damage Formula*, Modifiable Settings (`fDamageWeaponConditionBase` 0.5, `fDamageWeaponConditionMult` 0.5).
- **Description**: The function returns `0.5·(0.75+0.005A)·(0.2+0.015·MS)` (numbers correct); the leading 0.5 is `fDamageWeaponMult`. The condition term is a separate factor `0.5 + 0.5·ratio`. A caller following the doc multiplies by the raw ratio `r` and gets `0.5·r` where the source gives `(r+1)/4`.
- **Impact**: None today; a future Oblivion melee consumer under-damages worn weapons.
- **Suggested Fix**: Reword to "multiply by `fDamageWeaponConditionBase + ratio · fDamageWeaponConditionMult`", or take the factor as a parameter and pin `(r+1)/4`.

### CHAR-2026-11-07-L-04: The Skyrim `DamageResist` multiplier row (and Oblivion's armor-rating rows) have no reader, and the prose about where Skyrim armor lives is stale
- **Severity**: LOW · **Dimension**: Derived Formulas / Coverage (Dim 2 + Dim 5) · **Game**: skyrim, oblivion · **Status**: NEW (follow-up to closed #6567/#6568)
- **Location**: `docs/engine/charal-skyrim-ruleset.md:306-309` (added by #6568); rows at `crates/core/src/character/skyrim.rs:133-144` and `tes.rs:138-161`; `docs/engine/charal.md:345-346` ("the Skyrim armor model itself is `byroredux/src/armor.rs`, which reads this constant").
- **Source**: `crates/core/src/character/ruleset.rs:140-173`: a `Multiplier` row "composes the entry as carried"; `derives_for_any_actor` requires `ActorGeneral` + `Absolute`; `refresh_player_only_bases` skips `Multiplier`.
- **Description**: The Skyrim row is `PlayerOnly` + `Multiplier`, so `GetActorValue DamageResist` returns the carried entry, never `1 + 0.004·LightArmor`; nothing else calls `derived_value` for it. The capture's claim that the two models "can disagree" through a `GetActorValue` read is wrong. Oblivion's `LightArmorRating`/`HeavyArmorRating` rows are equally unread (Oblivion has no runtime ruleset). Separately, `charal.md` still says `armor.rs` reads the constants and never mentions the `ArmorModel` profile row.
- **Impact**: Misleads the next reader about where Skyrim armor lives (only in `ArmorModel`); the rows are tested coefficient carriers with no runtime effect.
- **Suggested Fix**: Correct the three sentences (the row has no reader; the model is the `ArmorModel` row in `character/armor.rs`, which `byroredux/src/armor.rs` reads), or retire the Skyrim row and its pin tests.

### CHAR-2026-11-07-L-05: `VatsHitInput::part_to_hit` documents a "255 = always" sentinel and `BPND`@6, relics of the pre-#6448 mis-decode
- **Severity**: LOW · **Dimension**: Coverage & Doctrine (Dim 3) · **Game**: fnv, fo3 · **Status**: NEW (follows #6448; #6622 fixed the sibling `weapon_base_to_hit` comment only)
- **Location**: `crates/core/src/character/vats.rs:387-388`, test doc `:553`, `part_to_hit: 255.0` at `:559` and `:594`
- **Source**: `crates/plugin/src/esm/records/misc/equipment.rs:187-199` (`BPND`@6 is health percent; the to-hit chance is `BPND`@8, 0-100); byte scan this run: FalloutNV.esm 304 parts and Fallout3.esm 207 parts, `BPND`@8 in 10..=100, no 255.
- **Description**: The production scan feeds `part.to_hit_chance as f32` (`BPND`@8) with no 255 handling; the doc and two tests treat 255 as a real input.
- **Impact**: Documentation only. A modded part authoring 255 would multiply the chance by 2.55 (capped), not mean "always".
- **Suggested Fix**: Re-word the field doc to `BPND`@8, 0-100, and drop or relabel the 255 test values.

### CHAR-2026-11-07-L-06: CHARAL prose still describes the pre-#6430 / #6350 / #6422 / #6403 states
- **Severity**: LOW · **Dimension**: Coverage & Doctrine, doc rot (Dim 3 + Dim 5) · **Game**: all (fo3/fnv in practice) · **Status**: NEW (partial overlap with closed #6622, which swept comments but did not reach these)
- **Location**:
  - `docs/engine/charal.md:247` (`PoolRegenConfig { fatigue_avif: u32, magicka_avif: u32, willpower_avif: u32, .. }`) and `:274-276` ("their ids are `0`; no actor carries AV 0").
  - `crates/core/src/character/regen.rs:184-188` ("arriving only with Oblivion's wiring ... four games have carried one ... with no config following") and its mirror `docs/engine/charal-oblivion-ruleset.md:421-425`.
  - `docs/engine/charal-fnv-fo3-ruleset.md:550` ("`iVATSConcentratedFireBonus` 5 ... not yet plumbed, #6422").
  - `docs/engine/charal.md:682`, `charal-fnv-fo3-ruleset.md:563` and `crates/core/src/character/vats.rs:630` cite #6403 as the open to-hit tracker.
  - Outside the dimension paths: `docs/prd/vats.md:147,151,6`, `docs/prd/perk-entry-points.md:182`.
- **Source**: code at HEAD. `PoolRegenConfig` ids are `Option<u32>` and `build_vats_model` sets them `None` (`byroredux/src/npc_spawn/stamps.rs:288-293`, #6430); the config is installed on FO3/FNV cell load (#6374; `regen.rs:126-132` itself says so). `vats_queue_part` plumbs Concentrated Fire (`5502c2f97`). #6403 and #6422 are CLOSED; the placeholder test now names #6350.
- **Description**: `regen.rs` contradicts itself (struct rustdoc says FO3/FNV install the config, the tick docstring says Oblivion only). `charal.md` presents the pre-#6430 `0`-sentinel design as current, which is the defect #6430 retired.
- **Impact**: A reader re-introduces the `0` sentinel, or concludes Concentrated Fire is unplumbed; the "AP only, not inert/live wholesale" rule is violated in comments.
- **Suggested Fix**: Edit these spots: `Option<u32>`/`None` (#6430); config installed for FO3/FNV (AP lane); Concentrated Fire plumbed (#6422); re-point #6403 to #6350/#6644/#6645; update the PRD rows.

### CHAR-2026-11-07-L-07: Capture documents lag the code: ten `fVATS*` defaults, Skyrim's jail divisor and `SendPlayerToJail` status, and the crime row's scope
- **Severity**: LOW (UNSOURCED-row rule: the values are right, the capture does not state them) · **Dimension**: Coverage & Doctrine (Dim 3 + Dim 5) · **Game**: fnv, fo3, skyrim, oblivion · **Status**: NEW
- **Location**: `crates/core/src/character/vats.rs:166-216` (`FALLOUT3`) against `docs/engine/charal-fnv-fo3-ruleset.md:513-551`; `crime.rs` `SKYRIM` against `docs/engine/charal-skyrim-ruleset.md:819-852` (heading "guards and jail (not built)", `:848-851` lists the `SendPlayerToJail` native's engine side as not built); `docs/engine/charal-oblivion-ruleset.md:838` ("`crime()`, Oblivion only").
- **Source**: `probe_gmst_vats` on both masters: `fVATSDistanceFactor` 2, `fVATSScreenPercentFactor` 1, `fVATSShotLongBurstTime` 1.75, `fVATSMeleeWarpDistanceMult` 0.32, `fVATSH2HWarpDistanceMult` 0.27, `fVATSParalyzePalmChance` 0.3, `fVATSPlaybackDelay` 0.17, `fVATSStrangerOdds` 0.1, `iVATSStrangerMaxHP` 150, `fVATSStrangerDistance` 164, identical in both; the GECK wiki *VATS Settings* lists the same. UESP *Skyrim:Crime* `:103` for the divisor.
- **Description**: The capture's two lists (six FNV re-tunes, wiki-only set) cover 11 settings; these ten appear in neither, and the second list says "neither master authors them", true only for the 11 it names. The Skyrim crime section states fines but no sentence rule, still lists jail and `SendPlayerToJail` as unbuilt although `36d7bfbd6`, #6569 and #6573 built them (`PayCrimeGold` and guard AI remain unbuilt: no `paycrimegold` native), and the Oblivion capture says "Oblivion only" though `CrimeModel::SKYRIM` landed in `4cfcee4b9`.
- **Impact**: An auditor cannot verify those rows from the capture; a reader scoping Skyrim crime work would rebuild jail.
- **Related**: M-02, #6425.
- **Suggested Fix**: Add a third "authored identically by both masters" list; add the divisor and 7-day cap to the Skyrim crime section and mark `SendPlayerToJail` built; say "Oblivion and Skyrim" at the Oblivion capture line.

### CHAR-2026-11-07-L-08: The player's `CharacterLevel` and `Background` are withheld whenever the derivation yields nothing (Oblivion, also FO76/Starfield)
- **Severity**: LOW (escalate to MEDIUM if an Oblivion census shows player-targeted `GetLevel`/`GetIsRace` CTDAs; not run) · **Dimension**: Population Boundary (Dim 4) · **Game**: oblivion (fo76/starfield profiles also `NpcStatModel::None`) · **Status**: NEW (residue of #4678)
- **Location**: `byroredux/src/inventory/catalog.rs:345-354` (the `pairs.is_empty()` early return) against `:362-367`; NPC counterpart `byroredux/src/npc_spawn/stamps.rs:198-228` (unconditional).
- **Source**: not numeric. `PlayerCharacterTemplate::background` doc ("exactly what `derive_npc_actor_values` already consumed"); `scene.rs` SIBLING note ("`CharacterLevel` + `Background` ride the same attach").
- **Description**: `build_player_character_template` returns early with only `factions`, `spells`, `racial_spells`, `perks` when no pairs derive. `Background` is pure record provenance and does not depend on the derivation. The player body root skips the identity stamps, so nothing else stamps them; Oblivion NPCs carry both.
- **Impact**: On the Oblivion player `GetIsRace`, `GetIsPlayableRace` and `GetLevel` are silently false/0 (`condition/catalog.rs:999`, `eval/actor.rs:31`). Loot bootstrap is unaffected (`attach.rs:251` `map_or(1, ..)`).
- **Suggested Fix**: Compute and stamp `level` and `background` before the early return; pin with a Player record that derives to nothing but carries race and class.

### CHAR-2026-11-07-L-09: `ResolvedNpc::level()` (#4232) is adopted by gear and spells only; `CharacterLevel`, the Health derivation and the player templates still read the unbackfilled level
- **Severity**: LOW (consistency; no source says which level is right for stats) · **Dimension**: Population Boundary (Dim 4) · **Game**: fnv (fo3 by the same data rule) · **Status**: NEW
- **Location**: backfilled readers `npc_spawn/equip.rs:229`, `:1032`, `:1048`; unbackfilled `npc_spawn/stamps.rs:218`, `crates/plugin/src/esm/records/actor_value_derive.rs:663`, `inventory/catalog.rs:362`, `:646`; stale comment `npc_spawn/equip.rs:219` ("same gate as `stamp_character_components`").
- **Source**: the Health term is sourced (`charal-fnv-fo3-ruleset.md:94`, `:113-114`: `base + 5·(END-1) + 5·(Level-1)`); which level a level-0 LVLN-picked terminal should carry for stats is unsourced, so no value is asserted.
- **Description**: For the ~30/3,816 FNV `NPC_` records authoring level 0 reached through an LVLN pick, gear expands at level N while `CharacterLevel` (which feeds `GetLevel`, kill-XP victim level, the save) and Health use `effective_actor_level(stats)` = 0. No doc, including the `effective_actor_level` docblock (`actor/mod.rs:150-168`), mentions `ResolvedNpc::level()`.
- **Impact**: At most a few HP, a level-0 `GetLevel` and kill XP for about 30 FNV actors; the likelier cost is a future reader choosing between two "actor level" helpers.
- **Suggested Fix**: Route `stamp_character_components` and the derivation through `resolved.level()`, or document the two-level contract and fix the `:219` comment; pin whichever with a test reading both.

### CHAR-2026-11-07-L-10: `docs/feature-matrix.md` rates FO3/FNV VATS "~ plumbing"
- **Severity**: LOW · **Dimension**: Coverage & Doctrine (Dim 5) · **Game**: fo3, fnv · **Status**: NEW (row last edited `870722b99`, 2026-10-26)
- **Location**: `docs/feature-matrix.md:284` ("VATS + Action Points runtime (#6350, open) | ✗ | ~ plumbing | ~ plumbing | ✗ | ✗ | ✗ | ✗") and prose `:324-329`
- **Source**: `byroredux/src/systems/vats.rs` (`vats_mode_system`, `vats_scan`, `vats_queue_part`, `vats_confirm`, `vats_cancel`), `docs/smoke-tests/vats-targeting.sh`, `6186ab77e`. `ROADMAP.md:306` is accurate.
- **Description**: The runtime is a working console-driven VATS (mode machine with world freeze, targeting scan, AP-gated queue/confirm, perk accuracy, damage through the combat funnel); absent are the menu, per-limb damage, playback and camera.
- **Impact**: The status floor understates shipped behaviour and omits what is missing.
- **Suggested Fix**: "~ console-driven, no menu / limb damage / playback" for FO3 and FNV.

### CHAR-2026-11-07-L-11: Two profile rows have no capture line: `add_perk_raises_rank` and the `perk_entry_points` dialect
- **Severity**: LOW · **Dimension**: Coverage & Doctrine (Dim 5) · **Game**: fo3, fnv, fo4, skyrim · **Status**: NEW (`2d9c5802c`/#6619 added the row; the #6284 convention is that each profile row is documented in the game's capture, as `charal-fo4-ruleset.md:527-545` does for bleedout/encumbrance/kill XP/barter)
- **Location**: `crates/core/src/character/profile.rs` (`add_perk_raises_rank`, `PerkEntryPointDialect`; `true` on FO3/FNV/FO4, `false` on Skyrim) against `docs/engine/charal-{fnv-fo3,fo4,skyrim}-ruleset.md` (`grep -n 'AddPerk\|raises' docs/engine/charal*.md` is empty)
- **Source**: `geck-uesp-wiki/(main)/AddPerk.wiki:3` ("Increments the rank of the specified perk"), `falloutck-uesp-wiki/(main)/Perk.wiki:34` (verified by Dim 4); cited only in `profile.rs` rustdoc.
- **Description**: These rows state per-game behaviour recorded only in rustdoc and the PRD; the Skyrim "open question" is not in the Skyrim capture.
- **Impact**: A later source finding on Skyrim `AddPerk` has no capture to correct.
- **Suggested Fix**: One short "Perk rows" section per capture citing the two wiki lines and marking Skyrim unverified.

## Known-Open Register

Re-checked with `gh issue view` on 2026-11-07. The SKILL's own list is stale (see the last block).

| Item | State | Note |
|---|---|---|
| #4137 (`template_flags` Script 0x0200 and Def Pack List 0x0400 have no consumer) | OPEN | still true; `reference_state.rs:795` reads `npc.script_form_id` raw (same item) |
| #6350 (VATS umbrella) | OPEN | menu, per-part damage routing / limb health, playback and camera still absent; phases #6644-#6650 |
| To-hit reconstruction (steps 1-4) | open under #6350, #6645 | the ground-truth test is still `#[ignore]`d; reconstructed chain yields about 9% for a 10mm torso at point blank (observation, not re-filed) |
| FNV/FO3 tag-skill per-level formula | open, deferred | undocumented; not guessed |
| FO3/FNV AP/Crit/Melee/Unarmed NPC scope | documented, `.player_only()` | #2937, #6436, #5240 all CLOSED as documented; Melee Damage scope CONFLICTED and kept actor-general |
| Oblivion `RulesetBuilder::None`; FO76/Starfield builder-less | deliberate | pinned by `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate` |
| `affliction_tick_system` | still unregistered | no `AfflictionTable` or `AfflictionStatus` insertion outside tests |
| Regen | AP lane only (FO3/FNV) | Fatigue/Magicka/Willpower lanes `None` |
| #6403, #6440, #6441, #6442, #6422, #4232, #4415, #6429, #6431, #6432, #6434, #6435 | **CLOSED** | SKILL still lists #6403/#6440/#6441/#6442 as open and Concentrated Fire as absent |
| #6567, #6568, #6569, #6573, #6619, #6625, #6430, #6428, #6427, #6449/#6450, #6469, #6374 | CLOSED | verified fixed in code |
| #3768 | CLOSED | still cited as "the blocker"; the real blocker (no pre-AVIF resolver) has no open issue |

None of the 15 findings re-files a known-open item.

**Stale audit-brief text for `/audit-sync`**: SKILL Scope says "16 modules" (17: `armor.rs`); it lists profile rows without `armor` and `add_perk_raises_rank`; Dim 3 should drop "#6403 (open)", "#6440/#6441/#6442" and the "Still absent: Concentrated Fire" clause; add "VATS ends on cell transition, death and load (#6625)"; refresh the known-open count.

## Cross-audit routing

- **M-04** (`FactionReputation` save registration) → `/audit-save`.
- **M-03** (ObScript faction command bypassing the ledger) → `/audit-scripting` also owns the command table.
- **M-02** (`send_player_to_jail` clock advance) → `/audit-gameplay` for the sleep/clock consumer.
- `/audit-ecs` (component shape), `/audit-esm` Dim 4 (AVIF/CLAS/NPC_), `/audit-concurrency` Dim 4 (scheduler access, incl. #6429 re-run): no findings this run.

Suggested next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-11-07.md` (domain label `character`; add `game:fnv`/`game:fo3`/`game:skyrim` per finding).
