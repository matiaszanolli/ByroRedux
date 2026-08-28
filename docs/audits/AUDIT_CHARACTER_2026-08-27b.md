# Character / CHARAL Audit — 2026-08-27b (second same-day pass)

**Scope**: `/audit-character`, all 6 dimensions, `--depth deep`, every implemented
family. Run solo (no sub-agent fan-out, per dispatch) as part of a
`--preset comprehensive` audit-suite run. Owner slice
`crates/core/src/character/` plus the CHARAL-adjacent siblings
`crates/core/src/combat.rs` and `crates/core/src/stealth.rs`, plus the
population boundary (`crates/plugin/src/esm/records/actor_value_derive.rs`,
`byroredux/src/npc_spawn.rs`, `byroredux/src/commands/actor_value.rs`).

**Filename**: written to `-27b.md` because `docs/audits/AUDIT_CHARACTER_2026-08-27.md`
is already taken by an earlier same-day **Dimension-5-only** run from the
`streaming-deep` preset. That report is reconciled below, not re-filed.

**Repo state**: HEAD `969d81c8`, branch `main`.

Delta since the last **full** sweep (`AUDIT_CHARACTER_2026-08-24.md`, HEAD
`048a8bd8`) inside the audited slice:

```
byroredux/src/npc_spawn.rs                          |  70 +++---
crates/core/src/character/ruleset.rs                |  10 +-
crates/core/src/character/skill.rs                  |  25 +-
crates/core/src/ecs/components/actor_values.rs      |   8 +
crates/plugin/src/esm/records/actor_value_derive.rs | 265 ++++++++++++++++---
docs/engine/charal.md                               |  26 +-
```

