# Character / CHARAL Audit — 2026-08-20

**Scope**: `/audit-character` — all 6 dimensions, all implemented families, `--depth deep`.
Run as part of the `comprehensive` audit-suite sweep (25 audits).

**Repo state**: HEAD `bb0b92f2`, branch `main`. Delta since the last sweep: **335 commits**.
**Prior report**: `docs/audits/AUDIT_CHARACTER_2026-08-16.md` (4 MEDIUM, filed #3092–#3095).
**Tests**: not run — the suite briefing forbids `cargo` (target-lock contention). All
verification below is static analysis plus **direct binary probes of four shipped masters**.

| Dimension | Area | Findings |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | 0 |
| 2 | Derived-Stat Formulas | **1 MEDIUM · 1 LOW** |
| 3 | Leveling & Progression | **1 MEDIUM** |
| 4 | Pools, Afflictions, Resistances & Reputation | 0 |
| 5 | Population Boundary | **1 MEDIUM** |
| 6 | Coverage, Documentation & Doctrine Drift | **1 MEDIUM** (+ 4 carried OPEN) |
| **Total** | | **0 CRITICAL · 0 HIGH · 4 MEDIUM · 1 LOW** |

---

## Executive Summary

### The delta question: is "CHARAL wiring gap closed" genuine?

**Yes, genuinely — with one roster still wrong and the test that would have caught it
still absent.**

The session-70 closeout claim rests on `b434e4c0` (*centralize actor value profiles*),
which is a real structural fix, not a label:

1. **`CharacterRulesProfile`** (`crates/core/src/character/profile.rs`, new) makes the
   per-game character policy a **data row selected once at the parser boundary**
   (`character_rules_profile(game, hedr_version)`, `records/mod.rs:143-194`) and stored on
   `EsmIndex::character_rules`. Every downstream consumer matches on
   `NpcStatModel` — a data enum — not on `GameKind`. Doctrine-clean.
2. **The `AV` prefix defect is fixed.** `EsmIndex::actor_value_form_id`
   (`records/index.rs:568-606`) now falls back to the `AV`-prefixed spelling. I re-probed
   all four masters: **every** EditorID the FO3, FNV and FO4 builders and rosters ask for
   now resolves (see the Constant Verification Table). The 2026-08-16 matrix's
   "0 live derived rows on FNV" is obsolete.
3. **`SkillSet::FALLOUT_FO3_FNV` is split** into `FALLOUT3` / `FALLOUT_NV`
   (`skill.rs:150-190`), keyed on *record* identity: FNV now carries `SmallGuns`
   (display "Guns") and `Throwing` (display "Survival") and **drops the obsolete
   `BigGuns`**. Verified against `FalloutNV.esm`: all 13 resolve, `AVBigGuns` /
   `Big Guns - OBSOLETE` is correctly excluded. #3094 is properly closed.
4. **FO4's phantom `MeleeDamage` row is gone**; the multiplier is documented as a
   combat-use formula instead (#3093). Confirmed: `Fallout4.esm` authors no
   `MeleeDamage` / `AVMeleeDamage` AVIF at all.
5. **Health now reaches an actor on three families.** FO3/FNV via the profile's
   `NpcHealthCurve`, Skyrim via `RACE.starting_health + ACBS.health_offset`, FO4 via
   baked `DNAM`; `stamp_actor_values` writes `ActorVitals` whenever the pairs contain the
   Health key, which is what makes NPCs damageable. The old
   `SKYRIM_HEALTH_ACTOR_VALUE = 24` enum-space hack is deleted — and it was a false
   premise: `Skyrim.esm` **does** author `AVHealth` at `0x000003E8`, which I confirmed
   directly.
6. **`MeleeDamageConfig`** (#3092) gives the combat consumer a real CHARAL read.
   `AVMeleeDamage` exists on both FO3 (`0x451`) and FNV (`0x451`), so the row is live.

So: wired *and* reaching real data, on FO3, FNV and FO4. The claim holds.

### What it did not close

- **Skyrim's skill roster still has one key that cannot resolve.** `SkillSet::SKYRIM`
  spells the Illusion skill `"Illusion"`; vanilla `Skyrim.esm` authors that AVIF with the
  legacy EditorID **`AVMysticism`** (`0x0000045B`). 17 of 18 resolve; Illusion does not.
  This is the *same class* as the `Guns`/`Survival` defect just fixed — a Bethesda
  display-name rename that left the record identity alone — and it is the third such
  retention in the Skyrim roster, alongside the two (`Marksman`, `Speechcraft`) the
  docstring already documents. CHAR-2026-08-20-D2-01.
- **The real-data existence test added for #3095 covers FNV only.** It loops
  `SkillSet::FALLOUT_NV` and asserts each resolves. Nothing loops `SkillSet::SKYRIM`,
  `SkillSet::FALLOUT3`, or any builder's *derived-row output keys* on any game. That is
  exactly the hole the Skyrim key fell through. The meta-finding the coordinator flagged
  — "fixtures build resolvers from the roster's own strings, so they cannot falsify the
  roster" — is **half-closed**: falsifiable for one roster, still unfalsifiable for four.
  CHAR-2026-08-20-D6-01.
- **The GMST seam has zero production reach.** #2942 was closed by
  `LevelingModel::with_gmst`, which handles only the `SkillXp` variant — i.e. Skyrim —
  and `CharacterRulesProfile::SKYRIM.ruleset` is `RulesetBuilder::None`, so that variant
  is never constructed. `build_character_ruleset`'s `gmst` closure
  (`index.game_setting_float`) is therefore called zero times on every wired game.
  Meanwhile FO3/FNV/FO4 hardcode constants the masters author verbatim.
  CHAR-2026-08-20-D3-01.
- **`effective_actor_level` has a third copy, with the divergence #3081 declared wrong.**
  `b434e4c0` introduced `effective_npc_level` in `actor_value_derive.rs` on Aug 17;
  `17b94d2e` (Aug 19) deleted `inventory.rs`'s copy for #3081 and stated in its own commit
  body that `.max(0)` is "the deliberate, tested answer". The surviving copy uses
  `.max(1)`. CHAR-2026-08-20-D5-01.

### Verification honesty

- **Verified against shipped game data this session** (independent parser, not the
  repo's): the complete `AVIF` EditorID space of `FalloutNV.esm` (64 records),
  `Fallout3.esm` (60), `Skyrim.esm` (149), `Fallout4.esm` (359); the `fAVD*` GMST family
  on FNV / FO3 / FO4; the `ACBS` level/flag distribution over 3,816 FNV `NPC_` records.
- **Verified against capture documents**: FO3, FNV, FO4 derived rows; the Oblivion
  combat-damage siblings (`crates/core/src/combat.rs`).
- **NOT verified**: FO76 and Starfield (still no builder — `CharacterRulesProfile` gives
  them `NpcStatModel::Stored` and `RulesetBuilder::None`, which is at least now explicit);
  Oblivion's pre-`AVIF` legacy-index resolution (still no resolver);
  `crates/core/src/stealth.rs` (unchanged since the last sweep, which verified it).
- **Not re-derived**: the deferred FNV/FO3 tag-skill per-level formula. Confirmed still
  absent and still not fabricated.

---

## Constant Verification Table

Rows marked **game-data** were checked this session against the shipped master with an
independent binary parser (`/tmp/audit/character/avif.py`, `gmst.py`), not against a
fixture and not against the repo's own reader.

| # | Constant / lookup | Code value | Authoritative value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FNV Health curve | `95 + 20·END + 5·L` (`profile.rs:88-95`) | `fAVDHealthEnduranceMult=20.0`, `fAVDHealthLevelMult=5.0`; doc `100 + END·20 + (Level−1)·5` | game-data (`FalloutNV.esm` GMST) + `charal-fnv-fo3-ruleset.md:93` | **PASS** |
| 2 | FO3 Health curve | `90 + 20·END + 10·L` (`profile.rs:75-82`) | `fAVDHealthEnduranceMult=20.0`, `fAVDHealthLevelMult=10.0` | game-data (`Fallout3.esm` GMST) | **PASS** |
| 3 | FNV Action Points | `65 + 3·AGI`, cap 95 (`fallout.rs:200-206`) | `fAVDActionPointsBase=65.0`, `fAVDActionPointsMult=3.0` | game-data (`FalloutNV.esm` GMST) | **PASS** (cap unsourced, pre-existing) |
| 4 | FO3 Action Points | `65 + 2·AGI`, cap 85 (`fallout.rs:163-170`) | `fAVDActionPointsBase=65.0`, `fAVDActionPointsMult=2.0` | game-data (`Fallout3.esm` GMST) | **PASS** |
| 5 | FO3/FNV Carry Weight | `150 + 10·STR` (`fallout.rs:48-51`) | `fAVDCarryWeightsBase=150.0`, `fAVDCarryWeightMult=10.0` | game-data (both masters) | **PASS** |
| 6 | FO4 Action Points | `60 + 10·AGI` (`fallout.rs:139-144`) | `fAVDActionPointsBase=60.0`, `fAVDActionPointsMult=10.0` | game-data (`Fallout4.esm` GMST) | **PASS** |
| 7 | FO4 Carry Weight | `200 + 10·STR` (`fallout.rs:146-149`) | `fAVDCarryWeightBase=200.0`, `fAVDCarryWeightMult=10.0` | game-data (`Fallout4.esm` GMST) | **PASS** |
| 8 | FO4 Health | `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` | `charal-fo4-ruleset.md` (locked) | capture doc | **PASS** (the one cross-term row, as documented) |
| 9 | FNV skill roster (13) | `SkillSet::FALLOUT_NV` | all 13 resolve: `AVBarter 4B0`, `AVEnergyWeapons 4B2`, `AVExplosives 4B3`, `AVLockpick 4B4`, `AVMedicine 4B5`, `AVMeleeWeapons 4B6`, `AVRepair 4B7`, `AVScience 4B8`, `AVSmallGuns 4B9` ("Guns"), `AVSneak 4BA`, `AVSpeech 4BB`, `AVThrowing 4BC` ("Survival"), `AVUnarmed 4BD` | game-data | **PASS** — #3094 closed correctly |
| 10 | FNV `BigGuns` exclusion | absent from roster | `AVBigGuns 4B1` = FULL `"Big Guns - OBSOLETE"` | game-data | **PASS** |
| 11 | FO3 skill roster (13) | `SkillSet::FALLOUT3` | all 13 resolve, incl. `AVBigGuns 4B1` (FULL `"Big Guns"`, not obsolete on FO3) and `AVSmallGuns 4B9` (FULL `"Small Guns"`) | game-data | **PASS** |
| 12 | **Skyrim skill roster (18)** | `SkillSet::SKYRIM` incl. `"Illusion"` | 17 resolve; **no `Illusion` / `AVIllusion` AVIF exists**. The 18 skill AVIFs are contiguous `0x44C..0x45D`, and the Illusion slot is `0x45B` = **`AVMysticism`** | game-data (`Skyrim.esm`) | **FAIL** → CHAR-2026-08-20-D2-01 |
| 13 | Skyrim derived-row keys | `DamageResist`, `LightArmor`, `CarryWeight`, `Stamina` | `AVDamageResist 5CE`, `AVLightArmor 452`, `AVCarryWeight 3F0`, `AVStamina 3EA` | game-data | **PASS** (but unreachable — no builder) |
| 14 | Skyrim Health key | `actor_value_form_id("Health")` | `AVHealth 0x000003E8` | game-data | **PASS** — the removed `SKYRIM_HEALTH_ACTOR_VALUE=24` premise was false |
| 15 | FNV/FO3 derived-row output keys | `CarryWeight`, `MeleeDamage`, `CritChance`, `UnarmedDamage`, `RadResist`, `PoisonResist` | `AVCarryWeight 44D`, `AVMeleeDamage 451`, `AVCritChance 44E`, `AVUnarmedDamage 5E6`, `AVRadResist 454`, `AVPoisonResist 453` — all present on **both** masters | game-data | **PASS** (untested in-repo — CHAR-2026-08-20-D6-01) |
| 16 | FO4 `MeleeDamage` absence | row deleted (#3093) | no AVIF matching `MeleeDamage` or `AVMeleeDamage` in `Fallout4.esm` | game-data | **PASS** — fix confirmed against data |
| 17 | Skill auto-calc | `2 + 2·gov + ceil(0.5·Luck)` | `fAVDSkill<Name>Base = 2.0` × 13 (per-skill, all 2.0 in vanilla). **`fAVDSkillPrimaryBonusMult` and `fAVDSkillLuckBonusMult` do not exist in `FalloutNV.esm` or `Fallout3.esm`** | game-data + geckwiki via `actor_value_derive.rs:24-28` | **PASS on value, UNSOURCED on GMST name** → CHAR-2026-08-20-D2-02 |
| 18 | Oblivion `modified_skill` | `Skill + 0.4·(Luck−50)` | `charal-oblivion-ruleset.md:282` | capture doc | **PASS** |
| 19 | Oblivion weapon-damage mult | `0.5·(0.75+0.005·A)·(0.2+0.015·S)` | `charal-oblivion-ruleset.md:205, 226-230` (`fDamageWeaponMult=0.5`, `fDamageStrengthBase=0.75`, `fDamageStrengthMult=0.5`, `fDamageSkillBase=0.2`, `fDamageSkillMult=1.5`) | capture doc | **PASS** |
| 20 | Oblivion hand-to-hand | `1 + 10.5·(STR/100)·(MS/100)`; fatigue `1 + 0.5·health`; **no** `[0,100]` clamp | `charal-oblivion-ruleset.md:298-317` (pure cross-term, clamp explicitly not stated for H2H) | capture doc | **PASS** |
| 21 | Skyrim leveling GMSTs | `fXPLevelUpBase` / `fXPLevelUpMult` / `fXPPerSkillRank` via `with_gmst` | correct names, **but the code path never runs in production** (`RulesetBuilder::None`) | code trace | **PASS on value, DEAD on reach** → CHAR-2026-08-20-D3-01 |
| 22 | FNV `NPC_` level distribution | — | 3,816 `NPC_` with `ACBS`; 268 carry `PC Level Mult`; **30** are non-mult with `level ≤ 0` | game-data | context for CHAR-2026-08-20-D5-01 |

---

## Coverage Matrix

| Game | Capture doc | Profile row | Ruleset builder | Ruleset **wired** | Derived rows in code | **Rows resolving on vanilla data** | NPC stat model | Leveling model | Regen wired | Affliction wired |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Oblivion** | ✓ | `OBLIVION` | `oblivion_ruleset` | ✗ (`RulesetBuilder::None`) | 8 (5 stats) | n/a — pre-`AVIF`, no legacy-index resolver | `None` | `OBLIVION` | builder exists, no caller | ✗ |
| **FO3** | ✓ | `FALLOUT3` | `fallout3_ruleset` | ✓ (HEDR < 1.0) | 8 | **8** ✓ | `ClassAutoCalc` (90/20/10) | `FO3` ✓ distinct | ✗ | ✗ |
| **FNV** | ✓ | `FALLOUT_NEW_VEGAS` | `falloutnv_ruleset` | ✓ | 8 | **8** ✓ | `ClassAutoCalc` (95/20/5) | `FNV` ✓ | ✗ | ✗ |
| **Skyrim SE** | ✓ | `SKYRIM` | `skyrim_ruleset` | ✗ (`RulesetBuilder::None`) | 2 | 2 would resolve (unreachable) | `RaceBaseOffsets` ✓ Health lands | `SKYRIM` (unreachable) | ✗ | ✗ |
| **FO4** | ✓ | `FALLOUT4` | `fallout4_ruleset` | ✓ | 3 | **3** ✓ | `Stored` (`PRPS` + `DNAM`) | `FO4` | ✗ | ✗ |
| **FO76** | ✓ | `FALLOUT76` | ✗ | ✗ | — | — | `Stored` | ✗ | ✗ | ✗ |
| **Starfield** | ✓ | `STARFIELD` | ✗ | ✗ | — | — | `Stored` | ✗ | ✗ | ✗ |

**Reading vs. 2026-08-16.** That report's honest number was "one game (FO4) with three
live derived rows". At HEAD it is **three games with 19 live derived rows** (FO3 8, FNV 8,
FO4 3), plus Health reaching an actor on FO3/FNV/Skyrim/FO4. That is the wiring gap, and
it is genuinely closed. Skyrim's *ruleset* remains unbuilt — but its NPC Health population
now works, which is the half that makes actors damageable.

`SkillSet::SKYRIM` is currently **unreachable in production** (`profile.skills()` is read
only on the `ClassAutoCalc` path, and Skyrim is `RaceBaseOffsets`), which is why
CHAR-2026-08-20-D2-01 is MEDIUM and not HIGH.

---

## Findings

### CHAR-2026-08-20-D2-01: `SkillSet::SKYRIM` spells Illusion `"Illusion"`; vanilla `Skyrim.esm` authors it as `AVMysticism`

- **Severity**: MEDIUM
- **Dimension**: Derived Formulas (roster identity)
- **Game**: skyrim
- **Location**: `crates/core/src/character/skill.rs:118-147` — the `SKYRIM` roster,
  specifically the `SkillDef::ungoverned("Illusion")` entry and the docstring at
  `:118-126` that enumerates the known EditorID/display-name divergences
- **Status**: NEW
- **Source**: vanilla `Skyrim.esm` `AVIF` group, read with an independent binary parser.
  The 18 Skyrim skills occupy a contiguous FormID block `0x0000044C..0x0000045D`:

  ```
  0000044C AVOneHanded    00000450 AVSmithing     00000454 AVLockpicking  00000458 AVAlteration
  0000044D AVTwoHanded    00000451 AVHeavyArmor   00000455 AVSneak        00000459 AVConjuration
  0000044E AVMarksman     00000452 AVLightArmor   00000456 AVAlchemy      0000045A AVDestruction
  0000044F AVBlock        00000453 AVPickpocket   00000457 AVSpeechcraft  0000045B AVMysticism
                                                                          0000045C AVRestoration
                                                                          0000045D AVEnchanting
  ```

  Exactly 18 records for exactly 18 skills. The only EditorID in that block that is not a
  Skyrim skill name is `AVMysticism` — Oblivion's retired school — occupying the slot
  between Destruction and Restoration where Illusion belongs. A search of the whole master
  for any `AVIF` whose EditorID contains `Illusion` returns only `AVIllusionMod`
  (`0x616`), `AVIllusionSkillAdvance` (`0x628`) and `AVIllusionPowerMod` (`0x63D`) — the
  three *modifier* actor values, never the skill itself.
- **Description**: Skyrim retained legacy `AVIF` record identities under new display
  names in at least three places. The roster docstring already documents two of them —
  "Archery = `Marksman`, Speech = `Speechcraft`" — and gets both right. It missed the
  third: Illusion reuses the `AVMysticism` record. Because `actor_value_form_id` now
  normalizes only the `AV` **prefix**, `resolve("Illusion")` tries `Illusion` then
  `AVIllusion`, and both miss.

  This is structurally the identical defect to CHAR-2026-08-16-D2-02 (#3094) — a display
  name written where the record identity was needed — one game over, and it survived the
  same commit that fixed FNV's.
- **Evidence**: `/tmp/audit/character/sky_avif.txt`, 149 `AVIF` records extracted from
  `Skyrim.esm`. `grep -i illusion` → three `*Mod` / `*SkillAdvance` records, no skill.
  `crates/core/src/character/skill.rs:135` reads `SkillDef::ungoverned("Illusion")`.
- **Impact**: Latent today and bounded — `SkillSet::SKYRIM` has no production reader
  (`CharacterRulesProfile::SKYRIM` uses `NpcStatModel::RaceBaseOffsets`, and
  `profile.skills()` is consumed only on the `ClassAutoCalc` branch;
  `skyrim_ruleset` has no construction site). It becomes live the moment Skyrim gets a
  `RulesetBuilder` arm or a skill-XP progression runtime, at which point Skyrim's
  `CharacterRuleset` carries 17 of 18 skills and every Illusion-gated condition, perk
  requirement and skill-XP feed silently reads the absent-AV default `0.0`. Filing it now
  is cheaper than debugging one missing magic school later.
- **Related**: #3094 (CLOSED — same defect class on FNV); CHAR-2026-08-20-D6-01 (the test
  that would have caught this covers FNV only); ESM-2026-08-16-D7-01 / #2986 (the `AV`
  prefix normalization, which this survives).
- **Suggested Fix**: Change the entry to `SkillDef::ungoverned("Mysticism")` and extend
  the roster docstring's rename list from two entries to three
  (`Archery = Marksman`, `Speech = Speechcraft`, `Illusion = Mysticism`). Then extend the
  `#[ignore]`d real-data test (`crates/plugin/tests/parse_real_esm.rs:177-193`) to loop
  `SkillSet::SKYRIM` the way its FNV sibling loops `SkillSet::FALLOUT_NV`, so the fix is
  pinned by data rather than by another hand-written string.

---

### CHAR-2026-08-20-D3-01: #2942's GMST-sourcing seam has zero production reach — `with_gmst` handles only the one `LevelingModel` variant that is never constructed

- **Severity**: MEDIUM
- **Dimension**: Leveling & Progression
- **Game**: all
- **Location**: `crates/core/src/character/leveling.rs:81-98` (`with_gmst`);
  `crates/core/src/character/profile.rs:139-155` (`build_ruleset`, the only caller);
  `byroredux/src/npc_spawn.rs:223-229` (`build_character_ruleset`, which builds the
  `gmst` closure)
- **Status**: NEW (Related: #2942, CLOSED)
- **Source**: `docs/engine/charal-oblivion-ruleset.md:361-372` — *"All future formula rows
  built for Oblivion should read these by name once GMST parsing lands (CHARAL §8 item 6),
  not re-hardcode the numeric constants captured here."* GMST parsing **has** landed
  (`EsmIndex::game_setting_float`).
- **Description**: #2942 ("every leveling constant is hardcoded, shadowing ~2039
  parsed-but-unreadable GMSTs") was closed by `1c9b8d7a`, which added:

  ```rust
  pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
      match self {
          Self::SkillXp { .. } => Self::SkillXp {
              xp_base: gmst("fXPLevelUpBase").unwrap_or(xp_base),
              xp_mult: gmst("fXPLevelUpMult").unwrap_or(xp_mult),
              xp_per_skill_rank: gmst("fXPPerSkillRank").unwrap_or(xp_per_skill_rank),
              ..
          },
          other => other,          // ← XpCurve (FO3/FNV/FO4) and SkillUse (Oblivion)
      }
  }
  ```

  `SkillXp` is Skyrim's variant and Skyrim's alone. `CharacterRulesProfile::SKYRIM` sets
  `ruleset: RulesetBuilder::None`, and `build_ruleset` returns `None` **before** reaching
  the `with_gmst` line for that arm. The three arms that do reach it — `Fallout3`,
  `FalloutNewVegas`, `Fallout4` — all carry `XpCurve`, which falls straight through
  `other => other`.

  Net effect at HEAD: `index.game_setting_float` is invoked **zero times** by CHARAL on
  every game that actually loads a ruleset. The fix executes only inside
  `leveling.rs`'s own unit test (`:322-331`).
- **Evidence**: `grep -rn "with_gmst"` returns three sites: the definition, the single
  call in `profile.rs:153`, and the unit test. `RulesetBuilder` (`profile.rs:40-46`) has
  four variants, no `Skyrim`. `profile.rs:147-152` returns `None` for
  `RulesetBuilder::None` before `ruleset.leveling.with_gmst(gmst)` is reached.

  The shadowing is not hypothetical. I read these straight out of the shipped masters:

  | GMST | FO3 | FNV | FO4 | CHARAL hardcodes |
  |---|---|---|---|---|
  | `fAVDActionPointsBase` | 65.0 | 65.0 | 60.0 | `65.0` / `65.0` / `60.0` |
  | `fAVDActionPointsMult` | 2.0 | 3.0 | 10.0 | `2.0` / `3.0` / `10.0` |
  | `fAVDCarryWeightsBase` | 150.0 | 150.0 | 200.0 (`fAVDCarryWeightBase`) | `150.0` / `150.0` / `200.0` |
  | `fAVDCarryWeightMult` | 10.0 | 10.0 | 10.0 | `10.0` |
  | `fAVDHealthEnduranceMult` | 20.0 | 20.0 | — | `20.0` |
  | `fAVDHealthLevelMult` | 10.0 | 5.0 | — | `10.0` / `5.0` |

  Every hardcoded value is *correct* for vanilla — which is the good news and also why
  nothing fails. But all six are authored, parsed, and readable today, and a mod that
  retunes any of them is silently ignored.
- **Impact**: Two things. (a) The closed issue's stated remedy does not apply to any
  shipped game, so a future reader will believe leveling/derived constants are
  GMST-sourced when they are not. (b) Every retune mod for FO3/FNV/FO4 derived stats —
  a common category — has no effect. No crash, no log line, no failing test: precisely
  the silent-wrong-constant class this audit exists for.
- **Related**: #2942 (CLOSED — the fix is present but unreachable, so this is not a
  regression, it is an incomplete close); #2934 (CLOSED — its doctrine note names #2942 as
  the paired precondition for moving `skill_calc` onto `CharacterRuleset`; that
  precondition now reads as satisfied when in practice it is not).
- **Suggested Fix**: Extend `with_gmst` to the `XpCurve` and `SkillUse` arms, and — more
  valuable — apply the same treatment to the *derived* table, where the sourced GMST names
  already sit in the capture documents and the code comments. The cheapest honest interim
  step is to give `RulesetBuilder` a `Skyrim` arm (`skyrim_ruleset` is written, sourced and
  its four keys all resolve on `Skyrim.esm` — see table row 13), which at minimum makes the
  existing `with_gmst` code reachable.

---

### CHAR-2026-08-20-D5-01: `effective_npc_level` is a third copy of `effective_actor_level` carrying the exact `.max(1)` divergence #3081 declared wrong

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fo3, fnv
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:182-190`
  (`effective_npc_level`) vs `byroredux/src/npc_spawn.rs:143-149`
  (`effective_actor_level`). Call sites: `actor_value_derive.rs:132` (template resolution)
  and `:229` (the Health curve's level term)
- **Status**: NEW (Related: #3081, CLOSED — incomplete)
- **Source**: `17b94d2e`'s own commit body, which settled the clamp question:
  *"`pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier`'s own `negative` case
  already asserts and comments 'Negative levels still clamp to 0 on the non-mult path
  (pre-existing behaviour, preserved)' — `.max(0)` is the deliberate, tested answer, not an
  oversight."* Plus `npc_spawn.rs:135-142`, which spells out why `1` must **not** be forced:
  *"a plain `level` of `0` is not a documented 'record carries none' sentinel — nothing
  distinguishes it from an authored `0`, so forcing it to `1` would be inventing data the
  record never claimed to have."*
- **Description**: The two functions are the same logic with one divergent line:

  ```rust
  // byroredux/src/npc_spawn.rs:143   — the documented, tested original
  } else { npc.level.max(0) }

  // crates/plugin/src/esm/records/actor_value_derive.rs:184 — the copy
  } else { npc.level.max(1) as u16 }
  ```

  `b434e4c0` (Aug 17) introduced the copy. `17b94d2e` (Aug 19) fixed #3081 by deleting
  `inventory.rs`'s copy — the second of what were by then three — and resolved the clamp
  divergence in favour of `.max(0)`. It did not touch this one, so the workspace still
  carries two copies whose non-multiplier branches disagree, and the surviving disagreement
  is the one the fix rejected.
- **Evidence**: `grep -rn "effective_npc_level\|effective_actor_level"` returns both
  definitions plus 12 call sites split across them.
  `git log -S"fn effective_npc_level"` returns exactly one commit: `b434e4c0`.

  Measured blast radius, from a direct `ACBS` scan of `FalloutNV.esm`:
  **3,816** `NPC_` records carry an `ACBS`; **268** set `PC Level Mult` (the branch both
  copies agree on); **30** are non-multiplier with `level ≤ 0` — the divergent set.

  For those 30 the two functions disagree by one level, which splits two ways:
  1. **The Health term.** `derive_autocalc_actor_values` (`:227-232`) evaluates the curve
     at level `1`, while `stamp_character_components` writes `CharacterLevel { level: 0 }`.
     The actor's Health is +5 (FNV) / +10 (FO3) above what its own recorded level implies.
  2. **The template tier.** `derive_npc_actor_values:132` passes `effective_npc_level` into
     `resolve_inherited_stats`, and `equip.rs:318-328` uses that number to filter
     `LVLN` entries (`e.level <= actor_level`). `stamp_character_components:176` passes
     `effective_actor_level`. A `Use Stats` shell with an `LVLN` entry at level 1 therefore
     resolves to a *different source record* for its `ActorValues` than for its
     `CharacterLevel` / `Background` — the numeric substrate and the structural component
     describing the same actor derived from two different NPCs.
- **Impact**: Small in magnitude (0.8 % of FNV actors, ±1 level) and invisible — no test
  covers it, because #2955's regression test only ever calls the original, which is exactly
  the failure mode #3081's commit body called out. The reason to fix it is not the 30
  actors; it is that the duplication that produced #3081 is still live, on the hotter of
  the two paths, and the next drift will be found the same way.
- **Related**: #3081 (CLOSED — this is the copy the fix missed); #2955 (CLOSED — the
  original's semantics); CHAR-D5-01.
- **Suggested Fix**: Delete `effective_npc_level` and move `effective_actor_level` down
  into `byroredux_plugin` (it takes a `NpcRecord` and belongs beside the record, not in the
  binary), then import it from both sides — the same resolution `17b94d2e` applied to
  `inventory.rs`. Pick `.max(0)`, per that commit's own reasoning, and extend
  `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` to call the shared
  function through the plugin crate so a future copy has something to fail against.

---

### CHAR-2026-08-20-D6-01: the #3095 real-data existence test covers one roster out of five and no derived-row output key on any game

- **Severity**: MEDIUM
- **Dimension**: Coverage, Documentation & Doctrine Drift
- **Game**: all
- **Location**: `crates/plugin/tests/parse_real_esm.rs:126-176`
  (`fnv_actor_value_roster_and_health_resolve_on_shipped_master`) and `:177-193`
  (`skyrim_health_resolves_to_authored_avif_form_id`); against
  `crates/core/src/character/fallout.rs:229-247` (`full` / `fo4_full`),
  `crates/core/src/character/skyrim.rs:168-176`, `crates/core/src/character/tes.rs`
- **Status**: NEW (Related: #3095, CLOSED — partial)
- **Description**: #3095 recorded that every CHARAL builder test hands the builder a
  resolver written from the roster's own strings, so no test can falsify a roster. The fix
  added two `#[ignore]`d real-data tests. One of them genuinely closes the gap — for FNV:

  ```rust
  for skill in SkillSet::FALLOUT_NV.skills() {
      assert!(index.actor_value_form_id(skill.editor_id).is_some(), ...);
  }
  ```

  That loop is falsifiable: it would have failed on the old `"Guns"` / `"Survival"`
  spellings. Nothing equivalent exists for `SkillSet::SKYRIM`, `SkillSet::FALLOUT3`,
  `SkillSet::OBLIVION`, or `AttributeSet::*`; and **no** test on any game asserts that a
  builder's *derived-row output keys* (`CarryWeight`, `MeleeDamage`, `CritChance`,
  `UnarmedDamage`, `RadResist`, `PoisonResist`, `DamageResist`, `LightArmor`, `Stamina`,
  `ActionPoints`) resolve against a shipped master. Those still rely exclusively on the
  hand-written `full()` fixture, which enumerates the same strings the builders pass.

  The Skyrim half of the fix asserts a single FormID (`health_actor_value_key() ==
  Some(0x3E8)`) and one non-zero derived Health, which is worth having but does not touch a
  roster.

  CHAR-2026-08-20-D2-01 is the demonstration: an unresolvable roster key survived the
  commit that closed #3095, in a roster the new test does not loop.
