**HEAD**: `2c36c29d8` · **Baseline**: [`AUDIT_CHARACTER_2026-09-29.md`](AUDIT_CHARACTER_2026-09-29.md) (HEAD `9fcfdc3fc`) · **Audited**: Dim 1 (Ruleset Seam), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2 (Derived Formulas: no commits to `derived/fallout/tes/skyrim/resistance.rs`, `combat.rs`, `stealth.rs`), Dim 3 (Progression & Pools: no commits to `leveling/regen/affliction/reputation/components.rs`)

# Character / CHARAL Audit — 2026-10-03

This run is `/audit-character` with the default scope and `--depth deep`. The dimensions were analysed in sequence, with no sub-agents. The delta is `9fcfdc3fc..HEAD`: 225 commits, about 20 in scope. The in-scope commits that matter:
- The fix wave for the baseline's five findings:
  - `c9254beb8` #5039 + #5042: the `CharacterRuleset::actor_value` composer, `ActorValue.base_authored`, `refresh_player_only_bases` and the new per-frame `player_derived_stats_system`.
  - `9bac2d87b` #5044
  - `c20dcdb04` #5047
  - `797a5b227` #5051
- Adjacent population work:
  - `be3cd9468` #5058: player faction reset on load.
  - `de6bdb381` #5005: FO3/FNV `NPC_ DATA` Base Health is parsed for the corpse rule.
  - `dca2bb401` #5013, `3c08cfe50` #5017, `ce73f658b` #5084.

