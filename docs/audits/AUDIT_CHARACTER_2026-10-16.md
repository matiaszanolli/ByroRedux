**HEAD**: fa7591ca2 · **Baseline**: [`AUDIT_CHARACTER_2026-10-12.md`](AUDIT_CHARACTER_2026-10-12.md) (HEAD `5d31fd637`) · **Audited**: Dim 1 (Ruleset Seam), Dim 2 (Derived Formulas), Dim 3 (Progression & Pools), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: none whole; within Dim 2 the sources `derived/fallout/tes/skyrim/resistance/profile.rs`, `combat.rs`, `stealth.rs` have zero changed lines, and within Dim 3 `affliction/reputation/resistance/tes/skyrim.rs` have none.

# Character / CHARAL Audit — 2026-10-16

`/audit-character` with the default scope (`--depth deep`). Dims 2, 3 and 4 ran as sub-agents; Dims 1 and 5 were done by the orchestrator, who also re-checked the load-bearing claims of the sub-agents against the code before merging (D3-01 `with_gmst`, D3-04/D4-07 early `return`, D4-03 restore order, D4-04 player `Perks`, D4-09 `GameKind` branches, D1-02). No engine was launched and no repo file was edited. The Skyrim DNAM oracle (D4-01/D4-02) was a throwaway read-only Python parse of `Skyrim.esm` kept outside the repo (`/mnt/data/tmp/d4/`).

