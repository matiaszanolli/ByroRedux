**HEAD**: a06d5f9db · **Baseline**: [`AUDIT_CHARACTER_2026-11-07.md`](AUDIT_CHARACTER_2026-11-07.md) (HEAD `09940503b`) · **Audited**: Dim 1 (Ruleset Seam), Dim 2 (Derived Formulas), Dim 3 (Progression/Pools/Crime/VATS), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: none. Every dimension had commits since 2026-11-07, most of them from the Oblivion runtime work (#7008–#7011, #7037/#7041, #7044, #7050–#7056), the player rule rows (spell cast, bow, block, fall, armor rating) and M65 (stealth on six games).

# Character / CHARAL Audit, 2026-11-21

`/audit-character` at default scope (`--depth deep`, all five dimensions). One Task agent per dimension (at most three concurrent), each told to read the capture documents before the Rust constants. Nothing was edited, committed or filed, and the engine was not launched. Master GMSTs were read by read-only byte scans of the shipped masters. The orchestrator re-read the code behind each MEDIUM before keeping it:
- **M-01**: `ResolvedNpc::level()` → `effective_actor_level` → `calc_min.max(1)` for ACBS bit 7, with no stat-model arm.
- **M-02**: `ArmorRatingModel::with_gmst` filters only non-finite values, then `combined.clamp(0.0, self.max_rating)`.
- **M-04**: `LevelingModel::with_gmst`'s `SkillUse` arm reads only the five `fSkillUse*` settings.
- **M-05**: `regen.rs` `FATIGUE_REGEN_PER_SEC` / `MAGICKA_REGEN_*` are module constants with no GMST read.
- **M-03**: confirmed in code. `weapon_resistance_factor` clamps to `[0, 1]`.

## Executive Summary

**28 findings: 0 CRITICAL, 0 HIGH, 5 MEDIUM, 23 LOW.** All are NEW. The five dimensions reported 34 between them; overlapping reports were merged and doc rot regrouped one finding per document. No coefficient disagrees with its capture on a vanilla load order. The one finding visible on vanilla data is M-01. M-02 is a crash that mod data can trigger.

- **Constants verified against captures and masters**:
  - **Oblivion**: the spell-cast cost chain, bow, block, fall, armor rating and Resist Normal Weapons; the player pools and the stored level-up Health; NPC auto-calc, reproduced exactly against the UESP *NPCs* tables; creature pools; skill-use costs and all 42 `SKIL` use values; the attribute bonus bands; disposition; regen; crime.
  - **Other families**: all six stealth rows, term by term (FO3, FNV, Oblivion, Skyrim, FO4, Starfield); the Skyrim bow; and the regression rows for Fallout, FO4 and Skyrim (derived stats, leveling, VATS, limbs, crime, reputation).
  - **Not sourced**: `fMagicSpellLevelCost{Mult,Base}` (L-11) and the FO3 sneak row's dropped armor term (L-16).
- **Baseline fixes confirmed**: all 15 baseline findings (#6756–#6770) are closed and still fixed. One partial exception: the `charal.md` prose of #6763 went stale again after #7010 (folded into L-19). No new `GameKind` branch in a CHARAL rule consumer; the two remaining consumer-side game tests are in L-15. `effective_actor_level` is still one copy. `build_character_ruleset` is still the only construction site. Every new profile row is an exhaustive literal on all eight profiles.
- **The five MEDIUMs**:
  - **M-01**: Oblivion PC-level-offset NPCs and creatures (1,568 of 2,482 `NPC_`, 284 of 914 `CREA`) expand leveled gear and spells at the FO3/FNV `calc_min` stand-in, not their player-relative level.
  - **M-02**: a negative authored `fMaxArmorRating` panics in the first weapon hit; a value over 100 makes armor heal.
  - **M-03**: Resist Normal Weapons drops Weakness to Normal Weapons, applies to creature attacks, and leaves the fist rule's source conflict undeclared.
  - **M-04**: Oblivion's level-up settings (`iLevelUpSkillCount`, the ten `iLevelUpXXMult` bands, `fStatsHealthLevelMult`) are hard-coded.
  - **M-05**: Oblivion's now-live Fatigue/Magicka regen ignores the four `f*Return*` GMSTs the master authors, and has no Endurance term.
- **Theme**: #6756's "read the load order's setting, never a restated literal" rule landed for Fallout but not for the new Oblivion rows. That is M-04, M-05, L-01 and L-02, plus the overlay hardening gaps M-02 and L-03.
- **Doc rot is the other large cluster** (L-17 to L-23). The Oblivion wiring and M65 left the feature matrix, `charal.md`, the Oblivion capture, `skill-use.md`, the M65 captures and about a dozen code comments describing the pre-wiring state.

## Tests recorded

| Command (pinned 1.100.0 toolchain) | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect character` | 236 passed, 0 failed, 1 ignored (the VATS to-hit ground-truth placeholder; it names #6350). Baseline: 173 |
| core `stealth` / `combat` | 31 passed / 7 passed |
| `byroredux-scripting` `oblivion_skill_use` | 16 passed |
| `byroredux-scripting` `oblivion_skill_use bleedout_defaults follow perk_dialect` | 54 passed |
| `byroredux-plugin --lib oblivion` | 93 passed, 6 ignored |
| `byroredux-plugin --lib actor_value_derive oblivion_population oblivion_actor_values` | 44 passed, 3 ignored |
| bin `resolve_inherited_call_sites_are_enumerated_and_pinned` | 1 passed (count unchanged) |
| bin `oblivion` / `skill_use_emit` / `player_cast` / `bow::` / `attach_to_player` | 48 passed, 15 ignored / 16 / 6 / 12 / 1 |
| bin `limbs` / `oblivion_rest` / `stance` / `combat::tests::` | 7 passed, 1 ignored / 2 passed, 1 ignored / 88 passed, 2 ignored / 51 |
| bin profile-oracle filters (`profile`, `follows_the_profile`, `per_game_rule_rows`, `an_oblivion_index`, `crime_` …) | 28 passed |

No `--ignored` plugin test was run (OOM risk). Read-only byte scans: `Oblivion.esm` GMSTs (leveling, regen, combat, armor, crime, disposition, `sLevelUp*`), ACBS flag census of `NPC_`/`CREA`; `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm` GMSTs for the stealth rows. Probe: `7.0f32.clamp(0.0, -5.0)` panics on the pinned toolchain (M-02).

## Regression checks (baseline findings, all closed)

| Baseline | Issue | Verdict |
|---|---|---|
| M-01 Fallout derived rows hardcode authored GMSTs | #6756 | HELD (`fallout.rs` `setting(...)` on every `fAVD*`; pin `retuned_favd_gmsts_reach_the_derived_rows`). The helper does not filter non-finite values: L-03 |
| M-02 Skyrim jail has no 7-day cap | #6757 | HELD (`max_sentence_days: Some(7.0)`) |
| M-03 `RemoveFromAllFactions` bypasses the ledger | #6758 | HELD (`FactionChange::RemoveAll` through the deferred writer; pinned) |
| M-04 `FactionReputation` exempt from the save registry | #6759 | HELD (`register_replacing_component::<FactionReputation>`) |
| L-01 profile-vs-game tests for #5921 readers | #6760 | HELD for the #5921 readers; the rows added since have none (L-06) |
| L-02 `fArmorRatingPCBase` provenance | #6761 | HELD |
| L-03 weapon-condition factor | #6762 | HELD |
| L-04 Skyrim DamageResist / Oblivion armor-rating readers + prose | #6763 | Skyrim half HELD; the Oblivion premise changed under #7010 (the rows are now unreadable for a different reason, L-04) and `charal.md:350-351` is stale again (L-19) |
| L-05 `part_to_hit` doc | #6764 | HELD |
| L-06 pre-#6430/#6350/#6422/#6403 prose | #6765 | HELD for its list; new rot from later wiring events in L-19 to L-21 |
| L-07 capture lag (fVATS, Skyrim jail, crime scope) | #6766 | HELD |
| L-08 player `CharacterLevel`/`Background` withheld | #6767 | HELD |
| L-09 `ResolvedNpc::level()` adoption | #6768 | Closed as documented, unchanged. M-01 is a different, Oblivion-specific level split |
| L-10 feature-matrix VATS | #6769 | HELD; the same table has new rot (L-18) |
| L-11 capture lines for `add_perk_raises_rank` / dialect | #6770 | HELD |

## Constant Verification Table

Verdicts: PASS (value matches the capture or master) · FAIL · UNSOURCED (no capture value; itself a finding). Rows 1-40 of the baseline (Fallout, FO4, Skyrim derived stats and Skyrim armor) are on files no commit touched since 2026-11-07; their pins are green and they carry over as PASS. Part 1 is from Dim 2 and Part 2 from Dim 3.

### Part 1: Oblivion rule rows, Oblivion pools and auto-calc, stealth (Dim 2)

Verdicts: PASS, FAIL, UNSOURCED. "Wiki" is the local offline dump (`cs-uesp-wiki` = Oblivion CS, `uesp-wiki` = UESP, `ck-uesp-wiki`).
"Master" is a byte-scan of the shipped master's GMST records. `obl` = `docs/engine/charal-oblivion-ruleset.md`.

#### Oblivion spell cast (`character/spell_cast.rs`)

| # | Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|---|
| 41 | Effect cost `0.1 x base x D x A x M x T`, each factor floored at 1 (D = duration or 1, A = max(1, area x 0.15), M = magnitude^1.28 or 1, T = 1.5 Target); each effect rounded down, minimum 1 | `effect_cost`, `whole` | obl:204-207; CS wiki *Category:Spell Cost* "Unscaled/Scaled Effect Cost ... rounded down ... (minimum 1)" | PASS. Magnitude is an integer in `EFIT`, so the wiki's "Magnitude factor minimum 1" is met without a clamp |
| 42 | `fMagicDurMagBaseCostMult` 0.1, `fMagicAreaBaseCostMult` 0.15, `fMagicRangeTargetCostMult` 1.5, `fMagicCostScale` 1.28 | `OBLIVION` consts | CS wiki setting pages (0.1 / 0.15 / 1.5 / 1.28); Master authors only `fMagicCostScale` 1.28 | PASS |
| 43 | Skill factor `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult x (1 - S/100)` = `1.4 - 0.012 S` | `modifier` | obl:214; CS wiki *fMagicCasterSkillCostBase*; Master 0.2 / 1.2 | PASS (the code's 0.2 is the master's; the exe's 0.1 is correctly not used) |
| 44 | Effective skill `S = clamp(skill + 0.4 x Luck - 20, 0, 100)` | `luck_skill_mult`, `luck_skill_base` | CS wiki *Effective Skill Level*, *fActorLuckSkillMult*, *iActorLuckSkillBase* (0.4, -20); upper cap 100 UESP *Illusion* (obl:140-146) | PASS. The lower clamp at 0 has no document line (benign) |
| 45 | Manual Spell Cost: `floor(stored x modifier(primary school))`, no minimum | `actual_cost` `Some(stored)` arm | CS wiki *Category:Spell Cost* "Scaled Spell Cost = Unscaled Spell Cost * Skill Factor (rounded down) (no minimum)"; pin `a_manual_spell_costs_...` | PASS |
| 46 | Auto-calc cost: sum of `floor(effect x modifier(own school))`, each at least 1; base cost sum for the tier | `actual_cost` `None`, `base_cost` | CS wiki *Category:Spell Cost* | PASS |
| 47 | Tier thresholds 25.5 / 62.5 / 149.5 / 399.5 on the unscaled sum | `spell_level_min` | CS wiki *fMagicSpellLevelXXMin*; Master authors all four | PASS |
| 48 | Player tier from the unfortified skill against `iSkill{Apprentice,Journeyman,Expert,Master}Min` 25/50/75/100 | `player_tier` + consumer reads `base_skills` | CS wiki *ISkillXXMin*; UESP *Magic Overview* "unfortified" (obl:219-225); Master does not author them | PASS |
| 49 | `fMagicSpellLevelCostMult` 1.0 / `fMagicSpellLevelCostBase` 0.0 applied as an affine map before the tier test | `level_cost_mult`, `level_cost_base`, `spell_tier` | CS wiki *FMagicSpellLevelCostMult* / *CostBase*: "Not used by the game", category Deprecated Settings; no obl line; Master does not author them | UNSOURCED (CHAR-D2-05) |
| 50 | Fatigue per cast `fFatigueCastBase + fFatigueCastMult x cost` = 1 + 0 | `fatigue_cost` | CS wiki *Fatigue Game Settings* (1.0 / 0.0); Master authors `fFatigueCastBase` 1.0 | PASS |
| 51 | Primary school = effect with the highest floored unscaled cost, ties to the first | `primary_school` | CS wiki *Spell Cost* "single most expensive unscaled effect" | PASS (the SCIT-over-MGEF school and bit 22 are the known executable-derived items) |
| 52 | Touch pick length 150 BU, projectile 1000 BU/s up to 10000 BU | `touch_pick_length`, `projectile_*` | CS wiki *IActivatePickLength* 150, *fMagicProjectileBaseSpeed* 1000, *MaxDistance* 10000; obl:299-336 trial | PASS |
| 53 | Every GMST in `with_gmst` ignores a non-finite value | `authored` closure | n/a (hardening) | PASS (`is_finite`), but no range check: see CHAR-D2-04 for the one setting where it matters |

#### Oblivion bow, block, fall, armor rating (`bow.rs`, `block.rs`, `fall.rs`, `armor_rating.rs`)

| # | Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|---|
| 54 | Draw fraction `min(1, fArrowBowTimerBase 0.25 + fArrowBowTimerMult 0.4 x t)`; full draw 1.875 s | `DrawRule::Timer`, `full_draw_seconds` | CS wiki *fArrowSpeedMult* "DrawMult = ... or 1.0 (whichever is less)", *fArrowBowTimerBase* 0.25, *Mult* 0.4; Master authors none | PASS |
| 55 | Arrow speed `ammo x fArrowSpeedMult 1500 x (fArrowWeakSpeed 0.01 + 0.99 x draw)`: 386 / 683 / 1500 | `arrow_speed`, `SpeedRule::Formula` | same page; *fArrowWeakSpeed* 0.01 | PASS (pin `bow_speed_follows_the_draw_timer`) |
| 56 | Oblivion bow damage `(bow + arrow) x 0.5 x (0.75 + Agi/200) x (0.2 + 0.015 MS) x (F/M + 1)/2`, A and MS in [0,100] | `shot_damage` -> `oblivion_weapon_damage_multiplier` | UESP *The Complete Damage Formula* "Weapon Damage" (`WeaponRating` for bows = bow + arrow; "constrained between 0 and 100"); obl:454-470 | PASS for the product; the Fatigue term's edge behaviour disagrees with obl:642 (CHAR-D2-07) |
| 57 | Skyrim draw `0.4 + 1.66/(speed x (1 + QuickShot)) + 0.6` | `DrawRule::Duration`, `SKYRIM` | UESP *Skyrim:Archery* line 166-167 | PASS (2.66 s example) |
| 58 | Skyrim damage `(bow + arrow) x (1 + 0.005 x Archery)` | `SkillBonus` | UESP *Skyrim:Archery* line 6 "+0.5% per skill point" | PASS |
| 59 | Skyrim tilt 2 / 2.5 degrees, range cap 4096 | `tilt_degrees`, `range_cap` | UESP *Skyrim:Archery* lines 255-261 (`fVisibleNavmeshMoveDist` 4096, `f1PArrowTiltUpAngle` 2, `f3P` 2.5) | PASS |
| 60 | Block: shield 1%/level capped 75%, blade/blunt/bow 0.5%/level, bare hands 0.25%/level against an unarmed attacker only, skill capped 100, ranged = melee; shield = biped bit 13 | `BlockModel::OBLIVION`, `reduction` | UESP *Oblivion:Block* "Blocked Melee Damage" + Notes; UESP *Biped Object Flags* 0x2000 = Shield | PASS (no row in obl: CHAR-D2-08) |
| 61 | Fall damage `max(fJumpFallTimeBase 0, t x fJumpFallTimeMult 80)`, 0 below `fJumpFallTimeMin` 0.2 s, x `fJumpFallSkillBase 1.25 + Acr x fJumpFallSkillMult(-0.01)/100`, floor 0, skill not capped | `FallModel::damage` | CS wiki *fJumpFall{TimeMult,TimeMin,TimeBase,SkillBase,SkillMult}* (80 / 0.2 / 0 / 1.25 / -0.01, formula line "final damage = base damage * (SkillBase + Acrobatics * SkillMult / 100)"); obl:359 `1.25 - Skill/10000` | PASS. Master authors only `fJumpFallTimeMult` 80. Boundary pins: `damage(0.1)=0`, `damage(0.2)=20`, NaN -> 0, never negative |
| 62 | Armor: piece `base x (0.35 + 0.0065 x skill) x condition`, sum capped `fMaxArmorRating` 85, `(100 - sum)/100`, 1 for a Master Sneak attacker (Sneak 100); Resist Normal Weapons `(100 - R)/100`, 1 for silver/daedric/enchanted | `ArmorRatingModel`, `weapon_resistance_factor` | UESP *The Complete Damage Formula* (OpponentArmorRating, OpponentWeaponResistance); UESP *Oblivion:Sneak* line 17 (Master ignores armor); obl:506-535; Master `fMaxArmorRating` 85 | PASS for the numbers. Edge handling: CHAR-D2-01 (weakness, creatures), CHAR-D2-04 (cap bounds), CHAR-D2-02 (duplicate coefficients) |
| 63 | ARMO `DATA` rating stored x100; heavy = `BMDT` general-flags bit 0x00800000 | `stored_scale`; `items.rs:780-785` (`general & 0x80`) | UESP *Oblivion Mod:ARMO* BMDT ("0x00800000 = Heavy armor"); `r.u16` then `r.u8` lands on bits 16-23, so `0x80` is bit 23 | PASS |

#### Oblivion combat siblings and derived pools (`combat.rs`, `tes.rs`, `oblivion_population.rs`)

| # | Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|---|
| 64 | `ModifiedSkill = Skill + 0.4 (Luck - 50)` | `modified_skill` | obl:466; UESP *Complete Damage Formula* | PASS (literal restated; see CHAR-D2-06) |
| 65 | Weapon multiplier `0.5 (0.75 + 0.005 A)(0.2 + 0.015 MS)`; `fDamageWeaponMult` 0.5, `StrengthBase` 0.75, `StrengthMult` 0.5, `SkillMult` 1.5, `SkillBase` 0.2, condition 0.5 / 0.5 | `oblivion_weapon_damage_multiplier` | UESP *Complete Damage Formula* "Modifiable Settings"; Master authors every one but `fDamageSkillBase` | PASS (values); literals not GMST-read (CHAR-D2-06) |
| 66 | Hand-to-Hand `1 + 10.5 (STR/100)(MS/100)`; fatigue `1 + 0.5 x health`; no clamp | `oblivion_hand_to_hand_damage` | UESP "Hand to Hand Damage": `fHandHealthMin 1 + (Max 15 - Min) x (StrBase 0 + Str/100 x StrMult 0.75) x (SkillBase 0 + MS/100 x SkillMult 1)` = 10.5; `fHandFatigueDamageBase 1`, `Mult 0.5` (Master authors Max 15, FatigueBase 1, FatigueMult 0.5) | PASS |
| 67 | Oblivion Health `2 x END`, Magicka `2 x INT`, Fatigue `STR+WIL+AGI+END`; per-level `floor(0.1 x END)` into the stored layer | `tes.rs` rows, `apply_oblivion_level_up_health` | obl:663-690, 720-724; UESP *Oblivion:Health* "rounded down", *Magicka* `INT x 2`; Master `fPCBaseMagickaMult` 1.0 | PASS (scope `player_only` stays UNsourced, #6431, pinned) |
| 68 | NPC auto-calc: skills minor `5 + 0.1n`, major `25 + n`, specialization `+5 + 0.5n`; attributes +0.6 / 1.4 / 2.2 / 3 per level by major-skill count, Personality and Luck fixed; Health `trunc(trunc((S+E) x 0.5) + n x (4 - magic + combat + END favored)) x LevelFactor 0.4/0.55/0.7/0.85/1`; Magicka `trunc(INT x 2.5)`; Fatigue `STR+END+AGI+WIL` | `oblivion_population.rs::npc_values` | UESP *Oblivion:NPCs* Skills / Attributes / Health tables (including the LevelFactor table and `ClassFactor` 3/4/5 +1); Master authors `fNPCAttributeHealthMult` 0.5, `iNPCBasePerLevelHealthMult` 4, `fLowLevelNPCBaseHealthMult` 0.4, `iLowLevelNPCMaxLevel` 4, `fNPCBaseMagickaMult` 1.5 | PASS (LevelFactor reproduces 0.4/0.55/0.7/0.85/1 exactly from the settings; the executable-derived rounding mode is known) |

#### Stealth (`stealth.rs`, `stealth/*`), all six rows

| # | Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|---|
| 69 | FNV/FO3 `value = base + sound + visual + (att x P x state - target) x skillMult`; base -35, range 2500 / x2 exterior, exponent 2, sounds 1.6, LOS 0.1 (=0.16), boot 12 + 0.5w, running 1.5, action x2, light 1.4 x min(100, level x 3 night eye), light move 0.01 / run 0.2 (=0.21), P = 10 + 8 PER, skill x0.5 | `stealth/fallout.rs::value` + `DetectionSettings` | `charal-fnv-fo3-ruleset.md:249-300`; `FalloutNV.esm` and `Fallout3.esm` author base -35, max 2500, ext 2.0, LOS 0.1, boot 12/0.5, running 1.5, action 2.0, light 1.4, skill 0.5; the rest fall to the exe defaults 1.6 / 0.01 / 0.2 / 10-90 / night eye 3 | PASS (every capture constant reproduced by master-or-default) |
| 70 | FNV `TargetSkill = Sneak + 5 (TL - DL) + max(50 - 10 TL, 0) - Armor(0/10/20)`, 0 if not sneaking | `level_terms` branch | `charal-fnv-fo3-ruleset.md:257-258,271-273` | PASS |
| 71 | FO3 row = FNV minus the level terms **and the armor term** | `level_terms = false` | `sneak-detection.md` 4.2 names only "the level terms" as FNV's addition; the armor term's absence on FO3 rests on the code and test `the_fo3_row_drops_the_level_and_armor_terms` alone | UNSOURCED for the armor half (CHAR-D2-09) |
| 72 | Oblivion value: base -25; sound `1.6 x DF x (movement [mastery < Expert] + 25 in combat)` x LOS 1.0, movement `(14 + 1.0 x boots [mastery < Journeyman]) x 1.3 running`; sight `1.4 x DF x (-5 + light x 3 night eye) x chameleon x blindness`, x0.5 swimming, 0 asleep; skill `0.5 x (detSneak x DF - tgtSneak [sneaking, not in combat])`; sleep -10; attacked +100; invisible/Chameleon 100 = -100; Seen > 0, Noticed (-20, 0), else Lost | `stealth/oblivion.rs` | CS wiki *Category:Detection* (table written out) + CS setting pages; `Oblivion.esm` authors base -25, max 1500, LOS 1.0, boot 14/1.0, running 1.3, light 1.4, light mod -5, skill 0.5, sleep -10, combat 25, attack 100; unauthored ones (sounds 1.6, ext x2, swim 0.5, night eye 3, Seen 0, Noticed -20, `iAICombatMinDetection` -50) equal the CS wiki defaults | PASS (the exe-default column for Oblivion is superseded by the master for all of the divergent ones: 0.75/7/1.5/1.2/0.5) |
| 73 | Oblivion sneak attack: melee ladder 4/6/6/6/6 for one-hand blade, one-hand blunt, fists; Marksman ladder 2/3/3/3/3 (master); two-handed and staves 1 | `sneak_attack_mult` | UESP *Complete Damage Formula* table (4/6, 2/3 by Sneak 25); UESP *Oblivion:Sneak* lines 119-123 ("Two-handed weapons do not get the damage bonus", fists count as melee) | PASS |
| 74 | Skyrim value `int(base + sound + visual + (att x state x P - sneaking x (0.4 Sneak + 10)) x 0.5)`, P = Sneak (min 0, max 100 authored), sound `(action + movement) x sounds x att x [LOS 0.3]`, base -15, range 2500 x2.1 | `stealth/skyrim.rs` | `charal-skyrim-ruleset.md:162-200` (-15, 2500, 0.3, 12 + 0.5 w, x2 running, action x2) + CK *Category:Detection*; `Skyrim.esm` authors base -15, max 2500, ext 2.1, LOS 0.3, sounds 1.0, action 2.0, running 2.0, light 0.33, perception 0 / 100, `fPlayerDetectionSneak` 10 / 0.4 | PASS. The doc names `fSneakSleepBonus` for the skill term and the code `fSneakSleepMod` for it (the exe form, known) |
| 75 | Skyrim sneak attack by animation type: hand 2, 1H sword/dagger/axe/mace 3, 2H 2, bow 2 (default), staff 1 | `sneak_attack_mult` | `charal-skyrim-ruleset.md:205-210`; `Skyrim.esm` authors hand 2.0, 1H x4 3.0, 2H x2 2.0 | PASS |
| 76 | Skyrim stealth points: drain 3.0 (master), regen min 5.0 / mult 0.2, thresholds 25 / 70, attacked cap 50, event cap 75 | `skyrim.rs::step_points` | CK *Stealth Points*, `sneak-detection.md` 4.3; master authors drain 3.0, regen min 5.0, waits 15 / 10 | PASS |
| 77 | Fallout 4 and Starfield pairs (sound and visual levels, ceil with 0 -> -1, stealth points by sense) | `stealth/fallout4.rs`, `stealth/starfield.rs` | Executable transcriptions (known, not re-filed); code agrees with the formula block in `sneak-detection.md` 4.2 term by term (Stealth Boy zeroes the light term only on FO4, the whole visual term on Starfield; Starfield's skill-movement term only while moving) | PASS for self-consistency with the document; provenance is the known disassembly one |
| 78 | Indicator tables | `stealth/indicator.rs` | `sneak-detection.md` 10 (Fallout 3 cases 1-5; Oblivion eye per UESP) | PASS |


### Part 2: skill-use leveling, regen, disposition, stance, and the regression rows (Dim 3)

Verdict = value vs its documented source. "Not read from GMST" is reported under the finding, not as FAIL, unless noted.

#### Oblivion skill-use leveling (new since baseline)

| Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|
| Rank cost `(factor·L)^exp · spec · (major ? major_mult : minor_mult)`; major replaces minor | `SkillUseCosts::cost` (`skill_use.rs`) | `skill-use.md:22-24`; UESP `Increasing Skills.wiki:311-317` ("Base to Major = x0.6 (`FSkillUseMajorMult`/`FSkillUseMinor`)", S+M x0.45) | PASS |
| Factor 0.35, Exp 1.5 | `SkillUseCosts::OBLIVION` | `skill-use.md:24`; UESP `:311`; **Oblivion.esm byte scan**: `fSkillUseFactor` 0.35, `fSkillUseExp` 1.5 | PASS |
| Minor 1.25, Major 0.75, Spec 0.75 | same | UESP `Increasing Skills.wiki:8,10,311-317` (Minor default 1.25; spec x0.75; major/minor 0.6; S+M 0.45 = 0.75 x 0.6). Oblivion.esm authors none of the three (scan) | PASS (UESP-supported; exe citation is historical provenance) |
| Cost table at 5/25/50/75/99 for Base/Spec/Major/S+M | test `the_cost_table_matches_uesp` (16 cells) + worked examples | UESP `Increasing Skills.wiki:304-317, 434, 459-467` | PASS (tolerances already justified in the test) |
| Rank only when cost is **strictly below** banked XP | `SkillUseCosts::bank` (`cost >= banked` breaks) | `skill-use.md:16`; test `an_equal_cost_does_not_rank` | PASS |
| Surplus stays banked after a rank | `bank`: `(banked - cost).max(0.0)` | `skill-use.md:16`; UESP `:304` "Any extra experience points will roll over", Example 4 | PASS (`surplus_rolls_over`: 1.11) |
| Cost 0 stored as 1.0 | `cost()` fallback | `skill-use.md` §1 (`0x663c50`) | PASS (`a_zero_cost_is_one`) |
| A skill at 100 banks nothing; never ranked past 100 | `SKILL_USE_CAP`, `bank` (`value >= CAP` early return, `while value < CAP`) | `skill-use.md:16` | PASS for the value comparison; reads *base* where the doc says `GetAV` (CHAR-D3-04) |
| `SKIL` use values, 21 rows x 2 (Armorer 1.5/1 ... Speechcraft 2.4/1) | `OBLIVION_SKILL_USE_VALUES` (`skill_use.rs`) | `skill-use.md:34-48` compared cell by cell (all 42 match, CLAS order 12-32) | PASS |
| `skill_use_amount`: value[usage] x mult when mult != 0, drop <= 0 | `skill_use_amount` | `skill-use.md:15`; test `every_oblivion_skill_has_a_use_row` | PASS |
| Armor-hit slot weights Helm 10 / Cuirass 25 / Greaves 15 / Gauntlets 10 / Boots 10, shield = remainder 30; 7 re-rolls | `ArmorHitSlot::ROLLED`, `from_roll`, `ARMOR_HIT_ATTEMPTS` | CS `IArmorDamageXXChance.wiki` (10/25/10/15/10/30 as Boots/Cuirass/Gauntlets/Greaves/Helm/Shield); `skill-use.md:109-117` (re-roll count is exe provenance) | PASS (re-roll count historical) |
| Biped masks Head 0, Hair 1, Upper 2, Lower 3, Hand 4, Foot 5, Shield 13 | `ArmorHitSlot::biped_mask` | xEdit `wbBipedFlags` (cited in code) | PASS |
| Weapon type -> skill: 0/1 Blade, 2/3 Blunt, none Hand to Hand, 5 Marksman (arrow hit only), 4 staff nothing | `weapon_skill` (`skill_use_emit.rs`) | `skill-use.md:109-114`; test in `skill_use_emit` | PASS |
| Governing attribute per skill (Str: Blade/Blunt/H2H; End: Armorer/Block/Heavy; Spd: Athletics/Acrobatics/Light; Agi: Marksman/Security/Sneak; Int: Alchemy/Conj/Myst; Wil: Alt/Dest/Rest; Per: Illusion/Merc/Speech) | `SkillSet::OBLIVION`, `resolve` via `OBLIVION_ACTOR_VALUES` names | UESP `Leveling.wiki:40-80` (both worked examples reproduce: e.g. Strength 4+4 -> +4, Speed 5+5 -> +5, Personality 6+4 -> +5) | PASS (names match the roster 1:1) |
| Tally on every rank-up; major count only for major skills; tenth major increase **counts in its own tally**, freezes tallies into a `PendingLevel`, fresh tallies start | `SkillUseLevel::record_rank_up` / `freeze_pending` | `skill-use.md:175-179`; `charal-oblivion-ruleset.md:1002-1004`; UESP Example 2 | PASS (`the_tenth_major_increase_freezes_its_tallies_into_a_pending_level`) |
| Attribute bonus 0->+1, 1-4->+2, 5-7->+3, 8-9->+4, 10+->+5; Luck always +1 | `oblivion_attribute_bonus`, `PendingLevel::bonus` | `charal-oblivion-ruleset.md:1003-1004`; CS `ILevelUpXXMult.wiki` defaults 2,2,2,2,3,3,3,4,4,5 for 01..10; UESP `Leveling.wiki` ("+5/+5/+1 ... Luck") | PASS for the value; **not read from the settings** (CHAR-D3-01) |
| 10 major increases per level | `LevelingModel::OBLIVION.major_skill_ups_per_level` | CS `ILevelUpSkillCount.wiki` default 10 | PASS for the value; **not read from the setting** (CHAR-D3-01) |
| Attributes cap at 100; picks distinct, <= 3, none at 100; unspent tally rolls to nothing | `pick_attribute`, `apply_level` (`min(100.0)`) | `skill-use.md:185-188`; UESP `Leveling.wiki` ("never possible to gain more than +5") | PASS |
| Perfection: all attributes at 100 -> no level, level stays pending, message text | `LevelUp::Perfection`, `PERFECTION` | UESP `Leveling.wiki:175` text matches exactly | PASS |
| Health +`floor(0.1·Endurance)`, Endurance read **after** the picks, into `ActorValue::stored`; Magicka/Fatigue stored untouched | `apply_oblivion_level_up_health`, `apply_level` | `charal-oblivion-ruleset.md:1146-1165`; UESP *Health* (End 98 -> +9); CS `FStatsHealthLevelMult.wiki` ("rounded down") ; I checked `floor(0.1f32 * n) == n/10` for every n in 0..=65535 (0 mismatches) | PASS for the value; `0.1` is a hard-coded constant for a documented setting (CHAR-D3-01) |
| Stored Health survives refresh, ability add/remove, save | `ActorValue::stored`, `stored_iter` in `actor_value_save.rs`, test `oblivion_level_up_health_gains_survive_the_refresh_and_an_ability_coming_and_going` | `charal-oblivion-ruleset.md:1160-1165` | PASS |
| `SkillUseLevel`, `PendingLevel`, `SkillProgress`, `CharacterLevel` saved | `save_io/registry.rs:88,313-315`; `SavedSkillProgress`; round-trip test `round_trip_tests.rs:2648` | PRD OSL-SAVE-001/-003 | PASS |
| Level-up messages `sLevelUp{N}` for N 2..20, 21+ distinct | `level_message` (`skill_use_emit.rs:268`) | UESP `Leveling.wiki:127-170`; **Oblivion.esm scan** (9 strings only) | **FAIL** (CHAR-D3-03) |
| Perfection / meditate wording | `PERFECTION`, `MEDITATE` | UESP `Leveling.wiki:175` (exact); `MEDITATE` has no repo-readable text source (executable default recorded in V12) | PASS / historical |
| Stance feeds: Sneak 0.75 (usage 0), Athletics swim 0.04 (usage 1) / run 0.03 (usage 0), Acrobatics jump 0.3 (usage 0) / damaging fall 3.0 (usage 1), Block 1.25, Marksman 0.8, schools usage 1 (4/6/1.2/3/3/0.6) | `stance_use_system`, `on_jump`, `on_fall`, `on_landed_hit`, `feed_school` | `skill-use.md:34-48,118-150`; UESP *Increasing Skills* per-action column | PASS (values are the table above) |
| Unfed set = Armorer, Alchemy, Mercantile, Security, Speechcraft; `FED_SKILLS` = 16 codes | `FED_SKILLS`, `skill_use_report` | `skill-use.md:133-150, 152-156` | PASS |

#### Oblivion regen, disposition, stance

| Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|
| Fatigue regen flat 10/s (`fFatigueReturnBase` 10, `fFatigueReturnMult` 0) | `FATIGUE_REGEN_PER_SEC` | `charal-oblivion-ruleset.md:580`; **Oblivion.esm scan**: base 10.0, mult 0.0 (the master spells it `fFatigueReturnMult`; docs/code comments spell `fFatigureReturnMult`) | PASS for the value; **not read from GMST** now that the lane is live (CHAR-D3-02) |
| Magicka regen `(Willpower·0.02 + 0.75) · Max/100`; Stunted Magicka zeroes only Magicka | `magicka_regen_per_sec`, `MAGICKA_REGEN_*`, tick system | `charal-oblivion-ruleset.md:732-766`; Oblivion.esm scan: `fMagickaReturnBase` 0.75, `fMagickaReturnMult` 0.02 | PASS for the value; **not read from GMST** (CHAR-D3-02) |
| Health regen none | no lane | `charal-oblivion-ruleset.md:673-682` | PASS |
| Lane install: `build_tes_pool_regen` keyed on the profile row, not the game | `profile.rs:900`, `cell_loader/references/mod.rs:379` | skill file "Known-open" regen paragraph | PASS |
| `POOL_REGEN_DT` 1/60; `MAX_REGEN_SUBSTEPS` 8; zero dt cannot spin | `regen.rs` | engine tuning, documented unsourced (#2953 closed) | Documented engine tuning |
| AP regen unit: FO3 4/s, FNV 0.06/frame -> 3.6/s via `ap_regen_per_second` (divides by `POOL_REGEN_DT`) | `VatsModel::ap_regen_per_second`, `ActionPointsRegen` | `charal-fnv-fo3-ruleset.md:582-590` ("value sourced, unit NOT") | PASS (installed 3.6 pinned); unit unverified, **Existing: #6644** (VATS-AP-002) |
| Starting disposition: personality `trunc((actor·aMult + aBase + target·tMult + tBase)·0.5)` with vanilla 1.5/0.5 (= `trunc(0.75·N + 0.25·P)`) | `DispositionModel::starting` | UESP *Disposition* (`charal-oblivion-ruleset.md:930-943`): differential `+floor((P-N)/4)` / `-ceil((N-P)/4)`. I reproduced the examples: N40/P39 -> -1, P35 -> -2, P44 -> +1 (test `personality_differential_and_fame_match_the_uesp_worked_examples`); Oblivion.esm authors `fDispActorPerMult` 1.5, `fDispTargetPerMult` 0.5 (scan) | PASS |
| Fame term `trunc(fame·0.3)` capped +-20 (`iDispFameMax`) | same | UESP `:950-952`: +3 per 10 Fame, cap +20 reached at 67 (test: 60 -> 43/60) | PASS |
| Infamy term `(Resp·0.02 − 0.3)·Infamy·(−0.3)` capped +-20, sign flips with low Responsibility | same | UESP `:953-966`: Resp 100 ~1 per 2 (code 0.51), 25 ~1 per 16 (0.06 -> 1 per 16.7), 10 gains (+0.03) | PASS |
| Race / faction reaction terms (`faction·fm·(rank·fm + rankBase)`, rankBase 1.0 authored by Oblivion.esm) | same | UESP says "scales per rank attained" with no formula; the multiplication shape is executable provenance (historical); `fDispTargetFactionRankBase` 1.0 verified in Oblivion.esm | Historical provenance only (no capture-doc formula); not filed |
| Final clamp 0-100; NaN input cannot panic | `.clamp(0.0, 100.0) as i32` | UESP (0-100 scale) | PASS |
| Sneak stance speed `fMoveSneakMult` (Oblivion master 0.6, FO3/FNV 0.57; exe defaults 0.75 / 0.6 only when unauthored) | `StanceModel::OBLIVION/FALLOUT`, `resolve` | `sneak-detection.md:49-52, 729` (Oblivion.esm scan: 0.6) | PASS (defaults moot on vanilla masters) |
| Skyrim/FO4 sneak speed = ratio of `NPC_Sneaking[_MT]` / `NPC_Default_MT` `SPED` forward walk/run (indices 4/5, 9/10) | `StanceModel::SKYRIM/FALLOUT4` | `sneak-detection.md:53-60` (47.2/80.1, 222/370); test `the_skyrim_row_is_the_ratio_of_the_two_movement_types` | PASS |

#### Fallout / Skyrim leveling, VATS, limbs, crime, reputation (regression rows)

| Formula | Code symbol | Document + line | Verdict |
|---|---|---|---|
| FO3/FNV `150·L + 50`; FO4 `75·L + 125`; Skyrim `25·L + 75` (GMST overlay) | `LevelingModel::{FO3,FNV,FO4,SKYRIM}`, `with_gmst` | fnv-fo3 :463; fo4 :494; skyrim :777 | PASS |
| `roll_levels`: cost `ceil(to_next)` >= 1, stops at cap and `u16::MAX`; `capped = cap > 0 && level >= cap` (no off-by-one) | `LevelingModel::roll_levels` | fnv-fo3 :468-469; tests `roll_levels_is_bounded_and_exact_for_odd_curves`, `experience_roll_stops_at_the_fallout_new_vegas_level_cap` | PASS |
| FO3 `10 + INT` + perk every level; FNV `10 + INT/2` (half carried) + perk every 2nd level; FO4 `(0, perk)`; TES `None` | `skill_point_reward`, `grant_experience` | fnv-fo3 :482-505 | PASS |
| Skyrim pool base 100, +10 per pick; skill curve 1.95 | `SKYRIM_POOL_BASE`, `pool_pick_gain`, `SKYRIM_SKILL_USE_CURVE` | skyrim :616-624, :789-807 | PASS |
| `fActionPoints*` standards (20/25/15/30/22/35/10/40/20/10/10, x1.5, x1) | `VatsModel::FALLOUT3.ap` | fnv-fo3 :544-549 | PASS |
| FNV re-tunes (0.75 / 1 / 1.1 / 100 / 0.01 / crit 5) vs FO3 (0.1 / 5 / 0.5 / 3 / 0.7 / 15) | `VatsModel::FALLOUT_NV`/`FALLOUT3` | fnv-fo3 :554-571 | PASS |
| Authored-by-both and wiki-only `fVATS*` defaults (22 values incl. 95 max chance, 5000 engage, 300/2 melee, 0.5 stealth, 2, 1.25, 0.43, 1.75, 0.32/0.27, 0.3, 0.17, 0.1/150/164, CF bonus 5) | `VatsModel::FALLOUT3` | fnv-fo3 :573-590 (now captured, L-07) | PASS |
| To-hit chain order | `to_hit_chance` | fnv-fo3 :592-596 "reconstructed" | Known-open (#6350/#6644/#6645), not re-filed |
| Leg speed 0.85 / 0.75 | `LimbEffectModel::FALLOUT`, `leg_speed_mult` | GECK `Movement Settings.wiki:39-47` | PASS |
| Arm spread `0.5 + (-0.5)·ArmCondition`; ArmCondition 1/0.5/0 | `gun_spread_arm_penalty`, `arm_condition` | GECK `Gun Spread Formula.wiki:20,37-39,105-112` | PASS |
| Melee arm `Base·(0.2 − 1 + 0.8·ArmCondition)/2` | `melee_arm_penalty` | GECK `Weapon Damage Formula.wiki:27, 41-43, 96-104` (formula as written; the page's "10%" prose disagrees with its own formula, disclosed in `limb_effects.rs` header) | PASS as written (see O-1) |
| Crippled = present AV with `current() <= 0`; absent AV never crippled | `limbs.rs::crippled` | vats PRD VATS-LIMB-005; test `missing_conditions_are_not_crippled` | PASS (un-cripple threshold unverified, known VATS-LIMB-005) |
| `DismemberRecord`: gone bits 0..13, killing blow; saved; `mark_gone` has no production caller | `components.rs:293-330` | vats PRD VATS-LIMB-008/-009 | PASS (known: nothing marks a limb gone) |
| Oblivion crime: steal 0.5 round up min 1, attack 40, murder 1000, trespass 5, pickpocket 25, divisor 100 (no cap), alarm 4000, guards 4, rush 500 | `CrimeModel::OBLIVION` | charal-oblivion :1388-1410; Oblivion.esm scan: `iCrimeAlarmRecDistance` 4000, `iCrimeGoldAttackMin` 500, others not authored | PASS |
| Skyrim crime: steal rounded down, cap `Some(7.0)`, `CrimeFaction` scope | `CrimeModel::SKYRIM` | skyrim :838+ | PASS (regression of #6757 clean) |
| Karma +-1000, bands 750/250/-249/-749; bump `[_,1,2,4,7,12]`; axis max 100; 13 faction rows (r1/r2/r3 and REPU ids) | `reputation.rs` | fnv-fo3 :613-785, compared row by row (13 of 13 match, ids and thresholds) | PASS |
| `damage_multiplier` clamps `[0, cap]`, never heals; `STANDING_GRID[infamy][fame]` | `resistance.rs`, `reputation.rs:420-437` | fnv-fo3 :744-760 | PASS |

## Coverage Matrix (Dim 5, re-derived from code)

Legend: Y wired and reachable, P partial, N absent. "Wired" is `RulesetBuilder` arm in `build_ruleset` plus the single construction site `build_character_ruleset` (`cell_loader/references/mod.rs`, `inventory/catalog.rs`).

| Facet | FO3 | FNV | FO4 | Skyrim | Oblivion | FO76 | Starfield |
|---|---|---|---|---|---|---|---|
| Ruleset implemented | Y `fallout3_ruleset_with_gmst` | Y `falloutnv_ruleset_with_gmst` | Y `fallout4_ruleset_with_gmst` | Y `skyrim_ruleset` | Y `oblivion_ruleset` | N (capture only, `charal-fo76-ruleset.md`) | N (capture only; XP curve PENDING) |
| Ruleset wired (`profile.rs` arm) | Y `RulesetBuilder::Fallout3` | Y `::FalloutNewVegas` | Y `::Fallout4` | Y `::Skyrim` | **Y `::Oblivion`** (since #7010; pinned by `oblivion_builds_its_runtime_ruleset`) | N `::None` | N `::None` |
| Derived stats (builder rows) | Y Health, AP, Carry, Melee/Unarmed, Crit; `fAVD*` GMST overlay (#6756) | Y same, FNV dialect | Y Health, AP, Carry, Melee | P two rows (Light Armor coefficient, Carry); pools are population-side (`derive_skyrim_actor_values`) | Y Health, Magicka, Fatigue (4 affine rows), Light/Heavy Armor Rating multipliers; the player's pools are formulas (#7037/#7041) | N | N |
| NPC population model | Y `ClassAutoCalc` | Y `ClassAutoCalc` | Y `Stored` | Y `RaceBaseOffsets` | Y `OblivionNpc` / `OblivionCreature` | N `None` | N `None` |
| Leveling model (`LevelingModel`) | XpCurve | XpCurve | XpCurve | SkillXp | SkillUse | none | none |
| Leveling runtime | Y `grant_experience`, kill XP (`awards_kill_experience` true) | Y same (+ cap raises, #6227) | P `grant_experience` via `RewardPlayerXP` / `levelup xp`; kill XP off (`kill_experience: false`) | P `scripting/src/skills.rs` (`AdvanceSkill`, SkillXp) | Y `oblivion_skill_use.rs` (`bank_skill_use`, `apply_pending_level`) plus sleep/level-up menu `oblivion_rest.rs` (`apply_level_menu_picks`); feeds counted by Dim 3 | N | N |
| Regen wired (lanes) | P AP lane only (`build_vats_model` on first cell load; Fatigue/Magicka lanes `None`) | P AP lane only | N | N (no Stamina/Magicka regen row sourced) | Y Fatigue, Magicka, Willpower, StuntedMagicka lanes (`build_tes_pool_regen` at `cell_loader/references/mod.rs`, gated on `NpcStatModel::OblivionNpc`) | N | N |
| Affliction wired | N | N | N | N | N | N | N (`affliction_tick_system` has zero production registrations; `AfflictionStatus` is `forward-latent` in `registry_completeness_tests.rs`; no shipped `AfflictionTable`) |
| Rule rows present (`Some`) | `vats`, over-encumbered `SpeedMult`, kill XP, limb effects (`build_limb_effects`, via `body_condition_base`) | same | `barter` (no perk hooks) | `armor`, `bow`, `crime`, `barter`, `NoSprint` | `spell_cast`, `bow`, `block`, `fall`, `armor_rating`, `crime`, disposition (`build_disposition`) | none | none |
| Stealth row (`StealthKind` / `indicator` / `stance`) | `Fallout3` / FALLOUT / FALLOUT | `FalloutNv` / FALLOUT / FALLOUT | `Fallout4` / FALLOUT4 / FALLOUT4 | `Skyrim` / SKYRIM / SKYRIM | `Oblivion` / OBLIVION / OBLIVION | `SightOnly` / NONE / NONE | `Starfield` / STARFIELD / NONE (stance unscaled, no source) |
| VATS | Y `VatsModel::FALLOUT3` (console-driven runtime) | Y `VatsModel::FALLOUT_NV` | N `None` (#6650) | N | N | N | N |
| Crime | N `None` (faction-based, unmodelled) | N | N | Y `CrimeModel::SKYRIM` | Y `CrimeModel::OBLIVION` | N | N |

Cross-cuts confirmed: SightOnly is now the stealth kind of FO76 and the unsupported `NONE` profile only (FO4 and Starfield have sourced rows, so prose saying they "answer from the sight test" is stale: CHAR-D5-05 and -07). Known-open (not findings): limb damage routing and crippling-by-play (`mark_gone` has no production caller), `affliction_tick_system` unregistered, FO76/Starfield builder-less, tag-skill per-level formula, FO3/FNV AP NPC scope.

Capture-document coverage ("implementation or explicit not-implemented note"): `charal-fnv-fo3`, `-fo4`, `-skyrim`, `-oblivion` have implementations. `-fo76` carries "Not yet in the CHARAL §8 rollout order" and `-starfield` carries "Not building `starfield.rs` speculatively" (both explicit). No capture is silently orphaned. Two captures do not say that a built stealth row exists (CHAR-D5-05).

## Findings

Finding bodies keep the dimension agents' cross-references (`CHAR-Dn-NN`). Map: M-01 ← D4-01 · M-02 ← D2-04 · M-03 ← D2-01 · M-04 ← D3-01 · M-05 ← D3-02 · L-01 ← D4-02 · L-02 ← D2-06 · L-03 ← D2-03 · L-04 ← D1-01 + D2-02 · L-05 ← D1-02 · L-06 ← D1-03 · L-07 ← D4-07 · L-08 ← D4-03 · L-09 ← D4-04 · L-10 ← D2-07 · L-11 ← D2-05 · L-12 ← D3-03 · L-13 ← D3-04 · L-14 ← D4-05 · L-15 ← D4-06 + D1-04 · L-16 ← D2-09 · L-17 ← D1-05 + D2-08 + D3-05(6) · L-18 ← D5-01 · L-19 ← D5-02 · L-20 ← D5-03 + D3-05(1,3,5) + D4-08(d,e) · L-21 ← D5-04 + D4-08(a-c) + D3-05(2,4,5) + D1-02 prose · L-22 ← D5-05 · L-23 ← D5-06 + D5-07 + D2-08 (Skyrim stealth).

### MEDIUM

### CHAR-2026-11-21-M-01: Oblivion PC-level-offset actors expand leveled gear and spells at the FO3/FNV `calc_min` stand-in, not at the actor's own (player-relative) level
- **Reported as**: CHAR-D4-01
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:216-222` (`effective_actor_level`), `crates/plugin/src/equip.rs:857-863` (`ResolvedNpc::level`), `crates/plugin/src/equip.rs:1008-1030` (`resolve_actor_spells`, `resolve_racial_spells`), `byroredux/src/npc_spawn/equip.rs:234` (`build_npc_equip_state`: `let actor_level = resolved.level();`); the correct level already exists at `crates/plugin/src/esm/records/oblivion_population.rs:40-46` (`actor_level_at_spawn`)
- **Status**: NEW (adjacent: #7022 observes flag timing only; #2955/#4812/#4232 closed, FO3/FNV-only semantics)
- **Source**: `/mnt/data/src/reference/uesp-wiki/Oblivion/Leveled Lists.wiki:109-110` ("The equipment carried by NPCs is determined by the lowest level between the NPC in question and the player") and `/mnt/data/src/reference/cs-uesp-wiki/(main)/How Oblivion engine deals with NPC leveled equipment.wiki` (the experiment: player 31, offset -21 -> level 10 gear; player 10, offset +20, min 30 -> still player-level gear). Level semantics of ACBS bit 7 on Oblivion: `charal-oblivion-ruleset.md` "NPC auto-calc stats" (player level + offset, clamped, floor 1) and `Oblivion.esm` ACBS read as `level: i16` offset.
- **Description**: `effective_actor_level` encodes the FO3/FNV meaning of ACBS bit 7 ("PC Level **Mult**": `level` is a x1000 multiplier, confirmed on FalloutNV.esm in its docstring) and answers `calc_min.max(1)` whenever the bit is set. On Oblivion the same bit is "PC Level **Offset**" and `level` is a small signed offset. The Oblivion population path was given the right level (`actor_level_at_spawn`: player level + offset, min/max clamps, floor 1) for stats and `CharacterLevel`, but every leveled-list expansion (inventory/outfit LVLI, LVSP spells, racial spells) still reads `ResolvedNpc::level()` -> `effective_actor_level`. So one Oblivion actor carries two levels: stats at `player + offset`, gear and spells at `calc_min` (or 1).
- **Evidence**: `Oblivion.esm` census (read-only byte scan of ACBS): 1,568 of 2,482 `NPC_` (63%) and 284 of 914 `CREA` set bit 7; of the 1,568 NPCs, 496 author `calc_min == 0` (gear level 1 at any player level) and 1,072 author a non-zero `calc_min` that then fixes their gear tier for the whole game. `ResolvedNpc::level()` has no game or stat-model arm; `build_npc_equip_state` passes only `resolved` and `index`.
- **Impact**: generic bandits, guards and most creatures with leveled inventories draw the lowest eligible tier regardless of the player's level (expected: `min(actor level, player level)`), so a level-20 player meets level-1 equipment on the majority of NPCs; spells from LVSP likewise.
- **Related**: #7022, #7040 (population work), CHAR-D4-04; this is not the #6768 split (that one is level-0 LVLN picks on FO3/FNV).
- **Suggested Fix**: make the leveled-list level a per-stat-model answer (the `NpcStatModel` already selects `OblivionNpc`/`OblivionCreature`), computed as `min(actor_level_at_spawn(npc, player_level), player_level)` per the two sources above; keep `effective_actor_level` for the Fallout multiplier semantics and say in its docstring that bit 7 is an offset on Oblivion. Add a census test over the vanilla offset actors (player level 1 and 30) beside `oblivion_population_census_stamps_every_standard_value`.

### CHAR-2026-11-21-M-02: `fMaxArmorRating` is unbounded in the Oblivion armor overlay: a negative value panics, a value over 100 makes armor heal
- **Reported as**: CHAR-D2-04
- **Severity**: MEDIUM / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/armor_rating.rs:48-50` (`with_gmst`), `:62-67` (`armor_factor`) / **Status**: NEW
- **Source**: the sibling overlay `armor.rs:60-73` (`value >= 0.0` filter, `.min(100.0)` "so a retune cannot make armor heal"); Rust `f32::clamp` panics when `min > max` (verified with a 5-line probe on the pinned toolchain: `7.0f32.clamp(0.0, -5.0)` panics).
- **Description**: `with_gmst` takes any finite `fMaxArmorRating`. `armor_factor` computes `combined.clamp(0.0, self.max_rating)`: a negative authored cap makes `min > max` and panics in the first weapon hit that reaches `weapon_defense`. A cap above 100 gives `(100 - combined) / 100 < 0`, a negative damage multiplier, so `apply_health_damage` receives negative damage.
- **Evidence**: `ArmorRatingModel::with_gmst(|n| (n == "fMaxArmorRating").then_some(-5.0)).armor_factor(10.0, false)` panics; with `150.0` and `combined = 120.0` it returns `-0.2`.
- **Impact**: a plugin (or a typo) in one game setting crashes the combat system or turns armor into a heal. Skyrim's equivalent was hardened in #6567.
- **Related**: #6567 (the Skyrim hardening this row missed).
- **Suggested Fix**: filter `value >= 0.0` and cap at 100 exactly as `ArmorModel::with_gmst` does, and floor the factor at 0.

### CHAR-2026-11-21-M-03: Resist Normal Weapons ignores three documented rules: Weakness to Normal Weapons, creature attacks, and the fist exception; one is mislabelled as an open question
- **Reported as**: CHAR-D2-01
- **Severity**: MEDIUM / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/armor_rating.rs:79-84` (`weapon_resistance_factor`), test `the_documented_resistance` (`-20.0 -> 1.0`), `byroredux/src/combat.rs:526-560` (`weapon_defense`), `:574-581` (`weapon_ignores_resistance`), `docs/engine/charal-oblivion-ruleset.md:533-535` / **Status**: NEW
- **Source**: UESP `Oblivion/Weakness to Normal Weapons.wiki` ("50% Weakness to Normal Weapons ... a weapon normally dealing 20 points ... your health would go down by 30"); UESP `Oblivion/Resist Normal Weapons.wiki` ("creature attacks, however, are unaffected by resist normal weapons"; "Normal weapons can be melee and ranged non-enchanted weapons or bare fists (only if below level 50 in Hand to Hand)"); UESP *The Complete Damage Formula* `OpponentWeaponResistance = (100 - NormalWeaponResistance) / 100` with no clamp. `crates/plugin/src/esm/records/oblivion_actor_values.rs:72-79` has `ResistNormalWeapons` and no separate weakness actor value.
- **Description**: (a) The roster has one actor value for the pair, so a Weakness to Normal Weapons effect (scrolls, Shivering Isles cursed gear) can only arrive as a negative `ResistNormalWeapons`; the documented factor is then above 1 (50% weakness -> 1.5). `weapon_resistance_factor` clamps to `[0, 1]` and a test pins `-20.0 -> 1.0` as "not modelled by this term". The ruleset doc's "Not modelled" list (`:532-535`) does not mention weakness. (b) UESP states creature attacks are unaffected by Resist Normal Weapons; the doc records it as an open question and says "(they do here)"; `weapon_defense` applies resistance to any aggressor with `ActorVitals`, creatures included. (c) The two UESP pages disagree on fists at Hand to Hand 50+ (Resist page: fists are normal only below 50; Weakness page: fists "do regular damage provided ... at least Journeyman"); the code treats every unarmed blow as normal (`weapon_ignores_resistance` returns `false` with no weapon). That conflict is also undeclared.
- **Evidence**: the quoted wiki lines above; `armor_rating.rs:83` `.clamp(0.0, 1.0)`; the doc sentence "Not modelled: armor condition (read as full), Shield-like boosting effects, and whether natural creature attacks count as normal weapons (they do here)".
- **Impact**: a player or NPC under a Weakness to Normal Weapons effect takes unmodified weapon damage instead of the documented increase; a ghost, wraith or vampire is also harder to hurt from creature attacks than the wiki says; the declared-gap list under-reports what is built.
- **Related**: #7056/#7054 siblings (same composition site); the ghost smoke `oblivion-ghost.sh` exercises only the 100% case.
- **Suggested Fix**: let the factor exceed 1 for a negative resistance (drop the lower bound of the clamp, keep `max(0)` on the result), add `aggressor is a creature` as a reason to ignore the term where the aggressor record is `CREA`, and either build or declare, in `charal-oblivion-ruleset.md:532-535`, the fist rule with its source conflict stated.

### CHAR-2026-11-21-M-04: Oblivion's level-up settings are hard-coded: `iLevelUpSkillCount`, the ten `iLevelUpXXMult` attribute bonuses and `fStatsHealthLevelMult` never reach the leveling model
- **Reported as**: CHAR-D3-01
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `crates/core/src/character/leveling.rs:117-130` (`with_gmst`, `SkillUse` arm overlays only the five `fSkillUse*`), `leveling.rs:219-222` (`major_skill_ups_per_level: 10`), `crates/core/src/character/tes.rs:187-196` (`oblivion_attribute_bonus` literal bands), `tes.rs:216` (`0.1 * f32::from(endurance)`); consumers `components.rs:46` (`PendingLevel::bonus`), `oblivion_skill_use.rs` (`apply_level`, `pick_attribute`, `per_level_of`)
- **Status**: NEW (same class as closed #6756 and #5915; the skill-use PRD OSL-XP-006 says "the rules MUST be `LevelingModel::SkillUse` data overlaid with the load order's settings")
- **Source**: CS wiki `ILevelUpSkillCount.wiki` ("Number of skill increases necessary to advance to next level", default 10); `ILevelUpXXMult.wiki` (ten settings 01..10, defaults 2,2,2,2,3,3,3,4,4,5, "determine how many potential attribute points the player will have, depending how many times they have used a linked skill"); `FStatsHealthLevelMult.wiki` ("The number by which the player's endurance is multiplied to calculate the amount of health to be added when the character levels up ... rounded down"); `charal-oblivion-ruleset.md:1001-1005` already names `iLevelUpSkillCount` and `iLevelUp01Mult`..`iLevelUp10Mult` as the settings. Oblivion.esm authors none of them (byte scan: all 12 absent), so vanilla is unaffected; the value of each default is correct (see table).
- **Description**: `LevelingModel::SkillUse::with_gmst` resolves the five `fSkillUse*` settings (PASS) and nothing else. The three other leveling settings the CS wiki documents are baked in: the 10-major-increase threshold is a literal in `LevelingModel::OBLIVION`, the +1..+5 bands are a `match` in a free function with no access to the ruleset, and the per-level Health gain is a literal `0.1`. `PendingLevel` stores *tallies*, not bonuses, so a retune would take effect at apply time without a save-format change; only the readers need to go through the model.
- **Evidence**: `with_gmst` arm: `costs: SkillUseCosts { factor: gmst("fSkillUseFactor")..., spec_mult: gmst("fSkillUseSpecMult")... }` with `major_skill_ups_per_level` and `level_cap` passed through unchanged; `oblivion_attribute_bonus(governed_skill_ups: u16) -> u8 { match ... 0 => 1, 1..=4 => 2, 5..=7 => 3, 8..=9 => 4, _ => 5 }`; `oblivion_health_gain_per_level(endurance) = 0.1 * f32::from(endurance)`. The test `skill_use_costs_overlay_the_load_orders_settings` (`leveling.rs:495`) pins only the five.
- **Impact**: Leveling-rule mods (a lower skill-increase count per level, flat +5 attribute bonuses, a retuned per-level Health) are a long-standing Oblivion mod category and change nothing here; no log line, no test. The engine's own mod-compatibility rule (every number a load order can author resolves GMST first) is met for the cost and violated for the level, which the same mods retune together.
- **Related**: #6756 (derived-stat rows, closed), #5915, OSL-XP-006, CHAR-D3-02. Dim 2 should confirm the Oblivion derived rows `fPCBaseHealthMult` (exe default 2, CS `FPCBaseHealthMult.wiki`) and `fPCBaseMagickaMult` (Oblivion.esm authors 1.0, scan) in `oblivion_ruleset(resolve)` have the same gap: `profile.rs:880` calls `oblivion_ruleset(resolve)` with no `gmst`, unlike the FNV/FO4 `_with_gmst` builders.
- **Suggested Fix**: Carry `iLevelUpSkillCount`, an `[u8; 10]` bonus row (index = tally 1..=10; tally 0 stays the engine's +1, which no setting covers) and the Health multiplier on `LevelingModel::SkillUse`, overlay them in `with_gmst`, and have `PendingLevel::bonus`/`apply_level`/the menu feed read them through the ruleset; extend `skill_use_costs_overlay_...` with a retuned-GMST test and fix the three docs in the same commit.

### CHAR-2026-11-21-M-05: Oblivion Fatigue/Magicka regen constants ignore the GMSTs Oblivion.esm authors, now that the lane is live
- **Reported as**: CHAR-D3-02
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `crates/core/src/character/regen.rs:73-91` (`FATIGUE_REGEN_PER_SEC` at :73, `MAGICKA_REGEN_WILLPOWER_COEFF`, `MAGICKA_REGEN_BASE`, `magicka_regen_per_sec`), `regen.rs:246` and `:285` (tick body); installed by `CharacterRulesProfile::build_tes_pool_regen` (`profile.rs:900`, `cell_loader/references/mod.rs:379`)
- **Status**: NEW. The baseline recorded these rows as "PASS (lane `None` everywhere)", which stopped being true when `278886579` (2026-11-15) made `build_tes_pool_regen` resolve the lanes; no open or closed issue covers it (searched "fFatigueReturnBase", "regen GMST retune").
- **Source**: `charal-oblivion-ruleset.md:580` (`FatigueRegen = Endurance x fFatigueReturnMult(0.0) + fFatigueReturnBase(10.0)`), `:732-736` (`MagickaRegen = (Willpower x 0.02 + 0.75) x MaxMagicka/100`); **Oblivion.esm byte scan**: `fFatigueReturnBase` 10.0, `fFatigueReturnMult` 0.0, `fMagickaReturnBase` 0.75, `fMagickaReturnMult` 0.02 are all authored records.
- **Description**: The four numbers are authored GMSTs, but they are module constants: `Fatigue` is `FATIGUE_REGEN_PER_SEC * elapsed` and the Endurance term (`fFatigueReturnMult`) has no code at all ("correctly not modelled" was true only while the lane was dead). A load order that retunes any of the four (regen overhauls are a staple Oblivion mod type, and the Endurance hook exists precisely for that) is ignored. The doc and `regen.rs:26` also misspell the Endurance setting `fFatigureReturnMult`; the master's name is `fFatigueReturnMult` (zero hits for the typo), so an implementer following the doc would read nothing.
- **Evidence**: `avs.restore(fatigue, FATIGUE_REGEN_PER_SEC * elapsed)`; `(willpower * MAGICKA_REGEN_WILLPOWER_COEFF + MAGICKA_REGEN_BASE) * (max_magicka / 100.0)`; `PoolRegenConfig` carries AVIF ids and (AP lane only) a normalized rate, no Oblivion rates, and `build_tes_pool_regen(resolve)` takes no GMST closure (the AP lane, by contrast, gets its rate through `VatsModel::with_gmst`).
- **Impact**: Mod-compat gap on a live system for Fatigue and Magicka of every actor with the pool (player and NPCs). Vanilla behaviour is correct.
- **Related**: CHAR-D3-01, #6756, `charal.md:263`.
- **Suggested Fix**: Give the Oblivion lane a small `TesPoolRegen { fatigue_base, fatigue_end_mult, magicka_base, magicka_will_mult }` row on `PoolRegenConfig`, built by `build_tes_pool_regen(resolve, gmst)` with today's numbers as fallbacks, add the Endurance term, correct the setting spelling in `regen.rs:26` and `charal.md:263`, and pin with a retuned-GMST test.

### LOW

### CHAR-2026-11-21-L-01: The Oblivion player's Magicka formula is a literal `2 x Intelligence`: neither `fPCBaseMagickaMult` nor the Magicka Multiplier actor value (40) is read
- **Reported as**: CHAR-D4-02
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/core/src/character/tes.rs:54-56` (`oblivion_magicka_formula`), `crates/core/src/character/profile.rs:889` (`RulesetBuilder::Oblivion => oblivion_ruleset(resolve)`, no GMST argument unlike the three Fallout arms)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:1175` (player Magicka = `trunc(Int x (1 + fPCBaseMagickaMult))`, GMST 1.0, then x AV 40 / 10; the zero-multiplier rewrite to 1.0 at `0x5ead3d` is stated in the same section) and `:723` (UESP: `Int + Int x fPCBaseMagickaMult (1.0)`). `Oblivion.esm` authors `fPCBaseMagickaMult` = 1.0 (byte scan); `fPCBaseHealthMult` is not authored (exe default 2.0), so Health's literal 2.0 is correct.
- **Description**: the NPC auto-calc path reads `fNPCBaseMagickaMult` from the load order (`oblivion_population.rs` `setting(...)`), but the player's row restates the coefficient as a literal 2.0 and takes no setting, which is exactly the "coefficient restated beside its GMST" regression that #6756 retired for the Fallout rows. Separately the capture's multiplier term (AV 40, 0 meaning 1.0) has no consumer: `MagickaMultiplier` is installed in the roster (`oblivion_actor_values.rs:51`) but nothing reads it, and no known-gap line in "Known gaps against the executable" or "The player's pools" declares that.
- **Impact**: a mod retuning `fPCBaseMagickaMult` is ignored for the player; an effect that writes the Magicka Multiplier value changes nothing. Vanilla values (GMST 1.0, multiplier unset) are unaffected. How often vanilla content writes AV 40 was not measured.
- **Related**: #6756 (Fallout precedent), #7037/#7041 (player pools).
- **Suggested Fix**: add `oblivion_ruleset_with_gmst` reading `fPCBaseMagickaMult` (slope `1 + mult`); either model the AV-40 multiplier as a `Multiplier`-style row or add it to the declared gaps with the capture line.

### CHAR-2026-11-21-L-02: the Oblivion weapon-damage family and its Luck term restate authored settings as literals while the spell-cast row reads the same settings
- **Reported as**: CHAR-D2-06
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/combat.rs:41-84` (`modified_skill` `0.4 x (luck - 50)`, `oblivion_weapon_damage_multiplier` `0.5`, `0.75`, `0.005`, `0.2`, `0.015`, `oblivion_hand_to_hand_damage` `10.5`, `1.0`, `0.5`), `crates/core/src/character/bow.rs:218` (the `(F/M + 1)/2` constants) versus `spell_cast.rs` (`luck_skill_mult`, `luck_skill_base` read from `fActorLuckSkillMult` / `iActorLuckSkillBase`) / **Status**: NEW
- **Source**: `charal-oblivion-ruleset.md:554-564` ("GMST names now captured ... AUTHORED, not hardcoded ... All future formula rows built for Oblivion should read these by name once GMST parsing lands"); UESP *Complete Damage Formula* "Modifiable Settings" (`fDamageWeaponMult` 0.5, `fDamageStrengthBase` 0.75, `StrengthMult` 0.5, `SkillBase` 0.2, `SkillMult` 1.5, `fHandHealthMin/Max`, `fHandDamageStrengthMult`, `fHandDamageSkillMult`, `fHandFatigueDamageBase/Mult`); CS wiki *Fatigue Game Settings* (`FatigueFactor = fFatigueBase - fFatigueMult x (1 - Fatigue/MaxFatigue)`, 1.0 / 0.5). `Oblivion.esm` authors `fDamageWeaponMult`, `StrengthBase`, `StrengthMult`, `SkillMult`, the condition pair, `fHandHealthMax`, `fHandFatigueDamageBase/Mult`, `fFatigueBase`, `fFatigueMult` (probe), so a load order can retune every one of them.
- **Description**: GMST parsing landed long ago and four Oblivion rows (`spell_cast`, `bow`, `fall`, `armor_rating`) overlay it, but the weapon and Hand-to-Hand damage functions and the Luck term are pure literals. The same quantity (`-20 + 0.4 x Luck`) has two homes, one retunable and one not: retuning `fActorLuckSkillMult` changes spell costs but not weapon damage.
- **Evidence**: `rg "0\.4 \* \(luck" crates/core/src` finds `combat.rs:60`; `spell_cast.rs` carries `0.4` / `-20.0` as GMST-backed fields.
- **Impact**: a rebalance plugin that retunes the documented settings is honoured by magic and ignored by melee/bow damage (mod-compat doctrine, CLAUDE.md and *embedded_community_fixes*). Values are correct for vanilla.
- **Related**: CHAR-D2-02 (same duplication pattern), CHAR-D2-07.
- **Suggested Fix**: an `OblivionDamageModel` row (or fields on `BowModel`'s `DamageRule::Rating`) holding the nine settings with a `with_gmst`, installed like the other rows; route `modified_skill` through the spell-cast Luck fields.

### CHAR-2026-11-21-L-03: the #6756 GMST overlay helper accepts non-finite authored values; every sibling overlay rejects them
- **Reported as**: CHAR-D2-03
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: fo3, fnv, fo4 / **Location**: `crates/core/src/character/fallout.rs:58-60` (`setting`), consumed at `:72-75,104-117,143-144,234-247,282-283,340-341`; caller `byroredux/src/npc_spawn/stamps.rs:262,282` (`index.game_setting_float`, which returns the raw `f32`) / **Status**: NEW (gap in closed #6756)
- **Source**: the family convention: `spell_cast.rs:157`, `bow.rs:167`, `fall.rs:49`, `armor_rating.rs:49`, `disposition.rs:57`, `limb_effects.rs:63` all use `.filter(|v| v.is_finite())`; `armor.rs:71` also drops negatives; `LevelingModel::with_gmst` (`leveling.rs:154`) filters too.
- **Description**: `setting(gmst, name, default)` is `gmst(name).unwrap_or(default)`. A load order authoring NaN or infinity for `fAVDHealthEnduranceMult`, `fAVDActionPointsMult`, `fAVDCarryWeightMult` and similar flows into the bilinear/affine rows.
- **Evidence**: `DerivedStatFormula::eval` ends `rounded.max(self.floor).min(self.cap)`; a NaN raw value becomes the floor (verified: `NaN.max(0.0).min(INF) = 0`), so a player's Health or AP reads 0, while `+inf` with an uncapped row (Health, Carry Weight) reads infinity.
- **Impact**: hostile or corrupt plugin data zeroes or unbounds a derived pool. Low likelihood, and the same data would be rejected by every other CHARAL overlay.
- **Related**: closed #6756 (M-01).
- **Suggested Fix**: `gmst(name).filter(|v| v.is_finite()).unwrap_or(default)` in `setting`, with a one-line test beside `retuned_favd_gmsts_reach_the_derived_rows`.

### CHAR-2026-11-21-L-04: Oblivion's `LightArmorRating`/`HeavyArmorRating` derived rows can never resolve, and duplicate `ArmorRatingModel`'s coefficients
- **Reported as**: CHAR-D1-01
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Oblivion
- **Location**: `crates/core/src/character/tes.rs:59-76,139-162` (`ARMOR_RATING_SKILL_COEFF`, `ARMOR_RATING_SKILL_BIAS`, the two rows in `oblivion_ruleset`); `crates/core/src/character/armor_rating.rs:43` (`ArmorRatingModel::OBLIVION`); `crates/plugin/src/esm/records/oblivion_actor_values.rs:10-82` (the roster)
- **Status**: NEW
- **Source** (numeric): `docs/engine/charal-oblivion-ruleset.md:454-535` (UESP *Oblivion:The Complete Damage Formula*: `PieceArmorRating = BaseArmorRating x (0.35 + 0.0065 x ArmorSkill)`). Both copies agree with it; the finding is the second home and the unreachable rows, not a wrong value.
- **Description**: `oblivion_ruleset` pushes two `as_multiplier` rows whose *output* keys are resolved by editor id `"LightArmorRating"` / `"HeavyArmorRating"`. Since #7008 Oblivion's `actor_value_form_id` resolves only the 72-name engine roster (`index.rs:1176-1180`) and then the installed AVIF rows, which are those same 72 names. The roster contains no `*ArmorRating` name, so `resolve("LightArmorRating")` is `None`, the `if let` at `tes.rs:141`/`:152` never fires on a real load and both rows are dead. The live armor-rating path is `ArmorRatingModel` (`combat.rs:560`), which carries its own `0.35`/`0.0065`. That is the #6568 pattern (two homes for one coefficient pair) with no pin tying them together.
- **Evidence**: `OBLIVION_ACTOR_VALUES` names (index 0 to 71) end `... "Vampirism","Darkness","ResistWaterDamage"` with no armor-rating entry; the only tests that exercise the rows use a synthetic `resolve` that answers those names (`tes.rs:343-408`). The capture (`charal-oblivion-ruleset.md:521`) describes the rows as wired ("two rows in `oblivion_ruleset()`"). Nothing reads either output outside `tes.rs`.
- **Impact**: dead code and an inaccurate capture; the duplicated constants can drift (a future retune of one copy changes only one path). No wrong runtime value today.
- **Related**: #6568 (closed, the Skyrim twin); CHAR-D1-02 (why no test caught it).
- **Suggested Fix**: delete the two rows and the two consts from `tes.rs` (and the capture sentence), or keep one set of coefficients in a single home that both `ArmorRatingModel::OBLIVION` and the rows read, with an equality pin.

- **Merged**: Dim 2 reported the same defect as CHAR-D2-02 (both copies equal UESP's `0.35 + 0.0065·skill`; #6568's Skyrim shape is the precedent). Its `charal.md:350-351` prose half is in L-19.

### CHAR-2026-11-21-L-05: The Oblivion `ROSTER_CASES` entry asserts nothing about the installed roster or derived outputs, and its neighbouring prose is stale
- **Reported as**: CHAR-D1-02
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Oblivion
- **Location**: `crates/plugin/tests/parse_real_esm.rs:938-948` (`RosterCase` Oblivion, `derived_rows: None`, `authors_actor_values: false`) and `:1008-1020` (early `return`); `crates/plugin/src/esm/records/tests.rs:166-193`; `docs/engine/charal.md:367`; `crates/core/src/character/tes.rs:109-112`
- **Status**: NEW
- **Description**: `assert_rosters_resolve` returns at the `!authors_actor_values` branch (it only checks that the *parsed* master has no AVIF) before it reaches the roster, `skills()` and `derived_rows` assertions, and it never installs the engine roster (`parse_esm` leaves `actor_values` empty; the loader installs afterward). So `AttributeSet::TES_CLASSIC`, `SkillSet::OBLIVION` and the `oblivion_ruleset` output keys are not falsified against the roster the engine actually uses, in an ignored test or in a unit test (`derived_row_len` is asserted for no Oblivion build). That is how CHAR-D1-01 escaped. The `derived_rows: None` arm's own message ("if one landed, give this case its expected derived_rows") was written for a builder that now exists (`RulesetBuilder::Oblivion`). Separately, `records/tests.rs:166-179` still says Oblivion "carries `RulesetBuilder::None`, confirmed unreachable in production", `charal.md:367` says the Oblivion builder "is unwired" and needs a legacy resolver, and `tes.rs:111` says "Pre-AVIF Oblivion supplies a resolver ... at the parser boundary": all false since #7008-#7010.
- **Evidence**: `oblivion_ruleset_resolves_nothing_against_avif_pending_a_legacy_resolver` (`records/tests.rs:180`) still passes only because it never installs the roster. `oblivion_builds_its_runtime_ruleset` (`profile.rs:1356`) is `build_ruleset(|_| None, ...).is_some()`.
- **Impact**: a misspelled or absent Oblivion roster/derived name (the exact failure of CHAR-D1-01) stays green; three docs mislead a reader about whether Oblivion has a runtime ruleset.
- **Related**: CHAR-D1-01; #4094 (the FO76/Starfield `ROSTER_CASES` precedent); Dimension 5 owns the doc sweep.
- **Suggested Fix**: install the engine roster in the Oblivion case (as `oblivion_population_census_stamps_every_standard_value` does) and assert `attributes`, `skills` and the built `derived_row_len()` with the GMST overlay; add a non-ignored unit test doing the same on a synthetic installed index; rewrite the three stale statements.

- **Merged**: the three stale statements named here (`records/tests.rs:166-179`, `charal.md:367`, `tes.rs:109-112`) are fixed under L-19 and L-21; this finding is the vacuous test.

### CHAR-2026-11-21-L-06: The new Oblivion/Skyrim rule installers have no direct test, and none proves the profile wins over the game
- **Reported as**: CHAR-D1-03
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Oblivion, Skyrim
- **Location**: `byroredux/src/player_cast.rs:23` (`install_spell_cast_model`), `byroredux/src/combat.rs:560,612,623` (`install_armor_rating_model`, `install_fall_model`, `install_block_model`); `byroredux/src/crime.rs:995-1030`; `byroredux/src/systems/faction_hostility.rs:82`; `crates/core/src/character/profile.rs` tests
- **Status**: NEW
- **Description**: #6760's rule (a new row-reading consumer adds a test whose index has a `GameKind` and a `CharacterRulesProfile` that disagree) is not followed for the rows added since the baseline. `grep -rn 'install_(spell_cast|armor_rating|fall|block)_model'` finds only the definitions and `scene_runtime.rs:79-83`: no test calls them, so the profile to resource arm and the `None => remove_resource` arm are unpinned. `install_bow_model` is covered only implicitly (default `GameKind::Fallout3NV` index with an Oblivion profile). `crime.rs`'s install tests always pass a matching game and profile (`GameKind::Oblivion` + `OBLIVION`, ...), so they never show the profile deciding. No value-pinning "row is stated on every profile" test exists for `spell_cast`, `bow`, `block`, `fall`, `armor_rating`, `stance`, `stealth`, `indicator` (the armor, crime, vats and perk rows each have one), so moving `block: Some(BlockModel::OBLIVION)` onto the Skyrim profile would break no test.
- **Evidence**: the grep above; `crime.rs:996,1011,1020-1022` pair `GameKind::{Oblivion,Skyrim,Fallout3NV,Fallout4}` with their own profiles.
- **Impact**: a future consumer-side game test, or a wrong row on a profile, would not fail a test; the consumers themselves are game-free today (0 production `GameKind` hits).
- **Related**: #6760 (closed), CHAR-D1-02.
- **Suggested Fix**: one `*_follows_the_profile_not_the_game` test per installer (Skyrim game + Oblivion profile installs; Oblivion game + FNV profile clears) and one `the_new_rule_rows_are_stated_on_every_profile` table in `profile.rs`; make the crime install test use a disagreeing pair. Optionally add the `armor.rs`-style `production_text` `GameKind::` scan to `combat.rs`, `bow.rs`, `player_cast.rs`.

### CHAR-2026-11-21-L-07: The Oblivion roster-order codes in the cast, bow and skill-use writers have no pin against `OBLIVION_ACTOR_VALUES`
- **Reported as**: CHAR-D4-07
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/scripting/src/player_cast.rs:28-31` (`AV_LUCK 7`, `AV_MAGICKA 9`, `AV_FATIGUE 10`, `AV_SCHOOL_FIRST 20`), `byroredux/src/skill_use_emit.rs:30-44` (`BLADE 14`, `BLUNT 16`, `HAND_TO_HAND 17`, `HEAVY_ARMOR 18`, `LIGHT_ARMOR 27`, `ATHLETICS 13`, `ACROBATICS 26`, `SNEAK 31`, `MARKSMAN 28`, `BLOCK 15`), `crates/scripting/src/oblivion_skill_use.rs:472` (`FED_SKILLS`), `crates/core/src/character/skill_use.rs:21-24` (`OBLIVION_SKILL_AV_FIRST/LAST`)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md` "The 72 records" (the one `const` roster in xEdit `wbActorValueEnum` order)
- **Description**: I checked every constant by hand against the roster (all correct today: Armorer 12 ... Speechcraft 32, Luck 7, Magicka 9, Fatigue 10, Alteration 20). They are a second copy of the roster order; the existing tests only compare table lengths (`skill_use.rs:424`) or use the same constants on both sides (`player_cast.rs:552`), so reordering or a roster correction would make the writers feed or spend the wrong value with every test still green.
- **Impact**: none today; the failure mode is silent mis-feeding.
- **Suggested Fix**: one test in the scripting or binary crate asserting `OBLIVION_ACTOR_VALUES[code]` equals the expected EditorID for each constant above and for `FED_SKILLS`.

### CHAR-2026-11-21-L-08: `ActorValueView::Base` / `Permanent` and `GetPermanentActorValue` leave out the `stored` layer, and the two branches of `actor_value_view` disagree
- **Reported as**: CHAR-D4-03
- **Severity**: LOW (latent: no live Oblivion reader today)
- **Dimension**: Population Boundary
- **Game**: Oblivion (the only game with a non-zero `stored`)
- **Location**: `crates/scripting/src/condition/eval.rs:487-489` (authored fast path: `Base => base + set_override`, `Permanent => base + set_override + permanent_mod`), `crates/scripting/src/obscript_more/state.rs:643` (`cmd_11ef`: `base + set_override + permanent_mod`); contrast the fall-through branch (`eval.rs:495-519`) which clones the entry and so keeps `stored` through `CharacterRuleset::actor_value` (`ruleset.rs:171`, `current() - base`)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:1123-1210`: the executable's base of Health is `(stored + derived) x multiplier` (`0x5ead00`), i.e. `stored` is part of the base, and the engine's own `ActorValue::undamaged` counts it (`actor_values.rs:84-89`, #7044).
- **Description**: #7044 added `stored` to `undamaged()`, the SDK fold and the finite check, but the hand-written view arms still sum the older layers. For a `base_authored` Health entry (the Oblivion player's refreshed pool) the Base/Permanent views therefore exclude the level-up gains, while the same function's fall-through composition includes them.
- **Evidence**: the Oblivion `GetBaseActorValue` (CTDA 277) is declined on purpose (#7038, `obscript_cmd.rs:181`), and `GetPermanentActorValue` is Fallout-only, so no Oblivion script or condition can observe the divergence at HEAD.
- **Impact**: becomes a wrong `GetBaseActorValue Health` after any level-up the moment #7038's decline is lifted; until then none.
- **Related**: #5508 / #5706 (hand-summed layers class), #7038, #7044.
- **Suggested Fix**: derive Base and Permanent from `ActorValue` accessors (as `undamaged()` does) instead of summing fields, and add a test with a non-zero `stored` for each view.

### CHAR-2026-11-21-L-09: PC-offset creature scaling is incomplete and the capture misstates what the engine does with attack damage
- **Reported as**: CHAR-D4-04
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/plugin/src/esm/records/oblivion_population.rs:222-260` (`creature_values`: skills are the group value, no level term), `byroredux/src/npc_spawn/stamps.rs:165-190` (`stamp_creature_attack`: flat `oblivion_creature_stats.damage`), `docs/engine/charal-oblivion-ruleset.md:1366-1370` ("the engine reads no attack damage from this model")
- **Status**: NEW (doc half: stale since `278886579`, 2026-11-15)
- **Source**: attack: `/mnt/data/src/reference/cs-uesp-wiki/(main)/Stats Tab - Creatures.wiki` ("For PC level offset creatures the entered value is added to 1/2 the level of the creature"); the capture's own `0x51c980` note gives `stored + level x fCreatureCalcDamage` with the exe default 1.0 and the setting not authored by `Oblivion.esm` (byte scan: absent), so the 0.5 vs 1.0 disagreement is unresolved. Skills: `/mnt/data/src/reference/uesp-wiki/Oblivion/Leveled Lists.wiki:17-19` ("CombatSkill = Combat + 2*CreatureLevel", same for Magic and Stealth); the executable's skill path was not traced for a level term (the capture decodes only the two-view remap).
- **Description**: #7040 locked the level multiply for Health, Magicka and Fatigue only. The creature skills and the attack damage of a PC-offset creature are stamped unscaled, and neither omission is listed as an open item; the capture also still says the engine reads no attack damage while `stamp_creature_attack` has stamped `CreatureAttack` from it since 2026-11-15.
- **Impact**: PC-offset creatures (284 `CREA` in the vanilla master) hit with their authored base damage and carry base skills at every player level, where the CS/UESP texts say both rise with the creature's level; magnitude depends on which of the unresolved sources is right.
- **Related**: #7040, #7022; "no guessing": do not pick 0.5 vs 1.0 or confirm the skill term from a disassembly, observe it in the original game or declare the gap.
- **Suggested Fix**: add both terms to "Known gaps against the executable" with the two source lines, correct the attack-damage sentence, and decide the coefficients only after an observation (the #7022 smoke harness is the natural place).

### CHAR-2026-11-21-L-10: the bow's Fatigue term is clamped to at most 1 and measured against the fortified maximum, contradicting the capture
- **Reported as**: CHAR-D2-07
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/bow.rs:218` (`traits.fatigue_fraction.clamp(0.0, 1.0)`), `byroredux/src/bow.rs:286-295` (`current() / undamaged()`) / **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:642-646` ("the `(Fatigue/MaxFatigue+1)/2` term is explicitly *not* clamped to 1, so Fortify Fatigue effects pushing current above max keep paying off past the nominal ceiling", citing UESP *Oblivion:Fatigue* "if your current Fatigue is higher than your normal maximum ... you'll do more damage"); CS wiki *Fatigue Game Settings* (`FatigueFactor` has no clamp). The offline UESP dump (2022) does not carry the "higher than your normal maximum" sentence, so the capture line is the only citation.
- **Description**: `shot_damage` clamps the Fatigue fraction to `[0, 1]`, and the consumer divides current Fatigue by `undamaged()`, which includes the temporary and permanent modifiers (Fortify Fatigue raises numerator and denominator together), so the ratio can never exceed 1 even without the clamp. The documented edge (more damage above the normal maximum) is unreachable; the lower clamp at 0 (a collapsed actor, Fatigue below 0) is also undocumented.
- **Evidence**: `bow.rs` `(traits.fatigue_fraction.clamp(0.0, 1.0) + 1.0) / 2.0`; `actor_values.rs:88` `undamaged()` "adds every layer but damage".
- **Impact**: a Fortify-Fatigue archer does up to 25% less damage than documented (ratio 1.5 -> factor 1.25 versus 1.0); small and rare.
- **Related**: CHAR-D2-06.
- **Suggested Fix**: read the denominator from the unfortified maximum (base layer sum of the four attributes) and drop the upper clamp; keep or document the lower one.

### CHAR-2026-11-21-L-11: `SpellCastModel` reads and applies two settings the CS wiki documents as unused, with no capture row
- **Reported as**: CHAR-D2-05
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/spell_cast.rs:109,143,167-168` (`level_cost_mult` 1.0, `level_cost_base` 0.0), `:257-266` (`spell_tier`) / **Status**: NEW
- **Source**: CS wiki `FMagicSpellLevelCostMult.wiki` and `FMagicSpellLevelCostBase.wiki`: "Not used by the game." (category Deprecated Settings); `Oblivion.esm` authors neither (probe); neither appears in `charal-oblivion-ruleset.md` "Spell casting" (the tier rule there is the bare base cost against `fMagicSpellLevel*Min`).
- **Description**: `spell_tier` evaluates `base_cost x level_cost_mult + level_cost_base` before the threshold test, and `with_gmst` lets a load order retune the two. The identity defaults hide it, but a plugin authoring either setting would change tiers in a way the documented rule does not, and the coefficients have no source line. By the no-guessing rule they are UNSOURCED coefficients.
- **Evidence**: the wiki text above; `rg SpellLevelCost docs` finds nothing outside `spell_cast_census.rs`.
- **Impact**: negligible today; dead flexibility that contradicts the source it cites.
- **Related**: #7077 (tracking), #7078 (the sourcing pass).
- **Suggested Fix**: delete both fields, or add a capture line that says the executable reads them (not by disassembly: owner rule) before keeping them.

### CHAR-2026-11-21-L-12: Level-up flavour text is missing for 10 of 19 levels and level 21+ reuses level 20's: `Oblivion.esm` authors only nine `sLevelUp` strings, not "2 to 20"
- **Reported as**: CHAR-D3-03
- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `byroredux/src/skill_use_emit.rs:266-277` (`level_message`: `game_setting_string(&format!("sLevelUp{}", level.clamp(2, 20)))`); `docs/engine/skill-use.md:189` ("`Oblivion.esm` authors 2 to 20")
- **Status**: NEW
- **Source**: UESP `Oblivion/Leveling.wiki:127-170` (local dump) lists a message for every level 2..20 and a distinct one for "21+" ("The results of hard work and dedication always look like luck..."); **Oblivion.esm GMST census**: the only `sLevelUp*` records are 4, 6, 11, 12, 15, 17, 18, 19, 20 (their text equals UESP's rows for those levels).
- **Description**: The code uses the load order's string and falls back to the bare "You are now level N." when none is authored. Levels 2, 3, 5, 7, 8, 9, 10, 13, 14 and 16 therefore get no flavour, and every level above 20 reuses level 20's text where UESP documents a different 21+ message. The doc's claim is the premise of the design and is false for the shipped master (the other strings are executable defaults, which the repo reads from no file). The data to close the gap is in a documented source, so no executable read is needed.
- **Evidence**: scan results above; `level.clamp(2, 20)` in `level_message`.
- **Impact**: Cosmetic (a notification line), but it is a documented player-facing message that a vanilla Oblivion player sees on 10 of the first 19 level-ups, and a wrong one beyond level 20.
- **Related**: PRD OSL-API-004, `skill-use.md:189`, CHAR-D3-05.
- **Suggested Fix**: Hold UESP's 2..20 and 21+ table as a documented fallback keyed by level (load order first, as now), use the 21+ text above level 20, and correct `skill-use.md:189` to say which nine the master authors.

### CHAR-2026-11-21-L-13: The "a skill at 100 banks nothing" gate reads the base value where the recorded rule reads `GetAV`
- **Reported as**: CHAR-D3-04
- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `crates/scripting/src/oblivion_skill_use.rs:172` (`let before = entry.base.max(0.0) as u16;`) feeding `SkillUseCosts::bank(value, ...)` (`skill_use.rs`, `value >= SKILL_USE_CAP`)
- **Status**: NEW (needs an owner decision; may be intended)
- **Source**: `skill-use.md:16` ("**a skill at 100 or more banks nothing** (`GetAV(skill) >= 100 -> return`); otherwise `exp += amount` ..., the rank-up writes the skill's **base** + 1"); PRD V2 / OSL-XP-003. The same row separates the two reads: the gate is on the current value, the write on the base.
- **Description**: `apply()` passes the base layer to `bank()`, so a player at base 95 wearing Fortify Blade 10 (current 105) still banks and ranks; the recorded rule would bank nothing. Whether the cost level `L` is also the current value is not recorded, so only the gate is flagged. The base-layer write itself (OSL-LVL-001) is correct and tested.
- **Evidence**: `values.get_base(skill.av)` -> `entry.base`; `ActorValue::current()` is not consulted anywhere in the file.
- **Impact**: Negligible on a vanilla load with no skill-fortifying gear; a divergence from the recorded rule that no test pins either way.
- **Related**: OSL-XP-003, V2.
- **Suggested Fix**: Either gate on `ActorValues::current` of the skill (keeping the base write) and pin it, or record in `skill-use.md:16` that the engine gates on the base layer by decision.

### CHAR-2026-11-21-L-14: "The Player `NPC_` is form 7" is restated as a literal in three population paths beside the canonical constant
- **Reported as**: CHAR-D4-05
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: all (Oblivion-added copies)
- **Location**: `crates/plugin/src/esm/records/oblivion_population.rs:62` (`resolved.shell.form_id == 7`), `byroredux/src/npc_spawn/stamps.rs:103` (`index.npcs.get(&7)`), `crates/scripting/src/oblivion_skill_use.rs:74` (`const PLAYER_NPC`); canonical: `byroredux/src/inventory/mod.rs:42,71` (`PLAYER_NPC_FORM_ID`, `player_npc_form_id`, with a per-game test `player_npc_form_id_is_canonical_across_supported_games`)
- **Status**: NEW
- **Source**: n/a (duplication, not a numeric rule)
- **Description**: three new Oblivion-era sites each hard-code the lineage-wide Player base id instead of one shared definition. `inventory/mod.rs` documents that an exhaustive per-game table is there so a divergent variant is a compiler-visible edit; the copies in the plugin and scripting crates bypass it (the plugin and scripting crates cannot see a `byroredux` binary item, so the constant belongs lower).
- **Impact**: no behavior today; a future master whose Player differs would be fixed in one site and silently missed in three.
- **Suggested Fix**: hoist the constant to a crate both sides see (the plugin crate next to `NpcRecord`) and have `player_npc_form_id` and the three sites use it. Per the repo rule, improve the existing definition rather than adding a fourth.

### CHAR-2026-11-21-L-15: Two consumer-side `GameKind` tests remain on the Oblivion leveling route: `install_skill_use_classes` and `oblivion_rest::supports_rest`
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: oblivion
- **Location**: `crates/scripting/src/oblivion_skill_use.rs:79-97` (`install_skill_use_classes`, `index.game == GameKind::Oblivion`); `byroredux/src/oblivion_rest.rs:47-51` (`supports_rest`, `ScriptGame.0 == GameKind::Oblivion`), callers `oblivion_rest.rs:111,331`, `interaction.rs:1512`, `menuxml_rest.rs:254`, `menu_stack.rs:415`
- **Status**: NEW (merges CHAR-D4-06 and CHAR-D1-04)
- **Description**: The skill-use runtime keys on `LevelingModel::SkillUse`, and its module doc says "there is no game test here" (`oblivion_skill_use.rs:9-11`). Two gates still test the game:
  - **The class table** is built only when `index.game == Oblivion`. That gate is redundant, because `ClassRecord::specialization` is decoded only for Oblivion (`actor/class.rs:119-141`).
  - **Sleep, wait and the bed prompt** are offered only on `GameKind::Oblivion`, and they lead to `apply_pending_level` and the level-up offer. So the entry to a data-driven level-up is a game test. It is partly a UI-routing fact, since only Oblivion ships these two MenuXml documents.
- **Evidence**: the two predicates above; the Dim 1 grep finds no other production `GameKind` hit in a CHARAL rule consumer.
- **Impact**: none today. The #5921 class: a second game with a classic-TES class row or a sleep/wait menu has no data row to flip and needs a code edit in each gate.
- **Related**: #5921, #7086 (Oblivion sleep and level-up menus), `docs/engine/skill-use.md:192`.
- **Suggested Fix**: key the class table on `class.specialization.is_some()` (or the installed leveling model); derive `supports_rest` from `leveling.skill_use_costs().is_some()` or a profile `rest` row, keeping the MenuXml document check on the UI side.

### CHAR-2026-11-21-L-16: the FO3 sneak row drops the armor-noise term with no document line
- **Reported as**: CHAR-D2-09
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: fo3 / **Location**: `crates/core/src/stealth/fallout.rs:41-47` (`level_terms` gate covers both the level and the armor terms), test `the_fo3_row_drops_the_level_and_armor_terms` / **Status**: NEW
- **Source**: `docs/engine/sneak-detection.md` 4.2 ("New Vegas is the FO3 shape plus the level terms in the target skill") and `charal-fnv-fo3-ruleset.md:257-258,271-273` (the armor penalty belongs to `TargetSkill` on the FNV page; the page calls the level reading "FNV's addition over FO3" and says nothing about the armor term on FO3).
- **Description**: The FO3 row removes `5 (TL - DL)`, `max(50 - 10 TL, 0)` and the armor penalty together. The prose documents only the first two as FNV's addition; the armor half rests on the code and test alone.
- **Evidence**: the gate `if level_terms { ... - i.target_armor.penalty() }`.
- **Impact**: if FO3 does carry the armor penalty, a heavily armored FO3 player is detected more easily than the engine computes. Unknowable from the repository; a declared gap is cheap.
- **Suggested Fix**: state in `sneak-detection.md` 4.2 which source the FO3 armor omission comes from, or label it a declared choice in the same way the Silent Running visual term is (`charal-fnv-fo3-ruleset.md:326-336`).

### CHAR-2026-11-21-L-17: Rule rows sourced only in module docs and PRDs: Oblivion block, fall and bow, Skyrim archery, FO3/FNV crippled-limb effects; the Acrobatics capture still calls fall "not a CHARAL build target"
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine (seam sourcing)
- **Game**: oblivion, skyrim, fo3, fnv
- **Location**: `crates/core/src/character/{block,bow,fall,limb_effects}.rs` module headers; `docs/engine/charal-oblivion-ruleset.md:354-382` (Acrobatics); no Block or Bow section in `charal-oblivion-ruleset.md`, no Archery section in `charal-skyrim-ruleset.md`, no crippled-limb section in `charal-fnv-fo3-ruleset.md`
- **Status**: NEW (merges CHAR-D1-05, the capture half of CHAR-D2-08, and item 6 of Dim 3's CHAR-D3-05)
- **Source**: the rows' own cited sources, all re-checked against the local wiki dumps and matching (Constant table rows 54-61 and the Dim 3 limb rows):
  - UESP *Oblivion:Block*: shield 1% per level, capped at 75%; weapon 0.5%; bare hands 0.25%; skill capped at 100.
  - CS wiki `fJumpFall*`: 80 / 0.2 / 0 / 1.25 / −0.01.
  - CS wiki `fArrowBowTimer*`, `fArrowSpeedMult`, `fArrowWeakSpeed`.
  - UESP *Skyrim:Archery*: +0.5% per point, tilt 2 / 2.5, range 4096.
  - GECK *Movement Settings* / *Gun Spread Formula* / *Weapon Damage Formula*: 0.85 / 0.75, 0.5 / −0.5, 0.2 / 0.8. The page's "10%" prose disagrees with its own formula, which `limb_effects.rs` discloses.
- **Description**: The skill makes `charal-*-ruleset.md` the authority for every constant, but these rows' numbers live only in module headers:
  - **Block**: also in `skill-use.md:144`.
  - **Bow**: in `docs/prd/player-archery.md`.
  - **Limb effects**: in `docs/prd/vats.md` §5.3.

  The Acrobatics capture records only `1.25 − Acro/10000`, omits the `fJumpFall*` defaults the built row encodes, and still says the formulas are "documented for completeness, not a CHARAL build target", which the built `fall` row now contradicts. No value is claimed wrong. The rows are cited, not invented, so this is a gap in the documents of record, not a guessed number.
- **Impact**: a deep `/audit-character` run cannot verify these rows from the capture documents.
- **Suggested Fix**: add Block, Bow and Fall subsections to `charal-oblivion-ruleset.md`, an Archery subsection to `charal-skyrim-ruleset.md` and a "Crippled-limb effects" subsection beside VATS in `charal-fnv-fo3-ruleset.md`, each quoting the source line; update the Acrobatics note.

### CHAR-2026-11-21-L-18: `docs/feature-matrix.md` CHARAL section (and the ROADMAP residue bullet) still rates Oblivion "unwired", with no stealth, crime, disposition, cast, bow, block, fall or skill-use rows
- **Reported as**: CHAR-D5-01
- **Severity**: LOW (doc rot, but it is the project's public status table and every Oblivion cell in it is wrong)
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion (also fo3/fnv/fo4/skyrim for the missing rows)
- **Location**: `docs/feature-matrix.md` "Character / Progression (CHARAL)" table and the prose beneath it; the "What Doesn't Work Yet" rows "CHARAL: player actor-value seed" and "CHARAL: Oblivion ruleset built but unwired ..." (section header says "live gaps as of 2026-10-22"); `ROADMAP.md` "CHARAL runtime-dead surface" bullet ("FO3, FNV, Skyrim and FO4 reach actors")
- **Status**: NEW (same family as closed #2961, #3768, #6769; those were earlier wiring events)
- **Description**: The matrix was last touched by `e7e753505` (a video commit) and was not updated by #7009-#7011, #7037-#7056 or M65 Phase 5/6. Each claim below is contradicted by code:
  - "Ruleset wired Oblivion: ~ built, unwired" and the paragraph "`CharacterRulesProfile::OBLIVION` still maps to `RulesetBuilder::None`, so it never reaches a live actor". Contradicted by `CharacterRulesProfile::OBLIVION` (`ruleset: RulesetBuilder::Oblivion`), the `RulesetBuilder::Oblivion => oblivion_ruleset(resolve)` arm of `build_ruleset`, and the test `oblivion_builds_its_runtime_ruleset`.
  - "NPC actor-value population Oblivion: ✗" and "Creature (`CREA`) Oblivion: ✗ `CREA.DATA` layout unsourced". Contradicted by `npc_stats: NpcStatModel::OblivionNpc` and `creature_stats: NpcStatModel::OblivionCreature`, and `oblivion_population::derive` (ROADMAP's own Oblivion row reports 2 482 / 2 482 `NPC_` and 914 / 914 `CREA`).
  - "Runtime leveling Oblivion: ✗" and the prose "Oblivion, FO76 and Starfield still have no live leveling path". Contradicted by `crates/scripting/src/oblivion_skill_use.rs` (`apply_pending_level`, `apply_level_menu_picks`) and `byroredux/src/oblivion_rest.rs`.
  - "Pool regen tick Oblivion: ✗ inert" and "the Fatigue/Magicka lanes are inert everywhere". Contradicted by `build_tes_pool_regen` (`profile.rs`) installed at `cell_loader/references/mod.rs` for `NpcStatModel::OblivionNpc`.
  - "What Doesn't Work Yet: player actor-value seed ... Oblivion no" and "Oblivion ruleset built but unwired; Fatigue/Magicka regen ... inert everywhere". Same code.
  - The table has no row at all for: M65 stealth/detection (six `StealthKind` rows; line ~260 even says "Not yet: detection meter"), crime (`CrimeModel::OBLIVION` / `SKYRIM`), starting disposition (`build_disposition`), Oblivion player cast / bow / block / fall / armor-rating (`install_*_model`), or Oblivion skill-use leveling. `grep -i 'stealth\|crime\|disposition\|archery\|spellcast' docs/feature-matrix.md` finds none in the CHARAL section.
  - `ROADMAP.md` "CHARAL runtime-dead surface": "FO3, FNV, Skyrim and FO4 reach actors" omits Oblivion, whose runtime ruleset has reached actors since #7010; the same ROADMAP's Oblivion paragraph (`Oblivion's actor values are live`) says so.
- **Evidence**: see the symbols above.
- **Impact**: A reader sizing Oblivion work concludes the game has no character runtime; a reader sizing M65 or crime sees no row for them.
- **Related**: CHAR-D3-05 and CHAR-D4-08 (same wiring event, different files), #7007 (tracking), #6769.
- **Suggested Fix**: Re-derive the table from the Coverage Matrix above: flip the four Oblivion cells, add rows for stealth (six games), crime (Oblivion, Skyrim), disposition, cast/bow/block/fall/armor-rating and skill-use leveling, rewrite the two prose paragraphs and the two "Doesn't Work Yet" rows (move the date), and add Oblivion to the ROADMAP bullet.

### CHAR-2026-11-21-L-19: `charal.md` §4.7 and §5 still describe the pre-#7010 Oblivion (builder unwired, regen lanes deleted, `PoolRegenConfig` shape, "unread" rows)
- **Reported as**: CHAR-D5-02
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion
- **Location**: `docs/engine/charal.md` §4.7 ("Mechanism BUILT and registered; live for FO3/FNV Action Points only", the Oblivion paragraph citing `oblivion_pool_regen_config` and "blocked on Oblivion's own wiring, #3768 (#4460)", "Stunted Magicka ... always passed `false`", and the `PoolRegenConfig` struct sketch); §5 (the paragraph "**The Oblivion ruleset builder is complete — it is unwired.** `CharacterRulesProfile::OBLIVION` carries `ruleset: RulesetBuilder::None` ... `Oblivion.esm` authors no `AVIF` records ... a legacy actor-value index resolver is needed" and "Oblivion's `LightArmorRating` / `HeavyArmorRating` rows are equally unread: Oblivion has no runtime ruleset yet")
- **Status**: NEW (the `charal.md:263` spelling is Dim 3's; Dim 3's regen item covers `charal-oblivion-ruleset.md`, not this file's prose)
- **Description**:
  - §4.7 says the Oblivion Fatigue/Magicka lanes need a deleted builder and that re-adding it "is blocked on Oblivion's own wiring". The lanes are now installed by `CharacterRulesProfile::build_tes_pool_regen` (`NpcStatModel::OblivionNpc` guard; fills `fatigue_avif`, `magicka_avif`, `willpower_avif`, `stunted_magicka_avif`) at `cell_loader/references/mod.rs`. The tick passes `stunted` from `stunted_magicka_avif` (not a constant `false`), and the struct sketch omits the `stunted_magicka_avif` field the real `PoolRegenConfig` has. The heading's "live for FO3/FNV Action Points only" is also false.
  - §5 says Oblivion's profile is `RulesetBuilder::None`. It is `RulesetBuilder::Oblivion`. It says no AVIF resolver exists. The engine supplies the 72 Oblivion records (ROADMAP: index coverage 70 / 70, `oblivion_actor_values.rs`), and `docs/feature-matrix.md` is cited as backing "~ built, unwired" (which is itself stale: CHAR-D5-01).
  - §5's pool formulas "Health = 2×Endurance, Magicka = 2×Intelligence" are the `tes.rs` builder rows, while the player's pools are formulas of `fPCBaseHealthMult` / `fPCBaseMagickaMult` (#7037/#7041; see the `RulesetBuilder::Oblivion` arm comment in `build_ruleset`). The section does not mention that split.
- **Evidence**: `profile.rs` `OBLIVION` const and `build_tes_pool_regen`; `regen.rs` `PoolRegenConfig` fields; `oblivion_builds_its_runtime_ruleset`.
- **Impact**: `charal.md` is the layer's design doc and `/audit-character` reads it first; it tells the reader the Oblivion ruleset is dead and regen is inert.
- **Related**: #3768 (CLOSED 2026-08-31, cited as an open blocker), #4455 (CLOSED, same paragraph), #4460 (CLOSED), CHAR-D3-05, CHAR-D5-01.
- **Suggested Fix**: Rewrite the §5 "complete — unwired" paragraph to "wired (#7010, `RulesetBuilder::Oblivion`)" with the engine-supplied roster as the AVIF source; rewrite the §4.7 Oblivion paragraph and the struct sketch (add `stunted_magicka_avif`, name `build_tes_pool_regen`); state the player-formula versus builder-row split; drop the #3768 / #4460 blocker wording.

- **Merged**: also fix `charal.md:350-351` ("Oblivion's `LightArmorRating` / `HeavyArmorRating` rows are equally unread: Oblivion has no runtime ruleset yet", from CHAR-D2-02; the rows are unread because the roster has no such value, L-04), `charal.md:367` ("the Oblivion builder is unwired", from CHAR-D1-02), and the `fFatigureReturnMult` spelling at `charal.md:263` (the master's setting is `fFatigueReturnMult`, from CHAR-D3-02 / M-05).

### CHAR-2026-11-21-L-20: The Oblivion capture and `skill-use.md` still describe a pre-wiring Oblivion: no ruleset, inert regen, skill-use "Not built", no level-up, 8 of 21 skills fed
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion
- **Location**: `docs/engine/charal-oblivion-ruleset.md`:
  - `:9-12` (header: "builder complete but unwired, `RulesetBuilder::None` … needs a legacy actor-value resolver first");
  - `:605-619` and `:765-766` (regen "no-ops today … inserted by no Oblivion path"; "`pool_regen_tick_system` always passes `false` today");
  - `:659` ("Oblivion has no wired ruleset today … it matters when #3768 lands");
  - `:1009` (skill-use "Not built: the PRD … owns the build");
  - `:1326` and `:1364` ("cannot happen until Oblivion has a level-up");
  - `:1366-1370` ("the engine reads no attack damage from this model").

  `docs/engine/skill-use.md:3` ("the emitters … feed 8 of the 21 skills").
- **Status**: NEW (merges CHAR-D5-03, items 1, 3 and 5 of CHAR-D3-05, and items d-e of CHAR-D4-08)
- **Description**: Each spot is contradicted by code at HEAD:
  - **Ruleset**: `RulesetBuilder::Oblivion` (#7010).
  - **Regen**: `CharacterRulesProfile::build_tes_pool_regen`, installed on the first cell load. The tick computes `stunted` from `stunted_magicka_avif`.
  - **Skill-use and level-up**: `oblivion_skill_use::apply_level`, `250a5ba67`.
  - **Attack damage**: `stamp_creature_attack` stamps `CreatureAttack` (since 2026-11-15).
  - **Fed skills**: `FED_SKILLS` holds 16 codes. The same document's later section says 16 of 21.

  The `:659` note matters beyond wording: the `.player_only()` scope of the three pool rows is now observable on a live actor path. `:1326` / `:1364` should also say that already-spawned PC-offset actors keep spawn-time stats until their cell reloads. That divergence is declared by the skill-use PRD (row 5, OSL-NG-003, owned by #7022/#7040).
- **Impact**: the capture opens by saying the layer it captures is unwired and contradicts itself about 1,000 lines later; `/audit-character` reads it first.
- **Related**: L-19, L-21, L-09 (the attack-damage gap itself), M-05 (the regen spelling), #7067 and #7094 (closed doc fixes of the same family).
- **Suggested Fix**: reword the header to "wired by #7010"; rewrite the regen paragraphs around `build_tes_pool_regen`; mark skill-use and the level-up built (#7050-#7056) and cite OSL-NG-003 at `:1326`/`:1364`; correct the attack-damage sentence; change `skill-use.md:3` to 16 of 21.

### CHAR-2026-11-21-L-21: About a dozen code comments still name the closed #3768 "pre-AVIF resolver" or say Oblivion has no AVIFs, no ruleset or no shot/guard producer
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine (code comments describing an intended insertion site)
- **Game**: oblivion
- **Location**:
  - **Citing #3768 or a pre-AVIF resolver**:
    - `byroredux/src/boot/world.rs:56-59`
    - `byroredux/src/boot/schedule/update/player_animation/player_stats.rs:37-44` (which also says every non-Fallout game never inserts the regen config)
    - `crates/core/src/character/regen.rs` `PoolRegenConfig::fatigue_avif` doc ("all of today's loads")
    - `crates/core/src/character/profile.rs:271` (`vital_pools`, "Oblivion pre-#3768")
    - `byroredux/src/inventory/catalog.rs:263` (`build_player_vitals`)
    - `crates/core/src/character/tes.rs:40-44` ("revisit with #3768") and `:109-112` ("Pre-AVIF Oblivion supplies a resolver … at the parser boundary")
  - **"Oblivion has no AVIFs" or no ruleset**:
    - `crates/plugin/src/esm/records/actor_value_derive.rs:165` ("Empty for every other game (Oblivion)")
    - `crates/scripting/src/actor_value_names.rs:270-272`
    - `crates/scripting/src/condition/eval.rs:107`
    - `crates/scripting/src/condition/catalog.rs:34`
    - `crates/plugin/src/esm/records/tests.rs:166-179` (Oblivion "carries `RulesetBuilder::None`, confirmed unreachable")
  - **Behaviour descriptions**:
    - `crates/core/src/character/regen.rs:81-82` (`magicka_regen_per_sec`: "always `false` from the tick system today")
    - `byroredux/src/skill_use_emit.rs:20-21` (module header: "a bow has no shot path here … a blocked hit feeds nothing"; the same file feeds Marksman and Block)
    - `crates/scripting/src/oblivion_skill_use.rs:497-498` (`skill_use_report`: pending levels and unfed skills "arrive with the phases that build them"; both are reported now)
- **Status**: NEW (merges CHAR-D5-04, items a-c of CHAR-D4-08, items 2, 4 and the code part of 5 of CHAR-D3-05, and the code prose of CHAR-D1-02)
- **Description**: Each comment states a precondition that no longer holds:
  - `build_tes_pool_regen` installs the Oblivion lanes.
  - The engine-supplied 72-record roster resolves Oblivion's AVIF ids (`EsmIndex::actor_value_form_id`, `condition_actor_value_form` via `vanilla_name` + `ActorValueNames`).
  - `derive_resolved_actor_values_with_player_level` routes Oblivion to `oblivion_population::derive`.
  - The emitters feed Marksman (#7055) and Block (#7054).

  #3768 is CLOSED and was a doc-claim issue rather than a resolver tracker, so it points at nothing actionable.
- **Impact**: a reader of the scheduler registration or the condition evaluator is told Oblivion's regen is inert and its actor values unreadable.
- **Related**: L-19, L-20, #4460 (closed, an earlier wiring event's twin).
- **Suggested Fix**: reword each comment to the live mechanism. Add a source-text pin like `mod_docstring_makes_no_unversioned_unreachable_claims` that forbids `pre-AVIF resolver` and `#3768` outside `docs/audits/`.

### CHAR-2026-11-21-L-22: `CharacterRulesProfile` accessor and field docs lag the rows they describe (stealth, crime, creature stats, essential time)
- **Reported as**: CHAR-D5-05
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: all (fo4/starfield for stealth, skyrim for crime, oblivion for creature stats)
- **Location**: `crates/core/src/character/profile.rs`: the `stealth` field doc (~:349-353: "Fallout 4 and Starfield wait on a source (Q2), so they answer from the sight test"); the `crime()` accessor doc (~:771: "`None` where no crime model is sourced and wired (every game but Oblivion)"); the `creature_stat_model()` doc (~:729: "[`NpcStatModel::None`] ... on Oblivion, whose `CREA` `DATA` is a different struct this has not been sourced for"); the `essential_death_time_secs()` accessor doc (~:758: "`None` where the game has no bleedout recovery")
- **Status**: NEW
- **Description**: Each is contradicted by a row in the same file:
  - `FALLOUT4.stealth` is `StealthKind::Fallout4` and `STARFIELD.stealth` is `StealthKind::Starfield` (M65 #6996 / #7046). Only `FALLOUT76` and `NONE` carry `StealthKind::SightOnly`.
  - `SKYRIM.crime` is `Some(CrimeModel::SKYRIM)`; the Oblivion capture was already corrected to "Oblivion and Skyrim" (#6766), but the accessor doc was not.
  - `OBLIVION.creature_stats` is `NpcStatModel::OblivionCreature`, wired by #7010.
  - The `essential_death_time_secs` field doc says `None` is not "the game has no bleedout recovery" (#6259: Starfield authors recovery data instead), and the accessor doc one screen below states the opposite.
- **Evidence**: `FALLOUT4` / `STARFIELD` / `SKYRIM` / `OBLIVION` const literals in `profile.rs`.
- **Impact**: the accessor docs are what a consumer author reads; they will wrongly conclude FO4/Starfield have no detection formula or that Oblivion creatures are unpopulated.
- **Related**: CHAR-D5-07 (the matching prose in `sneak-detection.md`).
- **Suggested Fix**: Reword the four docs to match the rows (state the rows by value, not by game list), or phrase them as "see the row".

### CHAR-2026-11-21-L-23: M65 status lags in the captures, `sneak-detection.md` and its index row; the `mod.rs` components bullet under-lists
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim, fo4, fo76, starfield
- **Location**:
  - `docs/engine/charal-skyrim-ruleset.md:211-215` ("Not wired into `stealth.rs`"; the header marks the Visual/skill halves PENDING).
  - `docs/engine/charal-fo4-ruleset.md:48` and `:201-204` (sneak detection a "DEAD END").
  - `docs/engine/charal-fo76-ruleset.md:119-121` (points at FO4's dead end).
  - `docs/engine/charal-starfield-ruleset.md` (silent on detection).
  - `docs/engine/sneak-detection.md:95-96` ("`SightOnly` (Starfield, per Q2)") and `:3` (status leads with "DESIGN (Phase 0 …)").
  - `docs/engine/index.md:24` ("DESIGN (Phase 0, 2026-11-15)").
  - `crates/core/src/character/mod.rs` docstring `components` bullet.
- **Status**: NEW (merges CHAR-D5-06, CHAR-D5-07 and the Skyrim-stealth half of CHAR-D2-08)
- **Description**: "Code ahead of capture is doc rot too."
  - `StealthKind` has seven variants: `Skyrim`, `Fallout4` and `Starfield` are built rows (`stealth/{skyrim,fallout4,starfield}.rs`) consumed by `detection/tick.rs`. Only FO76 and the unsupported `NONE` profile answer from `SightOnly`.
  - The PRD shipped 2026-11-19, but `sneak-detection.md` and its index row still lead with Phase 0 design.
  - The FO4 "dead end" is true of the documented wiki source. The built FO4/Starfield rows are executable-derived (the `stealth.rs` header says so). The capture should state both facts; whether to keep, re-source or relabel those rows is the owner's open call under the 2026-11-20 no-exe-decode ruling, and this audit recommends no decode.
  - The `mod.rs` `components` bullet lists seven types but omits `CharacterIdentity`, `DismemberRecord`, `SkillUseLevel` and `PendingLevel`. `mod_docstring_indexes_every_sub_module` checks module names only.
- **Impact**: a reader of the Skyrim or FO4 capture will scope work that is already built, or treat FO4 detection as impossible; the index row misstates the doc's currency.
- **Related**: L-22 (the matching `profile.rs` `stealth` field doc), #6908 (M65 tracking).
- **Suggested Fix**: add "Status: built as `StealthKind::X` (M65); provenance per `sneak-detection.md`" to the four captures; replace the `kind` line with the seven-variant list; mark the status line and the index row "BUILT (PRD SHIPPED 2026-11-19)"; add the four component names to the docstring bullet.

## Checked and not filed

- **Re-leveling of already-spawned PC-offset actors after a level-up**: declared by the skill-use PRD (row 5, OSL-NG-003, owned by #7022/#7040); only its stale prose is filed (L-20).
- **Oblivion character creation** writes identity only: declared out of scope (OAV-NG-001, OSL-NG-001).
- **Oblivion Encumbrance (AV 11)** is a spawn-time `5 × Strength` stamp with no consumer; over-encumbrance is declared not wired.
- **`fLowLevelNPCBaseHealthMult` 0.4 / `iLowLevelNPCMaxLevel` 4**: these defaults are the master's authored values, not the executable's. Immaterial, since every load order carries the master and a retune is read.
- **`ArmorHitSlot`'s shield is the remainder of the 100-point roll** rather than `iArmorDamageShieldChance` (CS default 30). This is identical unless a mod retunes the shield setting alone.
- **`AdvancePCLevel` with fewer than ten banked majors** loses the remainder: unspecified by the recorded decode, and no vanilla script calls it.
- **Disposition's race/faction reaction multiplication**: an executable-provenance shape with no capture formula, and its inputs are not wired to live records. Not filed, per the owner's no-exe-decode ruling.
- **Executable-derived provenance**: the M65 FO4/Starfield stealth rows, `disposition.rs`, `StanceModel` defaults, MGEF bit 22 and the equal-Magicka gate. All are recorded history from before the 2026-11-20 ruling, with the owner's keep / re-source / relabel call pending. None is re-filed, and no decode is recommended.
- **The FNV/FO3 DetectorState disagreement** (capture 0.8/1.2 versus the executable's form) is already recorded as superseded in `sneak-detection.md`.
- **Per-game asset path tables in `npc_spawn/{anim_clips,skeleton_paths}.rs`** select appearance and animation assets, not rule rows (accepted pattern). Two functions are new since the baseline.
- **`build_tes_pool_regen` and `build_disposition`** are gated on another profile row (`npc_stats`, `ruleset`) rather than their own. They are profile data, game-free in consumers and tested.
- **`charal.md` §3 "(§8.4)" cross-reference** points at the wrong subsection (cosmetic).

## Audit-brief corrections (route to `/audit-sync`)

- Dimension 2's Oblivion bullet writes fall damage as `80·t·(1.25 − 0.01·Acrobatics)`. The code and the CS wiki give `1.25 + Acrobatics·fJumpFallSkillMult/100` = `1.25 − Acrobatics/10000`.
- The known-open list's "Regen … Oblivion **Fatigue/Magicka**" paragraph is accurate. M-05 adds that the lane's rates are literal.
- Dimension 1's First-step grep should include `byroredux/src/{bow,player_cast,skill_use_emit,oblivion_rest}*`, `byroredux/src/detection/` and `crates/scripting/src/oblivion_skill_use.rs`. Neither consumer-side game test in L-15 is in the current path set.

## Known-Open Register (confirmed, not re-filed)

| Item | State |
|---|---|
| FNV/FO3 tag-skill per-level formula; CLAS SPECIAL in `ATTR` | deferred, unchanged |
| FO3/FNV AP / Crit / Melee / Unarmed NPC scope | `.player_only()`, disclosed and pinned (#2937, #6436) |
| VATS #6350 (open) with phases #6644 / #6645 / #6646 (open) | to-hit order still RECONSTRUCTED; ground-truth test `#[ignore]`d with an empty `GROUND_TRUTH`; FNV AP restore-rate unit unverified (VATS-AP-002); limb damage routing and crippling-by-play absent; `mark_gone` has no caller |
| `affliction_tick_system` | still no scheduler registration; rad accumulation unbuilt |
| Oblivion runtime ruleset | live (`oblivion_builds_its_runtime_ruleset`); FO76/Starfield `RulesetBuilder::None` |
| #7077 (open) player spellcasting tracking | MGEF bit 22 and the equal-Magicka gate stay marked executable-derived |
| #4137 (open) | Script 0x0200 and Def Pack List 0x0400 still have no consumer |
| Unfed Oblivion skills (Armorer, Alchemy, Mercantile, Security, Speechcraft) | documented, `FED_SKILLS` = 16 |
| Skyrim `add_perk_raises_rank` | open question, `false` |

## Cross-audit routing

- Component and saved shape of `SkillUseLevel`, `ReadiedSpell`, `EquippedAmmo` and `stored` (v136) → `/audit-save`.
- The Base/Permanent views (L-08) → `/audit-scripting`.
- Scheduler access of the new Late-stage systems → `/audit-concurrency` Dim 4.
- Gameplay correctness of the cast, bow, fall and armor-rating consumers (including M-03's composition) → `/audit-gameplay`.
- The `oblivion_rest` menu surface (L-15) → `/audit-ui`.
