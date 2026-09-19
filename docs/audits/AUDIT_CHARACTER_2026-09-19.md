# Character / CHARAL Audit — 2026-09-19

**Scope**: `/audit-character`, all 6 dimensions, `--depth deep`, every implemented
family. Run as an orchestrator: six dimension agents (batches of 3), merged here.
Owner slice `crates/core/src/character/` (14 modules) plus the CHARAL-adjacent
siblings `crates/core/src/combat.rs` and `crates/core/src/stealth.rs`, the
population boundary (`byroredux/src/npc_spawn.rs` + `npc_spawn/{resumable,ai_package,loot_appearance}.rs`,
`crates/plugin/src/esm/records/actor_value_derive.rs`,
`crates/plugin/src/esm/records/actor/mod.rs`,
`byroredux/src/cell_loader/references/`), and the console/runtime consumers
(`byroredux/src/commands/actor_value.rs`, `crates/scripting/src/condition.rs`,
the new consumable/timed-restoration writers).

**Repo state at time of this run**: `main`, HEAD `479163836` (2026-09-19). The
prior audit base was `b3db49fa` (AUDIT_CHARACTER_2026-09-11b); the delta in this
slice is substantial — `e13985dfc` closed #3848 (wired the Skyrim
`RulesetBuilder` arm, deleted the dead `oblivion_pool_regen_config`),
`702b0b020` fixed #4086 (absent-TPLT stats resolve down the chain — by adding
new resolver call sites), and the P3 gameplay commits (`4c0053322` consumable
effects, `2e2f40b23` timed restorations, `d8255b2e2` reference-state
persistence, `0c8ce1f54` loot appearance, `479163836` P3 Closure) added the
first *live* `ActorValues` mutation paths beyond console commands. Diffstat
since `b3db49fa`: ~2,800 insertions across 31 files in the audited slice.

Every dimension agent read the capture documents **before** the Rust constants
(per the ground-truth order) and re-derived every constant from the documents
rather than carrying the 2026-09-11b "PASS" verdicts forward. All prior
findings' fixes were re-verified on HEAD by reading code, not commit messages.
The orchestrator independently spot-checked the four highest-impact new claims
(`mod.rs` docstring, the `GameKind::Fallout3NV` branch, the player-stamp
absence vs. the consumable gate, the `spawn_placement_root` raw-shell shape)
before including them.

## Tests recorded (read-only; nothing launched, no engine process started)

| Command | Result |
|---|---|
| `cargo test -p byroredux-core character` (orchestrator, Phase 1) | 119 passed, 0 failed, 643 filtered (118 CHARAL + 1 `ecs::components::material` test matching the filter by name) |
| Dim 1: `--lib character::` / `-p byroredux-plugin --lib actor_value_derive` / `--lib actor::` | 118 / 27 / 65 passed, 0 failed (3 ignored: real-data). Bin-crate guard **not run** — toolchain: `cranelift` requires rustc 1.94, environment has 1.93.1 (pre-existing environment limit, unrelated to the code); guard verified by source read |
| Dim 2: `character::` / `combat` / `stealth` / `-p byroredux-plugin --lib actor_value` | 118 / 6 / 26 / 28 passed, 0 failed |
| Dim 3: `character::` / `-p byroredux-scripting --lib xp` / `-p byroredux-plugin --lib actor_value_derive` | 118 / 16 / 27 passed, 0 failed |
| Dim 4: `character::` | 118 passed, 0 failed; binary crate (scheduler-access tests) unbuildable in sandbox — same toolchain limit; declaration verified by direct read |
| Dim 5: `-p byroredux-plugin --lib actor_value_derive` / `character::profile` | 27 / 4 passed, 0 failed |
| Dim 6: `character::tests` / `character::profile` | 2 / 4 passed, 0 failed |

All-green across every dimension's targeted test slice. The `#[ignore]`d
real-master corpus gate (`charal_rosters_and_derived_keys_resolve_on_every_shipped_master`,
`crates/plugin/tests/parse_real_esm.rs`) was verified by read, not run.

## Executive Summary

**17 findings after cross-dimension dedup (21 raw) — 0 CRITICAL · 0 HIGH ·
6 MEDIUM · 11 LOW. The numeric surface is clean: not one coefficient, bias,
cross term, cap, or rounding mode deviates from its capture document.**

**Which families' constants were actually verified against capture
documents**: FO3, FNV, FO4, Oblivion, and Skyrim — all five implemented
rulesets, across derived-stat formulas (Dim 2, incl. the `combat.rs` /
`stealth.rs` siblings), leveling/progression (Dim 3), and
pools/afflictions/reputation (Dim 4). FO76 and Starfield have captures but no
code; their captures were read for deliberately-absent items only — nothing
speculative exists in code (their two `NpcStatModel::Stored` profile rows are
the one unsourced presumption, filed as D3-01).

**The two most consequential new findings are both about the *seam*, not the
numbers:**