- **Evidence**: `grep -n "SkillSet::" crates/plugin/tests/parse_real_esm.rs` → only
  `FALLOUT_NV`. `grep -rn "fn full(" crates/core/src/character/` still returns the
  synthetic resolvers in `fallout.rs`, `skyrim.rs` and `tes.rs`, unchanged in shape. I
  verified by hand this session that the FO3, FNV and FO4 derived keys *do* all resolve —
  but that verification lives in this report, not in the suite.
- **Impact**: Process, not runtime. The suite can now falsify one roster out of five; the
  other four, and every derived-row key on every game, remain in the pre-#3095 state where
  a key that does not exist on disk produces a green test and an empty table. Given that
  three of the last sweep's four findings were instances of exactly this, the residual
  exposure is the main reason to finish the job rather than call it closed.
- **Related**: #3095 (CLOSED — half-done); CHAR-2026-08-20-D2-01 (what slipped through);
  #2986 / ESM-2026-08-16-D7-01.
- **Suggested Fix**: Generalize the FNV loop into one helper taking `(master path,
  CharacterRulesProfile)` and assert, per implemented family: every `AttributeSet` member
  resolves, every `SkillSet` member resolves, and `build_ruleset` against the *real* index
  produces the expected `derived_row_len()`. Run it over `Fallout3.esm`, `FalloutNV.esm`,
  `Fallout4.esm` and `Skyrim.esm`. Existence is the whole assertion — no values needed.
  Keep the synthetic fixtures for the arithmetic, where they are the right tool.

