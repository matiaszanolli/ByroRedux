**HEAD**: fa6be4a6b · **Baseline**: [`AUDIT_CHARACTER_2026-10-26.md`](AUDIT_CHARACTER_2026-10-26.md) (HEAD `bf304851f`) · **Audited**: Dim 1 (Ruleset Seam), Dim 3 (Progression, Pools, Crime, VATS), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2 (Derived Formulas). `derived/fallout/tes/skyrim/resistance.rs`, `crates/core/src/combat.rs` and `stealth.rs` have no commits since `bf304851f`. The guard tests were re-run.

# Character / CHARAL Audit, 2026-10-27

This is `/audit-character` at default scope, run inside `/audit-suite --preset comprehensive`. Per the suite rules, every dimension was analysed synchronously without sub-agents. Nothing in the repo was edited and the engine was not launched. All builds went into the repo `target/`.

Read-only probes run this time:
- `crates/plugin/examples/probe_gmst_vats` on FalloutNV.esm.
- A GMST byte-scan of Fallout4.esm and Skyrim.esm.
- An executable setting-table read of Fallout4.exe and SkyrimSE.exe (the *exe_ini_default_tables* method). Each setting's default sits just before a pointer to its name string.

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect character` | 164 passed, 0 failed, 1 ignored. The ignored test is `vats::tests::to_hit_chance_matches_recorded_in_game_percentages`, tracked by #6403. |
| `cargo test -p byroredux --bin byroredux -- resolve_inherited_call_sites_are_enumerated_and_pinned vats build_vats_model install_tests install_catalog npc_spawn:: player_derived commands::actor_value regen` | 197 passed, 14 ignored (real-master). |
| `cargo test -p byroredux-plugin --lib -- actor_value_derive equip:: effective_actor_level pc_level_mult` | 98 passed, 5 ignored (real-master). |

## Delta since the baseline

The baseline's findings were published as #6422–#6436. These commits landed since then:

| Commit | Change |
|---|---|
| `20b5914bc` (#6423) | The VATS model becomes a `CharacterRulesProfile::vats()` row. |
| `9f9230e38` (#6424) | `FALLOUT_NV` gets its own six `fVATS*` values. |
| `a7826239e` (#6425) | The VATS, AP-regen and radiation-gauge capture is added. |
| `c4a1ec119` (#6426) | Pins for the crime row, the rad-gauge row, `install_crime_model` and `plugin_files`. |
| `1fb7e6f56` (Refs #6422) | The AP gate, and same-target shots folded into one `HitEvent`. |
| `68f200ec4` / `870722b99` (#6374 / #6376) | Doc sweeps. |
| `3111c7435` (#6283) | FO4 barter capture. |
| `27f022dfd` (#6285) | Skyrim pool prose. |
| `eae8fd740` (Refs #4137) | `equip.rs` `template_flags` doc. |
| `67722b6e4` (#6362) | `PlayerEntity::get`, a mechanical rewrite of 66 sites. |

## Executive Summary

**3 findings: 0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW.** All three are NEW.

- **The MEDIUM is a defect introduced by the #6422 fix.** Folding same-target VATS shots into one `HitEvent` makes the FNV Damage Threshold apply once to the summed damage instead of once per shot.
- **The baseline's resolved findings all hold.** That covers #6423, #6424, #6425, #6426, #6374, #6376, #6283 and #6285, plus the AP-gate half of #6422 (Regression Checks below).

**Constants verified this run:**
- All 29 `fVATS*`/`iVATS*` settings on the GECK *VATS Settings* page match `VatsModel::FALLOUT3`.
- FalloutNV.esm was re-probed. Every modelled `fVATS*`/`fActionPoints*` value it authors matches `VatsModel::FALLOUT_NV`, including the six #6424 re-tunes.
- FO4 and Skyrim `BarterModel` executable defaults match the executables, but no capture states them (L-02).
- The Dim 2 derived rows are unchanged and were PASS on 2026-10-26.

## Regression Checks

| Item | Result |
|---|---|
| #6423 VATS profile row | **PASS.** `build_vats_model` is `index.character_rules.vats()?.with_gmst(gmst)`. There is no `GameKind` test and no whole-profile comparison. Pin `vats_row_is_stated_on_every_profile` covers FO3 at 4 AP/s, FNV at 3.6 AP/s, and `None` on every other profile. The npc_spawn tests cover the FO3 leg. |
| #6424 FNV `fVATS*` fallbacks | **PASS.** Pin `fnv_fallbacks_carry_the_six_values_falloutnv_esm_authors`. The probe agrees value-for-value. `fVATSFortuneOdds` 0.1 is still unmodelled, and the capture says so. |
| #6425 capture | **PASS.** `charal-fnv-fo3-ruleset.md` § *VATS tuning, Action Points regen and the radiation gauge* lists the standards, the FNV re-tunes and the wiki-only defaults. It marks the restore-rate unit "unverified" and the to-hit order "reconstructed" (#6403). It also covers the rad-gauge source. `charal.md` §8 item 9 points to it. The capture lists only a subset of the wiki-only settings, but its *Sources* line cites the GECK page, and all 29 values match it. |
| #6426 pins | **PASS.** `crime_and_radiation_gauge_rows_are_stated_on_every_profile`; `crime.rs` `install_tests` (overlay and clear-on-`None`); `install_catalog_installs_the_rad_gauge_only_where_the_profile_says`; `load_order_parallel_tests` asserts `plugin_files`. |
| #6422 AP gate (partial fix) | **PASS for the gate.** `vats_queue_part` refuses once `ap_reserved + ap_cost > current(AP)`. `vats_confirm` re-checks before charging. A weaponless scan returns early. Damage is priced per target. Concentrated Fire is still unplumbed, as disclosed, and #6422 stays open for it. **The fold is new and wrong, see M-01.** |
| #6374 / #6376 doc sweeps | **PASS.** Stale-prose greps (*zero production insertion*, *Oblivion-shaped*, *inserted only inside unit tests*, *theft fine so far*) find nothing outside `docs/audits/`. The `regen.rs` header, the `mod.rs` docstring, the `crime.rs` header, `boot/world.rs`, the save-registry reason string, feature-matrix rows 281/324/364 and ROADMAP 289/647 all say "AP only". |
| #6283 FO4 barter capture | **PASS.** Fallout4.esm authors `fBarterBuyMax` 1.2, `fBarterSellMax` 0.8 and `fBarterMax`/`fBarterMin` 3.5/2.0 (byte-scan). The executable's 1.05/0.95 are fallbacks (exe table). |
| #6285 Skyrim prose | **PASS.** The SKYRIM row and the `actor_value_derive` module and fn docs name the #5914 class/level term and #5915. |
| Single ruleset sink, `effective_actor_level`, `resolve_inherited_*` pin, Use-Traits pin | **PASS.** All unchanged. Two production `resolve_inherited_*` calls, three `traits_terminal(` callers. |
| Regen / affliction | **PASS.** `PoolRegenConfig` is still inserted only through `build_vats_model` (`cell_loader/references/mod.rs:362`). `affliction_tick_system` still has no scheduler registration. |

## Constant Verification Table (delta rows only; the full table is in the 2026-10-26 report)

| Item | Code | Source | Verdict |
|---|---|---|---|
| `fVATS*`/`iVATS*` FO3 defaults (29) | `VatsModel::FALLOUT3` | GECK *VATS Settings* (`/mnt/data/src/reference/geck-uesp-wiki/(main)/VATS Settings.wiki`), *FVATSScreenPercentFactor* 1.0 | PASS ×29 |
| FNV `fVATS*` re-tunes (6) | `VatsModel::FALLOUT_NV` | FalloutNV.esm probe: crit 5, player damage 0.75, damage-to-weapon 1, spread 1.1, range spread max 100, uncertainty 0.01 | PASS |
| FNV shared `fVATS*` (DistanceFactor 2, H2H/MeleeWarp 0.27/0.32, ParalyzePalm 0.3, PlaybackDelay 0.17, LongBurst 1.75, Stranger 164/0.1, ScreenPercent 1) | `..Self::FALLOUT3` | FalloutNV.esm probe | PASS |
| `fActionPoints*` (13) and restore rate | `StandardActionPoints` | FalloutNV.esm probe (0.06; attack standards 20/25/15/30/22/35/10/40/20; reload/heal 10; power ×1.5; VATS melee ×1) | PASS (unit still unverified, disclosed in the capture) |
| FO4 barter `fBarterMax`/`fBarterMin` defaults | 2.5 / 1.5 (`profile.rs:456`) | Fallout4.exe setting table 2.5 / 1.5 (this run); master 3.5 / 2.0 | VALUE PASS, **capture silent** (L-02) |
| Skyrim barter `fBarterMax`/`fBarterMin` defaults | 2.4 / 1.4 (`profile.rs:422`) | SkyrimSE.exe setting table 2.4 / 1.4 (this run); master 3.3 / 2.0 | VALUE PASS, **capture silent** (L-02) |
| FO4 / Skyrim buy floor and sell ceiling | 1.05 / 0.95 | Both executables' tables 1.05 / 0.95 (FO4 under `fBarterBuyMax`, Skyrim under `fBarterBuyMin`) | PASS (captured, #6283 and the Skyrim capture) |
| VATS `NEUTRAL_SKILL` 50 | `systems/vats.rs:222` | none needed: an engine fallback where `to_hit_chance`'s skill term is neutral, not a game constant | n/a (see L-01) |

## Coverage Matrix

This is unchanged from 2026-10-26, except that VATS is now a profile row. It was re-derived from the `profile.rs` arms (`vats()`, `crime()`, `radiation_gauge()`) and workspace greps.

| Family | Ruleset | Wired | Derived stats | Leveling model | Leveling runtime | Regen | Affliction | Other CHARAL rows live |
|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ | ✓ | ✓ | `XpCurve` + GMST | ✓ kill XP, `levelup` | ~ ActionPoints lane only | ✗ unregistered | `vats()` = `FALLOUT3`, radiation gauge |
| FNV | ✓ | ✓ | ✓ | `XpCurve` + GMST + `level_cap_raises` | ✓ | ~ ActionPoints lane only | ✗ | `vats()` = `FALLOUT_NV`, radiation gauge |
| FO4 | ✓ | ✓ | ✓ | `XpCurve` uncapped | ~ no kill XP | ✗ | ✗ | barter |
| Skyrim | ✓ | ✓ | ✓ | `SkillXp` + GMST | ~ `AdvanceSkill` roll | ✗ | ✗ | `CrimeModel::SKYRIM`, barter |
| Oblivion | ✓ built | ✗ `RulesetBuilder::None` (deliberate, #3768) | n/a | `SkillUse` | ✗ | ✗ | ✗ | `CrimeModel::OBLIVION` |
| FO76 / Starfield | ✗ captures only | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – |

## Findings

### MEDIUM

### CHAR-2026-10-27-M-01: VATS same-target fold applies FNV Damage Threshold once to the summed shots, not once per shot
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools (VATS execute path; the combat funnel is `/audit-gameplay`'s)
- **Game**: fnv. FO3 has no `DamageThresholdRules`; its percentage DR is linear, so the fold is harmless there.
- **Location**:
  - `byroredux/src/systems/vats.rs:607-656`: the `vats_confirm` fold, `landed` → `hits` → one `HitEvent { damage: shot * shots }`.
  - Consumer: `byroredux/src/combat.rs:359` (`combat_damage_system` → `apply_damage_threshold(.., event.damage)`) and `:834-846` (`(damage - threshold).max(damage * rules.min_damage_mult)`).
  - Test: `landed_shots_at_one_target_sum_into_one_hit_event` (`vats.rs:966`). Its fixture installs no `DamageThresholdRules`, so it cannot see this.
- **Status**: NEW. The defect was introduced by `1fb7e6f56` (Refs #6422), which fixed #6422's "same-target shots overwrite each other". #6422 is still open, so this can be appended to it instead of filed separately.
- **Source**: DT is a per-shot defence.
  - GECK *fMinDamMultiplier*: "the proportion of damage an actor takes **from a shot** that would be negated by their Damage Threshold".
  - GECK *Ammo*, "# Proj per Shot": "the weapon's damage figure is divided amongst all of the projectiles … higher projectile counts may not be able to bypass Damage Threshold".
  - Both are in `/mnt/data/src/reference/geck-uesp-wiki/(main)/`.
- **Description**: `HitEvent` holds one component per entity. Rather than letting later shots overwrite earlier ones, `vats_confirm` now counts landed shots per target and writes a single event whose `damage` is `per_shot × shots`. The commit comment says so explicitly: "The target's own defenses then apply once to that total in the combat funnel." For FNV, `combat_damage_system` then computes `max(N·d − DT, N·d·m)` where vanilla computes `N · max(d − DT, d·m)`. DT is subtracted once instead of N times, so the per-shot `fMinDamMultiplier` floor never engages where it should.
- **Evidence**: with `fMinDamMultiplier` 0.2:
  - Three landed 10-damage shots at a DT-15 target: vanilla 3 × max(−5, 2) = **6**; folded max(30 − 15, 6) = **15** (2.5×).
  - Three 20-damage shots at DT 15: vanilla 3 × 5 = **15**; folded max(60 − 15, 12) = **45** (3×).
  - The error grows with the shot count and is largest exactly where DT matters: weak weapons against armoured targets.
- **Impact**: Multi-shot VATS against an armoured FNV target overstates damage two- to three-fold. That changes kill outcomes, which persist through `Dead`, kill XP and the crime hooks. One `HitEvent` also means one ObScript `OnHit` run instead of one per landed shot (CS *OnHit*: "run once when the scripted actor is hit"). The path is reachable today through `vats.queue` ×N + `vats.confirm` and the smoke driver. It becomes player-reachable when a VATS menu is wired. The previous behaviour under-delivered (one hit per target); this one over-delivers.
- **Related**: #6422 (open), #6350, #6403; ESM-2026-10-27-03 (the `vats.damage_mult` that feeds `weapon_mult` here is read from a mis-decoded sub-record).
- **Suggested Fix**: Carry the per-shot damage plus a `shots: u32` count on `HitEvent` (default 1). `combat_damage_system` would then apply `apply_damage_threshold` per shot and multiply. The alternative is to sequence same-target shots across frames. Add a DT-installed leg to the fold test asserting 6, not 15, for the first example above.

### LOW

### CHAR-2026-10-27-L-01: The VATS skill fallback comment says an uncarried skill reads neutral 50; the code reads 0 when the player has any `ActorValues`
- **Severity**: LOW
- **Dimension**: Progression & Pools (VATS)
- **Game**: fo3, fnv
- **Location**: `byroredux/src/systems/vats.rs:322-333`
- **Status**: NEW (a residue of #6422 item 7, fixed in `1fb7e6f56` for `skill_form == 0` only)
- **Description**: The comment reads "A skill the player does not carry keeps the neutral value too (`ActorValues` absent altogether)". The code is `world.get::<ActorValues>(player).map(|avs| avs.current(weapon.skill_form)).unwrap_or(NEUTRAL_SKILL)`. `ActorValues::current` returns `0.0` for an absent entry (`actor_values.rs:144-148`). So a player whose component lacks that skill, for example a mod weapon naming a non-roster AVIF, is priced at skill 0. `to_hit_chance` then uses its "unskilled" skill term (0.5×), not the neutral one. The parenthetical narrows the claim only by implication.
- **Impact**: A halved hit chance for a weapon whose skill AVIF the player does not carry. That cannot happen on vanilla, because the player carries the whole roster.
- **Suggested Fix**: Either read `avs.get(skill_form).map(ActorValue::current)` so a missing entry falls to `NEUTRAL_SKILL`, or reword the comment to "only when the player has no `ActorValues` at all".

### CHAR-2026-10-27-L-02: The FO4 and Skyrim `BarterModel` compiled-in curve ends (2.5/1.5, 2.4/1.4) have no capture line
- **Severity**: LOW (doc/sourcing; the values are correct)
- **Dimension**: Ruleset Seam (profile rows must be sourced) / Coverage
- **Game**: fo4, skyrim
- **Location**: `crates/core/src/character/profile.rs:420-426` (SKYRIM `barter_max: 2.4, barter_min: 1.4`) and `:452-461` (FALLOUT4 `2.5 / 1.5`). Capture: `docs/engine/charal-fo4-ruleset.md:238-247`, `docs/engine/charal-skyrim-ruleset.md:97-116`.
- **Status**: NEW (adjacent to closed #6283, which corrected only the floor/ceiling sentence)
- **Source**: this run read both executables' setting tables. Fallout4.exe has `fBarterMax` 2.5, `fBarterMin` 1.5, `fBarterBuyMax` 1.05, `fBarterSellMax` 0.95. SkyrimSE.exe has `fBarterMax` 2.4, `fBarterMin` 1.4, `fBarterBuyMin` 1.05, `fBarterSellMax` 0.95. The masters author FO4 3.5/2.0/1.2/0.8 and Skyrim 3.3/2.0 (byte-scan).
- **Description**: `BarterModel`'s doc defines its fields as "the executable defaults the masters' GMSTs override". The captures state the master values. They name the executable fallback only for the floor/ceiling pair (FO4 after #6283; the Skyrim capture's "executable default `fBarterSellMax=0.95` … `fBarterBuyMin=1.05`"). Neither capture gives the `fBarterMax`/`fBarterMin` fallbacks, and no code comment cites them: the literals arrived in `61ad95cf3` with no citation. A reader comparing the code to the capture sees 2.5 vs 3.5 and 2.4 vs 3.3, which is the same shape as the #6424 "wrong fallback" class, with no line to settle it. This run settled it, and the values are correct.
- **Impact**: None at runtime, since vanilla masters overlay all four. The audit trail is incomplete, so a future "fix" toward the master values would silently change the no-overlay fallback.
- **Suggested Fix**: Add one sentence to each capture's barter section giving the executable's `fBarterMax`/`fBarterMin` fallbacks and their read method. Add a `// Fallout4.exe / SkyrimSE.exe setting table` comment on the two literals.

## Known-Open Register

These were confirmed still true and none was re-filed:
- **FNV/FO3 tag-skill per-level formula** is undocumented. CLAS SPECIAL lives in `ATTR`.
- **FO3/FNV AP, Crit, Melee and Unarmed NPC scope** is unsourced and ships `.player_only()` (#2937). #6436 records the GECK lead.
- **Oblivion `RulesetBuilder::None`** is deliberate (#3768). FO76 and Starfield have captures only.
- **`affliction_tick_system`** is unregistered. Regen is live for FO3/FNV ActionPoints only.
- **VATS (#6350)**: no menu, no per-limb routing or limb health, no playback or camera, Concentrated Fire unplumbed (#6422 open). The to-hit order is reconstructed (#6403).
- **Open issues re-verified in code**:
  - #4137: two `template_flags` bits (Script 0x0200, Def Pack List 0x0400).
  - #4232.
  - #6403.
  - #6422 (Concentrated Fire half).
  - #6427: `TRESPASS_GOLD`/`PICKPOCKET_GOLD` consts.
  - #6428.
  - #6429.
  - #6430: TES lanes `fatigue_avif: 0`/`magicka_avif: 0` at `npc_spawn.rs:340-341`.
  - #6431, #6432, #6433.
  - #6434: `keywords.rs:40` reads `npc.race_form_id`.
  - #6435: `player_body.rs:406` `Gender::from_acbs_flags(npc.acbs_flags)`.
  - #6436.
- **Suite cross-references (not CHARAL-owned)**:
  - The `vats_scan` lock-order cycle is ECS-2026-10-27-02.
  - The VATS data decodes are ESM-2026-10-27-01/02/03; ESM-03 also feeds M-01's `weapon_mult`.
  - `visible_fraction` cost is PERF-D1-01.

## Cross-audit routing

- The M-01 funnel change (a `HitEvent` shot count) belongs to `/audit-gameplay`.
- Component shape → `/audit-ecs`.
- AVIF/CLAS/NPC_ parsing → `/audit-esm` Dim 4.
- Scheduler access → `/audit-concurrency` Dim 4.

Next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-27.md`. Use the domain label `character` plus `game:fnv` for M-01, `game:fo3`/`game:fnv` for L-01, and `game:fo4`/`game:skyrim` for L-02.