- **D5-08 (MEDIUM, new)** — the entire P3 consumable pipeline
  (`consume_item`, `TimedRestorations`, `restoration_system`) and the player
  drowning path gate on the player carrying `ActorValues` + `ActorVitals`, but
  **no production path stamps either on the player** — `spawn_player_body`
  deliberately withholds them (documented at the stamp site, #3004/#2986). In
  a fresh session, using a Stimpak in the P3 native inventory UI silently
  returns `Unavailable`. Every test passes because it hand-inserts the
  components. The writers themselves are layer-correct — this is a
  silent-no-op class, not a revert class.
- **D1-01 (MEDIUM, new)** — the first raw game-identity branch inside the
  profile-driven population path since the doctrine was established: FO3/FNV
  body-condition AV seeding (base 100, commit `2e2f40b23`) gates on
  `index.game == GameKind::Fallout3NV` six lines under a comment reading
  "Consumers never branch on game identity", instead of riding a
  `CharacterRulesProfile` field like every neighbouring rule. Unit tests
  cannot exercise it (the fixture lacks the AVIFs).

**D5-06 (MEDIUM, existing, never filed as an issue) got worse, not better.**
The TPLT `resolve_inherited_*` hoist into `spawn_placement_root` that the
2026-09-11 audit asked for still has not happened; all nine sites remain
(renumbered), a tenth exists on the player-inventory path, and the #4086 fix
(`702b0b020`) **added** a new `resolve_inherited_field` helper with two more
independent chain-walks — the fix-by-addition recurrence D5-06 predicted.
This is the eighth episode of the class (#2956, #3381, #3382, #3480, #4091,
#4092, #4093, #4086), every one individually correct on inspection.

**Post-#3848 fallout dominates the doc-rot findings.** The Skyrim wiring is
real and well-tested (verified on HEAD by three dimensions independently:
profile arm, GMST-overlay probe, ROSTER_CASES corpus row), but six documents
still describe the pre-#3848 world: the `character/mod.rs` entry docstring
(D6-01), the deleted `oblivion_pool_regen_config` references and the now-false
"inserted when a live `CharacterRuleset` lands" trigger phrasing (D6-02
cluster), `charal.md`'s stale line cite (D3-03/D6-04), ROADMAP's stale
known-issues bullet (D6-05), the FNV/FO3 capture's un-flipped Status column
(D6-06), and feature-matrix's wrong pool names on the regen row (D6-07).

**Wiring state (unchanged for regen/affliction)**: `pool_regen_tick_system` is
registered and armed at boot but `PoolRegenConfig` still has **zero production
insertion sites** — the one constructor was deleted under #3848 — so regen is
inert on all seven games; `affliction_tick_system` still has no scheduler
registration. Both are accurately documented at the struct level (see D6-02
for the two comment sites whose *trigger phrasing* is now false). No runtime
level-up system exists on any game (`level_cap()` has zero consumers).

## Constant Verification Table

Consolidated from Dimensions 2, 3, and 4 (Dimension 1 re-derived every
per-game constant in passing — zero drift; Dimension 6 verified doctrine
constants — module count 14 pinned by `mod_docstring_indexes_every_sub_module`,
`DerivedStatFormula` Copy+36 B pinned). Every row was independently re-derived
from its capture document this run. Doc keys: **FO4** = charal-fo4-ruleset.md,
**FNV/FO3** = charal-fnv-fo3-ruleset.md, **OBL** = charal-oblivion-ruleset.md,
**SKY** = charal-skyrim-ruleset.md, **CORE** = charal.md.

### Totals

| Area | Rows | PASS | FAIL | UNSOURCED | Deliberately absent |
|---|---|---|---|---|---|
| Derived-stat formulas + siblings (Dim 2: `derived.rs`, `fallout.rs`, `tes.rs`, `skyrim.rs`, `resistance.rs`, `profile.rs` curves, `combat.rs`, `stealth.rs`) | 64 | 63 | 0 | 1 (→ D2-01) | 0 |
| Leveling & progression (Dim 3: `leveling.rs`, `tes.rs`, `skyrim.rs`, `components.rs`, `profile.rs`) | 33 | 29 | 0 | 2 (→ D3-01 ×1 row, D3-02 ×1 partial) | 2 (FO76 XP curve, Oblivion skill-progression — both uncoded by documented design) |
| Pools, afflictions, resistance, reputation (Dim 4: `regen.rs`, `affliction.rs`, `resistance.rs`, `reputation.rs`) | 27 | 26 | 0 | 1 (known-disclosed #2949 `sentiment()`, not a new finding) | 0 |
| **Total** | **124** | **118** | **0** | **4** | **2** |

### Dimension 2 — derived-stat formulas (64 rows)

Core machinery (`derived.rs`): eval shape `bias + cₐ·A + c_b·B + cross·A·B`
(PASS); no game-identity branch in `eval` (PASS); uncapped sentinel is
`f32::INFINITY` — no table row relies on cap-0-means-uncapped (PASS);
`DerivedInput` sentinels 0/`u32::MAX` filtered by the production resolver
`actor_value_form_id` (index.rs:717-736, PASS); absent input reads the
documented 0.0 default (PASS); 36-byte `Copy` layout pinned and passing
(derived.rs:353-366); `a_from_base` base-layer read for Skyrim CW (SKY:663-675,
PASS).

| Formula/constant | file:line | Code | Doc | Verdict |
|---|---|---|---|---|
| FO4 Health bias / c_END / c_L / cross / floor / player-only | fallout.rs:133-135 | 77.5 / 4.5 / 2.5 / 0.5 / `.floored()` / `.player_only()` | FO4:72,88,94-99 | PASS ×6 (the only cross-term row in any table) |
| FO4 AP `60+10·AGI` player-only | fallout.rs:139-143 | affine(10,60), player_only | FO4:131-137,165-168 | PASS |
| FO4 Carry Weight `200+10·STR` actor-general | fallout.rs:146-148 | affine(10,200) | FO4:236-249 | PASS |
| FO4 no MeleeDamage row (3 rows total) | fallout.rs:126-150 | no row; ROSTER_CASES pins 3 | FO4:276-281 | PASS |
| FO3 Health `90+20·END+10·L` player-only | fallout.rs:159-163 | bilinear(20,L10,cross0,bias90) | FNV/FO3:93,103-104 | PASS |
| FO3 AP `65+2·AGI` cap 85 player-only | fallout.rs:165-186 | affine(2,65).capped(85).player_only() | FNV/FO3:94 | PASS (known-open #2937 scope) |
| FNV Health `95+20·END+5·L` player-only | fallout.rs:198-202 | bilinear(20,L5,cross0,bias95) | FNV/FO3:93 (`100+20·END+5·(L−1)`; worked END10/L30→445 ✓) | PASS |
| FNV AP `65+3·AGI` cap 95 player-only | fallout.rs:204-212 | affine(3,65).capped(95).player_only() | FNV/FO3:94 | PASS (known-open #2937 scope) |
| FO3+FNV Carry Weight `150+10·STR` actor-general | fallout.rs:48-50 | affine(10,150) | FNV/FO3:95,106-110 | PASS |
| FO3+FNV Melee Damage `0.5·STR` additive | fallout.rs:52-54 | affine(0.5,0) | FNV/FO3:97,350-353 | PASS |
| FO3+FNV Critical Chance `1.0·Luck` cap 10 (0-100 scale) | fallout.rs:63-68 | affine(1.0,0).capped(10) | FNV/FO3:96; #2936 convention | PASS |
| FO3+FNV Unarmed Damage `ceil((10+U)/20)` | fallout.rs:70-72 | affine(0.05,0.5).ceiled() | FNV/FO3:98,118-120 | PASS |
| RadResist `(END−1)·2` cap 85 actor-general | resistance.rs:75-80,106-114 | affine(2,−2).clamped_below(0).capped(85) | FNV/FO3:99,355-357 | PASS |
| PoisonResist `(END−1)·5` uncapped actor-general | resistance.rs:84-89 | affine(5,−5), cap INFINITY | FNV/FO3:100,371-374 | PASS |
| CritChance/MeleeDamage/UnarmedDamage **scope** | fallout.rs:44,52-72 | ActorGeneral (doc asserts "all actor-general") | FNV/FO3:91-101 — **no scope annotation for these three** | **UNSOURCED → D2-01** |
| FO3 NpcHealthCurve 90/20/10 · FNV 95/20/5 | profile.rs:100-127 | as cited | FNV/FO3:93 | PASS ×2 |
| Auto-calc base skill `2+2·gov+ceil(Luck/2)` | actor_value_derive.rs:132-139 | 2/2/0.5, ceil | FNV/FO3:46-49 (worked END5/Luck5→15 ✓) | PASS |
| Chaining: SPECIAL+skills populated before on-demand derivation | actor_value_derive.rs:394-433 + npc_spawn.rs:104-131 | one spawn-time batch; derived evaluated only post-spawn | CORE §6; FNV/FO3:122-127 | PASS |
| Oblivion Health `2·END` / Magicka `2·INT` player-only | tes.rs:39-48 | affine(2,0).player_only() ×2 | CORE:317-320 | PASS ×2 |
| Oblivion Fatigue STR+WIL+AGI+END (4 rows, coeff 1) | tes.rs:73-87 | 4× affine(1,0).player_only() | CORE:318-319; OBL:383-385 | PASS |
| Oblivion Armor Rating `0.35+0.0065·skill` ×2 rows, multiplier, actor-general | tes.rs:61-63,126-149 | affine(0.0065,0.35).as_multiplier() | OBL:326-346 (worked 50→0.675, 20→0.48 ✓) | PASS |
| Oblivion attribute-bonus tiers 0/1-4/5-7/8-9/10+ → +1..+5 | tes.rs:174-182 | same tier table | CORE:340-342; OBL:790-794 | PASS |
| Oblivion health gain/level `0.1·END`, caller rounds | tes.rs:202-204 | 0.1 exact f32 | OBL:457-461 (END98→9 ✓) | PASS |
| Oblivion Fatigue regen 10/s flat | regen.rs:75 | 10.0 | OBL:391-393; CORE:258 | PASS |
| Oblivion Magicka regen `(0.02·WIL+0.75)·(Max/100)` | regen.rs:78-91 | 0.02/0.75/÷100 | OBL:524; CORE:259 | PASS |
| Skyrim Light Armor `1+0.004·skill` player-only multiplier | skyrim.rs:79,120-131 | affine(0.004,1.0).as_multiplier().player_only() | SKY:206-221,242-259 | PASS |
| Skyrim NPC 0.015 constant deliberately unmodelled | skyrim.rs:77-78,99-103 | not shipped | SKY:209,219-221 | PASS |
| Skyrim Carry Weight `250+0.5·BaseStamina` base-read, actor-general | skyrim.rs:86-88,133-144 | affine(0.5,250).a_from_base() | SKY:651-659,709-711 | PASS |
| `combat.rs` `modified_skill` Luck coeff 0.4 (bias −20) | combat.rs:41-43 | `skill+0.4·(luck−50)` | OBL:285,293-294 | PASS |
| `combat.rs` weapon mult 0.5·(0.75+0.005·A)·(0.2+0.015·MS) + [0,100] clamp | combat.rs:56-60 | 0.5/0.75/0.005/0.2/0.015; clamped | OBL:209,230-235,256-258 | PASS ×2 |
| `combat.rs` hand-to-hand `1+10.5·(S/100)·(MS/100)`, no clamp; fatigue `1+0.5·HP` | combat.rs:73-82 | cross 10.5; unclamped; 0.5 | OBL:304-307,318-321,306 | PASS ×2 |
| `stealth.rs` top level `atten·(S+V+DS/2)−TS/2−35` | stealth.rs:284 | identical | FNV/FO3:231 | PASS |
| `stealth.rs` TargetSkill / DetectorSkill (10+8·PER) / DetectorState 0.8/1.2/1.0 | stealth.rs:272-282,168-177 | identical | FNV/FO3:232-244 | PASS ×3 |
| `stealth.rs` attenuation `((max−d)/max)²`, MaxDist 2500/5000 | stealth.rs:80-88,243-244 | identical | FNV/FO3:235 | PASS |
| `stealth.rs` SoundMultiplier 1.6/0.16; Sound=SM·(Move+2·Action); MovementMult 0/1.5/1; MovementSound `(12+W/2)·MM` | stealth.rs:243-253 | identical | FNV/FO3:259-265 | PASS ×4 |
| `stealth.rs` Visual zero; nighteye 3/1; Light `1.4·min(100,L·ne)`; VisualMovement 0/0.01/0.21 | stealth.rs:255-268 | identical (SR→0.01 disclosed) | FNV/FO3:267-274,301-310 | PASS ×4 |
| `stealth.rs` ActionSound 100/50/10/0; Armor penalty 20/10/0; classify bands <−20/≤0/>0 | stealth.rs:125-155,229-237 | identical | FNV/FO3:246,277-288 | PASS ×3 |

### Dimension 3 — leveling & progression (33 rows)

| Constant/formula | file:line | Code | Doc | Verdict |
|---|---|---|---|---|
| FO4 XP `75·L+125`, cap 0 (uncapped), reward SpecialOrPerk | leveling.rs:112-117 | 75/125/0 | FO4:471-486 | PASS ×3 |
| FO3 XP `150·L+50`, cap 20, `10+INT`, perk/level | leveling.rs:121-129 | 150/50/20/10/1.0 | FNV/FO3:424-442 | PASS ×4 |
| FNV XP `150·L+50`, cap 30, `10+INT/2` (0.5 carry documented caller duty), perk/2 | leveling.rs:134-141 | 150/50/30/10/0.5/2 | FNV/FO3:424-442 | PASS ×4 |
| Oblivion 10-major-skill-ups threshold, cap 0 | leveling.rs:146-149 | 10, 0 | OBL:788-792 | PASS ×2 |
| Skyrim XP `25·L+75` (base 75/mult 25); rank coeff 1.0 (engine-owned, settled #3221 — not re-flagged); pool pick +10; cap 0 | leveling.rs:154-159,181-198 | 75/25/1.0/10/0 | SKY:712-720,583-586 | PASS ×4 |
| `with_gmst` requests exactly `["fXPLevelUpBase","fXPLevelUpMult"]` | leveling.rs:100-103 | exactly two; no third GMST | SKY:714-722; CORE §8.6 | PASS (pinned by 3 tests incl. real-data probe vs Skyrim.esm) |
| `SKYRIM_POOL_BASE` 100 · `SKYRIM_SKILL_USE_CURVE` 1.95 · `skyrim_skill_xp_to_next` shape · `_between` = sum | skyrim.rs:31,36,50-73 | 100 / 1.95 / `mult·L^curve+offset` / sum | SKY:583-586,730-742 (Lockpicking 15→16 ≈ 349.13 ✓) | PASS ×4 |
| Oblivion health/magicka/fatigue formulas + attribute-bonus banding + gain/level | tes.rs:39-204 | (Dim-2 overlap) | OBL:379-387,455-461,510-513,791-794 | PASS ×5 |
| FO3/FNV NpcHealthCurve rows | profile.rs:100-127 | 90/20/10; 95/20/5 | FNV/FO3:93 | PASS ×2 |
| `CharacterLevel` u16/u32 · `grants_perk_at` cadences · `try_set_rank` rejects out-of-range (no clamp) | components.rs:15-23,96-102; leveling.rs:221-234 | u16/u32; cadence match; rejects rank 0 & >max | CORE:170; FNV/FO3:431-433; FO4:461-465 | PASS ×3 |
| FO76 XP curve `160·L−120` | — absent — | no `LevelingModel::FALLOUT76` | FO76:29-38 | DELIBERATELY ABSENT |
| Oblivion skill-progression curve | — absent — | SkillUse models the level trigger only | OBL:793-794 | DELIBERATELY ABSENT |
| FO76 / Starfield `NpcStatModel::Stored` rows | profile.rs:151-165 | `Stored` (FO4 PRPS/DNAM layout) | FO76:121-124 "not researched"; Starfield silent | **UNSOURCED → D3-01** |
| Skyrim NPC pool composition | profile.rs:129-132 + actor_value_derive.rs:288-313 | race base + offset only (2 of 3 terms) | SKY:603-609 (3-part composition; class term capture-deferred) | **PARTIAL → D3-02** |

Also verified: the three models are three enum **data** variants with zero
consumer-side matches on `LevelingModel` (sole runtime consumer:
`GetXPForNextLevel`, condition.rs:672-681); `level_cap()==0` sentinel
documented identically on all three variants via one accessor, and currently
has **zero consumers** (no level-up system exists, so no cap off-by-one is
possible today — the enforcement duty is recorded in D3's transcript).

### Dimension 4 — pools, afflictions, resistance, reputation (27 rows)

| Constant | file:line | Code | Doc | Verdict |
|---|---|---|---|---|
| `POOL_REGEN_DT` 1/60 · `MAX_REGEN_SUBSTEPS` 8 (backlog clamp — no unbounded catch-up; zero-dt cannot spin) | regen.rs:45,70,109-118 | 1/60; 8 | OBL:408-409; CORE:249 | PASS ×2 |
| Fatigue regen 10/s · Magicka regen 0.02/0.75/÷100 + stunted gate · Health passive regen absent (deliberate) | regen.rs:26,75,86-91 | as cited | OBL:391,524,544-556,467-472 | PASS ×3 |
| Radiation coeff 2 cap 85 · Poison coeff 5 uncapped · `damage_multiplier` `clamp(r,0,cap);(1−r/100).max(0)` (immunity floor, never heal) | resistance.rs:75-88,125-128 | as cited | FNV/FO3:99-100,355-374,414 | PASS ×3 |
| Karma clamp ±1000 · cut points +750/+250/−249/−749 (Evil=[−749,−250]) · bump points [0,1,2,4,7,12] · axis max 100 | reputation.rs:33-45,113,129 | as cited | FNV/FO3:462-477,537-539,556-561 | PASS ×4 |
| 13× `FactionRepThresholds` + 13× REPU FormIDs (incl. Powder Gangers' asymmetric R3=50) | reputation.rs:177-223 | identical row-for-row | FNV/FO3:572-586 | PASS ×2 (13/13 each) |
| 4×4 `STANDING_GRID` `[infamy][fame]` — axes NOT transposed | reputation.rs:420-428 | verified on asymmetric off-diagonals (Smiling Troublemaker / Sneering Punk / Dark Hero / Soft-Hearted Devil) | FNV/FO3:604-609 | PASS |
| FO4 affinity clamp [−1000,+1100] · 7 bands at −500/0/250/500/750/1000 · reactions ±15/±35 · sizes 0.5/1/1.5 · passive gain `40−0.033·a` (500→+23.5 ✓) | reputation.rs:247-374 | as cited | CORE:475-482 | PASS ×5 |
| `ReputationStanding::sentiment` buckets | reputation.rs:466-479 | heuristic Positive/Negative/Mixed | none | UNSOURCED — **known, disclosed in-code, #2949 CLOSED**; not a new finding |

## Coverage Matrix

From Dimension 6, cross-checked against Dimension 3's independently derived
reachability (both agree) and re-derived from `profile.rs`, `npc_spawn.rs`,
`boot/schedule/update.rs`, and workspace greps on HEAD:

| Game family | Ruleset implemented | Ruleset **wired** | Derived stats implemented | Leveling model implemented | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|
| **FO3** | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | ✓ 8 rows / 8 stats | ✓ `150·L+50`, cap 20 | ✗ registered-inert | ✗ |
| **FNV** | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | ✓ 8 rows / 8 stats | ✓ `150·L+50`, cap 30 | ✗ registered-inert | ✗ |
| **FO4** | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | ✓ 3 rows (no MeleeDamage AVIF authored) | ✓ `75·L+125`, uncapped | ✗ registered-inert | ✗ |
| **Skyrim SE** | ✓ `skyrim_ruleset` | ✓ `RulesetBuilder::Skyrim` — **#3848 CLOSED (`e13985dfc`, 2026-09-12); the only wiring change since 2026-09-11b**; GMST overlay production-reachable | ✓ 2 rows, resolve against real Skyrim.esm | ✓ `25·L+75` + skill-XP curve | ✗ registered-inert | ✗ |
| **Oblivion** | ✓ `oblivion_ruleset` (8 rows / 5 stats) | ✗ `RulesetBuilder::None` — **deliberate**, blocked on pre-AVIF resolver (#3768), pinned by test | ✓ synthetic-resolver tests only | ✓ 10-major-skill-ups | ✗ (the one config constructor was **deleted** under #3848; `git show` recovers it) | ✗ |
| **FO76** | ✗ no builder | ✗ | ✗ capture LOCKED | ✗ curve LOCKED, uncoded | ✗ | ✗ |
| **Starfield** | ✗ no builder | ✗ | ✗ | ✗ PENDING (XP curve + tier thresholds) | ✗ | ✗ |

"Registered-inert": `pool_regen_tick_system` runs every frame as a
`Stage::Update` exclusive with `PoolRegenAccumulator` armed at boot, but
`PoolRegenConfig` has **zero production insertion sites** (both constructors
are `#[cfg(test)]`) — it early-returns on every game. `affliction_tick_system`
has no scheduler registration at all; no `AfflictionTable` ships. Runtime
leveling (XP grant / level-up) is ✗ for all seven games — the matrix's
"leveling model" column is the *model*, not a runtime. The matrix is
game-uniform in the last two columns by construction (the regen mechanism is
Oblivion-shaped: Fatigue/Magicka; see D6-07 for the feature-matrix label that
gets this wrong).

## Findings

Grouped by severity; cross-dimension duplicates merged (merge noted inline).
21 raw dimension findings → 17 here.

### MEDIUM

#### D5-08: The P3 consumable/drowning writers gate on `ActorValues`+`ActorVitals` on the player, but no production path stamps either — every consumable use silently returns Unavailable in a fresh session
- **Dimension**: Population Boundary
- **Game**: Skyrim + FO3/FNV (consumables); all (drowning path)
- **Source**: not numeric — structural reachability: `byroredux/src/inventory.rs:918-923` (`consume_item` requires `world.get::<ActorVitals>(player)` and `world.get::<ActorValues>(player)`, else `MutationResult::Unavailable`) vs `byroredux/src/scene.rs:1715-1722` (`spawn_player_body` deliberately does NOT stamp `ActorValues`/`CharacterLevel`/`Background` on the player, deferring to #3004/#2986 per #3158's sibling note); exhaustive insert survey — the only production `ActorVitals`/`ActorValues` inserts are `npc_spawn.rs:121-130` (NPC placement roots) and `reference_state.rs:207-209` (whose `capture` explicitly skips the player at :99-101).
- **Location**: `byroredux/src/inventory.rs:878-1000` (`consume_item`), `byroredux/src/systems/character.rs:1371-1389` (`apply_player_drowning_damage`, same gate), `byroredux/src/scene.rs:1715-1722` (the deliberate absence)
- **Status**: NEW
- **Description**: The post-audit commits `4c0053322` / `2e2f40b23` / `479163836` built the entire consumable pipeline — catalog, `consume_item`, `TimedRestorations`, `restoration_system` — against a player that structurally cannot satisfy its own entry gate. The writers are layer-correct (`restore()` touches only the `damage` layer, floored at 0, no overheal bank), so this is not a revert-class defect; it is a silent no-op class. In a fresh session, "Use" on a Stimpak in the P3 native inventory UI returns `Unavailable` before the item is consumed — no effect, no notification. `TimedRestorations` can never attach, `restoration_system` has no eligible target, and the player cannot drown (a quieter symptom of the same gate). All 12+ tests pass because they hand-insert the components (`inventory.rs:1196-1199`, `character.rs:2055-2056`, `water.rs:1002`, `combat.rs:652`, `interaction.rs:1694` — all `#[cfg(test)]`).
- **Impact**: The P3 consumable feature — the headline of commit `479163836` — is unreachable for its intended user in production until the player actor gets a populated `ActorValues` (+ `ActorVitals`). The disclosure exists at the *stamp* site (written before these consumers existed) but nothing at the *consumer* sites records the gate.
- **Related**: #3158, #3004, #2986; known-open item 2 (player-actor deferral) is adjacent but covered only formula deferral, not this consumer-vs-stamp mismatch.
- **Suggested Fix**: (a) land the minimal player population slice the gate needs — stamp `ActorValues` (via `derive_npc_actor_values` on the player NPC_ record `build_player_template_for` already reads) and `ActorVitals` in `attach_to_player`, updating #3158's sibling note; or (b) until then, surface the Unavailable result in the P3 UI and document the gate at the consumer. (a) is the real fix and is already scoped by #3004/#2986.

#### D1-01: FO3/FNV body-condition AV seeding is a raw `GameKind` branch inside the profile-driven population path — CHARAL's first consumer-side game-identity branch since the doctrine was set
- **Dimension**: Ruleset Seam
- **Game**: FO3 / FNV (branch); Skyrim / FO4 excluded by kind
- **Source**: `docs/engine/charal.md` §1/§5 ("the per-game seam is the *data* in the tables, never a branch in the consumer" — restated at `crates/core/src/character/ruleset.rs:5-7`); `charal-fnv-fo3-ruleset.md` has no body-condition row — the base-100 constant's only in-repo citation is the "GECK Stats List" comment itself
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:227-239` (added by `2e2f40b23`, since the last audit)
- **Status**: NEW
- **Description**: `derive_npc_actor_values` selects its stat model through the data seam (`index.character_rules.npc_stat_model()` / `creature_stat_model()`) — and then, six lines under a comment reading "Consumers never branch on game identity", a new rule gates on `index.game == GameKind::Fallout3NV` to seed the 7 body-condition AVs at base 100. A per-game character-population rule expressed as a `game ==` compare in the consumer, where every neighbouring rule is a `CharacterRulesProfile` field. The gate also cannot distinguish FO3 from FNV — the very reason the profile exists (profile.rs:1-7) — and Skyrim ingestibles are supported (`consumables.rs:161`) while Skyrim actors are excluded by *kind*, not by data.
- **Evidence**: The branch is untested at unit level: the `fnv_index_with_class` fixture authors no body-condition AVIFs, so `derives_special_and_skills_from_class`'s `pairs.len() == 21` passes with the branch silently no-op'ing; the only exercising test is the `#[ignore]`d real-master test.
- **Impact**: Behaviorally correct on every shipped game today. The risk is structural: invisible to the profile (a future kind/profile split inherits or loses the rule silently), and the unit suite cannot catch a regression.
- **Related**: D1-02 (same off-seam shape); pinned convention `fallout_profiles_keep_roster_health_and_ruleset_in_lockstep` (profile.rs:223).
- **Suggested Fix**: Add a profile field (e.g. `body_condition_base: Option<f32>`) set on the FALLOUT3/FALLOUT_NEW_VEGAS rows; gate the seeding on it; add a plugin-crate unit test with one authored body-condition AVIF (fires for FO3/FNV profiles, not FO4/Skyrim).

#### D2-01: FO3/FNV Critical Chance, Melee Damage, Unarmed Damage ship an uncited `ActorGeneral` scope that `fallout.rs`'s doc asserts as fact and `GetActorValue` consumes live
- **Dimension**: Derived Formulas
- **Game**: FO3 / FNV
- **Source**: NONE — the finding is the unsourcedness itself. Nearest lines: charal-fnv-fo3-ruleset.md:91-101 (the derived table annotates scope on Health "(player ruleset…)", Carry Weight "(actor-general)", Rad/Poison "(actor-general)", AP "(#2937… conservatively player-only)" — and gives **no** scope for Critical Chance / Melee Damage / Unarmed Damage); :388-397 (#2937: the project explicitly rejects deriving scope without per-stat NPC evidence).
- **Location**: `crates/core/src/character/fallout.rs:44` ("The six derived stats shared verbatim by FO3 and FNV (all actor-general)") and `:52-72` (the three rows defaulting to `DerivedScope::ActorGeneral`); consumed as fact at `crates/scripting/src/condition.rs:500-531` and applied live by `byroredux/src/combat.rs:432-460`
- **Status**: NEW
- **Description**: When #2937 faced exactly this question for Action Points — formula locked, scope unstated — the code chose `player_only` conservatively, documented the choice in place, pinned it with a test, and left an open item. The three sibling rows in the same table received no such treatment: they default to `ActorGeneral`, and the module docstring upgrades the default to a factual claim. Nothing in any capture supports or contradicts it.
- **Impact**: If any of the three is genuinely player-only in the engine, NPCs silently receive computed Critical Chance / Melee Damage values they should not — the exact over-computation risk #2937 chose to avoid. Plausibly correct (the GECK auto-calc populates these for NPCs too), but "plausible" is what the no-guessing doctrine exists to prevent.
- **Related**: #2937 (the mirror-image decision); #2936 (convention discipline in the same rows — fixed).
- **Suggested Fix**: Mirror #2937: find a per-row citation (fandom Critical Chance / Melee Damage / Unarmed Damage NPC behavior) and record it in the capture's derived table, or annotate the three rows "scope unsourced" in the doc + code with a pinning test so a future flip is a reviewed edit.

#### D5-06: The TPLT `resolve_inherited_*` hoist still has not happened — now ten independent chain-walk sites, and the #4086 fix added two more instead of consolidating
- **Dimension**: Population Boundary
- **Game**: all templated families (FO3/FNV/FO4/Skyrim)
- **Source**: not numeric — `docs/audits/AUDIT_CHARACTER_2026-09-11.md`'s structural remedy ("hoist one `resolve_inherited_stats`/`resolve_inherited_traits` pair into `spawn_placement_root`… so a sixth site cannot repeat it"), re-verified against HEAD source.
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1592-1618` (`spawn_placement_root` — unchanged shape, still hands the raw `&NpcRecord` to all four stamps at :1613-1616); independent resolve sites at `byroredux/src/npc_spawn.rs:89,168,219-220,1184`; `byroredux/src/npc_spawn/ai_package.rs:534-538`; `byroredux/src/npc_spawn/resumable.rs:461-463,673-675,1305-1306`; `crates/plugin/src/esm/records/actor_value_derive.rs:197,209` (+ new `resolve_inherited_field` walks at :347-366); `byroredux/src/cell_loader/references/mod.rs:683`; `byroredux/src/inventory.rs:289`
- **Status**: Existing: AUDIT_CHARACTER_2026-09-11b D5-06 (unfiled — no GitHub issue was ever created; recommend `/audit-publish` file it this time)
- **Description**: Fresh count on HEAD: the nine previously-counted sites all remain (renumbered); #4086's fix (`702b0b020`, 2026-09-15) **added** a `resolve_inherited_field` helper with two more independent chain-walks inside `derive_stored_actor_values`; a tenth site exists on the player-inventory path (`inventory.rs:289`, outside the prior count's scope). The eighth episode of the class (#2956, #3381, #3382, #3480, #4091, #4092, #4093, #4086) — each fixed correctly on inspection, each in the fix-by-addition shape this finding warned about. A runtime-path NPC now walks the TPLT chain ~6× per spawn, plus the new per-field walks per FO4 spawn.
- **Impact**: Every current consumer is individually correct (all 27 `actor_value_derive` tests green, including the four #4091/#4092/#4093/#4086 regressions), but the type system still lets a new population-boundary read take `npc.<field>` directly and compile — and contributors demonstrably keep reaching for a fresh `resolve_inherited_*` call.
- **Related**: #2956, #3381, #3382, #3480, #4091, #4092, #4093, #4086 (all CLOSED); #4137 (OPEN, D5-07 — unchanged, not re-filed).
- **Suggested Fix**: Resolve once in `spawn_placement_root` (`stats`/`traits`/`factions` via the existing helpers) and change the four stamps' signatures to accept resolved records instead of `(npc, index)`; hand `apply_ai_package_behavior` and the `build_npc_equip_state` callers the same. `resolve_inherited_field` can fold behind one resolved-record type, making "which record shape do I have" a compile-time question.

#### D6-01 (merged with D1-03): `character/mod.rs` docstring — the layer's entry point — still says Skyrim's ruleset builder is "not yet reachable"
- **Dimension**: Coverage & Doctrine
- **Game**: Skyrim SE (claim is about the layer as a whole)
- **Source**: `crates/core/src/character/profile.rs:61,140,203` (`RulesetBuilder::Skyrim => skyrim_ruleset(resolve)`), pinned by `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst` (profile.rs:305) and the Skyrim ROSTER_CASES row
- **Location**: `crates/core/src/character/mod.rs:61-65`
- **Status**: NEW (doc drift introduced by `e13985dfc` leaving this sentence behind; found independently by Dimensions 1 and 6 — merged here; Dim 1 rated LOW, Dim 6 MEDIUM, merged at MEDIUM for the entry-point rationale below)
- **Description**: The docstring ends "(Oblivion's and Skyrim's do, and are not yet reachable: #2961's matrix row)." The Skyrim half is false since #3848 (2026-09-12): the builder arm is present and matched, `build_character_ruleset` is called from the live cell-reference spawn path, and the corpus gate pins 2 derived rows against real Skyrim.esm. `mod_docstring_indexes_every_sub_module` checks module *names* only, so the drift is test-invisible. Adjacent (same docstring, same fix pass, folded in): the `[`components`]` bullet under-lists — it omits the BUILT `FactionReputation`/`FactionStanding`/`PerkRank` that `mod.rs:93-95` re-exports.
- **Impact**: The docstring is the entry point every future CHARAL contributor reads first, and the sentence is precisely the "is game X wired?" answer it promises to give. A contributor reading only it would conclude Skyrim still needs #3848-style wiring — the exact misreading that cost Skyrim five weeks per #3848's own commit message.
- **Related**: #3848, #4355 (the post-#3848 sweep that missed this), #2959.
- **Suggested Fix**: Reword to "(Oblivion's does and is deliberately blocked on a pre-AVIF resolver (#3768); Skyrim's is wired (#3848))". Add `FactionReputation` to the components bullet. Consider extending the docstring guard to fail on "not yet reachable" unless the named builder is actually `RulesetBuilder::None`.

#### D6-02 (merged with D4-01 + D6-03): post-#3848 regen doc-rot cluster — deleted `oblivion_pool_regen_config` cited as live, and the false "inserted when a live `CharacterRuleset` lands" trigger phrasing
- **Dimension**: Pools, Afflictions & Reputation / Coverage & Doctrine
- **Game**: all (regen); Oblivion (capture)
- **Source**: `byroredux/src/boot/schedule/update.rs:378-387` (the real registration), `crates/core/src/character/regen.rs:122-134` (the accurate struct-level doc), `git show e13985dfc` (the deletion)
- **Location**: `docs/engine/charal-oblivion-ruleset.md:411-421` (cites `byroredux/src/main.rs` registration — no longer exists — and "the new `oblivion_pool_regen_config` in `tes.rs`" — deleted); `docs/engine/charal.md:265` (§4.7 parenthetical names the deleted builder); `byroredux/src/boot/world.rs:41-43` and `crates/core/src/character/regen.rs:153-157` (both say `PoolRegenConfig` arrives "when a live `CharacterRuleset` lands" — a live ruleset has landed on FO3/FNV/FO4/Skyrim since #3170/#3848 and no config follows, because the config is Oblivion-shaped and genuinely blocked on #3768)
- **Status**: NEW (three dimension findings merged — one root cause, one fix pass; rated MEDIUM per the D6 rubric: a doc actively asserts a false code location/state in a way that misleads a contributor's next change)
- **Description**: Four sites (two capture/spec docs, two code comments) still describe the pre-#3848 regen world. The stated trigger condition ("when a live CharacterRuleset lands") is false as written: rulesets land on four games and the config never appears. The Oblivion capture — the audit's own ground truth for that family — directs a future "wire Oblivion regen" contributor to a file that no longer schedules systems and a function that no longer exists.
- **Impact**: A regen-wiring pass would grep for a constructor that isn't there and look for "did the ruleset land?" as a success condition that is already satisfied on four games with regen still inert. Comment/doc only; no runtime behavior.
- **Related**: #4107 (CLOSED, doc-only — its factual substance "no production insertion" remains true), #4109 (fixed sibling `boot.rs` refs — the `main.rs` copy evaded that grep), #4355 (fixed charal.md:353, not :265), #3848, #3768, #3855.
- **Suggested Fix**: Reword the two comment sites to the (correct) update.rs phrasing ("arrives only with Oblivion's wiring — blocked on the pre-AVIF resolver, #3768"); repoint the capture's registration path to `boot/schedule/update.rs` and replace the builder sentence with regen.rs:130-134's "deleted under #3848, `git show` recovers it" wording; fix charal.md §4.7's parenthetical likewise.

### LOW

#### D1-02: `CharacterRulesProfile` used as a game-identity oracle outside the character layer
- **Dimension**: Ruleset Seam · **Game**: FO3 vs FNV (discrimination) · **Source**: charal.md §2; profile.rs:64-68 ("This is deliberately data: consumers do not branch on game identity") · **Location**: `byroredux/src/cell_loader/references/attach.rs:273-282` (xNVSE script-dialect selection); `crates/plugin/src/consumables.rs:92-94` (CTDA fn-586 gate) · **Status**: NEW
- Both non-character consumers `==`-compare the profile for data that is *not* character-ruleset data (script-extender dialect; condition-function semantics) — the profile is being borrowed as the codebase's only FO3-vs-FNV discriminator. No wrong behavior today (classification pinned against real headers), but a future profile restructure changes script-dialect selection and condition gating as an untested side effect. Fix: document the double duty on the profile + pin both consumers with tests, or promote the data to policy rows of their own.

#### D1-04: `tes.rs` armor-rating comment quotes "OpponentArmorSkill" — perspective-relative naming that contradicts the capture's resolved reading the code implements
- **Dimension**: Ruleset Seam · **Game**: Oblivion · **Source**: charal-oblivion-ruleset.md §"The Complete Damage Formula" item 3 (`ArmorSkill` = the wearer's own governing armor skill) · **Location**: `crates/core/src/character/tes.rs:50-57` · **Status**: NEW (comment-only)
- The code correctly reads the actor's own Light/Heavy Armor AV (worked values 0.675/0.48 verified), but the inline quote carries UESP's attacker-perspective "Opponent" naming — the one misreading a future consumer implementer could make. Fix: quote as "wearer's ArmorSkill" with a parenthetical about UESP's perspective.

#### D2-02: `RoundMode` doc overgeneralizes "Bethesda floors Health" beyond the one sourced case
- **Dimension**: Derived Formulas · **Game**: FO3/FNV/FO4 · **Source**: FO4 only — charal-fo4-ruleset.md:87-88; the FO3/FNV capture states no rounding mode · **Location**: `crates/core/src/character/derived.rs:96-98` vs `fallout.rs:159-163,198-202` (FO3/FNV rows correctly ship `None`) · **Status**: NEW
- The comment is the nearest reference a new-row implementer reads; an unqualified "Bethesda floors Health" teaches an unsourced default. Behavior-neutral today. Fix: scope the sentence to FO4 and note FO3/FNV ship `None` (exact for their integer domain).

#### D2-03: The `DerivedScope` consumer contract is honored by one of the two live consumers
- **Dimension**: Derived Formulas · **Game**: all · **Source**: `derived.rs:124-132` — DerivedScope's own contract ("a consumer that computes a derived stat for an arbitrary entity checks this before trusting the result") · **Location**: `byroredux/src/combat.rs:432-460` (`melee_damage_charal_bonus`, no scope check) vs `crates/scripting/src/condition.rs:505-533` (checks `scope == ActorGeneral && kind == Absolute`) · **Status**: NEW
- Coincidentally correct today (the only row it reads is ActorGeneral), but nothing enforces the contract, and the skipping consumer is the newer of the two. The containment of the #2937 player-only deferrals rests on consumers consulting `DerivedScope`. Fix: add the (currently tautological) scope filter or a doc-comment duty in `melee_damage_charal_bonus`.

#### D3-01: FO76 and Starfield profile rows claim `NpcStatModel::Stored` with no capture support
- **Dimension**: Leveling & Progression · **Game**: FO76, Starfield (both ruleset-unwired) · **Source**: charal-fo76-ruleset.md:121-124 ("NPC SPECIAL storage for FO76: **not researched**"); the Starfield capture is silent on stat storage · **Location**: `crates/core/src/character/profile.rs:151-165` · **Status**: NEW
- Both unwired families nonetheless author a wired NPC stat model; the profiles are selectable at parse time via HEDR detection, so loading such a master would route NPC_ records through the FO4 PRPS/DNAM wire format with no capture saying the records carry that layout. Silent presumption a future wiring would inherit as if verified. Fix: capture a source per row or make them `NpcStatModel::None` with the "blocked, not forgotten" comment pattern the Oblivion arm already uses.

#### D3-02: Skyrim NPC pool derivation implements 2 of the capture's 3 composition terms — leveled NPCs get flat pools, and the omission is undocumented in place
- **Dimension**: Leveling & Progression · **Game**: Skyrim (wired) · **Source**: charal-skyrim-ruleset.md:603-609 (3-part composition: race base + per-NPC offset + "0-10/level from class"; capture explicitly defers the class term) · **Location**: `crates/core/src/character/profile.rs:129-132` + `crates/plugin/src/esm/records/actor_value_derive.rs:288-313` · **Status**: NEW (coverage gap; the code models exactly the captured parts — implementing the third now would violate no-guessing)
- A level-40 NPC derives the same pools as a level-1 NPC with the same race and offset. Neither the profile row comment nor the derive fn mentions the omitted term (unlike regen.rs's documented-gap style). Fix: a one-line comment naming the omitted term + capture line; research thread for the per-class table before leveled-NPC gameplay lands.

#### D3-03 / D6-04 (merged): `charal.md` §5 cites `profile.rs:82-87` for the OBLIVION const — the const now lives at 92-98
- **Dimension**: Coverage & Doctrine · **Game**: Oblivion · **Source**: `profile.rs:92-98` (the live const; shifted by #3390's `creature_stats` field) · **Location**: `docs/engine/charal.md:348` · **Status**: NEW (found independently by Dimensions 3 and 6 — merged)
- The claim (`RulesetBuilder::None`) is still true and test-pinned; only the line-range rotted. Fix: re-point, or drop line ranges in favor of symbol names (ranges rot; `CharacterRulesProfile::OBLIVION` doesn't).

#### D4-02: `AfflictionTable` tie-break asymmetry between `band_for` (last-on-tie) and `band_by_key` (first-match) on duplicate thresholds
- **Dimension**: Pools, Afflictions & Reputation · **Game**: family-agnostic (mechanism) · **Source**: the mechanism's own contract (affliction.rs:68-72 "Band order is not significant") · **Location**: `crates/core/src/character/affliction.rs:97-104` vs `:114-116` · **Status**: NEW
- On a malformed table with two bands sharing a `min_pool`, classification (`max_by`, last-on-tie) and penalty application (`find`, first-match) can select different bands. No compounding or leak is possible (apply/reverse both route through `band_by_key`), no shipped table exists, and duplicate thresholds are authoring errors — but no current test can catch it. Fix: validate/reject duplicate `min_pool` at construction, or make the two selectors agree; pin with a one-line test.

#### D6-05: ROADMAP known-issues bullet still says "only FO4 and FNV reach an actor" and Skyrim has "no construction site"
- **Dimension**: Coverage & Doctrine · **Game**: all · **Source**: `docs/feature-matrix.md:259-266` (current, accurate); `profile.rs` arms · **Location**: `ROADMAP.md:1227` · **Status**: NEW
- Four claims in the date-stamped bullet are now false: #2941 CLOSED (FO3 shadowing), #3848 CLOSED (Skyrim wired — four games reach an actor), `boot.rs` is a dead path (#4109 class). The still-true parts (regen config never inserted; affliction never registered; FO76 no builder) are why it stays `[ ]`. Fix: update in place with a closure annotation; the surviving gap is Oblivion (#3768) + regen config + affliction + FO76.

#### D6-06: FNV/FO3 capture's derived-stat Status column marks 7 of its 8 implemented rows "LOCKED"
- **Dimension**: Coverage & Doctrine · **Game**: FO3 / FNV · **Source**: `fallout.rs:45-85,155-215`; corpus gate pins `derived_rows = Some(8)` per real master · **Location**: `docs/engine/charal-fnv-fo3-ruleset.md:89-101` (Status column) · **Status**: NEW
- Inverse-direction doc lag: the code is ahead of the capture. A reader auditing coverage from the Status column undercounts FO3/FNV by seven rows. Fix: flip the seven cells to BUILT (keeping their provenance parentheticals) or annotate the table.

#### D6-07: feature-matrix regen row names "Health/Magicka/Stamina" — the built tick is Fatigue/Magicka only
- **Dimension**: Coverage & Doctrine · **Game**: all · **Source**: `regen.rs` (`PoolRegenConfig { fatigue_avif, magicka_avif, willpower_avif }`); charal.md §4.7 ("Health… deliberately unmodelled") · **Location**: `docs/feature-matrix.md:265` and `:341` · **Status**: NEW
- The row is exactly what someone reads to scope "wire regen", and it implies Health/Stamina support that exists on no path. Fix: relabel to "(Fatigue/Magicka)"; note Health/Stamina rates are unsourced/unmodelled per charal.md §4.7.

## Known-Open Register

Restated per the audit protocol; confirmed not re-filed by any dimension this run:

1. **FNV/FO3 tag-skill per-level formula** — still undocumented and deliberately
   deferred; Dimension 5 confirmed the module doc's explicit deferral survives
   and the formula is absent, not guessed. CLAS SPECIAL still lives in `ATTR`,
   not `DATA`.
2. **FO3↔FNV divergent player Health/AP** — still deferred pending master-name
   disambiguation with the player actor. Scope containment verified this run:
   all player formulas ship `.player_only()`, NPCs read carried values through
   `GetActorValue`'s fast path, and the AP rows' #2937 conservative choice is
   disclosed in place (D2-01 asks for the same discipline on three sibling
   rows).
3. **VATS runtime** (AP pool/regen, time-pause, limb health, hit-chance roll)
   — still does not exist; only the AP formulas live in CHARAL. Not
   re-investigated as new.
4. **#4137 (OPEN)** = D5-07 (six `template_flags` bits parsed, stored, no
   consumer — transparently disclosed in the doc comment) — unchanged, not
   re-filed.
5. **#4232 (OPEN, FNV-D4)** — `effective_actor_level` returns 0 verbatim,
   emptying leveled lists; a different consumer (LVLN) outside this audit's
   fix scope. This run re-verified the single-implementation guard #3171
   holds: one definition, `.max(0)` rule intact, no fifth copy, guard test
   imports through the plugin crate.
6. **D5-06** (this report) — the one prior-audit finding that remains open as
   a report finding with no GitHub issue. It worsened (see above);
   `/audit-publish` should file it.

## Cross-Audit Routing

- Component storage/shape → `/audit-ecs`
- AVIF/CLAS/NPC_/RACE/CREA byte accounting → `/audit-esm` Dim 4
- CTDA condition evaluation (incl. the `GetActorValue` arm and fn-586
  semantics, D1-02) → `/audit-scripting`
- Scheduler access declarations → `/audit-concurrency` Dim 4 (this run
  spot-checked `pool_regen_tick_system`'s declaration-matches-body and the new
  `restoration_system`'s lock order under #3441's rule — both correct; full
  concurrency ownership remains there)
- D5-08's fix site (player stamping in `scene.rs`/`inventory.rs`) spans the
  P2/P3 gameplay slice, which has no owner audit — the finding is CHARAL's
  (population boundary), the fix will touch un-owned code.