---

### CHAR-2026-08-20-D2-02: the two GMST names cited for the skill auto-calc coefficients are not authored by any shipped Fallout master, and the 13 that *are* authored are shadowed by one shared constant

- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: fo3, fnv
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:81-97` —
  the `SKILL_BASE` / `SKILL_ATTR_MULT` / `SKILL_LUCK_MULT` block and its `#2934 — DOCTRINE
  NOTE`; echoed in the module docstring at `:23-28` and in
  `crates/core/src/character/ruleset.rs:120-133`
- **Status**: NEW
- **Source**: vanilla `FalloutNV.esm` and `Fallout3.esm` `GMST` group, read directly.
  `docs/engine/charal-fnv-fo3-ruleset.md:47` cites geckwiki *Derived Skill Settings* for
  `fAVDSkillBase=2`, `…PrimaryBonusMult=2`.
- **Description**: The code annotates each coefficient with a GMST name:

  ```rust
  const SKILL_BASE: f32 = 2.0;       // fAVDSkill<name>Base
  const SKILL_ATTR_MULT: f32 = 2.0;  // fAVDSkillPrimaryBonusMult
  const SKILL_LUCK_MULT: f32 = 0.5;  // fAVDSkillLuckBonusMult
  ```

  and #2934's doctrine note defers moving them onto `CharacterRuleset` because that move is
  *"deliberately paired with sourcing them from GMSTs (#2942)"*. Two problems at HEAD:

  1. **`fAVDSkillPrimaryBonusMult` and `fAVDSkillLuckBonusMult` do not exist.** A raw byte
     search of both masters finds neither string. (`fAVDTagSkillBonus` *is* present, so the
     search is sound and the family name is right.) There is nothing to source those two
     from; the planned route is a dead end as written, and #2942 being closed makes the
     precondition read as satisfied.
  2. **`fAVDSkill<Name>Base` is per-skill, not shared.** FNV authors thirteen of them —
     `fAVDSkillBarterBase`, `fAVDSkillBigGunsBase`, `fAVDSkillEnergyWeaponsBase`,
     `fAVDSkillExplosivesBase`, `fAVDSkillLockpickBase`, `fAVDSkillMedicineBase`,
     `fAVDSkillMeleeWeaponsBase`, `fAVDSkillRepairBase`, `fAVDSkillScienceBase`,
     `fAVDSkillSmallGunsBase`, `fAVDSkillSneakBase`, `fAVDSkillSpeechBase`,
     `fAVDSkillSurvivalBase` — all `2.0` in vanilla, and all collapsed into one engine
     constant. (Note the GMST family keys on the *display* name: `…SurvivalBase`, not
     `…ThrowingBase` — the inverse of the `AVIF` convention, and a trap for whoever wires
     this.)
- **Evidence**: `/tmp/audit/character/gmst.py` over both masters with pattern
  `AVDSkill|XPLevel|fAVDActionPoints|AVDHealth|fAVDCarry`; plus targeted raw
  `bytes-in-file` checks for the two absent names (both `absent` on FNV; the FO3 GMST scan
  with `AVDSkillPrimary|AVDSkillLuck` returns nothing either).
- **Impact**: No wrong number today — every vanilla value is `2.0` / `2.0` / `0.5`, which
  is what the code uses. The cost is directional: the recorded plan for closing #2934's
  doctrine gap points at two GMSTs that are not there, and the thirteen that *are* there
  are invisible to the reader of that comment. A mod retuning one skill's base is silently
  ignored.
- **Related**: #2934 (CLOSED — the doctrine note); #2942 (CLOSED);
  CHAR-2026-08-20-D3-01 (the same GMST-reach problem one layer up).
- **Suggested Fix**: Correct the comments to what the masters actually author — per-skill
  `fAVDSkill<DisplayName>Base`, and mark the two mult names as geckwiki-documented but
  unauthored (engine defaults). When `skill_calc` finally lands on `CharacterRuleset`,
  source the per-skill base by display name with `2.0` as the fallback, and leave the two
  mults as engine constants with that fact recorded rather than as a pending GMST read.

---

## Cross-Audit Dedup

| Item | Disposition |
|---|---|
| `AVIF` EditorIDs `AV`-prefixed on FO3/FNV/Skyrim | **Existing: #2986** — CLOSED and **verified fixed** at HEAD (`index.rs:568-606`), confirmed against all four masters. Not re-filed. |
| `health_actor_value_key` returning the Skyrim enum `24` | ESM-2026-08-16-D7-02 — CLOSED, verified fixed: the enum constant is deleted and `Skyrim.esm` genuinely authors `AVHealth 0x3E8`. Not re-filed. |
| `pool_regen_tick_system` 3-deep RwLock hold stack | **Existing: #2153** — CLOSED (`6dc4400c`). `/audit-concurrency` owns the general rule. Not re-filed. |
| `combat_input_system` comment vs. recomputed damage | **Existing: #2980** (OPEN) — `/audit-tech-debt`'s. Not re-filed. |
| `crates/core/src/combat.rs` still asserting no combat consumer exists | **Existing: #2979** (OPEN) — `/audit-tech-debt`'s. Not re-filed. |
| `combat_input_system` attack edge before the `PlayerMode` gate | **Existing: #3033** (OPEN) — `/audit-ecs`'s. Not re-filed. |
| Component storage/shape of `CharacterLevel` / `Perks` / `ActorValues` | `/audit-ecs`. No shape findings here. |
| `AVIF` / `CLAS` / `NPC_` / `RACE` sub-record decoding, `EsmIndex::merge_from` load-order semantics for `character_rules` | `/audit-esm` Dim 3/4. |
| `CTDA` / `GetActorValue` evaluation | `/audit-scripting`. |
| `pool_regen_tick_system` scheduler access declaration | `/audit-concurrency` Dim 4. |

**Carried OPEN, re-verified as still-live, NOT re-filed**: **#2957** (auto-calc deferral
note under-states its scale), **#2958** (`character/mod.rs` docstring omits sub-modules —
now worse: the omitted set has grown to `attribute`, `skill`, `fallout`, `skyrim`, `tes`
**and the new `profile`**, which is the single most important module a new contributor
needs), **#2959** (`charal.md` stale in four places), **#2961** (`docs/feature-matrix.md`
has no character rows).

**Verified fixed and not regressed this cycle**: #2153, #2986, #3081 *(partially — see
CHAR-2026-08-20-D5-01)*, #3092, #3093, #3094, #3095 *(partially — see
CHAR-2026-08-20-D6-01)*, #2936, #2937, #2939, #2941, #2942 *(mechanism unreachable — see
CHAR-2026-08-20-D3-01)*, #2944, #2956, #2960, #2962, #3096.

---

## Known-Open Register (confirmed NOT re-filed)

| Deferred item | Status this audit |
|---|---|
| FNV/FO3 **tag-skill per-level** formula (undocumented) | Still absent, still not fabricated. `base_skill` remains `2 + 2·gov + ceil(Luck × 0.5)` with no per-level term, and `actor_value_derive.rs:36-45` still records why. `fAVDTagSkillBonus` **is** authored in `FalloutNV.esm` — the flat +15 half is sourceable; the per-level half remains uncitable. Recorded as context only, not a finding. |
| FO3↔FNV divergent **player** Health/AP | No longer needs master-name disambiguation for the *NPC* half — `character_rules_profile` splits FO3 from FNV on `HEDR < 1.0` and each builds its own `LevelingModel` and Health curve. The **player** actor is still deferred. Not re-filed. |
| **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance roll) | Not re-filed. Still formulas only. |
| CLAS SPECIAL lives in `ATTR`, not `DATA` | Confirmed correct in code (`derive_autocalc_actor_values` reads `class.base_attributes`, pinned positionally against `AttributeSet::FALLOUT` by `fallout_roster_matches_attr_order`). |

---

## Disproved Candidates (investigated, not reported)

- **"`character_rules_profile` mis-splits FO3 from FNV on `HEDR < 1.0`."** Checked: the
  discriminator is sound for the two shipped masters, and `parse_real_esm.rs:140-143` pins
  FNV's side against real data. FO3's side is pinned only by a synthetic-HEDR unit test
  (`records/tests.rs`), which is weaker but not wrong. Folded into
  CHAR-2026-08-20-D6-01's fix rather than filed separately.
- **"`EsmIndex::merge_from` lets the last-merged plugin overwrite `character_rules`."**
  True (`index.rs:808`), but it is the identical pre-existing contract as the adjacent
  `self.game = other.game` line, and belongs to `/audit-esm`'s load-order dimension, not
  here. Not filed.
- **"`derived_value` ignores `DerivedScope`, so `melee_damage_charal_bonus` could apply a
  player-only row to an NPC."** The Melee Damage row is `ActorGeneral` on both wired
  games, so unreachable; the scope-as-caller-contract design was already adjudicated in the
  2026-08-16 report. Not filed.
- **"FO4 `derive_stored_actor_values` can double-write Health when `PRPS` already carries
  it."** `ActorValues::from_pairs` → `set_base` uses `entry().or_default().base = base`, so
  the later baked `DNAM` value simply wins. Deterministic and arguably correct (`DNAM` is
  the calculated result). Not a defect.
- **"`stamp_actor_values` skips `ActorVitals` for FO4 NPCs with no baked Health."** True,
  and correct — an actor with no Health value should not be damageable. Coverage
  information, recorded in the matrix, not a finding.
- **"`SkillSet::SKYRIM`'s `Marksman` / `Speechcraft` are also wrong."** Checked against
  `Skyrim.esm`: both resolve (`0x44E`, `0x457`). The docstring's claim is accurate. Only
  Illusion is wrong.
- **"Oblivion's `oblivion_ruleset` regressed when the profile refactor landed."** It was
  already unwired before `b434e4c0` (the old `build_character_ruleset` returned `None` for
  Oblivion too). `CharacterRulesProfile::OBLIVION` makes the gap explicit rather than
  creating it. Coverage, not a regression.

---

## Not Covered

- `cargo test -p byroredux-core character` was **not run** (suite briefing forbids cargo).
  The Phase 1 test-count baseline is therefore missing from this report; all findings are
  static-analysis or game-data based and none depends on a test outcome.
- **FO76 / Starfield**: no ruleset builder exists, so there is nothing to diff against
  `charal-fo76-ruleset.md` / `charal-starfield-ruleset.md`. `CharacterRulesProfile` now at
  least records them as `NpcStatModel::Stored` + `RulesetBuilder::None`, which is an
  improvement over their previous total absence.
- **Oblivion's pre-`AVIF` legacy actor-value index resolution** — `Oblivion.esm` has no
  `AVIF` group at all, and the legacy-index resolver the roster docstring describes does
  not exist. Unchanged from 2026-08-16 and out of reach of a data probe.
- `crates/core/src/stealth.rs` was re-read but not re-verified line-by-line against
  `charal-fnv-fo3-ruleset.md`'s "Sneak Detection (FNV)" section — it is unchanged since the
  2026-08-16 sweep verified it, and the delta budget went to Dimension 5 per the dispatch.
- **Dimension 4** (regen / affliction / reputation) was checked for regressions against the
  eight fixes landed there (#2948–#2954, #2153) and for new code in the delta; there is
  none — those files are untouched since 2026-08-16 apart from `6dc4400c`'s lock reorder.
  No constant re-derivation was performed beyond confirming the prior sweep's table stands.

---

## Suggested Fix Order

1. **CHAR-2026-08-20-D6-01** — generalize the real-data existence loop to all five rosters
   and every derived-row output key. It is the cheapest fix, it validates the other four,
   and it is the only one that stops this class recurring.
2. **CHAR-2026-08-20-D2-01** — `"Illusion"` → `"Mysticism"`; two-line change, pinned by #1.
3. **CHAR-2026-08-20-D5-01** — collapse `effective_npc_level` into the shared
   `effective_actor_level` on `.max(0)`, finishing #3081.
4. **CHAR-2026-08-20-D3-01** — give `RulesetBuilder` a `Skyrim` arm so the GMST seam is
   reachable at all, then extend `with_gmst` past `SkillXp`.
5. **CHAR-2026-08-20-D2-02** — correct the GMST names in the comments; do it in the same
   commit as #4 so the recorded plan and the code agree.

TALLY: CRITICAL=0 HIGH=0 MEDIUM=4 LOW=1
