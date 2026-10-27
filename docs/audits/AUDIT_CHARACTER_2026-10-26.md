**HEAD**: bf304851f · **Baseline**: [`AUDIT_CHARACTER_2026-10-21.md`](AUDIT_CHARACTER_2026-10-21.md) (HEAD `4a3e89d56`) · **Audited**: Dim 1 (Ruleset Seam), Dim 2 (Derived Formulas), Dim 3 (Progression, Pools, Crime, VATS), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2's sources (`derived/fallout/tes/skyrim/resistance.rs`, `combat.rs`, `stealth.rs` have no commits since 2026-10-16); the coefficients were still re-verified against captures and, where a master authors the setting, against the master.

# Character / CHARAL Audit, 2026-10-26

`/audit-character` at the default scope (`--depth deep`). Dims 1-2, 3 and 4 ran as three read-only sub-agents, and Dim 5 was done by the orchestrator. Nothing was written to the repo, and the engine was not launched. Read-only probes used: `crates/plugin/examples/probe_gmst_vats` on FalloutNV.esm and Fallout3.esm, and a byte-scan of the Oblivion, Skyrim and Fallout masters for `iCrime*` and `fAVD*` GMSTs. The MEDIUM finding's code claims were re-read by the orchestrator before merging.

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-core character` | 161 passed, 0 failed, 1 ignored. The ignored test is `vats::tests::to_hit_chance_matches_recorded_in_game_percentages`, tracked by #6403. |
| `cargo test -p byroredux --bin byroredux resolve_inherited_call_sites_are_enumerated_and_pinned` | Passed. There are still exactly two production `resolve_inherited_*` calls (`cell_loader/references/mod.rs:796`, `player_body.rs:163`). The #6228 pin also asserts that all three `traits_terminal(` callers are present. |
| plugin lib (`actor_value_derive`, `equip::`, `effective_actor_level`, `pc_level_mult`) | 98 passed, 5 ignored (real-master). |
| bin (`npc_spawn::`, `inventory::`, `vats`, `regen`, `player_derived`, `commands::actor_value`, `build_vats_model`, `obscript_dialect_follows`) | 236 passed, 19 ignored (real-master). |
| `mod_docstring_indexes_every_sub_module` | Green: 16 `pub mod`s, and `vats` and `crime` are indexed. |

## Delta since the baseline

- **VATS and Action Points (#6350, open).** New `character/vats.rs` (`VatsModel`, `077ff45d6`) and `systems/vats.rs`, `commands/vats.rs`, `components/vats.rs`. `3dc16208e` makes the ActionPoints pool live with load-order-priced regen. `f8a1b34bd` (#6389) divides the AP regen conversion by `POOL_REGEN_DT`.
- **Crime.** Profile rows `CrimeModel::OBLIVION` and `SKYRIM` (`e63fa457d`, `39ffea9a8`, `1a67c9f30`, `99051a4eb`, `e161c45f3`, `4cfcee4b9`), consumed by `byroredux/src/crime.rs` and `crates/scripting/src/{crime_faction,jail}.rs`.
- **Other.** `radiation_gauge` profile row (`e9b7bcead`). #6227 NV level-cap raise (`cafd3ccab`). #6228 Use-Traits readers (`b76a74e45`). #6284 profile-row captures (`edf364268`). #6288 effects end on death (`af9d3928f`).

## Executive Summary

**16 findings: 0 CRITICAL · 0 HIGH · 1 MEDIUM · 15 LOW.** None is a regression. The baseline's M-01 (#6227) and M-02 (#6228) both hold (Regression Checks below).

**Constants verified against captures and masters**:

- **Derived rows.** Every FO3, FNV, FO4, Oblivion and Skyrim derived coefficient, plus `combat.rs` and `stealth.rs`, matches its capture. Master-authored settings also match their masters. They were read from Fallout3, FalloutNV, Fallout4, Oblivion and Skyrim.esm.
- **Crime.** Each default matches the CS or UESP wiki, and Oblivion.esm authors `iCrimeGoldAttackMin` = 500.
- **VATS.** The 13 `fActionPoints*` standards match both masters. The FO3 `fVATS*` values match both Fallout3.esm and the GECK *VATS Settings* page.
- **One constant is wrong.** `VatsModel::FALLOUT_NV` carries FO3 values for six `fVATS*` settings (L-02). A real FNV load hides this, because the GMST overlay replaces them.

The one MEDIUM: the first live VATS gate has no AP affordability check, so the player's AP pool can be driven arbitrarily negative.

## Regression Checks

| Item | Result |
|---|---|
| #6227 NV level cap | **PASS.** `LevelingModel::raise_cap` lifts only a capped `XpCurve`. `level_cap_raise` matches the plugin file stem case-insensitively. `build_ruleset` applies it after the GMST overlay. `build_character_ruleset` passes `&index.plugin_files`, and the only producer is `load_order.rs:908`. Cap goes 30 → 50 with all four add-ons. Pin `new_vegas_add_ons_raise_the_level_cap_and_nothing_else_does`. Test gap L-04. |
| #6228 Use-Traits sex/voice | **PASS.** Fixed and pinned. See L-14 for a fourth base-FormID reader the pin does not list. |
| `effective_actor_level` | **PASS.** Single copy (`actor/mod.rs:176`), `.max(0)`. #4232 is still open. |
| Single ruleset sink | **PASS.** `build_character_ruleset` is the only construction site. Both callers handle `None`. |
| #6288 effects end on death | **PASS.** Only `ActiveMagicEffects` is written. `apply_constant_modifiers` stays symmetric through `mod_permanent`, and `set_permanent` has no production caller. |
| Derived-value composition and one ceiling | **PASS.** `CharacterRuleset::actor_value` is used by `GetActorValue` and `melee_damage_charal_bonus`. No hand-summed layers. `undamaged()` is the single ceiling. |
| `setav`/`modav`/SDK/script routing for PlayerOnly pools | **PASS.** All go through `is_player_derived_pool` into `set_override`. This covers the new AP pool. |
| AP pool vs player refresh | **PASS.** `refresh_player_only_bases` writes `base`. Regen and VATS touch only `damage`. The layers are disjoint. |
| Doctrine grep | **PASS** apart from L-01. No `GameKind` or master-name test in any CHARAL rule consumer. Crime and VATS consumers read resources. `install_crime_model` reads `character_rules.crime()`. |
| Writers vs stamps | **PASS** for ActorValues, ActorVitals, CharacterLevel, Background, RaceSpells, FactionRanks, Perks, and LevelUpPoints (on demand). `RadiationRads` has no writer (L-07). |

## Constant Verification Table

Condensed. Full per-row tables are in the sub-agent working files, which are removed in Phase 4. "Master" means the setting was read out of the game's ESM.

| Item | Code | Source | Verdict |
|---|---|---|---|
| FO3 / FNV Health (player) | `90+20·END+10·L` / `95+20·END+5·L`, `.player_only()` | `charal-fnv-fo3-ruleset.md:93`; masters 20/10 and 20/5 | PASS |
| FO3 / FNV AP | `65+2·AGI` cap 85 / `65+3·AGI` cap 95, player-only | `:95`, `:408`; masters 65/2 and 65/3; caps capture-only | PASS (scope: known-open #2937; lead L-10) |
| FO3/FNV Carry, Melee, Crit, Unarmed, Rad/Poison Resist | per `fallout.rs`, `resistance.rs` | `:96-101`, `:131`, `:375`, `:380`, `:396`; masters | PASS |
| FO4 Health, AP, Carry | `77.5+4.5·END+2.5·L+0.5·L·END` floored; `60+10·AGI`; `200+10·STR` | `charal-fo4-ruleset.md:36-88,140,246`; Fallout4.esm | PASS (floor: capture self-contradicts, L-12) |
| Oblivion Health/Magicka/Fatigue, per-level, attribute bonus, Armor Rating | `tes.rs` | `charal-oblivion-ruleset.md:204-345,461,517,797` | PASS (pool scope unsourced, L-11) |
| Oblivion damage (`combat.rs`) | `modified_skill` Luck 0.4; weapon multiplier; hand-to-hand 10.5 | Oblivion "Complete Damage Formula"; Oblivion.esm `fDamage*`, `fHand*` | PASS |
| Skyrim Light Armor, Carry, pool base/pick, skill-use curve | `skyrim.rs` | `charal-skyrim-ruleset.md:215,592,662-666,768`; Skyrim.esm `fSkillUseCurve` 1.95 | PASS |
| Stealth (`stealth.rs`) | attenuation, skill, bands, sound/visual terms | `charal-fnv-fo3-ruleset.md:257-326` | PASS in-domain (L-13 out-of-domain) |
| XP curve and cap | FO3/FNV `150L+50`, caps 20/30; FO4 `75L+125`; Skyrim `25L+75` | `charal-*:450-459`; masters' `iXPBumpBase`/`iMaxCharacterLevel`/`fXPLevelUp*` | PASS |
| NV cap raise | +5 × {DeadMoney, HonestHearts, OldWorldBlues, LonesomeRoad} | `charal-fnv-fo3-ruleset.md:459-460` | PASS |
| Level rewards, perk cadence, kill XP | FO3 `10+INT`/perk each level; FNV `10+INT/2`/every 2nd; 0/10/20/30/50 × 40 % | `:466-491` | PASS |
| Karma bands, reputation bump, 13 faction thresholds, `STANDING_GRID` | `reputation.rs` | `:516-542`, `:589-673` | PASS row-for-row; grid not transposed |
| Regen tick | `POOL_REGEN_DT` 1/60, `MAX_REGEN_SUBSTEPS` 8, zero-dt → 0 ticks, access declaration matches body | engine tuning, disclosed `regen.rs:60-76` | PASS |
| Fatigue / Magicka regen | 10/s; `(WIL·0.02+0.75)·Max/100` | `charal.md` §4.7 | PASS (inert, TES) |
| `fActionPoints*` standards (13) | `StandardActionPoints` | Fallout3.esm and FalloutNV.esm (probe) | PASS |
| `fActionPointsRestoreRate` | FO3 4/s; FNV 0.06 × 60 Hz = 3.6/s | masters 4 / 0.06; **unit has no source** | VALUE PASS, UNIT UNSOURCED (L-03) |
| `fVATS*` FO3 + wiki-only defaults | `VatsModel::FALLOUT3` | GECK *VATS Settings*; Fallout3.esm | PASS |
| `fVATS*` FNV | `FALLOUT_NV` = FO3 copy | FalloutNV.esm: crit 5, dmg-to-weapon 1, player dmg 0.75, spread max 100, uncertainty 0.01, spread mult 1.1 | **MISMATCH ×6** (L-02), masked by overlay |
| To-hit chain order | reconstructed | none (disclosed) | UNVERIFIABLE (#6403) |
| Crime defaults | steal 0.5, attack 40, murder 1000, prison /100, alarm dist 4000, guards 4, `rush_bounty` 500, trespass 5, pickpocket 25 | CS wiki `FCrimeGoldSteal`, `ICrimeGoldXX`, `ICrimeDaysInPrisonMod`, …; UESP *Oblivion:Crime*, *Skyrim:Crime*; Oblivion.esm `iCrimeGoldAttackMin` 500 | PASS (trespass/pickpocket not GMST-driven, L-06) |
| `theft_rounds_up`, saturating casts, `with_gmst` hygiene | Oblivion up with min 1; Skyrim down | UESP both | PASS |
| `radiation_gauge` | `("RAD","RadiationRads")` on FO3/FNV | GECK *Actor Value Codes* 54, *GetRadiationLevel*; masters carry the EDID | PASS vs GECK/masters; no capture line (L-03) |
| Sentinels, `formula_is_thirty_six_bytes_and_copy`, only cross term = FO4 Health | | skill Dim 2 | PASS |

## Coverage Matrix

Re-derived from the `profile.rs` arms and workspace greps. **Changed since the baseline:** the FO3/FNV regen column. `build_vats_model` → `cell_loader/references/mod.rs:362` now inserts a `PoolRegenConfig` with an `ActionPoints` lane.

| Family | Ruleset | Wired | Derived stats | Leveling model | Leveling runtime | Regen | Affliction | Other CHARAL rows live |
|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ | ✓ | ✓ | `XpCurve` + GMST | ✓ kill XP, `levelup` | **~ ActionPoints lane only** (TES ids are `0`) | ✗ unregistered | VATS (`FALLOUT3`), radiation gauge |
| FNV | ✓ | ✓ | ✓ | `XpCurve` + GMST + `level_cap_raises` | ✓ | **~ ActionPoints lane only** | ✗ | VATS (`FALLOUT_NV`), radiation gauge |
| FO4 | ✓ | ✓ | ✓ | `XpCurve` uncapped | ~ no kill XP | ✗ | ✗ | – |
| Skyrim | ✓ | ✓ | ✓ | `SkillXp` + GMST | ~ `AdvanceSkill` roll | ✗ | ✗ | `CrimeModel::SKYRIM` |
| Oblivion | ✓ built | ✗ `RulesetBuilder::None` (deliberate, #3768) | n/a | `SkillUse` | ✗ | ✗ | ✗ | `CrimeModel::OBLIVION` (theft/assault/murder, guards, jail) |
| FO76 / Starfield | ✗ captures only | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – |

## Findings

### MEDIUM

### CHAR-2026-10-26-M-01: VATS `queue`/`confirm` never check AP; an unarmed scan prices every part at `f32::MAX`; same-target shots overwrite each other
- **Severity**: MEDIUM
- **Dimension**: Population Boundary / Progression & Pools
- **Game**: fo3, fnv
- **Location**: `byroredux/src/systems/vats.rs:358` (`ap_cost` fallback), `:486-490` (`vats_queue_part`), `:508-557` (`vats_confirm`), the two `vats_scan` input constructions (`concentrated_shot: 1`); test `confirm_spends_reserved_ap_and_leaves_the_mode` (`:787-830`)
- **Status**: NEW (sits inside open umbrella #6350, whose phase list has no affordability item)
- **Source**: the invariant is vanilla "queued shots may not exceed current AP". The Concentrated Fire part is GECK *VATS Settings* `iVATSConcentratedFireBonus` ("63 % … 68 on the second shot, 73 on the third").
- **Description**: The new live AP pool has a spend side with no gate. The orchestrator re-read the code and confirmed:
  1. `vats_scan` sets `ap_cost = weapon.map_or(f32::MAX, |w| w.ap_cost)`. The scan's own doc says a weaponless scan yields nothing, but it does not skip.
  2. `vats_queue_part` does `vats.ap_reserved += part.ap_cost` with no comparison to the player's current AP.
  3. `vats_confirm` does `avs.apply_damage(ap_avif, reserved.max(0.0))`, with no insufficient-AP early return.
  4. `HitEvent` is stored in a `SparseSetStorage` (one component per entity, `scripting/events.rs:84`). `events.insert(part.entity, …)` inside the per-part loop therefore lets a second queued shot at the same target overwrite the first. At most one hit lands per target per confirm, while all AP is still charged.
  5. `concentrated_shot` is hard-coded to 1, so `iVATSConcentratedFireBonus` is unreachable at runtime although `to_hit_chance` implements it.
  6. `damage` is computed once from `queue[0].entity` and reused for every target.
  7. A weapon with no skill reads the absent-AV default `0.0` rather than the neutral 50.
- **Evidence**: two unarmed parts give `f32::MAX + f32::MAX = +inf`. Armed, the unit test pins `95 − 8·17 = −41` as intended behaviour. FNV regen restores 3.6/s, so the debt takes a long time to drain.
- **Impact**: The player's saved `ActorValues` AP damage is wrecked until `setav`/`modav`. The HUD AP bar goes hugely negative, and shots still roll. It is reachable today only through the `vats.queue`/`vats.confirm` console and smoke driver. It becomes player-reachable the moment a VATS menu is wired to `vats_queue_part`. Multi-shot VATS under-delivers damage, and a perk-facing GMST is inert. The #6403 ground-truth capture cannot catch any of this.
- **Related**: #6350, #6403.
- **Suggested Fix**: Refuse a queue when `ap_reserved + part.ap_cost` exceeds `ActorValues::current(AP)`, and drop weaponless targets in the scan. Re-check on confirm and return `false` on insufficient AP. Accumulate or sequence same-target hits. Pass the 1-based repeat count to `to_hit_chance`. Price damage per target. Flip the 95−136 test to assert refusal.

### LOW

### CHAR-2026-10-26-L-01: `build_vats_model` is a raw `GameKind` gate plus a whole-profile-equality FO3-vs-FNV discriminator; the VATS dialect is not a profile row
- **Severity**: LOW (Dim 1 rated MEDIUM. Behaviour is correct for both shipped profiles, so this is rated on impact.)
- **Dimension**: Ruleset Seam
- **Game**: fo3, fnv (matters for FO4 when its VATS lane lands)
- **Location**: `byroredux/src/npc_spawn.rs:322-335`; doc `profile.rs:196-210`
- **Status**: NEW
- **Source**: doctrine #5921 `3162b21b8` / #4447 / #4679, and `profile.rs:198-210` itself ("Prefer adding a policy row of the consumer's own before adding a third such consumer"). Not numeric.
- **Description**: `if index.game != GameKind::Fallout3NV { return None }` decides whether the game has VATS at all. Comparing the entire `CharacterRulesProfile` against `FALLOUT_NEW_VEGAS` decides which model applies. That is a third profile-identity borrower, and the profile's documented exceptions list only two. The FO3 leg (`FALLOUT3`, 4/s) is exercised by no test. A new family must edit the consumer, and the `per_game_rule_rows_are_stated_on_every_profile` pin cannot see it. The crime consumers were checked and do not have this problem.
- **Related**: #5921, #6350, L-02, L-04.
- **Suggested Fix**: Add `vats: Option<VatsModel>` to `CharacterRulesProfile` (`FALLOUT3`/`FALLOUT_NV` rows, `None` elsewhere). `build_vats_model` becomes `index.character_rules.vats()?.with_gmst(..)`. Extend the row pin, and add an FO3-profile leg asserting 4.0/s.

### CHAR-2026-10-26-L-02: `VatsModel::FALLOUT_NV` copies the FO3 values for six `fVATS*` settings that FalloutNV.esm authors differently; the doc says they are identical
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: fnv
- **Location**: `crates/core/src/character/vats.rs` (`FALLOUT_NV`'s `..Self::FALLOUT3` spread; docs "identical in both games"/"FO3 and FNV share them"; test `vanilla_defaults_match_the_geck_wiki_page` pins `FALLOUT3` only)
- **Status**: NEW
- **Source**: FalloutNV.esm via `probe_gmst_vats`. Each pair is the value FalloutNV.esm authors (FO3 default in parentheses):
  - `fVATSPlayerDamageMult` 0.75 (0.1)
  - `fVATSCriticalChanceBonus` 5 (15; Fallout3.esm does not author it, the 15 is the GECK wiki default)
  - `fVATSDamageToWeaponMult` 1 (5)
  - `fVATSSpreadMult` 1.1 (0.5)
  - `fVATSRangeSpreadMax` 100 (3)
  - `fVATSRangeSpreadUncertainty` 0.01 (0.7)

  FalloutNV.esm also authors `fVATSFortuneOdds` 0.1, which the model lacks.
- **Description**: The FNV model is "FO3 plus the AP restore-rate dialect". A real FNV load overwrites all six through `with_gmst`, so the shipped runtime is correct. The compiled-in fallback and the "identical" claim are wrong, and the model's contract is "GMST, then the game's own vanilla default".
- **Impact**: None on a real FNV load. Wrong for any no-overlay consumer (tests, a debug command, a stripped load order). Phase 3c already routes `fVATSPlayerDamageMult` into the HitEvent funnel.
- **Suggested Fix**: Give `FALLOUT_NV` its own six values (plus `fortune_odds` if a field is added), fix the two "identical" sentences, and add a per-game pin next to `ap_standards_pin_the_per_game_corpus_values`.

### CHAR-2026-10-26-L-03: The VATS tuning family, the AP restore-rate unit and the `radiation_gauge` row are in no `charal-*` capture
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**: `docs/engine/charal-fnv-fo3-ruleset.md` (no VATS, `fVATS*` or `RestoreRate` section; the radiation section at `:388-397` says "mechanism-only"); `docs/engine/charal.md`; code `vats.rs`, `regen.rs::ActionPointsRegen`, `profile.rs:238-244`
- **Status**: NEW (partly adjacent: #6376 asks only for a feature-matrix row, #6403 covers only the placeholder test)
- **Source**: the numbers are sourced elsewhere and verified (GECK *VATS Settings*, *Actor Value Codes* 54, *GetRadiationLevel*, both masters). Missing are the capture document and **any source for the unit claim** that FNV's `fActionPointsRestoreRate` 0.06 is "per frame on a 60 Hz clock" (4 per second on FO3). The only wiki mention of the setting is a `SetGameSetting` example.
- **Description**: The captures are the declared authority for every constant, and the repo cannot adjudicate these rows without one. `ap_regen_per_second` turns 0.06 into 3.6 AP/s. If the engine's unit were different, FNV AP regen would be off by a large factor and no test would notice. Also uncaptured: the reconstructed to-hit chain (disclosed in `vats.rs`), the 255 "always" part sentinel, and the radiation-gauge denominator ("highest sickness stage").
- **Suggested Fix**: Add a "VATS tuning, AP regen, radiation gauge" section to `charal-fnv-fo3-ruleset.md`. Include the probe table with the FNV divergences from L-02 and the GECK citations. Mark the per-frame unit explicitly "unverified" unless a source is found. Cross-reference #6403, and add one pointer line in `charal.md` §8.

### CHAR-2026-10-26-L-04: Test gaps: the `crime` and `radiation_gauge` rows, `install_crime_model`, the `build_vats_model` FO3 leg, and the `plugin_files` wiring
- **Severity**: LOW
- **Dimension**: Ruleset Seam / Coverage
- **Game**: all (oblivion and skyrim for crime; fo3 and fnv for the rest)
- **Location**: `profile.rs` (`crime` rows, `radiation_gauge` rows; test `per_game_rule_rows_are_stated_on_every_profile` covers only the #5921 columns); `byroredux/src/crime.rs:59-67`; `byroredux/src/inventory.rs:384-391`; `cell_loader/load_order.rs:908`
- **Status**: NEW
- **Description**:
  - Nothing asserts that only Oblivion and Skyrim carry a `CrimeModel`, or that exactly FO3 and FNV carry `("RAD","RadiationRads")`.
  - `install_crime_model` (profile → `with_gmst` overlay → resource, and `remove_resource` on `None`) is exercised by no test. Crime tests hand-build the resource.
  - `install_catalog_resolves_per_game_vital_keys` asserts only `["HP","AP"]`.
  - Nothing pins `merged.plugin_files = load_order.clone()` (`load_order.rs:908`), the one line carrying the #6227 fix. Deleting it leaves every test green and silently returns FNV to a cap of 30.
- **Impact**: Swapping the Oblivion and Skyrim crime rows, dropping the rad gauge, leaving a stale `CrimeModel` after a `None` profile, or regressing #6227 would pass every test.
- **Suggested Fix**: Add `crime` and `radiation_gauge` columns to the row pin. Add one `install_crime_model` test (installs plus overlay on Oblivion, clears on FO3/FNV/FO4). Assert the FNV `RAD` entry in the catalog test. Assert `index.plugin_files == order` in `load_order_parallel_tests`.

### CHAR-2026-10-26-L-05: Doc rot after the VATS/AP/crime wiring: sites beyond those #6374 and #6376 list
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv, oblivion, skyrim
- **Location**:
  - `byroredux/src/boot/world.rs:56-64` ("arrives only with Oblivion's wiring … the config is Oblivion-shaped")
  - `byroredux/src/save_io/registry_completeness_tests.rs:473` (reason for `PoolRegenConfig`: "inserted only inside unit tests today"; the not-saved verdict is still right, only the reason is inverted)
  - `crates/core/src/character/mod.rs` docstring: the `regen` bullet says Fatigue/Magicka and omits the `ActionPointsRegen` lane (lines ~38-43), and the `crime` bullet says "Oblivion's theft fine so far" (lines ~26-27)
  - `crates/core/src/character/crime.rs:1-13` (header: "Only Oblivion's fines are sourced so far", but `TRESPASS_GOLD`/`PICKPOCKET_GOLD` and the `SKYRIM` row exist)
  - `crates/core/src/character/regen.rs:1-36` (header scopes the module to Fatigue/Magicka)
  - `docs/feature-matrix.md`: the "Pool regen tick" table row (all seven games `✗ inert`) and the row at ~`:362` ("regen + affliction ticks inert everywhere")
- **Status**: Existing: #6374 and #6376. These sites are **not** in either issue's list, so add them to those fix passes. Same class as closed #4460 and #4107.
- **Description**: `3dc16208e` makes `build_vats_model` return a `PoolRegenConfig` with `action_points: Some(..)`, and `cell_loader/references/mod.rs:362-365` inserts it on every Fallout3NV load. `pool_regen_tick_system` is therefore no longer a no-op on FO3/FNV. It restores `ActionPoints`. It stays inert on the TES lanes, and on Skyrim, FO4 and Starfield. `mod_docstring_indexes_every_sub_module` checks names only.
- **Suggested Fix**: Reword to "inert on TES (no Fatigue/Magicka AVIFs); live for FO3/FNV Action Points only (#6350, `build_vats_model`)", and update the crime bullets and headers.

### CHAR-2026-10-26-L-06: Trespass and pickpocket fines are fixed constants while the other crime settings are GMST-overridable
- **Severity**: LOW
- **Dimension**: Progression & Pools (mod-compat)
- **Game**: oblivion
- **Location**: `crates/core/src/character/crime.rs:134-135` (`TRESPASS_GOLD`, `PICKPOCKET_GOLD`), `with_gmst`; consumer `crates/scripting/src/crime_faction.rs:118-121`
- **Status**: NEW
- **Source**: CS wiki `ICrimeGoldXX` names `iCrimeGoldPickpocket` (25) and `iCrimeGoldTresspass` (5, sic) as Game Settings. Oblivion.esm authors neither (byte scan), so only a mod that adds them is affected.
- **Description**: The module promises "engine-supplied defaults that the load order's GMSTs override by name". Two of the five `CrimeKind` fines are the exception. A mod retuning them is silently ignored on the faction-fine path.
- **Impact**: Mod incompatibility only. Neither kind has a live reporting consumer yet.
- **Suggested Fix**: Promote both to `CrimeModel` fields read in `with_gmst` (sic spelling included), and have `faction_fine` read the model.

### CHAR-2026-10-26-L-07: `RadiationRads` has no production writer, and the gauge and the affliction model disagree about which layer holds it
- **Severity**: LOW
- **Dimension**: Population Boundary / Progression & Pools
- **Game**: fo3, fnv
- **Location**: `byroredux/src/inventory.rs:384-390`, `:425-444` (`values.current(avif).clamp(0.0, scale)`) and test `vitals_snapshot_appends_the_rad_gauge_after_the_pools` (writes `mod_temporary`); contrast `crates/core/src/character/affliction.rs:84-93` (`pool_value` = the `damage` layer) and `docs/engine/charal.md:220-224`
- **Status**: NEW
- **Source**: `charal.md:221-224` ("the existing `ActorValue::damage` layer … read via `AfflictionTable::pool_value`, not `ActorValues::current`"). The gauge row cites GECK *Actor Value Codes* 54 / *GetRadiationLevel* (composed `GetAV radiationrads`).
- **Description**:
  1. Nothing in production creates or changes the value. `grep RadiationRads` finds only the ObScript name table. `consumables::restoration` models Health/AP/Magicka/Stamina only. No exposure system exists. So the gauge reads 0 outside ObScript, console or SDK edits, and the test that proves it hand-inserts the modifier.
  2. If rads accumulate as `damage` (the affliction design), `current()` is `−rads` and the gauge, clamped at 0, stays empty. If they accumulate as a modifier, `pool_value` reads 0 and no band ever applies.
  3. No test drives both readers from one write, and `affliction_tick_system` is unregistered.
- **Impact**: Latent. The first radiation writer will make one reader wrong with no test noticing.
- **Suggested Fix**: Decide the layer in `charal.md` (the engine's `GetAV radiationrads` is composed, so the affliction `pool_value` for rads probably should read `current()`). Pin it with a test that drives both readers from one write, and record in the slice doc that rad accumulation and RADS-stage penalties are unbuilt.

### CHAR-2026-10-26-L-08: `vats_mode_system` declares 7 accesses; its body (via `vats_scan`) touches a dozen more
- **Severity**: LOW
- **Dimension**: Progression & Pools (rule owned by `/audit-concurrency` Dim 4)
- **Game**: fo3, fnv
- **Location**: `byroredux/src/boot/schedule/early.rs:110-123` vs `byroredux/src/systems/vats.rs` (`vats_scan`: `LoadedCellIndex`, `ActorValues`, `ActorVitals`, `FormIdComponent`, `GlobalTransform`, `CreatureActor`, `EquippedWeapon`, `FormIdPool`, `StringPool`, `SubtreeCache`, `PhysicsWorld`, `RapierHandles`, `ActorColliderOwner`, camera)
- **Status**: NEW
- **Description**: The system is exclusive, so there is no race. The declaration exists only for the `sys.accesses` diagnostics report, and it understates the body. `pool_regen_tick_system`'s declaration matches its body exactly.
- **Suggested Fix**: Add the missing reads, or document that the declaration lists only the mode switch.

### CHAR-2026-10-26-L-09: `PoolRegenConfig` fills the unused TES lanes with AVIF id `0` instead of `Option`
- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: fo3, fnv
- **Location**: `byroredux/src/npc_spawn.rs:348-356`; consumed in `regen.rs` (`avs.get(config.fatigue_avif)`, `avs.get(config.magicka_avif)`)
- **Status**: NEW
- **Description**: The inert lanes are disabled only by "no actor carries AV 0". An actor ever holding an ActorValue keyed 0 (a failed upstream `resolve`, a fixture) would arm the Fatigue lane (+10/s). The AP lane correctly uses `Option<ActionPointsRegen>`. The code comment discloses the sentinel.
- **Suggested Fix**: Make the three TES ids `Option<u32>`.

### CHAR-2026-10-26-L-10: A GECK scope lead for FO3/FNV Action Points exists; the capture and the `player_only()` comment still say none does
- **Severity**: LOW
- **Dimension**: Derived Formulas / Coverage
- **Game**: fo3, fnv
- **Location**: `docs/engine/charal-fnv-fo3-ruleset.md:415-426`; the #2937 comment on the AP row in `fallout.rs`
- **Status**: NEW (relates to closed #2937 and #5240)
- **Source**: GECK *fAVDActionPointsBase* ("base amount of action points available to **an actor**") and *fAVDActionPointsMult* ("how many additional action points **an actor** has"), in `/mnt/data/src/reference/geck-uesp-wiki/(main)/`. Counter-evidence: *Stats Tab - NPC* has no AP entry (#5240). Masters: FNV `65/3`, FO3 `65/2`.
- **Description**: The capture says nothing cited for AP states NPC scope. The wording is weak and not conclusive, so the scope stays `player_only`, but the audit trail should record the lead. The live AP lane does not contradict `.player_only()`: FO3/FNV NPCs carry no AP actor value, so the lane runs for the player only.
- **Suggested Fix**: Add the two citations to the capture row and the `fallout.rs` comment. Keep `player_only` until an engine-side check settles it.

### CHAR-2026-10-26-L-11: The Oblivion Health / Magicka / Fatigue rows are `.player_only()` with no capture line stating their scope
- **Severity**: LOW
- **Dimension**: Derived Formulas (scope contract)
- **Game**: oblivion
- **Location**: `crates/core/src/character/tes.rs:39-41`, `:46-48`, `:83-90`; assumption in `regen.rs:~347`
- **Status**: NEW (same class as closed #2937 and #4450)
- **Source**: absent. `charal-oblivion-ruleset.md` states no player/NPC scope for any pool formula. The justification in `tes.rs` is analogy to the Fallout convention, which is itself FO4-evidence-specific. The sibling Armor Rating rows are `ActorGeneral`, with the capture saying the source is silent (`:344-345`).
- **Impact**: None today. Oblivion has no wired ruleset. It matters when #3768 lands.
- **Suggested Fix**: State the scope as unsourced in `tes.rs` and the capture, and extend the existing scope pin to the Oblivion rows.

### CHAR-2026-10-26-L-12: The FO4 capture says both "the actual value is the float (93.5)" and `TotalHP = floor(…)`; the code takes the floor
- **Severity**: LOW
- **Dimension**: Derived Formulas (RoundMode)
- **Game**: fo4
- **Location**: `docs/engine/charal-fo4-ruleset.md:76` vs `:87-89`; `fallout.rs:145-153` (`.floored()`)
- **Status**: NEW
- **Source**: `:76` ("93.5 (the Pip-Boy truncates the display to 93; the actual value is the float)") vs `:86-88` ("`TotalHP = floor(…)`").
- **Description**: The only `RoundMode::Floor` row rests on a capture that asserts both that the stored value is the float and that it is floored. The difference is 0.5 HP, and only where `L·END` is odd.
- **Suggested Fix**: Reword `:76`, or adjudicate with an xEdit or exe read and keep or drop `.floored()`.

### CHAR-2026-10-26-L-13: `stealth::detection_score` attenuation is not monotonic past `MaxDist`
- **Severity**: LOW
- **Dimension**: Derived Formulas (CHARAL-adjacent)
- **Game**: fo3, fnv
- **Location**: `crates/core/src/stealth.rs:243-244`; tests `attenuation_matches_the_captured_expression` sweep only `distance ≤ max_distance`
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:260` gives `((MaxDist − d)/MaxDist)²` with no out-of-range statement. The transcription is exact. The observation follows from the expression (`d = 2·Max` gives 1.0, the same as `d = 0`).
- **Description**: `distance` is unconstrained, and the doc does not tell a future consumer to gate on `distance ≤ MaxDist`. The module has no consumers today.
- **Impact**: A latent trap for the first detection system: an unguarded far call reports "Detected".
- **Suggested Fix**: Document the precondition on `DetectionInputs::distance`. Add a floor only if the source confirms detection is range-bounded.

### CHAR-2026-10-26-L-14: `KeywordTable::from_index` reads race (and FO4 keywords) off the placed shell, outside the #6228 pin
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: skyrim, fo4
- **Location**: `crates/scripting/src/keywords.rs:40-49`
- **Status**: NEW (same class as closed #6228; related to open #4137)
- **Description**: `index.races.get(&npc.race_form_id)` takes the raw `NpcRecord`'s race, not the Use-Traits terminal's. `HasKeyword` on a templated actor reads the shell's race keywords. The reader landed 2026-10-13 (`5bf9ac87e`), before #6228, and was missed by that sweep. The pin lists only dialogue voice, `actor_base_traits` and `anim_subgraph`.
- **Impact**: Latent, and only where a templated NPC's own RNAM disagrees with its terminal. Not measured on a real master.
- **Suggested Fix**: Resolve `traits_terminal(npc, index)` per NPC and read `race_form_id` from it. Add `scripting/keywords.rs` to the pin's file list.

### CHAR-2026-10-26-L-15: The player body takes its gender from the shell's ACBS but its race from the Use-Traits terminal
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: fo3, fnv, oblivion
- **Location**: `byroredux/src/player_body.rs:406` (`Gender::from_acbs_flags(npc.acbs_flags)`) beside `:163-168`
- **Status**: NEW
- **Description**: The race is resolved through `resolve_inherited_traits`, but `attach_player_locomotion_animation` receives the unresolved shell and picks the walk clip from `npc.acbs_flags`. The Female flag is a Use-Traits field. The NPC path got this fix in #6080.
- **Impact**: None for vanilla, since the Player `NPC_` is not templated. A wrong-gender walk clip only for a mod that templates the Player record.
- **Suggested Fix**: Pass the already-computed terminal into `attach_player_locomotion_animation`.

## Known-Open Register

Confirmed still true; none re-filed:

- **FNV/FO3 tag-skill per-level formula** undocumented. The masters author `fAVDTagSkillBonus` 15 and no per-level data. CLAS SPECIAL lives in `ATTR`.
- **FO3/FNV AP, Crit, Melee and Unarmed NPC scope** unsourced. They ship `.player_only()` (#2937, closed as documented). See L-10 for a new lead. Melee Damage stays CONFLICTED and actor-general.
- **Oblivion `RulesetBuilder::None`** is deliberate (#3768). Pin `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate` is green. FO76 and Starfield have captures only.
- **`affliction_tick_system`** has no scheduler registration.
- **Open issues**: #4137 (six `template_flags` bits with no consumer), #4232 (`effective_actor_level` returns 0 verbatim), #6283 (FO4 barter capture false sentence), #6285 (Skyrim row comments stale), #6350 (VATS umbrella), #6374, #6376 and #6403. #4415 is closed.

**Entries now STALE** (the `/audit-character` skill text should be corrected; this audit did not edit it):

1. "`PoolRegenConfig` has zero production insertion sites; `pool_regen_tick_system` early-returns on every game." This is false for FO3/FNV since `3dc16208e`: the ActionPoints lane is inserted at `cell_loader/references/mod.rs:362-365`. It is still true for the TES lanes and for Skyrim, FO4 and Starfield.
2. "VATS runtime (AP pool/regen, time-pause, limb health, hit roll) absent; only AP formulas exist." The model, mode machine, targeting scan, queue/confirm, `vats.*` console, world-time freeze and AP regen lane exist (#6350 phases 1-3). Still absent: limb health and per-limb damage routing, playback and camera, Concentrated Fire plumbing, AP gating (M-01), and the ground-truth percentages (#6403).
3. The Scope list omits `crates/core/src/character/vats.rs`, `byroredux/src/systems/vats.rs`, `commands/vats.rs`, `components/vats.rs` and `build_vats_model`. Dim 3 omits `VatsModel` and the `radiation_gauge` row.
4. `install_crime_model` is called from `asset_provider/script.rs:1069`, not from `crime.rs`.
5. The "16 modules" count is still correct (`vats` was added alongside `crime`).

## Cross-audit routing

- Component shape → `/audit-ecs`.
- AVIF/CLAS/NPC_ parsing → `/audit-esm` Dim 4.
- CTDA evaluation → `/audit-scripting`.
- Scheduler access (L-08) → `/audit-concurrency` Dim 4.
- Gameplay writers → `/audit-gameplay`. The VATS spend path (M-01) and the missing radiation writers (L-07) also belong there.