## Tests recorded (read-only; nothing launched, no `--ignored`, no GPU)

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect -- character combat stealth` | 162 passed, 0 failed. That is +6 vs the baseline's 156: the #5039/#5042 composer and refresh pins plus the #5044 Starfield pin. |
| `cargo test -p byroredux-plugin --lib -- actor_value_derive consumables::tests::fn586 equip` | 103 passed, 1 ignored |
| `cargo test -p byroredux --bin byroredux -- resolve_inherited obscript_dialect player_character_template attach_to_player vitals_snapshot vital melee_damage player_npc_form_id hud modav setav player_derived reset_player_factions` (rustc 1.96) | 21 passed. **It was run in a clean detached worktree at HEAD**, because the shared working tree does not compile the bin test target: uncommitted `load_screen` edits remove `LoadScreenTransform::rotation_rad`, and those edits are outside CHARAL scope and are not this audit's. The worktree was removed afterwards. |
| Throwaway probe test (in that worktree only, discarded) | `setav <player> 0x2D4 500` then one `player_derived_stats_system` tick printed `105 -> 500` and then `after_tick=105`. This is the evidence for D1-01. |
| Real-master probe | Read-only scratch crate `/mnt/data/tmp/char_probe`, outside the repo and reused from the baseline run. It ran three binaries on `FalloutNV.esm` and `Fallout3.esm`. `basehealth` gives the `DATA` Base Health histogram, `npchp` gives per-NPC level, END, Base and seeded Health, and `gmst` gives the `fAVD*Health*` GMST values. |

## Executive Summary

**5 new findings: 0 CRITICAL · 1 HIGH · 1 MEDIUM · 3 LOW. 0 regressions.**
- All 5 baseline findings are closed and verified fixed on HEAD: #5039, #5042, #5044, #5047 and #5051.
- 3 still-open issues were re-confirmed and cited, not re-filed: #4137, #4232 and #4415.
- Open #5079 (LC-D3-02, the duplicated "Child" race-flag translation in `player_body.rs`) is owned by `/audit-legacy-compat` and is not re-filed.

**The headline finding is a constant earlier runs verified against the wrong source: FO3/FNV NPC Health (D4-01, HIGH).**
- The FO3/FNV NPC Health curve has been graded PASS in five earlier runs (08-27b, 09-11, 09-19 and the two carried forward since). Each time it was checked against capture row 93. That row's "+ NPC auto-calc seed" status is contradicted by the capture's own note four lines down: "Player formulas (NPCs derive separately)".
- The NPC seed reuses the **player** curve (`20·END`, `10·L`/`5·L`). It also drops the authored `NPC_ DATA` Base Health, which the GECK documents as additive. And it ignores the NPC-specific GMSTs that both masters author: `fAVDNPCHealthEnduranceMult = 5`, `fAVDNPCHealthLevelMult = 5`.
- 15 wiki-sampled NPCs (8 FNV, 7 FO3) all fit `Base + 5·END + 5·L − 10`. The seed is wrong for every one of them, overstating low-Base NPCs by up to about 7×: Three Dog is seeded at 180 against 25 in game.
- #5005 began parsing the Base Health field 4 days ago, and its field doc claims the actor-value seeding path consumes it. Nothing does.

**The baseline's fix wave is correct and introduced one new defect (D1-01).**
- What it got right:
  - `CharacterRuleset::actor_value` is the single composer for `GetActorValue` and the melee bonus.
  - `base_authored` separates placeholder bases from authored ones.
  - The per-frame `refresh_player_only_bases` keeps FO4/FO3/FNV player Health/AP on their formulas.
- The defect: that same refresh treats any authored base that differs from the formula as stale. So `setav` (console) and the SDK's `ActorValueOperation::SetBase` on the player's Health or AP report success and are silently reverted one frame later. The GECK documents the opposite: the set value persists on top of the derived base.

**Constants: no drift in Dim 2/3 code**, for the fourth run in a row. The baseline's formula verdicts stand, except the two FO3/FNV NPC Health rows, which are re-graded FAIL below. FO76 and Starfield still have captures but no builders. The Starfield vital-pool roster is now sourced (#5044).

**Wiring state is unchanged:**
- `PoolRegenConfig` has zero production inserts. All six inserts are in `regen.rs` `mod tests`.
- `affliction_tick_system` is unregistered.
- `level_cap()` has test consumers only.
- `FactionReputation` has no production insert.
- New this cycle: `player_derived_stats_system` is a live, non-inert `Stage::Update` exclusive. It is the first CHARAL system with a real per-frame effect.

## Constant Verification Table

Doc keys: **FNV/FO3** = `charal-fnv-fo3-ruleset.md`, **FO4** = `charal-fo4-ruleset.md`, **OBL** = `charal-oblivion-ruleset.md`,
**SKY** = `charal-skyrim-ruleset.md`, **SF** = `charal-starfield-ruleset.md`, **GECK** = local geck.uesp dump
(`/mnt/data/src/reference/geck-uesp-wiki`), **ESM** = GMST value read from the vanilla master.

### Derived-stat formulas (Dim 2): code unchanged, baseline verdicts carried forward

| Formula / constant | Code | Document | Verdict |
|---|---|---|---|
| FO4 Health `floor(77.5+4.5·END+2.5·L+0.5·L·END)` player-only | `fallout4_ruleset` | FO4 (the only cross-term row) | PASS |
| FO4 AP `60+10·AGI` player-only · CW `200+10·STR` | `fallout4_ruleset` | FO4; FNV/FO3 CW table | PASS ×2 |
| FO3 player Health `90+20·END+10·L` / AP `65+2·AGI` cap 85 | `fallout3_ruleset` | FNV/FO3 derived table; fandom *Hit Points* | PASS ×2 |
| FNV player Health `95+20·END+5·L` / AP `65+3·AGI` cap 95 | `falloutnv_ruleset` | same | PASS ×2 |
| FO3/FNV CW `150+10·STR` · Melee `0.5·STR` · Crit `1.0·Luck` cap 10 · Unarmed `ceil(0.5+0.05·U)` | shared rows | FNV/FO3 derived table | PASS ×4 for the formulas. **Scope**: new GECK evidence, see D2-01 |
| RadResist `(END−1)·2` cap 85 · PoisonResist `(END−1)·5` · `damage_multiplier` | `resistance.rs` | FNV/FO3 derived table | PASS ×3 |
| Oblivion Health `2·END` · Magicka `2·INT` · Fatigue · Armor `0.35+0.0065·skill` | `tes.rs` | CORE §5; OBL | PASS ×4 |
| Skyrim Light Armor `1+0.004·skill` · CW `250+0.5·base Stamina` | `skyrim_ruleset` | SKY | PASS ×2 |
| `combat.rs` / `stealth.rs` coefficients | unchanged | OBL damage formula; FNV/FO3 Sneak Detection | PASS (32 tests green) |

### FO3/FNV NPC Health seed (Dim 4): re-graded this run

| Row | Code | Document / data | Verdict |
|---|---|---|---|
| FO3 `NpcHealthCurve {90, 20·END, 10·L}` applied to NPCs | `CharacterRulesProfile::FALLOUT3` | FNV/FO3 "Player formulas (NPCs derive separately)"; ESM `fAVDNPCHealthEnduranceMult`=5, `fAVDNPCHealthLevelMult`=5 (0xAE66A/B) | **FAIL → D4-01** |
| FNV `NpcHealthCurve {95, 20·END, 5·L}` applied to NPCs | `CharacterRulesProfile::FALLOUT_NEW_VEGAS` | same GMSTs (5 / 5 in `FalloutNV.esm`) | **FAIL → D4-01** |
| NPC `DATA` Base Health additive term | absent (`NpcRecord::data_base_health` parsed, unread) | GECK *Stats Tab - NPC*: "Health is calculated with Endurance and level. This value is then added to that result." | **FAIL → D4-01** |
| NPC Health constant offset (`−10` in the empirical fit) | n/a | no document | **UNSOURCED**. It must be sourced before it is encoded (D4-01). |

### Progression / pools / reputation (Dim 3)
Code is unchanged, so the baseline's 60 rows stand.

### Player seed and profile rows

| Row | Value | Document | Verdict |
|---|---|---|---|
| FO4 player HP after an END change | re-evaluated each frame (`player_derived_stats_system`) | FO4 "rescales dynamically" | PASS (#5039 fixed) |
| Player `setav Health/AP` | reverted next frame | GECK *SetActorValue* note: the set value persists above the derived base | **FAIL → D1-01** |
| Starfield `vital_pools` `("O2","Oxygen")` | `Oxygen` = 0x2D5 | SF AVIF EDID scan (#5044) | PASS |
| FO3/FNV body-condition base 100 | `Some(100.0)` | FNV/FO3 body-condition row | PASS |

## Coverage Matrix

Re-derived from the `profile.rs` arms, workspace greps, scheduler registrations and the real-master probe:

| Family | Ruleset impl | Wired | Derived rows | Leveling model | NPC seed | Player seed | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | 8 | ✓ `150·L+50`, cap 20 (model only) | ✓ SPECIAL/skills; **Health wrong (D4-01)** | ✓ HP/AP refreshed per frame (#5039); setav reverted (D1-01) | ✗ registered-inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | 8 | ✓ `150·L+50`, cap 30 | ✓ SPECIAL/skills; **Health wrong (D4-01)** | ✓ as FO3 | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | 3 | ✓ `75·L+125`, uncapped | ✓ stored PRPS + DNAM | ✓ HP/AP refreshed per frame | ✗ | ✗ |
| Skyrim SE | ✓ `skyrim_ruleset` | ✓ `RulesetBuilder::Skyrim` | 2 | ✓ `25·L+75` + skill-XP curve | ✓ race+offset (class/level term deferred, disclosed) | ✓ race+offset | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` (deliberate, pinned) | tests only | ✓ 10 major-skill-ups | ✗ | ✗ | ✗ | ✗ |
| FO76 | ✗ | ✗ | ✗ | ✗ curve LOCKED, uncoded | ✗ `NpcStatModel::None` (#4453) | ✗ empty | ✗ | ✗ |
| Starfield | ✗ | ✗ | ✗ | ✗ PENDING | ✗ `NpcStatModel::None` (#4453) | ✗ empty (vital roster sourced, #5044) | ✗ | ✗ |

What "registered-inert" means: `pool_regen_tick_system` is registered in `boot/schedule/update.rs` but early-returns on every frame. The new `player_derived_stats_system` sits beside it and is **live**.

## Findings

**HIGH (1)**
- D4-01: the FO3/FNV NPC Health seed uses the player curve and drops the authored Base Health.

**MEDIUM (1)**
- D1-01: `setav`/SDK `SetBase` on the player's Health/AP is reverted one frame later.

**LOW (3)**
- D2-01: the GECK sources the NPC scope of the #4450 rows, and Melee Damage reads "Not used".
- D1-02: the #5039 registration split the pool-regen comment from its system.
- D5-01: a feature-matrix paragraph still says FO76/Starfield share FO4's stored NPC mechanism.

---

### CHAR-2026-10-03-D4-01: FO3/FNV NPC Health is seeded with the *player* curve; the authored `NPC_ DATA` Base Health and the masters' `fAVDNPCHealth*` GMSTs are ignored, so every auto-calc NPC's Health is wrong (Three Dog 180 vs 25, Easy Pete 180 vs 65)
- **Severity**: HIGH
- **Dimension**: Population Boundary (with a Coverage & Doctrine component: the capture self-contradiction)
- **Game**: fo3, fnv
- **Location**:
  - `crates/core/src/character/profile.rs`: the `FALLOUT3` / `FALLOUT_NEW_VEGAS` `npc_stats: NpcStatModel::ClassAutoCalc { health: NpcHealthCurve { … } }` rows, and the `NpcHealthCurve` doc, which reads "A sourced linear END + level curve".
  - `crates/plugin/src/esm/records/actor_value_derive.rs`: `derive_autocalc_actor_values`, at `health_curve.evaluate(endurance, level)`.
  - `crates/plugin/src/esm/records/actor/mod.rs`: `NpcRecord::data_base_health`, which is parsed and has zero readers.
  - `docs/engine/charal-fnv-fo3-ruleset.md`: the derived-table Health row.
- **Status**: NEW. The curve dates from `b434e4c0e`. It was graded PASS by `AUDIT_CHARACTER_2026-08-27b` rows 14–15, `2026-09-11` rows 7–8 and `2026-09-19`, each against capture row 93, which is the "reading the capture as confirmation" trap. `de6bdb381` (#5005, 2026-09-30) began parsing `DATA` Base Health for the corpse rule only.
- **Source**:
  - (1) GECK *Stats Tab - NPC*: "**Base Health:** Health is calculated with Endurance and level. This value is then added to that result."
  - (2) The capture contradicts its own Health row. Under the table it says: "Health: `fAVDHealthLevelMult` = 10 (FO3) / 5 (FNV); base 90 → 100. **Player formulas (NPCs derive separately).** Source: fandom *Hit Points*". The fandom article likewise says "The formulas given below generally only apply to the player character."
  - (3) Both masters author dedicated NPC GMSTs, read with the probe: `fAVDNPCHealthEnduranceMult` (0xAE66A) = **5.0** and `fAVDNPCHealthLevelMult` (0xAE66B) = **5.0**, in both `Fallout3.esm` and `FalloutNV.esm`. The player GMSTs the seed copies are `fAVDHealthEnduranceMult` = 20 and `fAVDHealthLevelMult` = 10 (FO3) / 5 (FNV).
- **Description**: `NpcHealthCurve` is the player Health formula with "sourced" in its doc. The capture marks the row "BUILT (player ruleset + NPC auto-calc seed)". The only sources state, in turn:
  - the player formula does not apply to NPCs;
  - NPC Health adds the record's Base Health;
  - the engine carries separate NPC multipliers.

  The NPC path consumes none of the three. The `data_base_health` field doc says "a positive value stays here for the actor-value seeding path", which describes a consumer that does not exist.
- **Evidence**:
  - **Population.**
    - `FalloutNV.esm`: 3,643 of 3,816 `NPC_` carry `DATA` Base Health > 0. Top values: 50 (1,590), 10 (947), 100 (202), 40, 25, 30 …
    - `Fallout3.esm`: 1,526 of 1,647 carry Base Health > 0. Top values: 10 (612), 50 (531) …
  - **Seed vs in-game totals** (fandom infobox `hp`). Base, END and L come from the probe; Seed is `derive_resolved_actor_values`:

| Game | NPC | Base | END | L | Seed | Wiki HP | Base+5·END+5·L−10 |
|---|---|---|---|---|---|---|---|
| FNV | GSEasyPete | 50 | 4 | 1 | 180 | 65 | 65 |
| FNV | GSSunnySmiles | 70 | 4 | 2 | 185 | 90 | 90 |
| FNV | GSRingo | 75 | 6 | 5 | 240 | 120 | 120 |
| FNV | CraigBoone | 190 | 5 | 5 | 220 | 230 | 230 |
| FNV | Benny | 150 | 5 | 8 | 235 | 205 | 205 |
| FNV | Veronica | 190 | 6 | 5 | 240 | 235 | 235 |
| FNV | RoseofSharonCassidy | 175 | 4 | 5 | 200 | 210 | 210 |
| FNV | RaulTejada | 200 | 4 | 14 | 245 | 280 | 280 |
| FO3 | ThreeDog | 10 | 4 | 1 | 180 | 25 | 25 |
| FO3 | Gob | 10 | 4 | 3 | 200 | 35 | 35 |
| FO3 | Nova | 10 | 4 | 4 | 210 | 40 | 40 |
| FO3 | MoiraBrown | 50 | 4 | 4 | 210 | 80 | 80 |
| FO3 | LucasSimms | 10 | 6 | 6 | 270 | 60 | 60 |
| FO3 | Charon | 200 | 5 | 5 | 240 | 240 | 240 |
| FO3 | Clover | 225 | 4 | 1 | 180 | 240 | 240 |

  - **How to read the table.**
    - The 5·END and 5·L slopes are exactly the authored NPC GMSTs. The same coefficients hold in FO3, whose player level mult is 10. This is the strongest evidence that NPCs run on their own GMST pair.
    - The `−10` constant is an empirical fit only: no document gives it.
    - Two samples were excluded as non-discriminating. Doc Mitchell's page says 50 against a computed 65. Jessup's page lists a different level (10→15 ×1.3) from the record (L8, no PC-mult).
- **Impact**: Every FO3/FNV auto-calc NPC spawns with the wrong Health. Most are 2–7× tankier than in game; the Goodsprings and Megaton populations triple or worse. High-Base companions are under-statted: Clover is seeded at 180 against 240. This affects combat, death, the HUD's target bar, and every `GetActorValue Health` / `GetHealthPercentage`-style CTDA, across the two reference titles of the playable slice. No crash and no failing test. `CREA` creatures are unaffected, because they read `DATA` Health verbatim.
- **Related**:
  - #5005 (closed), which introduced the unread field.
  - #2937 / #4450, the same "NPC scope unsourced" class for AP/Crit.
  - D2-01, which uses the same GECK page.
  - The Skyrim `with_gmst` precedent: `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst`.
- **Suggested Fix**:
  1. Add the Base Health term from the `Use Stats` terminal (`resolved.stats.data_base_health`).
  2. Read the NPC multipliers from `fAVDNPCHealthEnduranceMult` / `fAVDNPCHealthLevelMult`. `index.game_setting_float` is already plumbed.
  3. Source the constant offset before encoding it. The exe default tables or xNVSE are the candidates; the `−10` fit is not a source.
  4. Correct capture row 93 and the `NpcHealthCurve` doc.
  5. Add a real-master `#[ignore]` pin over a few of the samples above.

### CHAR-2026-10-03-D1-01: `setav` and the SDK's `SetBase` on the player's derived pools (FO3/FNV/FO4 Health + AP) report success and are silently reverted one frame later by #5039's per-frame refresh
- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam (the refresh contract) / Population Boundary (the skill's "`setav`/`modav` write the *base* component, not a derived output the next tick recomputes" check)
- **Game**: fo3, fnv, fo4
- **Location**:
  - `crates/core/src/character/ruleset.rs`: `CharacterRuleset::refresh_player_only_bases`, which re-stamps whenever `base_authored && base != value` is false.
  - `byroredux/src/systems/character.rs`: `player_derived_stats_system`, which runs every `Stage::Update` frame.
  - The writers: `byroredux/src/commands/actor_value.rs` (`AvEdit::SetBase`) and `byroredux/src/extensions/commands.rs` (`ActorValueOperation::SetBase`).
- **Status**: NEW (`c9254beb8`, 2026-09-30, the #5039 fix). Before it the stamp was static, so `setav Health` stuck.
- **Source**: the GECK *SetActorValue* note: "For the player, this will not modify base health … If you use player.SetAv Health 100 the player will have 180 total health — 80 from base health, and 100 for the rest." The arithmetic is inherited from the CS wiki (`fPCBaseHealthMult`). The structural claim is what matters: a player SetAV on a derived pool persists alongside the formula-derived base and is not replaced by it.
- **Description**: The refresh treats the authored base as a pure cache of the formula. Any authored base that differs from the formula output is overwritten. `set_base` is also the only way a console command, a script-facing SDK command or a save writes a base, so a deliberate player SetAV on Health/AP is indistinguishable from a stale stamp.
- **Evidence**: A throwaway test in a HEAD worktree used the FO4 player Health row, END 5, L1:
  ```
  setav 0x2D4 on entity 0: 105 -> 500     (command output)
  after_cmd=500  after_tick=105           (one player_derived_stats_system call)
  ```
  `charal.md` §6 lists "`setav`/`modav`" among the writers the refresh serves. That is true for writes to the *inputs* (END/AGI). Writes to the *output* key are never mentioned and no test covers them.
- **Impact**:
  - The engine console's `setav . <Health> N` and every SDK mod issuing `SetBase` on the player's Health or AP are no-ops that report success. The SDK path even reads the mutated state back and returns it to the sandbox.
  - Debug and repro workflows that set the player's HP silently fail.
  - Save/load is unaffected: the refresh recomputes the same base.
- **Related**: #5039 (its fix introduced this); #5042 (`base_authored`); D4-01 (a separate defect on the same Health rows).
- **Suggested Fix**: Keep the formula and the user-set value apart. One option is to have the refresh write only the formula-owned portion: compare against the last *stamped* formula output, which needs one stored value, and leave any `SetBase` delta in place. Another is to route a `SetBase` on a `PlayerOnly` output into the permanent-modifier layer at the write sites. At minimum, have `setav` refuse or warn on a `PlayerOnly` output key, and document the behaviour in `charal.md` §6.

### CHAR-2026-10-03-D2-01: the GECK's *Stats Tab - NPC* page now sources the NPC scope of four #4450 "scope unsourced" rows, and marks Melee Damage "Not used" for NPCs, while `melee_damage_charal_bonus` applies the actor-general `STR×0.5` to NPC aggressors
- **Severity**: LOW. Escalate to MEDIUM if "Not used" is confirmed to mean the engine applies no Melee Damage AV to NPC attacks.
- **Dimension**: Derived Formulas (scope contract) / Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**:
  - `crates/core/src/character/fallout.rs`: the shared Crit/Melee/Unarmed rows, pinned by `fo3_fnv_crit_melee_unarmed_scopes_are_actor_general_pending_a_source`.
  - `docs/engine/charal-fnv-fo3-ruleset.md`: the derived table and the #2937 blockquote.
  - `byroredux/src/combat.rs`: `melee_damage_charal_bonus`.
- **Status**: NEW evidence on known-open #4450 / #2937, both closed as documented. The deferral is not re-filed; the citation it was waiting for now exists.
- **Source**: GECK *Stats Tab - NPC*, section "Reported Stats" (NPC-scoped):
  - "Critical Chance: … Generally the same as the NPC's luck"
  - "Unarmed Damage: … Calculated from the Unarmed skill"
  - "Poison Resistance / Radiation Resistance: … Derived from the NPC's Endurance"
  - "**Melee Damage: Not used.**"
  - The list has no Action Points entry.
- **Description**: The page settles four of the capture's "scope unsourced" cells as actor-general:
  - Crit, Unarmed and the two resists match the code's choice.
  - AP's absence is consistent with the conservative `.player_only()`.

  It also says Melee Damage is "Not used" for NPCs. `MeleeDamageConfig` + `melee_damage_charal_bonus` add `0.5·STR` to every aggressor with the row, and `npc_combat_ai_system` makes NPCs aggressors. "Not used" may refer only to the dialog field, so the line needs adjudication before the ActorGeneral Melee choice is kept or changed.
- **Impact**: Today this is documentation. If "Not used" is engine behaviour, every NPC melee hit is overstated by `0.5·STR` (2.5–5 damage).
- **Suggested Fix**: Cite the page in the capture rows for Crit, Unarmed, Rad and Poison, and in the #2937 note for AP. Then adjudicate the Melee Damage line, with xNVSE `ActorValueOwner` usage or an in-game NPC melee measurement, and flip the row to `.player_only()` if it is confirmed. Update the pin's name and message either way.

### CHAR-2026-10-03-D1-02: #5039's `player_derived_stats_system` registration was inserted between the pool-regen comment block and its system, so the "#2153 3-deep hold stack" comment now heads the wrong registration
- **Severity**: LOW
- **Dimension**: Ruleset Seam / doc rot
- **Game**: all
- **Location**: `byroredux/src/boot/schedule/update.rs`, in the `register_update_systems` block above `pool_regen_tick_system`.
- **Status**: NEW (`c9254beb8`).
- **Description**: The comment "#2391 / ECS-D5B-03 — declared via `add_exclusive_with_access` … This system is #2153's site: it builds a 3-deep hold stack (`PoolRegenConfig` read held across `PoolRegenAccumulator` write …)" describes `pool_regen_tick_system`. The diff added the #5039 comment and the `player_derived_stats_system` registration directly under it. Read top-down, the hold-stack rationale (`PoolRegenConfig`, `PoolRegenAccumulator`) now sits on a system that touches neither, and `pool_regen_tick_system` has no rationale of its own.
- **Impact**: A `/audit-concurrency` Dim 4 reader or a future refactor attributes the exclusivity rationale to the wrong system. Comment only.
- **Suggested Fix**: Move the #5039 comment and registration above the "CHARAL pool regen" comment block, or below the `pool_regen_tick_system` registration.

### CHAR-2026-10-03-D5-01: `feature-matrix.md`'s CHARAL prose still says "FO4/FO76/Starfield share one 'stored' mechanism" and that FO76/Starfield "inherit the same decoder by lineage", contradicting the table row above it (✗, #4453)
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo76, starfield
- **Location**: `docs/feature-matrix.md`, § Character / Progression (CHARAL), the paragraph beginning "Skyrim's NPC population derives Health, Magicka and Stamina…".
- **Status**: NEW. The paragraph was last edited by `8175cb706` (2026-08-31). It went stale with #4453 (2026-09-23), and the sibling #5051 fix (`797a5b227`) edited only the player-seed row.
- **Description**: Since #4453, `CharacterRulesProfile::FO76` / `STARFIELD` carry `NpcStatModel::None`, pinned by `fo76_and_starfield_claim_no_npc_stat_model_until_captured`, and the table's "NPC actor-value population" row says ✗ for both. The prose below still describes them as sharing FO4's PRPS + DNAM population. The parser's PRPS decoder still runs for them, but nothing populates actors from it.
- **Impact**: A reader takes FO76/Starfield NPC stats as wired.
- **Suggested Fix**: Re-scope the sentence to FO4. Say that FO76/Starfield parse PRPS/DNAM but populate nothing until a capture lands (#4453).

## Observations (not findings)

- **Absent-level default diverges between the two composer callers**: `GetActorValue` uses `CharacterLevel` → `map_or(0, …)`, while `melee_damage_charal_bonus` uses `map_or(1, …)`. This is inert today. Only `PlayerOnly` Health reads `DerivedInput::LEVEL`, and `actor_value` never evaluates `PlayerOnly` rows for anyone. Every spawned NPC also carries `CharacterLevel` (`npc_spawn.rs`). Unify the defaults when the first actor-general level-dependent row lands.
- **New live system's scheduler declaration** (`player_derived_stats_system`): it reads `PlayerEntity`, `CharacterLevel` and `CharacterRuleset` and writes `ActorValues`. That matches the body, and the lock order is `CharacterRuleset → ActorValues` (#3441-canonical). It is allocation-free per frame. Its idempotence is pinned (`refresh_player_only_bases` returns `false` on the second call).
- **#5047 guard**: `player_body.rs` is scanned as a whole-file `include_str!`. That is acceptable today because its `mod tests` has no `resolve_inherited_*` call, but it would go vacuous-in-reverse if a test fixture added one. `actor_value_derive.rs`'s production split is the stronger pattern.
- **#5058** resolves the player's factions through `ResolvedNpc::resolve`, which honours TPLT. No raw `player.<field>` reads remain in `inventory.rs`'s template builders.

## Regression checks (closed since baseline: all fixed on HEAD)

| Issue | Verified |
|---|---|
| #5039 | `refresh_player_only_bases` runs at stamping, after the constant spells in `attach_to_player`, and every frame in `player_derived_stats_system`. `modav_endurance_rescales_the_players_fo4_health` is green. The output-key write path is the new D1-01. |
| #5042 | `CharacterRuleset::actor_value` is the only composer. `GetActorValue` (`condition.rs`) and `melee_damage_charal_bonus` both call it, and `base_authored` is set only by `set_base`. The FORMAT_MAJOR bump is recorded in the commit; that belongs to `/audit-save`. |
| #5044 | STARFIELD `vital_pools` is `("O2","Oxygen")`, with the capture line in `charal-starfield-ruleset.md` and the pin `starfield_oxygen_pool_names_the_real_avif_editor_id`. |
| #5047 | The player template reads level, race and class from the `Use Stats` / `Use Traits` terminals and the outfit from `Use Inventory`. The guard expects `player_body.rs` = 1 and is green. |
| #5051 | The feature-matrix player-seed row says FO76/Starfield "no". The sibling prose paragraph was missed → D5-01. |

## Known-Open Register (re-confirmed on HEAD; none re-filed)

1. **FNV/FO3 tag-skill per-level formula**: still undocumented and absent, not guessed. CLAS SPECIAL is still read from `ATTR`.
2. **FO3/FNV AP (and Crit/Melee/Unarmed) NPC scope (#2937 / #4450, closed as documented)**: AP still ships `.player_only()`, and the three others still ship an explicit unsourced ActorGeneral. A citation now exists → D2-01; that is evidence, not a re-file.
3. **VATS runtime**: absent; only the AP formulas exist.
4. **Regen / affliction / level-up**:
   - `PoolRegenConfig` has 6 inserts, all in `regen.rs` `mod tests`.
   - `pool_regen_tick_system` is registered-inert.
   - `affliction_tick_system` is unregistered (`registry_completeness_tests.rs` classifies `AfflictionStatus` as forward-latent).
   - `level_cap()` has test consumers only (`profile.rs`).
5. **Oblivion `RulesetBuilder::None`**: deliberate and pinned. FO76/Starfield have captures but no builders and `NpcStatModel::None`.
6. **#4137** (OPEN): six `template_flags` bits have no consumer.
7. **#4232** (OPEN): `effective_actor_level` still returns `npc.level.max(0)` verbatim on the non-multiplier branch. There is a single definition (`actor/mod.rs`), and `attach.rs`'s `.max(1)` is loot policy, not a copy.
8. **#4415** (OPEN): the magic runtime is partial. `apply_constant_modifiers` is symmetric (`direction` ±1 over the same resolved modifier list) and writes only the permanent layer.
9. **`CharacterLevel` unsaved**: `validate_progression_state` refuses once `xp != 0`. Owned by `/audit-save`.
10. **`FactionReputation`**: no production insert.
11. **#5079** (OPEN, `/audit-legacy-compat`): the duplicated Child race-flag translation in `player_body.rs`.

## Cross-Audit Routing

- **D4-01**: the seed lives in the plugin crate's `actor_value_derive.rs` and the profile row, so CHARAL owns it. The `DATA` Base Health parse is `/audit-esm` Dim 4, and it is correct; only the consumer is missing. Combat balance follow-through goes to `/audit-gameplay`. The capture correction goes to Dim 5.
- **D1-01**: the console surface is `/audit-tooling` (`commands/`); the SDK `SetBase` path is `/audit-tooling` (SDK / mod-runtime). The fix belongs in CHARAL's refresh contract.
- **D2-01**: NPC melee damage → `/audit-gameplay` (combat).
- **D1-02**: the scheduler comment → `/audit-concurrency` Dim 4.
- Component shape → `/audit-ecs`.

Suggested next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-03.md`. Use the domain label `character`, plus:
- `game:fnv` + `game:fo3` on D4-01 and D2-01;
- `game:fo4` (and fo3/fnv) on D1-01;
- `game:starfield` on D5-01.