`derived.rs`, `fallout.rs`, `tes.rs`, `skyrim.rs`, `leveling.rs`, `profile.rs`,
`regen.rs`, `affliction.rs`, `resistance.rs`, `reputation.rs`, `components.rs`,
`attribute.rs`, `combat.rs` and `stealth.rs` are **byte-unchanged** since that
sweep. The one commit that landed *after* the earlier same-day Dimension-5
report is `7445506c` ("Fix #3381, Fix #3382: resolve TPLT 'Use Stats' for every
stat model, not just auto-calc") — the fix for that report's own two leading
findings. **It has never been audited, and two of this pass's three MEDIUM
findings are in it.**

**Tests recorded** (read-only; nothing launched, no `byroredux` process started):

| Command | Result |
|---|---|
| `cargo test -p byroredux-core character` | **113 passed**, 0 failed |
| `cargo test -p byroredux-plugin actor_value` | 0 passed, 0 failed, 1 `#[ignore]`d (real-data gate) |

**Verification method**: static analysis + capture-document cross-check, plus
**three purpose-written census probes** compiled against the live
`byroredux-plugin` parser and run over four shipped masters (`Skyrim.esm`,
`Fallout4.esm`, `FalloutNV.esm`, `Fallout3.esm`). Every count below is measured,
not estimated. The probes were temporary `crates/plugin/examples/_tmp_char_*.rs`
files, run and then deleted; the working tree is unchanged.

| Dimension | Area | New findings |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | 0 |
| 2 | Derived-Stat Formulas (+ CHARAL-adjacent siblings) | **1 MEDIUM** |
| 3 | Leveling & Progression | 0 |
| 4 | Pools, Afflictions, Resistances & Reputation | **1 LOW** |
| 5 | Population Boundary | **2 MEDIUM** |
| 6 | Coverage, Documentation & Doctrine Drift | **3 LOW** |
| **Total** | | **0 CRITICAL · 0 HIGH · 3 MEDIUM · 4 LOW** |

---

## Executive Summary

### The `7445506c` template fix is a real improvement that overshot on two arms

The earlier same-day report established that `derive_npc_actor_values` was a
three-arm match in which only the auto-calc arm resolved `TPLT` inheritance, and
that the Skyrim arm therefore contradicted `Background` on the same entity.
`7445506c` fixed that by hoisting one `resolve_inherited_stats` call ahead of
the stat-model match (`actor_value_derive.rs:188`). Measured against
`Skyrim.esm`, the wrong-pool population dropped **512 → 118** actors. That is a
genuine, large win and should not be reverted.

But `resolve_inherited_stats` walks the **`Use Stats` (`0x0002`)** chain, and
two of the fields the arms then read are not stats:

1. **Race is a `Use Traits` (`0x0001`) field**, and the Skyrim arm derives its
   pool *bases* from `RACE.DATA` via `npc.race_form_id`. The codebase's own
   documentation says so — `NpcRecord::template_flags`'s doc comment
   (`crates/plugin/src/esm/records/actor/mod.rs:370-371`) reads
   *"`0x0001` — **Use Traits** (race). Consumed by
   `equip::resolve_inherited_traits`"* — and `stamp_character_components`
   (`byroredux/src/npc_spawn.rs:174`) writes `Background.race_form_id` from the
   **traits**-resolved record. So the same-entity contradiction the fix was
   filed to close still exists, on **1,180 of 5,118** Skyrim NPCs, and now
   points the other way. See D5-01.

2. **`calculated_health == 0` is an "absent" sentinel, not a value.** The FO4
   arm reads the resolved template's baked `DNAM` unconditionally, so a shell
   that authors its own Health but inherits from a template with no `DNAM` now
   emits nothing where it used to emit the authored value: **54** `Fallout4.esm`
   NPCs lose their Health actor value, **35** gain one, net **+19** actors with
   no `ActorVitals` at all (330 → 349). Those actors cannot be damaged or killed
   by the P2 melee slice. See D5-02.

### The stealth-constant verification trail is circular

`crates/core/src/stealth.rs` carries roughly a dozen numeric sub-coefficients
inside `detection_score`'s `Sound` and `Visual` terms. **None of them appears in
`docs/engine/charal-fnv-fo3-ruleset.md`**, which records only the top-level
`Detection` expression and describes `Sound`/`Visual` qualitatively. Worse, the
three most recent reports each attribute the verification to a predecessor that
did not perform it: `AUDIT_CHARACTER_2026-08-15.md:2256` states plainly
*"Dimension 2 verified 26 constants; none of them are these"*; the 08-16 sweep
never touched the file; `AUDIT_CHARACTER_2026-08-20.md:570` says
*"unchanged since the 2026-08-16 sweep verified it"*; `AUDIT_CHARACTER_2026-08-24.md`
repeats the same attribution. The chain terminates in nothing. See D2-01.

### What is clean

Everything else this pass re-derived from the capture documents matched, and the
CHARAL doctrine holds: `grep` for `GameKind` / master-name / game-identity
comparisons across `crates/core/src/character/` and every `CharacterRuleset`
consumer returns **zero** production branches. `character_rules_profile`
(`crates/plugin/src/esm/records/mod.rs:143`) is the single game-identity match,
at the sanctioned parser boundary, and it produces data (a
`CharacterRulesProfile` row), not a code path.

---

## Constant Verification Table

Verdicts: **PASS** = code equals the capture document. **UNSOURCED** = no
per-game capture document carries the value. Every numeric row cites its source.

### Fallout family (`fallout.rs`, `profile.rs`)

| # | Constant / formula | Code | Authoritative value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FO4 Health | `bilinear(END, 4.5, LEVEL, 2.5, cross 0.5, bias 77.5).floored().player_only()` | `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` | `charal-fo4-ruleset.md` Health | **PASS** |
| 2 | FO4 Action Points | `affine(AGI, 10.0, 60.0).player_only()` | `60 + 10·AGI` | `charal-fo4-ruleset.md` | **PASS** |
| 3 | FO4 Carry Weight | `affine(STR, 10.0, 200.0)` | `fAVDCarryWeight{Base=200, Mult=10}` | `charal-fnv-fo3-ruleset.md:106-109` (cross-game GMST family) | **PASS** |
| 4 | FO3 Health | `bilinear(END, 20.0, LEVEL, 10.0, 0.0, 90.0).player_only()` | `90 + END·20 + Level·10` | `charal-fnv-fo3-ruleset.md:93` | **PASS** |
| 5 | FNV Health | `bilinear(END, 20.0, LEVEL, 5.0, 0.0, 95.0).player_only()` | `100 + END·20 + (Level−1)·5` ≡ `95 + 20·END + 5·L` | `charal-fnv-fo3-ruleset.md:93` | **PASS** (algebraically identical) |
| 6 | FO3 Action Points | `affine(AGI, 2.0, 65.0).capped(85.0).player_only()` | `65 + 2·AGI` (cap 85) | `charal-fnv-fo3-ruleset.md:94` | **PASS** on numbers; scope still **UNSOURCED** by the document's own admission (#2937, disclosed in code) |
| 7 | FNV Action Points | `affine(AGI, 3.0, 65.0).capped(95.0).player_only()` | `65 + 3·AGI` (cap 95) | `charal-fnv-fo3-ruleset.md:94` | **PASS**; same disclosed scope caveat |
| 8 | FO3/FNV Carry Weight | `affine(STR, 10.0, 150.0)` | `150 + 10·STR` | `charal-fnv-fo3-ruleset.md:95,106` | **PASS** |
| 9 | FO3/FNV Melee Damage | `affine(STR, 0.5, 0.0)` | `STR × 0.5`, additive | `charal-fnv-fo3-ruleset.md:97,265` | **PASS** |
| 10 | FO3/FNV Critical Chance | `affine(Luck, 1.0, 0.0).capped(10.0)` | `Luck × 1%`, cap 10 % — on the 0–100 scale | `charal-fnv-fo3-ruleset.md:96` + `derived.rs` § Percentage convention | **PASS** |
| 11 | FO3/FNV Unarmed Damage | `affine(Unarmed, 0.05, 0.5).ceiled()` | `ceil((10 + Unarmed)/20)` | `charal-fnv-fo3-ruleset.md:98,118` | **PASS** (algebraic expansion exact) |
| 12 | Radiation Resistance | `k = 2.0`, cap `85.0`, bias `−2.0`, `clamped_below(0.0)` | `(END−1)·2`, cap 85 % | `charal-fnv-fo3-ruleset.md:99,270` | **PASS** |
| 13 | Poison Resistance | `k = 5.0`, cap `INFINITY` | `(END−1)·5`, uncapped | `charal-fnv-fo3-ruleset.md:100,286` | **PASS** |
| 14 | FNV NPC Health curve | `NpcHealthCurve { bias 95.0, END 20.0, level 5.0 }` | as row 5 | `charal-fnv-fo3-ruleset.md:93` | **PASS** |
| 15 | FO3 NPC Health curve | `NpcHealthCurve { bias 90.0, END 20.0, level 10.0 }` | as row 4 | `charal-fnv-fo3-ruleset.md:93` | **PASS** |
| 16 | Skill auto-calc | `2 + 2·gov + ceil(0.5·Luck)` | `fAVDSkill<Name>Base=2` + geckwiki mults | `actor_value_derive.rs:121-134` (#3173 settled) | **PASS** |

### Oblivion (`tes.rs`) + CHARAL-adjacent `combat.rs`

| # | Constant / formula | Code | Authoritative value | Source | Verdict |
|---|---|---|---|---|---|
| 17 | Oblivion Health | `affine(END, 2.0, 0.0).player_only()` | `2 × Endurance` | `charal-oblivion-ruleset.md` § Health | **PASS** |
| 18 | Oblivion Magicka | `affine(INT, 2.0, 0.0).player_only()` | `2 × Intelligence` | `charal-oblivion-ruleset.md` § Magicka | **PASS** |
| 19 | Oblivion Fatigue | four `affine(attr, 1.0, 0.0)` rows, uncapped/unrounded, summed | `STR + WIL + AGI + END` | `charal-oblivion-ruleset.md` § Fatigue | **PASS** (multi-row contract honoured) |
| 20 | Oblivion Armor Rating | `ARMOR_RATING_SKILL_COEFF 0.0065`, `BIAS 0.35` | `0.35 + 0.0065 × ArmorSkill` | `charal-oblivion-ruleset.md:323` | **PASS** |
| 21 | `oblivion_attribute_bonus` bands | `0→1`, `1..=4→2`, `5..=7→3`, `8..=9→4`, `_→5` | `+1..+5` on `0, 1–4, 5–7, 8–9, 10+` | `charal-oblivion-ruleset.md:788-789` | **PASS** |
| 22 | `oblivion_health_gain_per_level` | `0.1 × Endurance` | "10 HP if your Endurance is at 100" | `charal-oblivion-ruleset.md` (UESP anchor) | **PASS** |
| 23 | `modified_skill` | `skill + 0.4·(luck − 50)` | `ModifiedSkill = Skill + 0.4×(Luck−50)` | `charal-oblivion-ruleset.md:281` | **PASS** |
| 24 | `oblivion_weapon_damage_multiplier` | `0.5·(0.75 + 0.005·A)·(0.2 + 0.015·S)`, both inputs clamped `[0,100]` | identical, clamp stated | `charal-oblivion-ruleset.md:200-232,315` | **PASS** (all four coefficients + the clamp) |
| 25 | `oblivion_hand_to_hand_damage` | `1 + 10.5·(STR/100)·(MS/100)`; fatigue `1 + 0.5·health`; **no** clamp | identical, clamp explicitly not stated for H2H | `charal-oblivion-ruleset.md:301-317` | **PASS** (cross term + the deliberate clamp omission) |
| 26 | Oblivion `LevelingModel::OBLIVION` | `major_skill_ups_per_level 10`, `level_cap 0` | "a level becomes available after 10 increases in major skills" | `charal-oblivion-ruleset.md:786-787` | **PASS** |

### Skyrim (`skyrim.rs`, `leveling.rs`)

| # | Constant / formula | Code | Authoritative value | Source | Verdict |
|---|---|---|---|---|---|
| 27 | `SKYRIM_POOL_BASE` | `100.0` | "…points of magicka" base 100 | `charal-skyrim-ruleset.md:585` | **PASS** |
| 28 | `pool_pick_gain` | `10.0` | 10-point pool pick per level | `charal-skyrim-ruleset.md:714-719` | **PASS** |
| 29 | `xp_mult` / `xp_base` | `25.0` / `75.0` | `fXPLevelUpMult=25`, `fXPLevelUpBase=75`; `25 × level + 75` | `charal-skyrim-ruleset.md:713-716` | **PASS** (was UNSOURCED on 2026-08-15; the doc section has since landed) |
| 30 | `xp_per_skill_rank` | `1.0`, engine-owned, no GMST read | "A skill raised to rank `R` awards `R` character XP; that coefficient is an engine rule, not a `fXPPerSkillRank` GMST" | `charal-skyrim-ruleset.md:716-718` | **PASS** — settled design, not re-flagged |
| 31 | `with_gmst` overlay set | requests exactly `["fXPLevelUpBase", "fXPLevelUpMult"]` | as row 30 | `leveling.rs:92-109` + `skyrim_gmst_overlay_reads_only_authored_curve_settings` | **PASS** — no third GMST has crept back |
| 32 | `LIGHT_ARMOR_RATING_COEFF` | `0.004` (player), `.player_only()`; NPC `0.015` not modelled | `1 + 0.004 × LightArmorSkill` (player) / `1 + 0.015 ×` (NPC) | `charal-skyrim-ruleset.md:208-209,219` | **PASS** |
| 33 | `CARRY_WEIGHT_BIAS` / `_STAMINA_COEFF` | `250.0` / `0.5`, `a_from_base()` | `CarryWeight = 250 + 0.5 × BaseStamina`, base layer only | `charal-skyrim-ruleset.md:655-666` | **PASS** (including the base-vs-current reading mode) |
| 34 | `SkillSet::SKYRIM` Illusion | `SkillDef::ungoverned("Mysticism")` | vanilla `Skyrim.esm` authors the slot at `0x45B` as `AVMysticism` | `skill.rs:138-148` (#3169) | **PASS** — #3169 is fixed on `main` |
| 35 | `SKYRIM_SKILL_USE_CURVE` + `skyrim_skill_xp_to_next` | `1.95`; `mult·level^curve + offset` | present in `charal.md:147` only; **absent from `charal-skyrim-ruleset.md`** | — | **UNSOURCED vs the per-game authority** — prior finding, see Prior-Finding Register |

### FNV reputation / karma (`reputation.rs`)

| # | Constant / structure | Code | Authoritative value | Source | Verdict |
|---|---|---|---|---|---|
| 36 | 4×4 standing grid orientation | `STANDING_GRID: [[..;4];4]` indexed `[infamy][fame]`; row 0 = Neutral/Accepted/Liked/Idolized | table headed `Infamy ↓ \ Fame →`, row **0** = Neutral, Accepted, Liked, Idolized | `charal-fnv-fo3-ruleset.md:519-524` | **PASS** — axes are **not** transposed; all 16 cells match row-for-row |
| 37 | `REPUTATION_AXIS_MAX` | `100` | steepest vanilla Range-3 threshold (Caesar's Legion `100`) | `reputation.rs:125-129` + `charal-fnv-fo3-ruleset.md` faction table | **PASS** |
| 38 | `ReputationStanding::sentiment` | 3-bucket colour map | doc gives the colour *legend* only, never per-cell colours | — | **UNSOURCED** — disclosed in code (#2949), already tracked, not re-filed |

### CHARAL-adjacent `stealth.rs`

| # | Constant | Code | Capture-document value | Verdict |
|---|---|---|---|---|
| 39 | Top-level `Detection` expression | `atten·(Sound + Visual + DetectorSkill/2) − TargetSkill/2 − 35` | `charal-fnv-fo3-ruleset.md:231` | **PASS** |
| 40 | `Attenuation`, MaxDist 2500/5000 | `((max − d)/max)²` | `charal-fnv-fo3-ruleset.md:235` | **PASS** |
| 41 | `DetectorSkill = (10 + 8·PER) × state`, state `0.8/1.2/1.0` | matches | `charal-fnv-fo3-ruleset.md:234` | **PASS** |
| 42 | `TargetSkill` expression | matches, incl. the `max(50 − 10·TargetLevel, 0)` term | `charal-fnv-fo3-ruleset.md:232-233` | **PASS** |
| 43 | Band cut points `< −20` / `−20..=0` / `> 0` | matches | `charal-fnv-fo3-ruleset.md:238` | **PASS** |
| 44 | LOS sound multiplier `1.6` / `0.16` | `stealth.rs:212` | **absent** | **UNSOURCED** → D2-01 |
| 45 | Movement sound `12.0 + weight/2` and `×1.5 / ×1.0 / ×0` | `stealth.rs:213-219` | **absent** | **UNSOURCED** → D2-01 |
| 46 | Action-sound values `0 / 10 / 50 / 100` and the `×2.0` weight | `stealth.rs:95-98,220` | **absent** | **UNSOURCED** → D2-01 |
| 47 | Visual `1.4 × min(light × nightEye, 100)`, night-eye `3.0` | `stealth.rs:225-231` | **absent** | **UNSOURCED** → D2-01 |
| 48 | Visual movement `0.21 / 0.01 / 0.0` | `stealth.rs:231-234` | **absent** | **UNSOURCED** → D2-01 |
| 49 | Armour-class penalty `0 / 10 / 20` | `stealth.rs:116-118` | **absent** (doc says only "armor class (heavy/medium/light)") | **UNSOURCED** → D2-01 |

**Tally — 49 rows: 40 PASS, 8 UNSOURCED, 0 numeric mismatches against a
document that carries the value.** Two of the UNSOURCED rows (38, 35) are prior
findings; six (44–49) are the new D2-01.

---

## Coverage Matrix

| Game | Capture doc | Profile row | Ruleset builder | Ruleset **wired** | Derived rows | NPC stat model | Leveling model | Regen wired | Affliction wired |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Oblivion** | ✓ | `OBLIVION` | `oblivion_ruleset` | ✗ (`RulesetBuilder::None`) | 8 rows / 5 stats | `None` | `OBLIVION` | ✗ (`oblivion_pool_regen_config` has no production caller) | ✗ |
| **FO3** | ✓ | `FALLOUT3` | `fallout3_ruleset` | ✓ | 8 | `ClassAutoCalc` (90/20/10) | `FO3` | ✗ | ✗ |
| **FNV** | ✓ | `FALLOUT_NEW_VEGAS` | `falloutnv_ruleset` | ✓ | 8 | `ClassAutoCalc` (95/20/5) | `FNV` | ✗ | ✗ |
| **Skyrim SE** | ✓ | `SKYRIM` | `skyrim_ruleset` | ✗ (`RulesetBuilder::None`) | 2 (unreachable) | `RaceBaseOffsets` — Health + Magicka + Stamina, each independent | `SKYRIM` (unreachable) | ✗ | ✗ |
| **FO4** | ✓ | `FALLOUT4` | `fallout4_ruleset` | ✓ | 3 | `Stored` (`PRPS` + `DNAM`) | `FO4` | ✗ | ✗ |
| **FO76** | ✓ | `FALLOUT76` | ✗ | ✗ | — | `Stored` (unverified) | ✗ | ✗ | ✗ |
| **Starfield** | ✓ | `STARFIELD` | ✗ | ✗ | — | `Stored` (unverified) | ✗ | ✗ | ✗ |

Derived-row counts are all within the 6–10 band the flat-`Vec` rationale
assumes (max 8, Oblivion); the data-structure choice remains justified.

**Reachability of the leveling models** (Dimension 3): the *only* production
consumer of `LevelingModel` anywhere in the workspace is
`crates/scripting/src/condition.rs:647` (`GetXPForNextLevel` → `xp_to_next`).
`level_cap()`, `grants_perk_at()`, `skill_points()`, `pool_pick_gain()` and
`xp_from_skill_rank()` have **zero** non-test callers — there is no level-up
runtime, so the `level_cap == 0` sentinel cannot be got wrong at a boundary
that does not exist yet. Recorded as coverage, not as a bug.

**Inert systems**: `pool_regen_tick_system` is registered
(`byroredux/src/boot.rs:993-1000`) but `PoolRegenConfig` is inserted only in
unit tests, so it early-returns forever. `affliction_tick_system` has **no**
scheduler registration at all (confirmed by workspace grep, and by
`byroredux/src/save_io/registry_completeness_tests.rs:104`'s own note).

---

## Findings

### CHAR-2026-08-27b-D5-01: the Skyrim pool bases now come from the `Use Stats` template chain, but race is a `Use Traits` field — 1,180 NPCs contradict `Background` on the same entity

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: `skyrim`
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:188` (the
  hoisted resolve) and `:201-220` (`derive_skyrim_actor_values`)
- **Status**: NEW — introduced by `7445506c`, which landed *after* the earlier
  same-day report and is unaudited. Not a re-file of #3381: that issue was
  "the arm reads the shell", this is "the arm now reads the wrong chain".
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:370-371` —
  *"`0x0001` — **Use Traits** (race). Consumed by
  `equip::resolve_inherited_traits` (#2956)"*; `crates/plugin/src/equip.rs:314-318`
  — `resolve_inherited_traits` is documented as
  *"the NPC record that should supply **race** (and other 'traits' fields)"*.
- **Description**: `derive_npc_actor_values` resolves the `TPLT` chain **once**,
  through `resolve_inherited_stats` (gate bit `0x0002`, "Use Stats"), and hands
  the single resolved record to all three arms. The Skyrim arm then reads
  `npc.race_form_id` off it to look up `RACE.DATA`'s starting Health / Magicka /
  Stamina. Race is not a stat: it is inherited through the independently-set
  `Use Traits` bit, which the same codebase resolves with a *different* function
  and which `stamp_character_components` correctly uses for `Background` on the
  very same entity. The two bits are separate, so a shell can carry one without
  the other, and the shipped data confirms it does — at scale.
- **Evidence**:
  ```rust
  // actor_value_derive.rs:188
  let npc = crate::equip::resolve_inherited_stats(npc, effective_actor_level(npc), index);
  // …:201-209
  fn derive_skyrim_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
      let Some(race) = index.races.get(&npc.race_form_id) else {
          return Vec::new();
      };
      …
      ("Health", race.starting_health, npc.health_offset),
  ```
  against
  ```rust
  // byroredux/src/npc_spawn.rs:151-152,174
  let stats_npc  = resolve_inherited_stats(npc, shell_level, index);
  let traits_npc = resolve_inherited_traits(npc, shell_level, index);
  …
  race_form_id: traits_npc.race_form_id,
  ```
  Measured on `Skyrim.esm` this session (temporary probe over
  `index.npcs`, using the crate's own `resolve_inherited_stats` /
  `resolve_inherited_traits`):

  | Measure | Count | Share of 5,118 `NPC_` |
  |---|---:|---:|
  | `NPC_` with a `TPLT` | 3,651 | 71.3 % |
  | `Use Stats` bit set | 3,182 | 62.2 % |
  | **stats-chain race ≠ traits-chain race** | **1,180** | **23.1 %** |
  | …and the two races give a different `RACE.DATA` (H, M, S) triple | **118** | 2.3 % |
  | stats-chain race ≠ the shell's own `RNAM` | 1,680 | 32.8 % |

  Concrete cases, all from vanilla `Skyrim.esm`:
  ```
  MS07LvlBlackbloodMissileNordM (000DC8DC)
    stats-race 00109C7C → (H 12,  M —,    S 200)      ← what the code uses
    traits-race 00013746 (NordRace) → (H 50, M 50, S 50)  ← what Background says
  DA02CultistF2 (0004D8D3)
    stats-race 00109C7C → (H 12,  M —,    S 200)
    traits-race 00013742 → (H 50,  M 50,  S 50)
  ```
  Note `00109C7C` authors **no** starting Magicka, so those actors also lose
  their Magicka actor value entirely — not merely a wrong number.
- **Impact**: 118 Skyrim NPCs receive pool bases from the wrong `RACE` record
  (some losing a pool outright), and 1,180 carry a `Background.race_form_id`
  that disagrees with the race their own vitals were derived from — the exact
  same-entity contradiction #3171 and #3381 were each filed to eliminate.
  Against the pre-fix baseline this is still a large net win (512 wrong pool
  triples before, 118 after), so the fix must be refined, not reverted.
- **Related**: #3381 (the fix this refines), #3171 (same defect class),
  CHAR-2026-08-27-D5-01.
- **Suggested Fix**: Resolve **both** chains in `derive_npc_actor_values` and
  hand each arm the record its fields actually belong to — pass
  `resolve_inherited_traits(...)` to `derive_skyrim_actor_values` for the
  `race_form_id` lookup while keeping the `Use Stats` record for
  `health_offset` / `magicka_offset` / `stamina_offset` (which *are* stats, from
  `ACBS`). A regression test should assert that a shell with `Use Stats` set and
  `Use Traits` clear keeps its own `RNAM` race, and that
  `Background.race_form_id` and the race behind the derived pools are always the
  same FormID.

---

### CHAR-2026-08-27b-D5-02: FO4's `calculated_health == 0` "absent" sentinel is not honoured across template resolution — 54 NPCs lose their authored Health

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: `fo4`
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:188`
  (the hoisted resolve) and `:228-242` (`derive_stored_actor_values`)
- **Status**: NEW — a regression introduced by `7445506c`, unaudited until now.
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:390-394` —
  *"FO4+ `DNAM` baked `Calculated Health` (u16 @ 0). … **`0` = absent** (no live
  NPC has 0 base Health, so the sentinel is unambiguous and avoids an `Option`
  discriminant)."*
- **Description**: `derive_stored_actor_values` reads
  `npc.calculated_health` / `npc.calculated_action_points` off the
  `Use Stats`-resolved record and pushes the value only `if baked > 0`. Because
  `0` means *absent*, not *zero*, a shell that authors its own baked `DNAM`
  Health but inherits from a template that authors none now yields **nothing**
  where it previously yielded the shell's authored value. Template precedence is
  correct for a field the template actually carries; an *absent* field should
  fall back down the chain, exactly the way `resolve_inherited_record` already
  falls back to the input NPC when the flag or the template is missing.
- **Evidence**:
  ```rust
  // actor_value_derive.rs:231-240
  for (avif_editor_id, baked) in [
      ("Health", npc.calculated_health),           // ← npc is the resolved template
      ("ActionPoints", npc.calculated_action_points),
  ] {
      if baked > 0 {
          if let Some(fid) = index.actor_value_form_id(avif_editor_id) {
              out.push((fid, f32::from(baked)));
  ```
  Measured on vanilla `Fallout4.esm` (3,015 `NPC_`), comparing each record's own
  `calculated_health` against the one `resolve_inherited_stats` now returns:

  | Measure | Count |
  |---|---:|
  | own `DNAM` Health > 0 but the resolved template's `== 0` (**Health lost**) | **54** |
  | own `DNAM` Health `== 0` but the resolved template's > 0 (Health gained) | 35 |
  | own `PRPS` non-empty but the resolved template's empty (PRPS lost) | **0** |
  | actors ending with no Health value at all | **349** (was **330** pre-fix, net **+19**) |

  The `PRPS` half is clean — no FO4 shell loses its property array — so the
  defect is specific to the `> 0` sentinel test on the baked `DNAM` pair.
- **Impact**: `stamp_actor_values` (`byroredux/src/npc_spawn.rs:99-112`) only
  inserts `ActorVitals` when the derived pairs contain the Health AVIF key, so
  these actors spawn with **no `ActorVitals`** and cannot be damaged or killed by
  `combat_damage_system` — the project's active P2 execution focus. 19 more FO4
  actors are in that state than before the fix.
- **Related**: #3382 (the fix this regresses out of),
  CHAR-2026-08-27-D5-02 (which measured the 330 baseline).
- **Suggested Fix**: Make the baked-`DNAM` read fall back down the chain on the
  absent sentinel — take the resolved template's value when it is `> 0`,
  otherwise the shell's own — and pin it with a test built from a shell whose
  `Use Stats` template has no `DNAM`. Consider making the same fallback explicit
  for `calculated_action_points`, which carries the identical sentinel.

---

### CHAR-2026-08-27b-D2-01: six `stealth.rs` sub-coefficient groups are supported by no capture document, and three prior reports each credit a predecessor that never verified them

- **Severity**: MEDIUM
- **Dimension**: Derived Formulas (CHARAL-adjacent siblings, CHAR-D6-05 / #2962)
- **Game**: `fnv`, `fo3`
- **Location**: `crates/core/src/stealth.rs:93-98` (`ActionSound::value`),
  `:114-118` (`ArmorClass::penalty`), `:211-238` (`detection_score`'s `Sound`
  and `Visual` terms)
- **Status**: NEW. Not a re-file: `AUDIT_CHARACTER_2026-08-15.md` filed the
  *ownership* gap (CHAR-D6-05 / #2962, since closed by adding the siblings to
  this skill's scope); no report has ever filed the *constants* gap.
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:224-258` is the whole
  "Sneak Detection (FNV)" capture. It records the top-level `Detection`,
  `TargetSkill`, `DetectorSkill` and `Attenuation` expressions — verified PASS
  above, rows 39–43 — and then says only that *"`Sound` and `Visual` fold in
  movement/weapon-noise level, light level + night-eye, and armor class
  (heavy/medium/light)"*. It states no coefficient for any of them.
- **Description**: The six coefficient groups in rows 44–49 of the table above
  exist in code with no line in any of the six `charal-*-ruleset.md` captures to
  check them against. That alone makes them `UNSOURCED` under this audit's own
  rule. What makes it worth filing rather than tabulating is the **audit trail**:
  the three most recent CHARAL reports each record the file as verified, and each
  points at a predecessor that did not verify it.
- **Evidence**: The chain, quoted from the reports themselves:
  - `docs/audits/AUDIT_CHARACTER_2026-08-15.md:2256` —
    *"Dimension 2 verified 26 constants; none of them are these."* (`stealth.rs`
    is named two lines later as having "the same status" as `combat.rs`.)
  - The 2026-08-16 report mentions `stealth.rs` only twice, both times as an
    *ownership* reference (`:201`, `:381`); it verifies nothing in it.
  - `docs/audits/AUDIT_CHARACTER_2026-08-20.md:570-572` —
    *"`crates/core/src/stealth.rs` was re-read but not re-verified line-by-line
    against `charal-fnv-fo3-ruleset.md`'s 'Sneak Detection (FNV)' section — it is
    unchanged since the **2026-08-16 sweep verified it**"*.
  - `docs/audits/AUDIT_CHARACTER_2026-08-24.md` § Verification honesty —
    *"`crates/core/src/stealth.rs` (unchanged since the **2026-08-16 sweep
    verified it**)"*.

  The module is honest about its own limits (`stealth.rs:41-50`, "## No-guessing
  caveat" — it discloses that the source gives no worked numeric example and
  that the tests are structural/monotonicity only). The gap is that the
  *capture* never recorded what the transcription was made from, so nothing in
  the repository can now falsify it.
- **Impact**: ~12 gameplay constants that this audit exists to check are
  unverifiable from the repository. No live impact today — `detection_score` and
  `classify` have zero non-test callers (the `HitEvent.sneak_attack` hook is
  hardcoded `false`) — but the deferral is exactly the condition under which the
  trail rotted unnoticed for four sweeps, and the numbers will be believed the
  day an AI/perception system consumes them.
- **Related**: #2962 (the ownership half, closed); `feedback_no_guessing`;
  the analogous still-open UNSOURCED row 35 (`SKYRIM_SKILL_USE_CURVE`).
- **Suggested Fix**: Extend `charal-fnv-fo3-ruleset.md`'s "Sneak Detection (FNV)"
  section with the `Sound` and `Visual` sub-expressions and the armour/action
  tables from the cited fandom *Sneak (Fallout: New Vegas)* page, so each
  coefficient has a line to be checked against; then correct the "verified"
  attribution in this file's Known-Open register rather than in a future report.

---

### CHAR-2026-08-27b-D4-01: flat Fatigue regen is gated behind the `CharacterRuleset` lookup that only Magicka needs

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: `oblivion` (the only game whose regen config exists)
- **Location**: `crates/core/src/character/regen.rs:174-200`
- **Status**: NEW (latent — the system is inert today; see the Coverage Matrix).
- **Source**: `regen.rs:70-73` — Fatigue's rate is
  `FATIGUE_REGEN_PER_SEC = 10.0`, *"vanilla Oblivion's Endurance coefficient
  (`fFatigueReturnMult`) is `0.0`, so this is the whole formula"*
  (`charal-oblivion-ruleset.md:386-388`). It reads no ruleset row.
- **Description**: `pool_regen_tick_system` acquires `CharacterRuleset` with a
  `let … else { return; }` **before** the actor loop, but the ruleset is used
  only inside the Magicka branch (to look up the max-Magicka row and check its
  `DerivedScope`). Fatigue's regen is a flat constant that needs no ruleset at
  all, yet a load with a `PoolRegenConfig` and no `CharacterRuleset` silently
  regenerates neither pool.
- **Evidence**:
  ```rust
  let Some(ruleset) = world.try_resource::<CharacterRuleset>() else {
      return;                       // ← Fatigue never reached
  };
  let Some(mut avs_q) = world.query_mut::<ActorValues>() else { return; };
  for (_entity, avs) in avs_q.iter_mut() {
      if avs.get(config.fatigue_avif).is_some() {
          avs.restore(config.fatigue_avif, FATIGUE_REGEN_PER_SEC * elapsed);
      }
  ```
  The three prior "silent gate" fixes in this file (#2950's two-resource gate,
  #2932's scope check, #2153's guard scope) all addressed the *documented*
  preconditions; this fourth one is undocumented — the system's own docstring
  enumerates the gates as `PoolRegenConfig` and `PoolRegenAccumulator` and does
  not mention `CharacterRuleset`.
- **Impact**: None today (`PoolRegenConfig` has no production insertion site).
  When Oblivion wiring lands, a load order that resolves the regen AVIFs but not
  a ruleset loses Fatigue regen with no log line — the same
  "indistinguishable from *no game loaded*" failure mode #2950 was filed for.
- **Related**: #2950, #2932; the concurrently-filed #2153 guard-drop finding at
  `regen.rs:153-180` (not re-filed here).
- **Suggested Fix**: Move the `CharacterRuleset` acquire inside the Magicka
  branch (or make it a `try_resource` whose `None` only disables the scoped-max
  lookup, falling back to `base_max` as the branch already does for player-only
  rows), and add `CharacterRuleset` to the docstring's gate list either way.

---

### CHAR-2026-08-27b-D6-01: `docs/feature-matrix.md` still records Skyrim NPC population as "Health only" four days after Magicka and Stamina landed

- **Severity**: LOW
- **Dimension**: Coverage, Documentation & Doctrine Drift
- **Game**: `skyrim`
- **Location**: `docs/feature-matrix.md:251` (matrix cell) and `:266-268` (prose)
- **Status**: NEW. The section itself is #2961's fix; this is fresh rot inside it.
- **Source**: `crates/plugin/src/esm/records/actor_value_derive.rs:201-220` —
  `derive_skyrim_actor_values` loops
  `[("Health", …), ("Magicka", …), ("Stamina", …)]`, landed in `1d0c5d4b`
  (2026-08-24) and verified against `Skyrim.esm` by
  `AUDIT_CHARACTER_2026-08-24.md` (rows 23–25 of its table).
- **Description**: The per-game matrix row "NPC actor-value population at spawn"
  gives Skyrim `~ Health only`, and the prose two paragraphs down repeats it:
  *"Skyrim's NPC population derives Health only (`race.starting_health +
  NPC_.ACBS.health_offset`) — no skills or other actor values."* Both are stale:
  Magicka and Stamina are each resolved independently from their own
  `RACE.DATA` starting value plus their own signed `ACBS` offset, and the AVIF
  FormIDs (`AVMagicka 0x3E9`, `AVStamina 0x3EA`) were confirmed against the
  shipped master.
- **Evidence**: `feature-matrix.md:251`
  `| NPC actor-value population at spawn | ✗ | ✓ class auto-calc | ✓ class auto-calc | ~ Health only | … |`
  versus the three-element array at `actor_value_derive.rs:206-210`.
  `git log --oneline -1 -- crates/plugin/src/esm/records/actor_value_derive.rs`
  confirms the file has changed twice since (`9e44a0dd`, `7445506c`) with no
  matching matrix update.
- **Impact**: The document `_audit-common.md` designates as the living "what
  works at runtime per game" reference under-reports shipped Skyrim capability
  by two of three pools, in the row a milestone-planning read would land on. Two
  full audits verified the code and neither cross-checked the matrix — the
  precise cross-check this dimension exists for.
- **Related**: #2961 (created the section), #3219 (the parse half).
- **Suggested Fix**: Update the cell to `~ Health/Magicka/Stamina` and rewrite the
  paragraph to state the per-pool independent resolution (a race missing
  `starting_magicka` degrades one pool, not the NPC).

---

### CHAR-2026-08-27b-D6-02: this skill's Dimension 2 checklist pins `DerivedStatFormula` at 32 B; it has been 36 B since the `#2939` floor field landed

- **Severity**: LOW
- **Dimension**: Coverage, Documentation & Doctrine Drift
- **Game**: `all`
- **Location**: `.claude/commands/audit-character/SKILL.md:188`
- **Status**: NEW. Same class as CHAR-2026-08-24-D6-01 / #3271 (a stale fact in
  the skill file that directs the audit rather than in the code audited), one
  dimension over.
- **Source**: `crates/core/src/character/derived.rs:23` —
  *"[`DerivedStatFormula`] is `Copy` and 36 bytes"*; pinned by
  `formula_is_thirty_six_bytes_and_copy` (`derived.rs:340-345`,
  `assert_eq!(std::mem::size_of::<DerivedStatFormula>(), 36)`).
- **Description**: Dimension 2's checklist instructs the auditor to verify that
  `DerivedStatFormula` *"is still `Copy` + 32 B"*. It is 36 B, and has been since
  `clamped_below`'s `floor: f32` and the `base_reads: u8` bitfield were added.
  An auditor following the checklist literally would report a **false positive**
  against a struct-size contract the code already pins with a live test — and
  `_audit-common.md`'s own symbol-advisory rule exists precisely because
  *"`GpuMaterial` still being documented at 300 B after it grew to 348 B … is a
  wrong number in a GPU layout contract, not a typo."* The same standard applies
  to a `Copy` formula struct held by the thousand in a flat `Vec`.
- **Evidence**: `grep -n "32 B" .claude/commands/audit-character/SKILL.md` → line
  188; `grep -n "36 bytes\|size_of::<DerivedStatFormula>" crates/core/src/character/derived.rs`
  → lines 23, 179, 340, 345.
- **Impact**: Bounded — one checklist line that manufactures a false finding.
  The path/symbol validate gate cannot catch it (`32 B` is neither a path nor a
  backticked symbol).
- **Related**: #3271, #3236, #3143 (the same skill-file-drift family).
- **Suggested Fix**: Change `32 B` to `36 B` and cite
  `formula_is_thirty_six_bytes_and_copy` by name so the next size change fails
  the symbol advisory instead of silently re-rotting.

---

### CHAR-2026-08-27b-D6-03: the `template_flags` documentation is scoped to "FNV / FO3" and cites one offset, but the bits now gate Skyrim and FO4 actor-value population at three different offsets

- **Severity**: LOW
- **Dimension**: Coverage, Documentation & Doctrine Drift
- **Game**: `skyrim`, `fo4`
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:357-375`
  (`NpcRecord::template_flags`'s doc comment) and
  `crates/plugin/src/equip.rs:241-249` (`TEMPLATE_FLAG_*`)
- **Status**: NEW — created by `7445506c` widening the consumers without
  widening the provenance.
- **Source**: the three ACBS arms in the same file —
  FO4 `:919-927` (`template_flags` at byte **14**), Skyrim `:939-949` (byte
  **18**), FNV/FO3 `:953-975` (byte **22**).
- **Description**: Two doc comments scope the template-flag bits to FNV/FO3:
  `equip.rs:241` — *"**FNV / FO3** `NpcRecord::template_flags` bits. Sourced from
  xEdit `wbDefinitionsFNV.pas`"* — and `actor/mod.rs:357-358` — *"**FNV / FO3**
  template-inheritance bitmask from `ACBS` (u16 **at offset 22**)"*. Since
  `7445506c` those same three constants gate `derive_npc_actor_values` for
  **every** game, so `0x0002` now decides the stats of 3,182 Skyrim `NPC_`
  records and the `PRPS`/`DNAM` of the FO4 corpus. The stated offset is right for
  exactly one of the three families the parser handles, and the cited authority
  (`wbDefinitionsFNV.pas`) covers only that one.
- **Evidence**: `grep -n "template_flags" crates/plugin/src/esm/records/actor/mod.rs`
  → assignments at `:927` (FO4), `:948` (Skyrim), `:974` (FNV/FO3), each behind a
  distinct `SubReader` cursor; the doc comment at `:358` names only offset 22.
  `actor_value_derive.rs:188` is the single call site that now routes all three.
- **Impact**: Documentation only — the *parse* is per-game correct (three arms,
  three offsets, each with its own layout comment), and the TES5/FO4 bit
  meanings for `0x0001`/`0x0002`/`0x0100` do in fact match FNV's. But the one
  place a reader goes to learn what these bits mean asserts a game scope and a
  byte offset that are wrong for two of the three families now depending on
  them, and names no source for those two.
- **Related**: #2956 (introduced the constants), #3381 / #3382 (widened the
  consumers), D5-01 above (the substantive consequence of the bits' semantics
  being under-documented).
- **Suggested Fix**: Re-scope both doc comments to "FNV / FO3 / Skyrim / FO4",
  drop the single offset in favour of pointing at the three ACBS arms, and add
  the `wbDefinitionsFO4.pas` / TES5 xEdit citation the FO4 and Skyrim arms
  already carry for their own layouts.

---

## Prior-Finding Register — reconciled, not re-filed

### From `docs/audits/AUDIT_CHARACTER_2026-08-27.md` (the earlier same-day Dimension-5 run)

| Finding | Issue | State at HEAD `969d81c8` |
|---|---|---|
| D5-01 Skyrim arm reads the shell `NPC_`, not the `TPLT` source | #3381 | **CLOSED, fixed by `7445506c`** — and measurably so (512 → 118 wrong pool triples). Refined, not re-filed, by D5-01 above |
| D5-02 FO4 stored arm has the same un-resolved-template gap | #3382 | **CLOSED, fixed by `7445506c`**. The fix's sentinel regression is D5-02 above |
| D5-03 FO3/FNV creatures receive zero `ActorValues` | #3383 → **#3390 OPEN** | Still unfixed by design — #3390 is explicitly "needs a sourced `CREA` `DATA` layout". Confirmed still absent and still not guessed at; the deferral note at `actor_value_derive.rs:168-176` is intact |
| D5-04 `EsmIndex::merge_from` adopts `character_rules` unconditionally | #3384 | **CLOSED and verified fixed** — `index.rs:868-878` now keeps the first non-`NONE` profile and logs on conflict |

### From `docs/audits/AUDIT_CHARACTER_2026-08-24.md`

| Finding | Issue | State |
|---|---|---|
| D6-01 this skill's Scope never names `profile.rs` | #3271 | **CLOSED and verified fixed** — `SKILL.md`'s Scope block and Dimensions 1/3/5 all name `crates/core/src/character/profile.rs` now |
| #3169 `SkillSet::SKYRIM` spells Illusion "Illusion" | #3169 | **CLOSED, fixed** — `skill.rs:148` is `SkillDef::ungoverned("Mysticism")`, with `SkillSet::SKYRIM.get("Illusion").is_none()` asserted |
| #3170 / #3171 / #3172 | all CLOSED | Re-confirmed: `with_gmst` requests exactly the two authored GMSTs; `effective_actor_level` has one definition; `ROSTER_CASES` falsifies rosters against five masters |

### Filed by concurrent audits this session — referenced, deliberately not re-filed

- The `ActorValues` ↔ `CharacterRuleset` **lock-order cycle**
  (`crates/scripting/src/condition.rs:470-509` vs
  `crates/core/src/character/regen.rs:176-180`).
- **#2153's inert guard-drop** at `crates/core/src/character/regen.rs:153-180`,
  where `let config = *config;` shadows the resource guard without dropping it.
  D4-01 above is a *different* defect at an adjacent line (an over-broad
  early-return, not a lock hold) and does not overlap.

### Partially-remediated older finding — status update only

`AUDIT_CHARACTER_2026-08-15.md:969` filed the circular sourcing of the
Skyrim/Oblivion leveling constants ("the document that verifies the code was
written from the code"). Most of it has since been fixed:
`charal-skyrim-ruleset.md:711-720` now carries a real *"## XP / level curve —
LOCKED"* section sourcing `fXPLevelUpBase`/`fXPLevelUpMult`, and
`charal-oblivion-ruleset.md:784-791` carries the leveling section with the
`+1..5` bands. **The remainder is `SKYRIM_SKILL_USE_CURVE = 1.95` and the
`mult · level^curve + offset` skill-XP cost shape**, still recorded only at
`charal.md:147` — an implementation summary, not a per-game capture. Recorded
here as an open remainder (table row 35) rather than re-filed as new.

---

## Known-Open Register — confirmed not re-filed

1. **FNV/FO3 tag-skill per-level formula** — still deliberately absent. The
   deferral note at `actor_value_derive.rs:39-47` is intact and no per-level term
   has been fabricated. CLAS SPECIAL is read from `ATTR`
   (`class.base_attributes`), not `DATA`. Not reported.
2. **FO3 ↔ FNV divergent *player* Health/AP** — still deferred with the player
   actor. The FO3/FNV split now lives in `CharacterRulesProfile`
   (`FALLOUT3` / `FALLOUT_NEW_VEGAS`, distinct `NpcHealthCurve` and distinct
   `RulesetBuilder`), selected by `character_rules_profile`'s `hedr_version < 1.0`
   test. Not reported.
3. **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance roll) —
   still does not exist; only the AP *formulas* are in CHARAL. Not reported.
4. **Skyrim `xp_per_skill_rank` is engine-owned, not GMST-authored** — the
   settled 2026-08-24 design. Verified intact (table rows 30–31) and explicitly
   **not** re-flagged as an authoring gap.

---

## Cross-Audit Dedup

| Area | Owner | Note |
|---|---|---|
| `NpcRecord` / `ACBS` / `PRPS` / `DNAM` / `RACE.DATA` sub-record decoding | `/audit-esm` Dim 4 | D5-01 and D5-02 are about which *record* CHARAL reads, not how the bytes decode; the decode itself verified correct |
| `resolve_inherited_stats` / `_traits` / `_inventory` as an equip mechanism | `/audit-skyrim`, `/audit-fo4` | D5-01 concerns only the CHARAL consumer's choice of chain |
| Component storage / shape (`ActorValues`, `Perks`, `AfflictionStatus`) | `/audit-ecs` | Not re-examined here |
| Scheduler access declarations | `/audit-concurrency` Dim 4 | `pool_regen_tick_system`'s declaration (`boot.rs:993-1000`) was checked against what the system touches and is **complete and accurate** — all four types declared |
| CTDA condition evaluation of derived stats | `/audit-scripting` | `condition.rs`'s `DerivedScope` enforcement noted as the correct sibling precedent, not audited |
| P2 melee slice as a CHARAL consumer | un-owned (`_audit-common.md` gap list) | #3092's `MeleeDamage` bonus wiring verified present at `byroredux/src/combat.rs:349`; D5-02's impact statement depends on it |

---

## Verification Honesty

- **Verified against shipped game data this session** (three temporary probes
  built on the live parser, run, then deleted): `Skyrim.esm` `NPC_` ×5,118
  (`TPLT` counts, `Use Stats` bit, stats-chain vs traits-chain race, `RACE.DATA`
  pool triples); `Fallout4.esm` `NPC_` ×3,015 (`DNAM` Health/`PRPS` retention
  across template resolution); `FalloutNV.esm` ×3,816 and `Fallout3.esm` ×1,647
  (class resolution across template resolution — **0 change**, the auto-calc arm
  is unaffected by `7445506c`).
- **Verified against capture documents**: every row of the 49-row table above,
  re-derived from the documents this session rather than carried forward. The
  documents were read before the corresponding code, per Phase 1 item 6.
- **Not verified**: FO76 and Starfield numbers (still no ruleset builder — both
  are `NpcStatModel::Stored` + `RulesetBuilder::None`); Oblivion's pre-`AVIF`
  legacy actor-value index resolution (`Oblivion.esm` authors no `AVIF` group and
  no resolver exists); the `AfflictionTable` threshold numbers (none are sourced
  for any game, and every table ships empty).
- **Not re-derived**: the deferred FNV/FO3 tag-skill per-level formula.
- **Nothing was launched.** No `byroredux` process was started; no GitHub issues
  were created.

---

**7 findings — 0 CRITICAL · 0 HIGH · 3 MEDIUM · 4 LOW.** All NEW; none duplicates
an OPEN issue or a finding in either `AUDIT_CHARACTER_2026-08-24.md` or
`AUDIT_CHARACTER_2026-08-27.md`.

Suggested next step:

```
/audit-publish docs/audits/AUDIT_CHARACTER_2026-08-27b.md
```

(domain label `character`; add `esm-plugin` for D5-01 / D5-02 / D6-03,
`doc-rot` for D6-01 / D6-02 / D6-03, and the matching `game:*` —
`game:skyrim` for D5-01, `game:fo4` for D5-02, `game:fnv` + `game:fo3` for
D2-01.)