The delta `5d31fd637..HEAD` is 431 commits. Those that matter for CHARAL: the whole M47.2 level-up runtime (`dbe562c1f` kill experience + `CharacterLevel` saved, `38539eee8` skill points/perks + `levelup`, `722421899` `RewardPlayerXP`/FO4 level points, `57447c7e5` `Game.AdvanceSkill`, `84a17c7cf` #5874); `b3556ef6c` scripted faction ledger; `7c740eca0` + `a8dc9e8e6` Skyrim NPC/player skills; `0ea4a87d4` perk entry-point composition; new consumers `ed59c81f7` encumbrance, `61ad95cf3` barter, `dd843f9e6` bleedout, `52b59e43d` DT; `ad26650ec` #5706 (`CharacterRuleset::actor_value` composes every layer); `1fe974cec` #5708; `2ba5c8c12` #5794 (shared spawn finalize tail).

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect -- character combat stealth actor_values` | **176 passed**, 0 failed (baseline 174) |
| `cargo test -p byroredux-core --features inspect -- character::profile character::ruleset mod_docstring` | 24 passed |
| Dim 2 filter set (`character::derived/fallout/tes/skyrim/resistance`, `combat`, `stealth`) | 76 passed; `cargo test -p byroredux-scripting --lib -- actor_value_views bribe get_actor_value` 11 passed |
| Dim 3 filter set (`character::leveling/regen/affliction/reputation/components`) | 58 passed; bin filter `grant_experience levelup skill_point level_up` → 1 passed (`level_ups_bank_skill_points_and_levelup_spends_them`) |
| `cargo test -p byroredux-plugin --lib -- actor_value_derive` / `… equip effective_actor_level pc_level` | 32 passed, 2 ignored / 109 passed, 4 ignored (real-master pins not run) |
| `byroredux` bin (rustc 1.96.0): `resolve_inherited_call_sites_are_enumerated_and_pinned` + the 10-12 filter list + `grant_experience level_up scripted_faction encumbrance bleedout` | 24 passed, 0 failed; the enumerated production `resolve_inherited_*` calls are still exactly two (`cell_loader/references/mod.rs`, `player_body.rs`) |

## Executive Summary

**18 new findings: 0 CRITICAL · 0 HIGH · 8 MEDIUM · 10 LOW. 0 regressions. 2 existing issues carried (#5710, #5712).**

- **No constant drifted.** The eighth run in a row with no change to any Dim 2 source file. Every coefficient of the FO3, FNV, FO4, Skyrim and Oblivion rulesets, the NPC Health curve (all 15 infobox samples by hand), Oblivion combat math and FNV/FO3 stealth was re-checked document-first and agrees; Dim 2 also cross-read the GMSTs from the shipped masters and the Skyrim/FO4 exe default tables. Verified: FO3/FNV/FO4/Skyrim/Oblivion. FO76/Starfield have captures only.
- **All three open items from 10-12 are closed or carried:** #5706 (D4-01) and #5708 (D5-01) are fixed and pinned; #5710 (D5-02) and #5712 (D5-03) are still open and still true (the `ruleset.rs` cite is now at `:262`).
- **The risk moved from numbers to the new M47.2 runtime.** The 8 MEDIUMs sit almost entirely in code that landed since the last audit:
  - **M-01** Skyrim NPC pools omit the class/level term, and the premise for deferring it ("captured nowhere", #4454) is now false: the CLAS formula reproduces the baked `DNAM` pools of 929/929 vanilla auto-calc NPCs, the shipped code only 302/929 for Health (67 % of sampled NPCs spawn under-HP'd).
  - **M-02** the Fallout level cap and XP curve ignore the authored GMSTs (FO3 + Broken Steel stops at 20; the DLC authors 30).
  - **M-03** Skyrim levels now roll but bank no perk or pool pick.
  - **M-04** level-up skill points read *current* INT, the capture says base only.
  - **M-05** `grant_experience` can spin forever on a `0 < xp_to_next < 1` curve.
  - **M-06** `ScriptedFactionChanges` is not restored before the reload, so NPCs lose scripted faction membership on load.
  - **M-07** the player never gets a `Perks` component (same class as #4458/#5413).
  - **M-08** five new consumers branch on `GameKind` in place of `CharacterRulesProfile`.
- **The SKILL.md known-open text is stale in three places** (feeds `/audit-sync`): "the TES models have no XP-driven roll" (Skyrim rolls via `Game.AdvanceSkill`); `level_cap()` "has no consumer" (it does: `grant_experience`); `condition.rs` as the `GetActorValue` home (it is `crates/scripting/src/condition/eval.rs` since the split).
- **Overlap check:** merged findings that two dimensions found independently — D3-02≡D4-06, D3-04≡D4-07, D3-03≡D4-08, D1-02≡D4-09 — are filed once. The bribe/barter/encumbrance consumers were also read by Dim 2: they go through the single composer and respect scope.

## Constant Verification Table

Document first, then code. "Master" = value read straight out of the shipped ESM/exe.

| Item | Code | Document + line | Verdict |
|---|---|---|---|
| FO3 Health (player) | `bilinear(END,20,L,10,0,90)`, `player_only` | `charal-fnv-fo3-ruleset.md:93`; Fallout3.esm `fAVDHealthEnduranceMult` 20 / `LevelMult` 10 | PASS |
| FNV Health (player) | `bilinear(END,20,L,5,0,95)` ≡ `100 + 20·END + 5(L−1)` | same row; FalloutNV.esm 20 / 5 / 0 | PASS |
| FO3/FNV Health (NPC) | `NpcHealthCurve{5,5, offset −1, level_anchor −1}` | `:94` + "Health (NPCs, #5238)"; both masters 5/5, offset not authored | PASS; 15/15 samples recomputed (FNV Pete 65, Sunny 90, Ringo 120, Boone 230, Benny 205, Veronica 235, Cass 210, Raul 280; FO3 Three Dog 25, Gob 35, Nova 40, Moira 80, Lucas 60, Charon 240, Clover 240) |
| `level_anchor` not overlaid | `with_gmst` copies it | `:94`; no level-offset GMST in either master | PASS |
| FO3 / FNV AP | `affine(AGI,2,65).cap(85)` / `affine(AGI,3,65).cap(95)`, `player_only` | `:95`; masters 65/2 and 65/3 | PASS (scope known-open #2937) |
| FO3/FNV Carry Weight, Melee Dmg, Crit, Unarmed, RadRes, PoisonRes | `150+10·STR`; `0.5·STR`; `1·Luck cap 10`; `ceil(0.05·U+0.5)`; `(END−1)·2 cap 85`; `(END−1)·5` | `:96-101`, `:379-406`; masters match | PASS |
| FO4 Health / AP / Carry Weight | `floor(77.5+4.5·END+2.5·L+0.5·L·END)` player-only; `60+10·AGI` player-only; `200+10·STR` | `charal-fo4-ruleset.md:72,87-89,140,245` (LOCKED) | PASS |
| Skyrim Light Armor mult / Carry Weight | `0.004·skill+1` player-only; `250+0.5·Stamina` | `charal-skyrim-ruleset.md:208,655,664` | PASS |
| Oblivion Health/Magicka/Fatigue/Armor Rating | `2·END`, `2·INT`, 4 affine rows, `0.0065·s+0.35` | `charal-oblivion-ruleset.md:326,379-385,458-470,517-519` | PASS (builder unwired, deliberate) |
| Oblivion `modified_skill`, weapon mult, Hand-to-Hand | `skill+0.4(Luck−50)`; `0.5(0.75+0.005A)(0.2+0.015S)`; `1+10.5(STR/100)(S/100)`, fatigue `1+0.5H` | "Complete Damage Formula" `:209-321` | PASS |
| FNV/FO3 `stealth.rs` | every coefficient of `detection_score` + `classify` | `charal-fnv-fo3-ruleset.md:249-335` | PASS |
| FO3/FNV XP to next | `xp_a 150, xp_b 50` ≡ GECK `iXPBase 200`/`iXPBumpBase 150` | `:453`; GECK *Experience Settings* | PASS as defaults; GMST not overlaid → **M-02** |
| Level caps | FO3 20, FNV 30, FO4/TES 0 (uncapped) | `:459-460`, `charal-fo4-ruleset.md:494`; FalloutNV.esm `iMaxCharacterLevel` 30 | PASS base; FO3+Broken Steel authored 30 → **M-02** |
| Cap semantics | `cap>0 && level>=cap` before rolling | SKILL Dim 3 | PASS by reading (a cap-level player never reaches cap+1); unpinned → **L-04** |
| Skill points / perk cadence / carry | FO3 `10+INT`, perk every level; FNV `10+INT/2`, perk every 2nd level; FO4 `(0, one point)`; half-point carry; hook 10 at the consumer | `:70-79`, `:464-469`; `charal-fo4-ruleset.md:488-492` | PASS values; INT source → **M-04**; cadence uncited in capture → **L-08** |
| FO4 level XP | `75L+125`; XP modifier `fXPModBase+fXPModMult·INT` | `charal-fo4-ruleset.md:44,481-484,497` | PASS |
| Skyrim XP | `25L+75`, `with_gmst` requests exactly `fXPLevelUpBase/Mult` | `charal-skyrim-ruleset.md:715-722` | PASS |
| Skyrim skill-use curve / pool base / pool pick | `1.95`; `100`; `+10` | `:585-586,731-737` | PASS values; pool pick/perk never banked → **M-03** |
| Skyrim NPC pools | `race + signed offset` | UESP *CLAS* "Weight Calculations / Attributes"; Skyrim.esm `DNAM` oracle 929/929 | **FAIL → M-01** |
| Skyrim skills | `15 + race bonus + 8·(L−1)·weight share`, cap 100 | UESP *CLAS* | PASS values; leftover tie-break **FAIL → L-01**; no capture section → **L-08** |
| Skyrim race skill bonuses, `CLAS DATA` weights | index−6 slot; bytes 6..24 | UESP + NordRace | PASS |
| Bribe formula + GMSTs | `fBribeSpeechcraftMult 0.1`, `fBribeScale 0.5`, `fBribeCostCurve 1.4`, exe 1.0/0.1 | `charal-skyrim-ruleset.md:78-81`; Skyrim.esm + exe | PASS (Morality input absent → **L-06**) |
| Barter | buy floor 1.05, sell 0.95, exe 2.4/1.4 (Skyrim) | `charal-skyrim-ruleset.md:95-107` says sell cap **1.00** | code = exe value; capture contradicts → **L-07** |
| DT | `max(d−DT, d·fMinDamMultiplier 0.2)` | `charal-fnv-fo3-ruleset.md:436-446` | PASS |
| Karma / Affinity / rep bump / `STANDING_GRID` / 13 `FactionRepThresholds` + REPU FormIDs | all 11 karma boundaries, `[infamy][fame]` 4×4, 13 rows row-for-row | `charal-fnv-fo3-ruleset.md`; `charal.md:520-531` | PASS (none transposed) |
| Fatigue / Magicka regen | `10`; `(WIL·0.02+0.75)·(Max/100)` | `charal-oblivion-ruleset.md:391,529` | PASS |
| Resistance clamp | `[0,cap]`, never heals | fnv-fo3 percentage model | PASS |
| Kill-XP defaults 0/10/20/30/50, 40 % share, bleedout 10/30 s + 0.3, `fMoveEncumEffect` 0.4 | `combat.rs`, `bleedout.rs`, `encumbrance.rs` | GECK/CK/exe tables in code and `scripting.md`; **in no `charal-*` capture** | values correct, capture missing → **L-08** |

No row is `UNSOURCED` in the capture-vs-code sense except the last (a documentation gap, not a guessed number) and `POOL_REGEN_DT` / `MAX_REGEN_SUBSTEPS` (documented engine tuning).

## Coverage Matrix

Re-derived from the `profile.rs` arms and workspace greps, not from prose.

| Family | Ruleset impl | Wired | Derived rows | Leveling model | Level roll live | NPC seed | Player seed | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ | 8 | ✓ `XpCurve` | ✓ kill XP, `RewardPlayerXP`, `levelup xp`; skill points + perk banked into `LevelUpPoints`; cap hardcoded 20 (M-02) | ✓ auto-calc from class; non-auto-calc from authored `DATA` (#5338); NPC Health curve | ✓ HP/AP refreshed per frame; `setav` → `set_override` | ✗ registered, inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ | 8 | ✓ | ✓ same; cap 30 | as FO3 | as FO3 | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ | 3 | ✓ uncapped | ~ `RewardPlayerXP` path + 1 perk point per level (no SPECIAL spend path) | ✓ stored `PRPS`+`DNAM`, per-flag `TPTA` | ✓ HP/AP refreshed per frame | ✗ | ✗ |
| Skyrim SE | ✓ `skyrim_ruleset` | ✓ (GMSTs overlaid) | 2 | ✓ `SkillXp` | ~ rolls via `Game.AdvanceSkill` only; **no reward banked** (M-03) | ~ race + offset pools (class/level term missing, M-01), 18 skills (`7c740eca0`) | ~ race + offset, skills, `RaceSpells`; **no `Perks`** (M-07) | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` (deliberate, pinned) | tests only | ✓ `SkillUse` | ✗ | ✗ | ✗ | ✗ | ✗ |
| FO76 | ✗ capture only | ✗ | ✗ | ✗ | ✗ | ✗ `NpcStatModel::None` (#4453) | ✗ | ✗ | ✗ |
| Starfield | ✗ capture only | ✗ | ✗ | ✗ | ✗ | ✗ `NpcStatModel::None` (#4453) | ✗ | ✗ | ✗ |

Wiring greps on HEAD:
- `PoolRegenConfig` still has no production insert (every hit outside `regen.rs` is a `reads_resource`, a comment, or `registry_completeness_tests.rs:468`); `pool_regen_tick_system` is registered (`boot/schedule/update.rs:674`) and early-returns; `PoolRegenAccumulator` is armed at `boot/world.rs`.
- `affliction_tick_system` has no scheduler registration (`registry_completeness_tests.rs:461`).
- `FactionReputation` has no production insert.
- **Changed since 10-12:** `level_cap()` now has a production consumer (`grant_experience`, `combat.rs:640`); `CharacterLevel` is saved (v46) and no longer on the refusal list; `LevelUpPoints` is saved (`save_io/registry.rs:301`).
- Production consumers of `LevelingModel`: `xp_to_next`, `level_cap`, `skill_point_reward`, `xp_from_skill_rank`, `GetXPForNextLevel`. No production consumer: `skill_points`, `grants_perk_at`, `pool_pick_gain`, `SKYRIM_POOL_BASE`, `oblivion_attribute_bonus`, `oblivion_health_gain_per_level`, `Perks::try_set_rank`.
- `combat.rs` / `stealth.rs` (core): still zero non-test callers; math verified anyway.

## Findings

### MEDIUM

### CHAR-2026-10-16-M-01: Skyrim NPC Health/Magicka/Stamina omit the class/level term; the "captured nowhere" premise of that gap is stale
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs` `derive_skyrim_actor_values` (doc "Documented gap (#4454)" + the three-pool loop); `crates/plugin/src/esm/records/actor/class.rs` Skyrim `DATA` arm (reads bytes 6..24 only)
- **Status**: NEW (Related: #4454, closed as a documented gap whose reason no longer holds)
- **Source**: UESP *Skyrim Mod:Mod File Format/CLAS* "Weight Calculations / Attributes" (local `/mnt/data/src/reference/uesp-wiki/Skyrim Mod/Mod File Format/CLAS.wiki`): `Attribute = racial + NPC offset + 10·(L−1)/(sum of attribute weights)·weight`; `Health = racial + offset + 5·(L−1) + 10·(L−1)/sum·healthWeight`; leftovers one at a time by weight desc, ties in *reverse* actor-value order (Stamina, Magicka, Health). Weights are `CLAS DATA` bytes 0x20/0x21/0x22. `charal-skyrim-ruleset.md` ~605-609 records only "0-10/level from class".
- **Description**: The function emits `race.starting_X + offset` and nothing else. The deferral said the class pool-growth numbers are "captured nowhere"; the commit that added skills (`7c740eca0`) reads the same UESP CLAS page, which documents this term two paragraphs below the skills formula. Only `CLAS DATA` 0x20..0x22 needs parsing.
- **Evidence**: Over 929 non-templated, auto-calc, non-level-multiplier vanilla NPCs compared against the baked `DNAM` calculated pools (UESP *NPC_*, bytes 36-41), the documented formula reproduces **929/929** (all three pools exact). The code matches Health for 302/929, Magicka 542/929, Stamina 355/929. Health shortfall median 25 HP, max 1242 HP; 627 of 929 (67 %) spawn with too little Health.
- **Impact**: Every leveled Skyrim NPC spawns under-HP and under-Stamina: time-to-kill, bleedout thresholds and `GetActorValue Health` conditions read the low value. The level-1 player is unaffected.
- **Related**: L-01 (same allocator), L-08.
- **Suggested Fix**: Parse the three attribute weights into `ClassRecord`; add the weighted `10·(L−1)` share (shared leftover allocator with `skyrim_class_skill_points`, reverse-index tie) and `5·(L−1)` for Health; update the #4454 comment; pin with the Saadia (0x13BA2, level 4) numbers.

### CHAR-2026-10-16-M-02: Fallout `LevelingModel` hardcodes the level cap and XP curve; the authored `iMaxCharacterLevel` / `iXPBumpBase` GMSTs are ignored (FO3 + Broken Steel capped at 20)
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/leveling.rs:92-107` (`with_gmst` handles only `SkillXp`; `other => other`), `:118-142` (`FO3`/`FNV` consts), consumer `byroredux/src/combat.rs:640`
- **Status**: NEW (the `XpCurve` doc records the DLC-cap deferral, "not wired yet", with no issue, and the deferral predates `grant_experience` making the cap live)
- **Source**: `charal.md:144` ("XP / level curve … AUTHORED — GMST — pending"); `charal-fnv-fo3-ruleset.md:459-460` (FO3 cap 20, "30 with *Broken Steel*"; FNV 30, "50 with the four add-ons"); GECK *Experience Settings* (local `/mnt/data/src/reference/geck-uesp-wiki/(main)/Experience Settings.wiki`): needed XP for level n = `(n−1)·((n−2)/2·iXPBumpBase + iXPBase)`, defaults 200 / 150; differencing gives `150·L + 50`, i.e. `xp_a = iXPBumpBase`, `xp_b = iXPBase − iXPBumpBase`.
- **Description**: Only Skyrim's two curve GMSTs are overlaid. Measured from the masters: `FalloutNV.esm` authors `iMaxCharacterLevel` 30, `iXPBumpBase` 150; `Fallout3.esm` authors `iXPBumpBase` 150 and no cap (exe default 20); **`BrokenSteel.esm` authors `iMaxCharacterLevel` 30**; `Fallout4.esm` authors `iXPBase` 200 (its `fXPMod*` pair is already consumed). `build_character_ruleset` already passes the merged-index `game_setting_float` closure into `build_ruleset`, so the seam is one match arm away.
- **Evidence**: `leveling.rs` `with_gmst`: `Self::SkillXp {..} => …, other => other`; `grant_experience`: `capped = leveling.level_cap() > 0 && progress.level >= leveling.level_cap()`.
- **Impact**: A FO3 load order with `BrokenSteel.esm` stops at level 20 (authored: 30). Any mod retuning `iXPBumpBase`/`iMaxCharacterLevel` is ignored. Not a crash.
- **Related**: M-05 (a retuned curve is exactly what makes the loop hazard reachable for Fallout).
- **Suggested Fix**: Extend `with_gmst`'s `XpCurve` arm (`xp_a = iXPBumpBase`, `xp_b = iXPBase.unwrap_or(200) − xp_a`, `level_cap = iMaxCharacterLevel` when authored, FO3 20 fallback); add a request-list pin like `skyrim_gmst_overlay_reads_only_authored_curve_settings` and a capture line citing the GECK page.

### CHAR-2026-10-16-M-03: Skyrim's level roll is live (via `Game.AdvanceSkill`) but banks no perk and no +10 pool pick
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: skyrim
- **Location**: `byroredux/src/combat.rs:659-663` (`let Some((points, perk)) = leveling.skill_point_reward(..) else { return; }`), `:1175-1206` (`scripted_skill_system` → `grant_experience`); `crates/core/src/character/leveling.rs:150-170`
- **Status**: NEW (SKILL.md's "the TES models have no XP-driven roll" is stale for Skyrim)
- **Source**: `charal-skyrim-ruleset.md:715-722` ("Each level also grants a 10-point Health/Magicka/Stamina pool pick and a perk"), `:585-586` (`pool_pick_gain()=10`).
- **Description**: `scripted_skill_system` feeds `xp_from_skill_rank` into `grant_experience`. With `SKYRIM`, `xp_to_next = 25·L+75` and `level_cap() == 0`, so levels roll, the saved `CharacterLevel` advances and "Level up!" fires. `skill_point_reward` is `None` for `SkillXp`, so the function returns: no `LevelUpPoints.perk_points`, and nothing applies `pool_pick_gain`. `pool_pick_gain`, `grants_perk_at` and `SKYRIM_POOL_BASE` have no production consumer. The early `return` also discards levels already collected in `reached` for that call.
- **Evidence**: grep `pool_pick_gain|grants_perk_at|SKYRIM_POOL_BASE` over production code → only `leveling.rs`/`skyrim.rs`/tests.
- **Impact**: Each Skyrim level reached through a scripted skill advance permanently loses its perk and pool pick. Because the level is saved, a later implementation cannot recover them without a retro-grant. Latent while no UI spends them.
- **Related**: M-07 (the player has no `Perks` either).
- **Suggested Fix**: Bank a Skyrim reward (perk point into `LevelUpPoints` plus a pending pool-pick counter), or make `grant_experience` decline to roll for models whose rewards are unmodelled; at minimum log that the reward was dropped.

### CHAR-2026-10-16-M-04: Level-up skill points read the player's *current* Intelligence; the capture says base INT only
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools / Population Boundary
- **Game**: fo3, fnv
- **Location**: `byroredux/src/combat.rs:1128-1143` (`player_intelligence`), used at `:655` (`grant_experience`) and `:1166` (`scripted_experience_system`)
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:78-79`: "Temporary INT (apparel/chems) does **not** change Skill Rate — only the base INT at level-up." (Skill Rate rows `:72-73`: FO3 `10 + INT`, FNV `10 + INT/2`.)
- **Description**: `player_intelligence` evaluates `GetActorValue`, which is `ActorValueView::Current` (`base + set_override + permanent + temporary − damage`). A +INT chem or apparel effect, or a damage layer, at the instant of the roll changes the banked points. `ActorValueView::Base` exists (since `44e4d948d`) but is not used. `permanent_mod` also carries equipped gear, so no existing view equals "base INT incl. Intense Training but excl. gear/chems": the fix needs a deliberate choice.
- **Evidence**: `player_intelligence` → `evaluate_function(GetActorValue, Intelligence)`; `condition/eval.rs:458` `GetActorValue => actor_value_view(.., Current)`.
- **Impact**: Wrong skill points whenever INT is temporarily modified (exploitable: take an INT chem, then kill). The FO4 XP multiplier shares the helper but its capture (`charal-fo4-ruleset.md:44,497`) does not say base-vs-current, so only the Fallout 3/NV use is claimed.
- **Suggested Fix**: Read INT through the Base view plus the non-gear permanent layer, or `undamaged() − temporary_mod`; pin with a test that applies `mod_temporary(INT, +3)` and asserts the banked points are unchanged.

### CHAR-2026-10-16-M-05: `grant_experience`'s `to_next as u32` truncation spins forever when an authored curve gives `0 < xp_to_next < 1`
- **Severity**: MEDIUM (hardening; impact is a hang, trigger is an odd mod GMST)
- **Dimension**: Progression & Pools
- **Game**: skyrim (overlaid curve); latent for Fallout once M-02 lands
- **Location**: `byroredux/src/combat.rs:637-651`
- **Status**: NEW
- **Source**: no constant claimed; reachable through `LevelingModel::with_gmst`, which overlays mod-authored `fXPLevelUpBase`/`fXPLevelUpMult` (`charal-skyrim-ruleset.md:715-722`).
- **Description**: The loop breaks on `to_next <= 0.0`, then does `progress.xp -= to_next as u32`. For `0.0 < to_next < 1.0` the cast is 0, `xp` never decreases, `level` saturating-adds and `reached` grows without bound; `capped` is false when `level_cap() == 0`, so the loop never ends. A fractional `to_next` (e.g. 100.5) also drops 0.5 per level.
- **Impact**: A main-thread hang in the Late stage on the first scripted skill advance with a GMST pair such as base 0.5 / mult 0. Vanilla Skyrim.esm (75 / 25) is unaffected.
- **Suggested Fix**: Guard `to_next < 1.0` as well as `<= 0.0` (or use `to_next.ceil() as u32`), and bound the loop (e.g. at `u16::MAX` iterations).

### CHAR-2026-10-16-M-06: `ScriptedFactionChanges` is not in `PRE_RELOAD_RESOURCES`, so NPCs resident at a save-load lose their scripted faction membership
- **Severity**: MEDIUM
- **Dimension**: Population Boundary (writers vs stamps / restore order)
- **Game**: skyrim, fo4 (Papyrus-driven)
- **Location**: `byroredux/src/save_io/live_load.rs:32` (`PRE_RELOAD_RESOURCES`); consumer `byroredux/src/cell_loader/references/synth_child.rs:97` (`stamp_quest_reference` → `reapply_scripted_faction_changes`); `live_load.rs:659` (only the player is replayed after the reload)
- **Status**: NEW (introduced by `b3556ef6c`)
- **Source**: not numeric. The rule is `PRE_RELOAD_RESOURCES`'s own doc ("a resource belongs here only if some spawn-time code path consults it while the reload is actively building the new cell", #3789/#4136). `FactionRanks` is re-derived and not saved (`registry_completeness_tests.rs:600`, `REDERIVED_NOT_SAVED`), so the ledger is the only carrier.
- **Description**: The reload re-spawns the cell's NPCs, and each ends in `stamp_quest_reference`, which replays the *live* ledger. The saved ledger is installed only afterwards by the wholesale `restore_resources`; nothing re-applies it to NPCs already spawned.
- **Evidence**: `grep reapply_scripted_faction_changes` has exactly two production callers (`synth_child.rs:97`, `live_load.rs:659` for the player). `PRE_RELOAD_RESOURCES = ["ReferenceEnableState","ReferenceLockState","ReferenceScriptState"]` (re-read at HEAD).
- **Impact**: On "start engine, quickload" the live ledger is `Default`, so every NPC in the loaded cell returns with its record factions: a recruited follower's `PlayerFaction`, crime-faction changes and quest flags read by `GetInFaction`/`GetFactionRank` are silently reverted. In a same-session reload the post-save ledger replays instead of the saved one. Actors in other cells recover as they stream in.
- **Related**: #3789, #4136 (same class), #5058.
- **Suggested Fix**: Add `"ScriptedFactionChanges"` to `PRE_RELOAD_RESOURCES` (it has no teardown-side reader) and a live-reload test that places a ledgered NPC and asserts its rank after the reload.

### CHAR-2026-10-16-M-07: The player never receives a `Perks` component from the Player `NPC_`; Skyrim's Player record authors 12 perks
- **Severity**: MEDIUM
- **Dimension**: Population Boundary (writers vs stamps)
- **Game**: skyrim (FO4 by lineage; FO3/FNV/Oblivion author none)
- **Location**: `byroredux/src/inventory.rs` `PlayerCharacterTemplate` / `build_player_character_template` / `attach_to_player` (no `perks` field, no insert); NPC counterpart `byroredux/src/npc_spawn.rs:317` `stamp_character_components`
- **Status**: NEW (Related: #3158, closed — fixed the NPC half; `crates/plugin/src/esm/reader.rs` `uses_npc_perk_entries` still says "the remaining gap there is the player")
- **Source**: not numeric. `Skyrim.esm` Player `NPC_` 0x7 carries `PRKZ=12` and 12 `PRKR` entries (dumped): `PerkSkillBoosts` 0xCF788, `AlchemySkillBoosts` 0xA725C, `AllowShoutingPerk` 0xF11A9, `TorchBashPerk` 0xFE304, `PlayerUnderforgeAccessPerk` 0xAEC05, `VampireFeed` 0xCF02C, `PlayerWerewolfFeed` 0x2BA1D, `DBWellFitted`, `TGWellFitted` and others, all rank 1.
- **Description**: `PlayerCharacterTemplate` carries values, vitals, level, background, factions, spells and racial spells but no perks. Every production writer of the player's `Perks` is on demand (`levelup perk`, script `AddPerk`/`ChangePerk`). Readers (`HasPerk` CTDA, `perk_entry_points::compose_entry_point` — strike damage, Max Carry Weight, XP, DT, skill points — and consumable conditions) see an absent component. All unit tests hand-insert `Perks` on the player (`inventory.rs` tests ~2057, 2197, 2461, 2605), which hides the gap (#4458/#5413 class).
- **Evidence**: `grep '\.perks\b'` in `byroredux/src` finds only `npc_spawn.rs:317`; `attach_to_player` stamps no perks (re-checked: the only `perks` hits in `inventory.rs` are test inserts).
- **Impact**: The Skyrim player's innate perks do nothing: `HasPerk AllowShoutingPerk`-style CTDAs read false and the 0ea4a87d4 entry-point composition cannot see them.
- **Related**: M-03, L-03.
- **Suggested Fix**: Add `perks: Perks` to `PlayerCharacterTemplate` from `resolved.shell.perks` (same source and "ride no template flag" rule as `stamp_character_components`), insert in `attach_to_player`, and add an attach test that does not hand-insert `Perks`.

### CHAR-2026-10-16-M-08: Five new character-rule consumers select their per-game rules by `GameKind` instead of `CharacterRulesProfile` / parser data
- **Severity**: MEDIUM (doctrine; no wrong value today)
- **Dimension**: Ruleset Seam & Doctrine
- **Game**: all
- **Location**: `byroredux/src/encumbrance.rs:84-91` (`install_encumbrance_rules`: `Fallout3NV → SpeedMult`, `Skyrim → NoSprint`, `_ → None`); `byroredux/src/barter.rs:92-111` (`BarterRules::from_index`: skill name, scale, cap, fallback GMST defaults, perk hooks, and a second `match` for `(barter_max, barter_min)`); `crates/scripting/src/bleedout.rs:49` (`ActorProtection::from_acbs_flags`: `!matches!(game, Oblivion | Fallout3NV)` decides whether the Protected ACBS bit exists) and `:143` (`engine_defaults`); `byroredux/src/combat.rs:506` (`install_experience_rules`: `index.game != Fallout3NV`); `crates/scripting/src/perk_entry_points.rs:151,209,239-251,284-342` (per-game entry-point tables)
- **Status**: NEW (all landed in the M47.2 wave: `ed59c81f7`, `61ad95cf3`, `dd843f9e6`, `dbe562c1f`, `0ea4a87d4`)
- **Source**: SKILL Dim 1 doctrine ("data in the tables, never a branch in a consumer"); precedents #4447 (`body_condition_base`) and #4679 (`vital_pools`), both of which moved a consumer-side `GameKind` list into the profile; `charal.md` format-translation rule.
- **Description**: Each is a per-game rule table written as a consumer `match`. The ACBS Essential/Protected decode is the clearest case: a record-format difference that belongs at the parser boundary (`NpcRecord.essential/protected`), not in a scripting-crate constructor taking `GameKind`. The DT and XP-modifier installs gate on authored data (AVIF/GMST presence), which is the preferred shape. `GameKind::Fallout3NV` also conflates FO3 and FNV, where only the profile row discriminates (kill-XP and encumbrance are documented for both, so no behavioural error today). FO4 encumbrance silently has `_ => None` with no capture note saying that is intended.
- **Evidence**: `grep -n GameKind` at the cited lines; none appears in any doctrine pin (`fallout_profiles_keep_roster_health_and_ruleset_in_lockstep` does not see them). Dim 1 re-checked `bleedout.rs:49` and `combat.rs:506` directly.
- **Impact**: A new game or an FO3/FNV divergence needs edits in five scattered places, and none is covered by the profile pins.
- **Related**: #4447, #4679, #4448.
- **Suggested Fix**: Decode Essential/Protected in `parse_npc` per game; move the encumbrance effect, kill-XP applicability, barter row and bleedout defaults onto `CharacterRulesProfile` rows; or at minimum add a test that fails when a new `GameKind` is added without a decision in each table.

### LOW

### CHAR-2026-10-16-L-01: `skyrim_class_skill_points` assigns leftover points lower-index-first; the page's own worked example and the baked data say higher-index-first
- **Severity**: LOW (±1 on a few skills, but 273 of 929 sampled NPCs)
- **Dimension**: Population Boundary / Derived Formulas
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs` `skyrim_class_skill_points` (`order.sort_by_key(|&i| (Reverse(weights[i]), i))`) and its test comment "then lower index first"
- **Status**: NEW
- **Source**: UESP *CLAS* Skills prose says "lower indices are ordered first", but the same page's worked example (TrainerRestorationMaster Priest) lists the weight-2 skills Enchanting (23), Illusion (21), Alchemy (16), Smithing (10) — descending — and its Attributes paragraph states "reverse actor value index order". The page is internally inconsistent; the master settles it.
- **Description**: The implementation took the prose. `skyrim_class_points_follow_the_documented_assignment_order` uses level 2 / weights (4,3,2,2,2,2), where leftovers always complete a round, so it cannot distinguish the two orders.
- **Evidence**: Oracle over 929 auto-calc NPCs: ascending tie-break reproduces all 18 baked skills for 643, descending for 916. Where order matters (280 NPCs) descending is right for 273, ascending for 0, neither for 7 (all reach skill 100, the documented unmodelled saturation regime).
- **Impact**: ±1 on a few skills of ~29 % of auto-calc Skyrim NPCs (Speechcraft feeds the bribe; Lockpicking/Pickpocket feed their checks).
- **Related**: M-01 (same allocator), L-08.
- **Suggested Fix**: Sort ties by `Reverse(i)`; add a case whose leftover cuts a tied round mid-way (e.g. weights (3,3,3,…), 2 leftover points) and cite the Skyrim.esm DNAM check.

### CHAR-2026-10-16-L-02: Script `SetActorValue`/`SetAV`/`SetValue` writes the base of the player's derived pools; the per-frame refresh reverts it (third writer of the #5239 class)
- **Severity**: LOW (reach: FO4 Papyrus; Skyrim has no `PlayerOnly`+`Absolute` row)
- **Dimension**: Population Boundary (writers vs derived stamps)
- **Game**: fo4
- **Location**: `crates/scripting/src/fragment/effects/evaluate.rs` `set_actor_value_base` (called at `fragment/effects.rs:492` and `:513`)
- **Status**: NEW (Related: #5239, #5412, which routed only `setav` and the SDK `SetBase`)
- **Source**: FO4 CK `SetValue` "modifiers are left intact", quoted in `charal.md` §6 / `ruleset.rs` (the #5412 reroute rationale).
- **Description**: `set_actor_value_base` calls `ActorValues::set_base` unconditionally. For the FO4 player's Health/AP, `refresh_player_only_bases` overwrites `base` with the formula on the next `player_derived_stats_system` tick. The console (`commands/actor_value.rs:88`) and SDK (`extensions/commands.rs:467`) both ask `CharacterRuleset::is_player_derived_pool` first; this writer does not. `#5862` added `queue_health_death` after the write, so a lethal write still kills, but a non-lethal one is dropped within a frame.
- **Evidence**: `grep is_player_derived_pool` shows exactly two call sites; `set_actor_value_base` has no ruleset read.
- **Impact**: A Papyrus `SetValue(Health, x)` / `SetActorValue("ActionPoints", x)` on the FO4 player reports no error and is undone in one tick.
- **Suggested Fix**: Route the player-derived-pool case into `set_override` here too (read the ruleset before taking the `ActorValues` write guard, #3441 order); better, hoist the "route SetBase" decision into one helper that the three writers share.

### CHAR-2026-10-16-L-03: `levelup` spends points in the wrong layer and skips the rank gate
- **Severity**: LOW (console stopgap until the level-up menu exists)
- **Dimension**: Progression & Pools / Population Boundary (layer contract)
- **Game**: fo3, fnv
- **Location**: `byroredux/src/commands/gameplay.rs` `LevelUpCommand` (`values.mod_permanent(avif, spend)`, `:404-415`; `owned.set_rank(perk, rank)`, `:437-460`); test `:598-600` pins rank 2
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:60-72` (player skill is "base + tag + Skill Rate points", so allocated points are a base component); `actor_values.rs` (`permanent_mod` is perks/enchantments/gear and the magic runtime owns it as the constant-spell sum); `Perks::try_set_rank` doc ("a level-up path granting a rank the PERK record never defined … is a bug at the call site").
- **Description**: Allocated skill points land in `permanent_mod`, which `ActorValueView::Base` peels off (`condition/eval.rs:411`), so `GetBaseValue` under-reports spent points (Skyrim's own `advance_skill` correctly uses `set_base`). `levelup perk` computes `rank = owned.rank(perk) + 1` and calls raw `set_rank`: no `PerkRecord::num_ranks` bound, no check the id is a PERK, and `try_set_rank` has zero production callers. The unit test grants Educated rank 2 from a `PerkRecord { ..Default::default() }` and asserts success, enshrining the bypass.
- **Impact**: `GetBaseValue`/`GetBaseActorValue` reads are low after spending; repeated `levelup perk` can push a perk past its rank count, and `levelup perk DEADBEEF` creates a ghost entry that feeds `compose_entry_point` and is saved.
- **Suggested Fix**: Write the spend with `set_base(base + spend)`; resolve `index.perks[perk].num_ranks` and use `try_set_rank`; fix the test to use a record with explicit `num_ranks`.

### CHAR-2026-10-16-L-04: The level-cap off-by-one duty has no consumer-level pin, and `skill_points` / `grants_perk_at` duplicate `skill_point_reward`
- **Severity**: LOW
- **Dimension**: Progression & Pools
- **Game**: fo3, fnv, fo4
- **Location**: `byroredux/src/combat.rs:619-652` (the only bin tests are at level 1 and L1→L3); `crates/core/src/character/leveling.rs:150-215`
- **Status**: NEW
- **Description**: (a) The behaviour is correct by reading (`capped = cap > 0 && level >= cap` breaks before rolling) but nothing pins it; `level_caps_per_game` asserts only the constants. (b) `skill_point_reward` (the only production caller, from `grant_experience`) re-derives `base + int_mult·INT` and `level % perk_cadence`, duplicating `skill_points(u8)` and `grants_perk_at(u16)`, which now have no production callers and disagree for FO4 (`skill_points(FO4) = None`, `skill_point_reward(FO4) = Some((0.0, true))`). Per CLAUDE.md ("improve existing code rather than duplicating logic") one should win.
- **Suggested Fix**: Add a `grant_experience` test (FNV level 30 + 10 000 XP stays 30; level 29 + enough XP ends at exactly 30). Implement `skill_points`/`grants_perk_at` in terms of `skill_point_reward`, or delete them and port their tests.

### CHAR-2026-10-16-L-05: `SKYRIM_SKILLS` is a second hand-typed 18-name roster with no non-ignored pin to `SkillSet::SKYRIM`
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:343-362` (`SKYRIM_SKILLS`) vs `crates/core/src/character/skill.rs:128-160` (`SkillSet::SKYRIM`)
- **Status**: NEW
- **Description**: Since `7c740eca0`/`a8dc9e8e6` the Skyrim skill derivation keys off a plugin-side array rather than the CHARAL roster. Both hold the same 18 EditorIDs (including the `Mysticism` rename that bit #3169) in different orders (AV-index order vs school order). An unresolved name is dropped silently (`index.actor_value_form_id(name)?`), so a roster edit would stop deriving that skill with no diagnostic.
- **Evidence**: `grep SKYRIM_SKILLS` → only `actor_value_derive.rs`; the only guard is the `#[ignore]`d `vanilla_skyrim_skill_names_all_resolve_and_a_vanilla_npc_derives_them`; `ROSTER_CASES` (`crates/plugin/tests/parse_real_esm.rs:567`) pins the core roster only.
- **Impact**: None today (the two lists are set-equal); latent silent skill loss.
- **Suggested Fix**: Add a plain unit test asserting `SKYRIM_SKILLS` sorted == `SkillSet::SKYRIM` editor ids sorted (the plugin crate already depends on `byroredux-core`), or build the plugin array from the core roster plus an explicit index map.

### CHAR-2026-10-16-L-06: No actor-value writer exists for NPC Morality, so `GetBribeAmount` prices every NPC as Morality 0
- **Severity**: LOW
- **Dimension**: Derived Formulas / Population
- **Game**: skyrim
- **Location**: `crates/scripting/src/bribe.rs` (`bribe_amount`/`bribe_inputs` read `rules.morality` through `actor_value_view`); `ActorAiData` (`crates/plugin/src/esm/records/actor/mod.rs:39`) carries only aggression, confidence and attack radius
- **Status**: NEW
- **Source**: `charal-skyrim-ruleset.md:78-81` `NpcMorality = NpcMoralityAV × fBribeMoralityMult + 1.0`; domain 0 Any Crime / 1 Violence Against Enemies / 2 Property Crime Only / 3 No Crime (`uesp-wiki/Skyrim/Followers.wiki:42-49`). **The AIDT byte that carries it is in no charal capture**, so the population mapping is unsourced and must be captured before fixing.
- **Impact**: Exact for Any-Crime NPCs, low for the rest (a "No Crime" NPC is under-quoted up to 4×). Bounded to one dialogue option.
- **Related**: #4414 (AIDT decode, closed).
- **Suggested Fix**: Capture the AIDT Responsibility byte (xEdit `wbAIDT`) in a charal-skyrim note, decode it into `ActorAiData`, stamp it as the `Morality` AV at spawn.

### CHAR-2026-10-16-L-07: The Skyrim capture states the barter sell cap as 1.00× item value; the code (and the exe) use 0.95×
- **Severity**: LOW (doc rot; the code value is exe-sourced)
- **Dimension**: Derived Formulas
- **Game**: skyrim (and the FO4 fallback pair)
- **Location**: `docs/engine/charal-skyrim-ruleset.md:107`; code `byroredux/src/barter.rs` `BarterRules::from_index`
- **Status**: NEW (introduced by `61ad95cf3`)
- **Source**: capture `:107` "sellPrice ≤ itemValue × 1.00, buyPrice ≥ itemValue × 1.05" (UESP *Speech* agrees). `SkyrimSE.exe` static table: `fBarterSellMax` 0.95, `fBarterBuyMin` 1.05; `Skyrim.esm` authors neither (only `fBarterMax` 3.3 / `fBarterMin` 2.0).
- **Impact**: None today; a future audit reads a false mismatch or "fixes" the code to 1.00.
- **Suggested Fix**: Amend `:107` to say the cap is `fBarterSellMax` (exe default 0.95), and add the exe defaults to the FO4 Barter Prices block (`charal-fo4-ruleset.md:229-240`).

### CHAR-2026-10-16-L-08: The captures lag the code: Skyrim NPC derivation, level-up rewards and kill-XP/bleedout/encumbrance constants are in no `charal-*` document
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim, fo3, fnv
- **Location**: `docs/engine/charal-skyrim-ruleset.md` (no section for `SKYRIM_BASE_SKILL` 15, `SKYRIM_SKILL_POINTS_PER_LEVEL` 8, the allocator); `docs/engine/charal-fnv-fo3-ruleset.md:459-461` (perk cadence still "mark for a citing pass") and no kill-XP section; `docs/engine/scripting.md:279-295`
- **Status**: NEW
- **Source**: SKILL: "a coefficient no capture supports is a finding". UESP *CLAS* supports the Skyrim numbers; the GECK *Experience Settings* page supports the kill-XP defaults 0/10/20/30/50, the 40 % health share and the `level <= highest` band rule (verified by Dim 3: FalloutNV.esm/Fallout3.esm author no `iXPRewardKillNPC*`); the exe tables support bleedout 10/30 s + 0.3 and `fMoveEncumEffect` 0.4. Every value is correct; the captures are missing.
- **Description**: Also `scripting.md:287` says the authored `iLevelUpSkillPointsBase` 11 / `Interval` 1 "give FO3's rule". Both masters author exactly those values, identical in FO3 and FNV, so they cannot be the FO3/FNV discriminator and nothing derives `10 + INT` from them.
- **Impact**: The next audit cannot verify those numbers document-first; M-01 and L-01 are exactly the drift a capture would have caught.
- **Suggested Fix**: Add a "Skyrim NPC derivation (CLAS)" section (skill and attribute formulas, allocator order as resolved by the master, the DNAM oracle and the saturation caveat) and a "Level-up rewards & kill experience" section to the FNV/FO3 capture; reword `scripting.md:287`.

### CHAR-2026-10-16-L-09: `docs/feature-matrix.md` and the `character/mod.rs` docstring still say there is no runtime leveling
- **Severity**: LOW (doc rot)
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `docs/feature-matrix.md:271` (row "Runtime leveling (XP grant / level-up)": `✗` for all seven games) and `:309-310` ("Leveling … uniformly `✗` … `CharacterLevel.xp` is stamped `0` at spawn and nothing increments it"); `crates/core/src/character/mod.rs:38-40` (the `components` bullet omits `LevelUpPoints`, added to the re-exports at `:95`)
- **Status**: NEW (the M47.2 runtime landed 2026-10-12/13)
- **Description**: `grant_experience` (`byroredux/src/combat.rs:619`) increments `CharacterLevel.xp`, rolls levels to the cap and banks `LevelUpPoints`, reachable from kill XP, `RewardPlayerXP`, `levelup xp` and (Skyrim) `Game.AdvanceSkill`. The matrix row should be ✓ for FO3/FNV, ~ for FO4 and Skyrim (see the Coverage Matrix above) and ✗ for Oblivion/FO76/Starfield. The regen/affliction sentences in the same paragraph remain true.
- **Impact**: The status floor that other audits and readers trust says the opposite of the code.
- **Suggested Fix**: Update the row and the paragraph; add `LevelUpPoints` to the docstring bullet.

### CHAR-2026-10-16-L-10: Stale comments naming moved files and the wrong modifier layer
- **Severity**: LOW (doc rot)
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `byroredux/src/commands/actor_value.rs:16-18` (`SetAvCommand` doc) and `:253` (test doc `setav_on_player_derived_pool_survives_the_refresh`) say the player-pool redirect "routes into the permanent-modifier layer"; `byroredux/src/combat.rs` (~887, ~901) and `.claude/commands/audit-character/SKILL.md` name `condition.rs` for the `GetActorValue` arm
- **Status**: NEW (the #5708 sweep fixed the four-layer sum prose but not these)
- **Description**: Since #5412 the redirect is `set_override` and the permanent layer is deliberately untouched, so the comments describe the exact bug #5412 fixed. The `GetActorValue` arm lives in `crates/scripting/src/condition/eval.rs` since the split.
- **Suggested Fix**: Say `set_override`; re-point the `condition.rs` references.

## Existing Issues Carried (not re-filed)

| Issue | State | Check |
|---|---|---|
| #5710 | OPEN | The FO3/FNV capture's "NPC stat storage" note (`charal-fnv-fo3-ruleset.md:473-480`) is still the pre-#5338 text. Still true. |
| #5712 | OPEN | `charal.md:642` still cites `ruleset.rs:128` for `impl Resource for CharacterRuleset`; it is now at `ruleset.rs:262`. Still true. |

## Regression Checks

All verified fixed on HEAD:
- #5706 / last D4-01: `actor_value` derives its layer term from `value.current() − value.base`; `set_override_composes_onto_the_formula` passes.
- #5708 / last D5-01: no `base + permanent + temporary` prose left outside the exempt dated narrative at `charal-skyrim-ruleset.md:668`.
- #5338, #5340, #5342, #5412, #5413, #5508, #5498: pins pass.
- #4452 `melee_damage_charal_bonus` scope check, #3441 lock order (`get_actor_value_does_not_hold_actor_values_across_ruleset`), #4450/#5240 scope pins, #2937 AP `player_only`, #4451 rounding modes, #2936 percentage scale, #2939 resistance floor, #4456 `band_for`/`band_by_key` tie-break: all unchanged and passing.
- #5794 shared finalize tail: CHARAL stamping (`stamp_faction_ranks`, `stamp_actor_values`, `stamp_spell_list`, `stamp_creature_attack`, `stamp_character_components`, `ActorProtection`) lives in `spawn_placement_root`, called by both resumable arms (`runtime.rs:179`, `prebaked.rs:86`).
- One derivation, one implementation: `effective_actor_level` exists once (`actor/mod.rs:149`, `.max(0)`); `resolve_inherited_*` direct production calls are still exactly two; `build_character_ruleset` is still the single sink (callers `inventory.rs:415`, `cell_loader/references/mod.rs:348`; both treat `None` as "no CHARAL").
- Magic modifiers: `set_permanent` has one live caller (the Base-view peel on a private copy); no writer overwrites `permanent_mod`.

## Known-Open Register (restated, none re-filed)

- FNV/FO3 tag-skill per-level formula undocumented, deferred; CLAS SPECIAL lives in `ATTR`.
- FO3/FNV AP (and Crit/Melee/Unarmed) NPC scope unsourced → `.player_only()` (#2937, documented, pinned). Melee Damage CONFLICTED and kept actor-general (#5240).
- VATS runtime absent; only AP formulas exist.
- Regen: `PoolRegenConfig` has zero production insertion sites; `pool_regen_tick_system` registered and inert; `affliction_tick_system` unregistered (verified again by grep, not comment).
- Level-up is **no longer absent** (M47.2). Correct the register: `level_cap()` has a production consumer; "the TES models have no XP-driven roll" is false for Skyrim (M-03).
- Oblivion `RulesetBuilder::None` is deliberate (pinned by `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate`). FO76/Starfield: captures, no builders.
- Open: #4137 (six `template_flags` bits with no consumer), #4232 (`effective_actor_level` returns 0 verbatim), #5710, #5712.

## Cross-Audit Routing

- `/audit-save`: no load-time sanity gate for `LevelUpPoints.skill_point_fraction` (a NaN would poison `grant_skill_points` permanently: `total = NaN + points`, floor NaN, `as u32` 0; a huge value would over-grant) or for `CharacterLevel.level` against `level_cap` (`validate_progression_state` was retired when `CharacterLevel` became saved). `LevelUpPoints` (v52→v53) and `CharacterLevel` (v46) are otherwise registered and bumped consistently with the `FORMAT_MAJOR` discipline.
- `/audit-concurrency` (Dim 4): `scripted_experience_system` and `scripted_skill_system` (`boot/schedule/late.rs:229-239`) under-declare their `Access` (`Perks`, the perk-entry-point resources via `compose_entry_point`, `PlayerNotifications` via `notifications::push`). Both are exclusive, so informational.
- `/audit-gameplay`: the Damage Resistance percentage has no consumer (`grep -i damageresist` → none); DT takes armor DT additively from `DamageThresholdRules::armor_dt` rather than through the `permanent_mod` layer the capture's boundary note describes (a double count if a future equip path also writes `permanent_mod`); `consumables.rs:51-54,148-152` branches on `GameKind` for the `HasPerk` CTDA index and the EFIT actor-value code map (per-game wire decode tables, seen and not flagged); kill-XP, bleedout and encumbrance constants (see L-08).
- `/audit-esm`: Skyrim `CLAS DATA` attribute weights (0x20..0x22) and the NPC AIDT Responsibility byte are not decoded (M-01, L-06).
- `/audit-sync`: SKILL.md known-open text needs the three corrections listed in the Executive Summary.
- Observation, not filed: the FO4 capture says the derived-row coefficients are "AUTHORED GMSTs, read not hardcoded" (`charal-fo4-ruleset.md:144,250-252`) while `fallout.rs` ships them as engine constants and `charal.md` §8.6 lists only `fAVDSkill*` as the remaining GMST seam. The shipped masters were measured and every value equals the hardcoded one, so a retune mod is the only way this matters; a one-line `charal.md` §8.6 correction would end the recurring ambiguity.
