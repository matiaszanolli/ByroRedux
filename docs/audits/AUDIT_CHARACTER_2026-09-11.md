# Character / CHARAL Audit — 2026-09-11

**Scope**: `/audit-character`, all 6 dimensions, `--depth deep`, every implemented
family. Run as an **orchestrator**: six dimension agents, max 3 concurrent, merged
here. Owner slice `crates/core/src/character/` (14 modules) plus the
CHARAL-adjacent siblings `crates/core/src/combat.rs` and
`crates/core/src/stealth.rs`, the population boundary
(`byroredux/src/npc_spawn.rs` + `npc_spawn/resumable.rs`,
`crates/plugin/src/esm/records/actor_value_derive.rs`,
`crates/plugin/src/esm/records/actor/mod.rs`), and the console/runtime consumers
(`byroredux/src/commands/actor_value.rs`, `crates/scripting/src/condition.rs`,
`byroredux/src/combat.rs`).

**Repo state**: HEAD `8151cded`, branch `main`, 2026-09-11.

**Tests recorded** (read-only; nothing launched, no `byroredux` process started,
no `--ignored` ESM-parsing test run):

| Command | Result |
|---|---|
| `cargo test -p byroredux-core character` | **115 passed**, 0 failed, 638 filtered out |
| `cargo test -p byroredux-core stealth combat` (Dim 2) | **32 passed**, 0 failed |

Delta since the last full sweep (`docs/audits/AUDIT_CHARACTER_2026-08-30.md`,
HEAD `64f64480`) inside the audited slice — 15 commits:

```
byroredux/src/npc_spawn.rs                          | 144 +++------
crates/core/src/character/affliction.rs             |  29 +-
crates/core/src/character/derived.rs                |  18 +-
crates/core/src/character/regen.rs                  | 151 +++++++--
crates/core/src/character/skill.rs                  |   6 +-
crates/core/src/stealth.rs                          | 353 +++++++++++++++++++-
crates/plugin/src/esm/records/actor/mod.rs          |  39 ++-
crates/plugin/src/esm/records/actor/tests.rs        |  40 +--
crates/plugin/src/esm/records/actor_value_derive.rs | 359 +++++++++++++++++++--
docs/engine/charal-fnv-fo3-ruleset.md               |  91 +++++-
docs/engine/charal-oblivion-ruleset.md              |   5 +-
docs/engine/charal.md                               |  17 +-
```

`fallout.rs`, `tes.rs`, `skyrim.rs`, `leveling.rs`, `ruleset.rs`, `reputation.rs`,
`resistance.rs`, `components.rs`, `attribute.rs`, `profile.rs`, `mod.rs` and
`combat.rs` are **byte-unchanged** since that sweep; their constants were still
re-derived from the capture documents this pass rather than carried forward.

| Dimension | Area | New findings |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | **3** (1 MED · 2 LOW) |
| 2 | Derived-Stat Formulas (+ adjacent siblings) | **3 LOW** |
| 3 | Leveling & Progression | **4 LOW** |
| 4 | Pools, Afflictions & Reputation | **2 LOW** |
| 5 | Population Boundary | **5** (2 MED · 3 LOW) |
| 6 | Coverage, Documentation & Doctrine Drift | **2 LOW** |
| **Total** | | **0 CRITICAL · 0 HIGH · 3 MEDIUM · 16 LOW** |

## Executive summary

**19 findings — 0 CRITICAL · 0 HIGH · 3 MEDIUM · 16 LOW.** All NEW; none
duplicates an OPEN issue. The only OPEN character-labelled issue in the repo,
**#3848** (Oblivion/Skyrim rulesets production-unreachable), was cited as
Existing by four dimensions and re-filed by none.

**The numbers are in excellent shape — the best state this subsystem has been
in.** 116 constant rows were re-derived from the capture documents across
Dimensions 2, 3 and 4: **113 PASS, 0 FAIL, 3 UNSOURCED**, of which two are
already self-declared as unsourced at their definition site with a named issue.
Every shipped coefficient that a capture document covers is correct. Zero
gameplay-affecting numeric defects were found anywhere in CHARAL this pass.

**The delta's headline fix is genuine.** #3482's stealth-sourcing closure
(`847426fa`) added **91 lines of actual source text** to
`charal-fnv-fo3-ruleset.md:243-302` — the verbatim
`SoundMultiplier`/`MovementMultiplier`/`Visual`/`Light` derivations plus the
`ActionSound` and `Armor` tables — not a "still unsourced" relabel. All six
previously-flagged coefficient groups now have a document line and match the
code exactly, and `stealth.rs` replaced monotonicity-only coverage with nine
exact-value tests. The single missed coefficient one document line above the
fixed block is D2-01.

**Process health is strong.** All four of the 2026-08-30 Dimension-6
documentation findings are **FIXED** with zero carry-forwards. The
`/audit-character` SKILL.md self-audit is **clean**: 28/28 paths exist, 24/24
symbols resolve, both pinned numbers correct (`DerivedStatFormula` = 36 B;
module count = 14), all four cited commit hashes resolve, and all three
known-open claims re-verified true. The `effective_actor_level` regression guard
(#3171, twice-duplicated historically) **held**: exactly one definition,
byte-unchanged across the delta, still `.max(0)` on the plain branch, six
production call sites all importing the one copy.

**The three MEDIUMs are all one shape: a population-boundary stamp that forgets
to resolve its template chain.** D1-01 is the *fifth* instance of that exact
defect class (#2956, #3381, #3382, #3480 were the prior four), and D5-01 and
D5-02 are two more axes of the same omission — race-dependent meshes, and the
`Use Factions` / `Use AI Packages` flags. The spawn tail now resolves TPLT
correctly for `ActorValues`, `Background` and the Skyrim pools, and not for
creature attack damage, race meshes, factions or AI packages. **The structural
remedy is the same for all three**: hoist one `resolve_inherited_stats` /
`resolve_inherited_traits` pair into `spawn_placement_root` and hand every
consumer the resolved record, so a sixth site cannot repeat it.

### Merge-time correction: D1-01 downgraded HIGH → MEDIUM

Dimension 1 rated the creature-attack TPLT bypass **HIGH**, resting half its
impact on "a live consumer (`combat_damage_system`)". Dimension 5 independently
falsified that half (**D5-03**), and the merge verified it directly rather than
accepting either agent's claim:

- `attack_damage` has exactly two callers. `byroredux/src/combat.rs:636` is
  inside `mod tests` (the `damage_fixture` helper).
- The sole production `HitEvent` producer is `combat_input_system`
  (`combat.rs:250`), whose `aggressor` is
  `world.try_resource::<PlayerEntity>().and_then(|player| player.0)`
  (`combat.rs:129-131`), gated on `InputAction::Attack` and
  `PlayerMode::Character`.

**Creatures never produce a `HitEvent`**, so no wrong creature-damage number can
reach gameplay today. The bypass still writes a wrong, save-serialised value for
the majority of both bestiaries with no test covering the stamp at all — real,
but latent. MEDIUM (incorrect stored state, no current consumer), not HIGH
(incorrect behaviour). Both findings are retained and cross-linked; D5-03
carries the unreachable-consumer half.

## Constant verification summary

Full per-row tables live in the Dimension 2, 3 and 4 sections below.

| Family / area | Rows | Verdict |
|---|---|---|
| FO4 (Health / AP / Carry Weight / XP curve / rewards) | 6 | all PASS |
| FO3 (Health / AP / shared six / leveling / rewards) | 13 | all PASS |
| FNV (Health / AP / shared six / leveling / rewards) | 13 | all PASS |
| Oblivion (3 pools / armour / attribute bonus / health gain / regen / combat math) | 16 | all PASS |
| Skyrim (Light Armor / Carry Weight / pools / XP curve / skill-XP) | 10 | 9 PASS · 1 UNSOURCED (D3-04) |
| Reputation family (Karma / 39 FNV faction thresholds / 13 REPU FormIDs / FO4 Affinity / 16-cell grid) | 13 | all PASS |
| Regen / affliction / resistance | 11 | 10 PASS · 1 UNSOURCED-in-code (`MAX_REGEN_SUBSTEPS`, self-declared) |
| `stealth.rs` (FO3/FNV detection, now 12 individually verified rows) | 18 | 17 PASS · 1 UNSOURCED (D2-01) |
| `combat.rs` (Oblivion damage math) | 3 | all PASS |
| Structural (`eval` shape, cap sentinel, `DerivedInput` sentinels, scope tagging, cross-term audit, 36 B pin) | 13 | all PASS |
| **Total** | **116** | **113 PASS · 0 FAIL · 3 UNSOURCED** |

Notably re-confirmed this pass, each by evidence rather than by carry-forward:
the 4×4 Fame/Infamy grid is **not** transposed (checked via the asymmetric
off-diagonal pair, not the corners); Karma's asymmetric −249/−250 boundary is
exact; all 39 FNV faction threshold numbers and all 13 REPU FormIDs match in
document order; the FO4 Health cross term is still the **only** non-zero `cross`
in any shipped row; `DerivedStatFormula` is still `Copy` + 36 B; and the Skyrim
GMST overlay still requests exactly `["fXPLevelUpBase", "fXPLevelUpMult"]` with
no third GMST crept back in.

**Not verified**: FO76 and Starfield numbers (no ruleset builder — both are
`NpcStatModel::Stored` + `RulesetBuilder::None`); Oblivion's pre-`AVIF` legacy
actor-value index resolution (`Oblivion.esm` authors no `AVIF` group and no
resolver exists); `AfflictionTable` threshold numbers (none sourced for any game,
every table ships empty — `charal.md` §4.6 PENDING). **Not re-derived**: the
deferred FNV/FO3 tag-skill per-level formula.

### Coverage matrix

Legend: **implemented** = a `*_ruleset()` builder / `LevelingModel` const / derived table exists
and compiles. **wired** = reachable from the live load/spawn/tick path
(`CharacterRulesProfile::build_ruleset` → `build_character_ruleset` →
`cell_loader/references/mod.rs:342`, or a production scheduler registration), not merely
buildable. Evidence for every cell is `crates/core/src/character/profile.rs:72-147`
(the seven `CharacterRulesProfile` consts and their `RulesetBuilder` arm),
`profile.rs:172-186` (`build_ruleset`'s `RulesetBuilder::None => return None`),
`leveling.rs:112-159` (the five `LevelingModel` consts) and the `push_derived` census below.

| Game family | Ruleset implemented | Ruleset **wired** | Derived stats implemented | Leveling model implemented | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|
| **FO3** | ✓ `fallout3_ruleset` (`fallout.rs:155`) | ✓ `RulesetBuilder::Fallout3` | ✓ **8 rows** (Health, AP + the 6 shared) | ✓ `LevelingModel::FO3` (`150·L+50`, cap 20) | ✗ | ✗ |
| **FNV** | ✓ `falloutnv_ruleset` (`fallout.rs:194`) | ✓ `RulesetBuilder::FalloutNewVegas` | ✓ **8 rows** | ✓ `LevelingModel::FNV` (`150·L+50`, cap 30) | ✗ | ✗ |
| **FO4** | ✓ `fallout4_ruleset` (`fallout.rs:126`) | ✓ `RulesetBuilder::Fallout4` | ✓ **3 rows** (Health, AP, Carry Weight) | ✓ `LevelingModel::FO4` (`75·L+125`, uncapped) | ✗ | ✗ |
| **FO76** | ✗ no builder | ✗ `RulesetBuilder::None` | ✗ — capture has 13 LOCKED rows, none coded | ✗ — capture has `160·L−120` LOCKED, no `LevelingModel::FO76` | ✗ | ✗ |
| **Oblivion** | ✓ `oblivion_ruleset` (`tes.rs:101`) | ✗ `RulesetBuilder::None` — **#3848 (OPEN)** | ✓ **8 rows / 5 stats** — exercised only by a synthetic in-crate resolver | ✓ `LevelingModel::OBLIVION` (10 major-skill-ups) | ✗ `oblivion_pool_regen_config` (`tes.rs:159`) has **no caller** | ✗ |
| **Skyrim SE** | ✓ `skyrim_ruleset` (`skyrim.rs:116`) | ✗ `RulesetBuilder::None` — **#3848 (OPEN)** | ✓ **2 rows** (Light Armor, Carry Weight) — synthetic resolver only | ✓ `LevelingModel::SKYRIM` (`25·L+75`) | ✗ | ✗ |
| **Starfield** | ✗ no builder | ✗ `RulesetBuilder::None` | ✗ — roster LOCKED, derived rows uncaptured | ✗ — curve + tier thresholds PENDING (`charal.md` §9) | ✗ | ✗ |

Derived-row census (re-counted this pass from `push_derived` call sites, loops expanded):
FO3/FNV `add_fnv_fo3_shared` = 4 direct + 2 from the `Affliction::ALL` loop = 6 shared, + Health + AP = **8**;
FO4 = **3**; Oblivion = Health + Magicka + 4 Fatigue (loop) + 2 = **8**; Skyrim = **2**. All within the
6–10 range the flat-`Vec` rationale claims (Skyrim/FO4 below it, which is the harmless direction).

**What each gap means**

- **Regen: zero families, all seven.** `pool_regen_tick_system` *is* registered
  (`byroredux/src/boot/schedule/update.rs:304-311`, `add_exclusive_with_access`) and its
  `PoolRegenAccumulator` *is* inserted unconditionally (`byroredux/src/boot/world.rs:48`), but
  `PoolRegenConfig` is inserted only inside unit tests. The one builder that could produce it,
  `oblivion_pool_regen_config`, has zero callers — so the tick runs every frame and early-returns
  forever. This is documented at all three sites and in `feature-matrix.md:288-292`; it is
  coverage information, not a defect, and is **not** re-filed.
- **Affliction: zero families, and worse than regen** — `affliction_tick_system` has no
  production scheduler registration at all (only `save_io/registry_completeness_tests.rs:196`
  names it, as a "forward-latent" classification). Also documented in `feature-matrix.md:292`.
- **Oblivion / Skyrim unwired** is **#3848 (OPEN)** — not re-filed. The structural extra for
  Oblivion (no `AVIF` group in `Oblivion.esm`, so wiring needs a legacy actor-value resolver
  first) is now correctly stated in `charal.md:343-349` and `charal-oblivion-ruleset.md:7-13`;
  that was last sweep's D6-01 and it is fixed.
- **Skyrim's and Oblivion's derived output keys remain unfalsifiable by construction** —
  `ROSTER_CASES` asserts `derived_rows: None` for both (i.e. asserts `build_ruleset` returns
  `None`), so the only thing exercising their output EditorIDs is an in-crate resolver that
  enumerates the strings the builders pass. Carried forward from the 2026-08-30 matrix as
  coverage, not re-filed (it is the same fact as #3848 seen from the test side).
- **FO76 and Starfield have captures and no code — but both carry explicit notes**, so this is
  not silent scope loss: `charal-fo76-ruleset.md:6-9` ("Not yet in the CHARAL §8 rollout order")
  and `charal-starfield-ruleset.md:18` + `charal.md:606-611` / §9 (curve and tier thresholds
  unpublished). `feature-matrix.md:248-256` gives both `✗`/`~` columns. See "dropped" below.

---


## Findings index — by severity

### MEDIUM (3)

| ID | Title | Dimension | Game |
|---|---|---|---|
| **D1-01** | `stamp_creature_attack` reads the shell record, bypassing the `Use Stats` TPLT resolution its three sibling stamps all perform | Ruleset Seam | FO3 / FNV |
| **D5-01** | The spawn tail resolves the `Use Traits` chain for `Background` and the Skyrim pools, and ignores it for every mesh that depends on race | Population Boundary | all templated |
| **D5-02** | `Use Factions` and `Use AI Packages` are parsed, have live consumers, and are never resolved — the shell's own empty list wins | Population Boundary | all templated |

All three are the same defect class — a consumer at the population boundary
reading the unresolved shell instead of the template-resolved record — and all
three are fixed by the same structural change (one `resolve_inherited_stats` /
`resolve_inherited_traits` pair hoisted into `spawn_placement_root`). D1-01 is
the fifth recorded instance of this class (#2956, #3381, #3382, #3480).

### LOW (16)

| ID | Title | Dimension |
|---|---|---|
| D1-02 | No `ROSTER_CASES` entry for the FO76/Starfield profiles, whose `Stored` model resolves two AVIF EditorIDs falsified only against `Fallout4.esm` | Ruleset Seam |
| D1-03 | `probe_combat_fixture` re-derives actor level as `npc.level.max(1)` — the exact divergence #3081/#3171 rejected twice | Ruleset Seam |
| D2-01 | `DetectorState`'s state→multiplier assignment is unsourced — the gap #3482 closed for `Sound`/`Visual`, left open one document line above it | Derived Formulas |
| D2-02 | `charal-fnv-fo3-ruleset.md` still repeats the three expired blocker claims that #3878 corrected in `stealth.rs` only | Derived Formulas |
| D2-03 | `charal-skyrim-ruleset.md` still pins `DerivedStatFormula` at 32 bytes and names a test that no longer exists | Derived Formulas |
| D3-01 | `charal.md` §4.2 declares `CharacterLevel.xp` as `f32`; the shipped component has always been `u32` | Leveling |
| D3-02 | `charal-oblivion-ruleset.md` cites `AttributeSet::OBLIVION`, a symbol that exists nowhere (it is `TES_CLASSIC`) | Leveling |
| D3-03 | `tes.rs`'s docstring calls the Oblivion per-level Health accrual "deferred" — the same file implements it 200 lines below | Leveling |
| D3-04 | The Skyrim skill-XP cost curve is captured in no `charal-*-ruleset.md`; `fSkillUseCurve` is filed only under *Oblivion* | Leveling |
| D4-01 | #3767 declared band order insignificant, but `ActiveAffliction` still stores a raw `Vec` index and `reevaluate_affliction` indexes it unchecked | Pools & Afflictions |
| D4-02 | `pool_regen_tick_system` evaluates the max-pool formula at a hardcoded `level = 1`, discarding the `CharacterLevel` it has the entity id to read | Pools & Afflictions |
| D5-03 | `6b73c84d`'s `CreatureAttack` reaches the right component on the right entity and has no reachable reader | Population Boundary |
| D5-04 | No test falsifies `derive_skyrim_actor_values`'s per-pool independence — the `Option<f32>` contract is asserted in prose only | Population Boundary |
| D5-05 | Three CHARAL rows in `NOT_SAVED_BY_DESIGN` claim a boot installation that no site performs | Population Boundary |
| D6-01 | `charal.md` still documents the deleted `SkillSet::FALLOUT_FO3_FNV` and its merged 15-skill roster, at two sites | Coverage & Doctrine |
| D6-02 | Four CHARAL-scope references to the deleted `byroredux/src/boot.rs` survive the boot-module split | Coverage & Doctrine |

**Documentation-defect cluster**: eight of the sixteen LOW findings (D2-02,
D2-03, D3-01, D3-02, D3-03, D3-04, D6-01, D6-02) are capture-document or
docstring drift. Three of them — D2-03, D3-02, D6-01 — are *surviving copies* of
facts that were corrected elsewhere: the 32-byte pin, the stale symbol name and
the retired FO3↔FNV roster collapse each had one copy fixed and another missed.
That pattern argues for a repo-wide grep pass per correction rather than a
per-file edit, and it is the cheapest batch in this report to close.

---

## Dimension reports

Each section is the dimension agent's full report: checks performed with
evidence, the constant-verification table, findings, and the candidates it
verified and dropped. The dropped lists are deliberately preserved — they stop
the next sweep re-chasing a falsified premise.


## Dimension 1 — Ruleset Seam & CHARAL Doctrine

**Repo state**: HEAD `8151cded`, branch `main`. Read-only pass. No engine process
launched, no census probe written (system memory was at ~2–3 GB available for the
whole sweep; parsing a 245 MB master was judged an unacceptable OOM risk per
`plugin_ignored_tests_oom` — every population figure below is therefore quoted
from an in-code measured census, never invented).

**Tests recorded**: `CARGO_BUILD_JOBS=2 cargo test -p byroredux-core character`
→ **115 passed, 0 failed**, 638 filtered out. Matches the brief exactly.

### Checks performed

1. **The doctrine check — no game-identity branch in any `CharacterRuleset`
   consumer.** PASS.
   Enumerated every reference to `CharacterRuleset` workspace-wide, then grepped
   each consumer for `GameKind` / `game ==` / `game_kind` / `master_name`:
   - `crates/core/src/character/**` — zero hits outside doc comments
     (`mod.rs:53-54`, `profile.rs:3` *describe* the seam, they do not branch on it).
   - `crates/scripting/src/condition.rs` — the only `Fallout`/`Skyrim` string
     literals are at lines 1103/1140/1589/1646, all inside `mod tests` (starts at
     line 895). Production arms read `try_resource::<CharacterRuleset>()` and
     `derived_value` with no game predicate.
   - `byroredux/src/combat.rs:453`, `crates/core/src/character/regen.rs:201`,
     `byroredux/src/commands/actor_value.rs`, `byroredux/src/cell_loader/references/mod.rs:339`
     — resource lookups, no game predicate.
   - **The rewritten `npc_spawn.rs`** (checked explicitly, as instructed): its
     `GameKind` matches (lines 305-312, 385-423, 490-497, 641-646, 787, 805) are
     all *asset-path / skeleton / idle-KF / biped-mask* dispatch — NIFAL-side
     seams, not ruleset consumption. Nothing in that file branches on game
     identity to pick a stat rule.
   - **The +359-line `actor_value_derive.rs`** (checked explicitly): the arm is
     still selected from `index.character_rules.{npc_stat_model,creature_stat_model}()`
     (lines 211-223) — a data row, not a game id. `is_creature` is a *record kind*
     predicate, which is the documented exception.
   - The one legitimate `GameKind` → policy translation is at the **parser
     boundary**: `character_rules_profile` (`crates/plugin/src/esm/records/mod.rs:156-167`),
     called once from `parse_esm_with_load_order:209`. That is exactly where the
     doctrine says the match belongs.
   - `actor/mod.rs`'s many `GameKind` arms are wire-layout (ACBS offsets 14/18/22,
     CREA `DATA` length, Oblivion `XNAM`) — `Imported*` tier, explicitly "allowed
     to be messy".
2. **Single sink.** PASS.
   `build_character_ruleset` (`byroredux/src/npc_spawn.rs:228-234`) survives the
   −144-line surgery intact and has **exactly one** production caller:
   `byroredux/src/cell_loader/references/mod.rs:342`, behind a
   `try_resource::<CharacterRuleset>().is_none()` idempotence gate. Grep for
   `insert_resource(CharacterRuleset` finds one production site
   (`references/mod.rs:343`) and two test sites (`condition.rs:1347`,
   `combat.rs:708/862`). `build_ruleset` itself is called from `npc_spawn.rs:233`
   plus tests. The two commits that shrank the file (`211a23cc`, `6b73c84d`,
   `8175cb70`) deleted `spawn_npc_entity` / `spawn_prebaked_npc_entity` — dead
   `#[allow(dead_code)]` compatibility shims over `NpcSpawnJob` — and added
   `stamp_creature_attack`; neither touched ruleset construction.
   No production path writes a derived stat straight into `ActorValues`: the only
   production `ActorValues::from_pairs` is `npc_spawn.rs:105` (fed by
   `derive_npc_actor_values`), and the only `set_base`/`set_*` mutators are the
   deliberate live-edit console command (`commands/actor_value.rs:84`) and the mod
   SDK's `ActorValueOperation` (`extensions/commands.rs:435`).
   `build_melee_damage_config` is still a resolved-id resource, not a second
   ruleset.
3. **Derived-table N.** PASS — every implemented game still inside the flat-`Vec`
   rationale. Measured from the builders and pinned by tests:
   FNV 8 rows, FO3 8 (`fallout.rs:317-318`), FO4 3 (`records/tests.rs:129`),
   Skyrim 2 (`skyrim.rs:184`), Oblivion 8 rows / 5 stats. Nothing grew toward
   "dozens"; no game is near a hot-path scan problem (`derived_value` is called
   per-condition, not per-frame-per-actor).
4. **Output keys are AVIF FormIDs in global load-order space.** PASS.
   Every builder resolves through `EsmIndex::actor_value_form_id`
   (`index.rs:711-730`), which returns `AvifRecord::form_id`. Those come from
   `extract_records` → `RecordHeader::form_id`, which `EsmReader::read_record_header`
   (`reader.rs:701`) routes through `remap_form_id` — pinned by
   `read_record_header_applies_installed_remap`. Checked the analogue of
   `1ee804c2`'s gap on the **actor-value** path specifically:
   - FO4 `PRPS` `(AVIF FormID, value)` pairs — `remap_fid(avif, remap)` at
     `actor/mod.rs:1379`. Remapped.
   - `NpcRecord::race_form_id` (RNAM), the input to the Skyrim arm's
     `index.races` lookup — `remap_fid` at `actor/mod.rs:1018`. Remapped.
   - `class_form_id` (CNAM), the auto-calc arm's `index.classes` key — remapped
     at parse time (#1996), documented at `actor_value_derive.rs:86-88`.
   No raw plugin-local id reaches an actor-value key.
5. **Roster split (membership ENGINE-SUPPLIED / FormIDs AUTHORED).** PASS.
   No hex FormID literal anywhere in the non-test bodies of `attribute.rs` or
   `skill.rs` (the only `0x` occurrences, `skill.rs:142-143`, are inside a prose
   comment citing `AVMysticism` 0x45B). Rosters are `&'static [SkillDef]` /
   `&'static [Attribute]` of EditorID strings; every count is `members.len()` on
   a static array, never derived from parsed data. `skill.rs`'s 6-line delta is
   **pure `cargo fmt` comment re-indentation** of the #3169 note inside
   `mod tests` — no semantic change (diff verified against `64f64480`).
6. **`effective_actor_level` — the #3171 regression guard.** PASS, not regressed.
   **Exactly ONE definition**, `crates/plugin/src/esm/records/actor/mod.rs:96-102`,
   byte-unchanged in this delta (`git diff 64f64480..HEAD` on that file touches
   only `parse_race`'s remap, two doc blocks and an `NpcFaceGenRecipe` comment).
   Branch bodies are `npc.calc_min.max(1)` (PC-level-mult) and **`npc.level.max(0)`**
   (plain) — the required `.max(0)`, not `.max(1)`.
   Re-exported through `records/mod.rs:48` and `npc_spawn.rs:154`.
   **Production call sites (6):** `npc_spawn.rs:181`, `npc_spawn.rs:196`,
   `npc_spawn.rs:823`, `inventory.rs:219`, `actor_value_derive.rs:194`,
   `actor_value_derive.rs:399`. Test call sites: `npc_spawn/tests.rs:1675/1684/1698/1707/1714`,
   `actor_value_derive.rs:576/806`. No fourth copy in any crate; a workspace grep
   for `effective_npc_level` returns only the two historical mentions inside
   doc comments. A workspace grep for `.max(1)` adjacent to a level finds exactly
   one non-`dds.rs` hit, and it is **not** a copy of this function — see
   finding D1-03.
7. **#3172 roster falsification against real masters.** PASS for every roster and
   every *reachable* builder's output keys, with one uncovered profile pair
   (finding D1-02).
   `ROSTER_CASES` (`crates/plugin/tests/parse_real_esm.rs:296-354`) carries the
   five entries FNV / FO3 / FO4 / Skyrim / Oblivion; `assert_rosters_resolve`
   (`:355-520`) walks, per master: header→profile classification, the GMST
   leveling-overlay probe (#3923), the attribute roster, the skill roster, and
   `derived_row_len()` against the real AVIF table. Every EditorID the current
   builders pass for FNV/FO3/FO4 is covered by the `derived_rows` count.
   **Correction to the previous sweep's coverage note**: `skyrim_ruleset`'s output
   keys are *not* wholly unfalsifiable — `records/tests.rs:141-155`
   (`skyrim_ruleset_resolves_against_the_real_master`, `#[ignore]`) resolves them
   against real `Skyrim.esm` and pins `derived_row_len() == 2`, and there is an
   equivalent `#[ignore]` pin for Oblivion's known-zero. Those tests are opt-in
   (and out of scope to run here), so the CI-visible gap stands, but the "no test
   exists" half of the note is stale.
8. **Blast-radius read of `0dcb5cf0` ("resolve both TPLT chains…").** PASS.
   Diff is confined to `actor_value_derive.rs` (+337/−22, all inside
   `derive_npc_actor_values` and its three arms plus tests). It introduced
   **no game-identity branch** (the arm is still `match model`) and **no second
   construction path** — it splits one already-resolved record into two
   already-existing resolvers (`resolve_inherited_stats` / `resolve_inherited_traits`,
   both pre-existing public functions in `equip.rs`) and adds one 6-line private
   helper (`baked_or_shell`). The `is_creature` predicate moved from the shell to
   the resolved record, which is consistent with the arm it selects.

### Constant verification

Dimension 1 owns no shipped coefficients (the numbers live in Dimensions 2–4).
The only numeric assertions in scope are structural counts, verified above:
derived rows FNV 8 / FO3 8 / FO4 3 / Skyrim 2 / Oblivion 8, roster sizes
Oblivion 21 / Skyrim 18 / Fallout 13+7, `Affliction::ALL.len() == 2`, and the
14-sub-module count pinned by `mod_docstring_indexes_every_sub_module`. All match
the builders and their tests. No unsourced number in this dimension.

### Findings

#### CHAR-2026-09-11-D1-01: `stamp_creature_attack` reads the shell record, bypassing the `Use Stats` TPLT resolution its three sibling stamps all perform
- **Severity**: MEDIUM (downgraded from HIGH at merge — see the Impact correction below)
- **Dimension**: Ruleset Seam & CHARAL Doctrine (population boundary)
- **Game**: FO3 / FNV (the only families with a `CREA` stat model)
- **Location**: `byroredux/src/npc_spawn.rs:131-145` (definition);
  `byroredux/src/npc_spawn/resumable.rs:1424-1427` (call site)
- **Status**: NEW (introduced in this delta by `6b73c84d`, "Give creatures their
  authored attack damage")
- **Source**: Not a numeric finding — no capture-document constant is involved.
  The rule it violates is CHARAL's own, `actor_value_derive.rs:154-160`: "`TPLT`
  template inheritance is resolved first for **every** stat model (#2956, #3381,
  #3382)".
- **Description**: `spawn_placement_root` calls four stamps with the same raw
  shell `&NpcRecord`. Three of them resolve the TPLT chain internally before
  reading anything — `stamp_actor_values` → `derive_npc_actor_values` resolves
  both `Use Stats` and `Use Traits` (`actor_value_derive.rs:194-207`), and
  `stamp_character_components` resolves both again (`npc_spawn.rs:179-181`).
  `stamp_creature_attack` does not: it reads `npc.creature_stats` off the
  unresolved shell. `CREA.DATA` is the creature's stat block, so it rides the
  same `Use Stats` (`0x0002`) bit that already gates `derive_creature_actor_values`
  — which *is* handed the resolved record (`actor_value_derive.rs:222`). The
  result is that one entity gets its SPECIAL and Health from the resolved
  template and its attack damage from the shell.
- **Evidence**:
  ```rust
  // resumable.rs:1424-1427 — all four handed the same unresolved `npc`
  stamp_faction_ranks(world, placement_root, npc);
  stamp_actor_values(world, placement_root, npc, index);      // resolves TPLT inside
  stamp_creature_attack(world, placement_root, npc);          // does NOT
  stamp_character_components(world, placement_root, npc, index); // resolves TPLT inside

  // npc_spawn.rs:131-134
  fn stamp_creature_attack(world: &mut World, placement_root: EntityId, npc: &NpcRecord) {
      let Some(stats) = npc.creature_stats else { return; };   // shell's DATA, not the template's
  ```
  vs. the resolved path it is supposed to match:
  ```rust
  // actor_value_derive.rs:219-222
  let stats = crate::equip::resolve_inherited_stats(npc, level, index);
  ...
  NpcStatModel::CreatureData => derive_creature_actor_values(stats, index),
  ```
  Population, quoted from the in-code census at `equip.rs:473-491` (#3390): **815
  of 1578 FNV and 399 of 533 FO3 creatures are templated**, and
  `resolve_inherited_record` grew its `index.creatures` arm precisely because
  those shells were otherwise "deriving the generic spawn-shell stat block
  instead of their authored one". Both sub-cases are wrong: a shell with no
  `DATA` gets **no** `CreatureAttack` at all (`combat.rs:402` then falls through
  to the flat `UNARMED_DAMAGE` baseline — the exact "Deathclaw hitting for 8
  instead of 125" symptom #3762 was filed to fix), and a shell with a generic
  `DATA` gets the generic damage while its Health/SPECIAL come from the template.
- **Impact**: **Corrected at merge.** This dimension originally rated the finding
  HIGH on the strength of "a live consumer (`combat_damage_system`)". Dimension 5
  falsified that half independently (`CHAR-2026-09-11-D5-03`), and the merge
  re-verified it directly: `attack_damage` has exactly two callers, one of which
  is a test fixture (`byroredux/src/combat.rs:636`, inside `mod tests`), and the
  sole production producer is `combat_input_system` (`combat.rs:250`), whose
  `aggressor` is `world.try_resource::<PlayerEntity>()` (`combat.rs:129-131`)
  gated on `InputAction::Attack` + `PlayerMode::Character`. **Creatures never
  produce a `HitEvent`**, so no wrong number reaches gameplay today.

  What remains is real and unchanged: the stamp writes a **wrong value** into
  `CreatureAttack` for templated creatures (815/1578 FNV, 399/533 FO3 per the
  `equip.rs:473-491` census), there is **no test for `stamp_creature_attack` at
  all** (every `CreatureAttack` test in `combat.rs:743-805` inserts the component
  by hand; `save_io/round_trip_tests.rs:1105` only checks registry membership),
  and the component is save-serialised — so the wrong value persists. It becomes
  wrong gameplay the instant creature-initiated attacks are wired, which is the
  express purpose of the commit that introduced it. MEDIUM: incorrect stored
  state with no current consumer, not incorrect behaviour.
- **Related**: #3762 (the fix this incompletes), #3390 (the creature stat model),
  #2956 / #3381 / #3382 / #3480 (the four prior "a stamp forgot to resolve its
  TPLT chain" defects, each measured in the hundreds-to-thousands of records),
  #4086 (OPEN — the adjacent intermediate-template sentinel gap).
- **Suggested Fix**: Resolve the chain before reading, exactly as the siblings do
  — `let stats = resolve_inherited_stats(npc, effective_actor_level(npc), index);`
  then read `stats.creature_stats` — or, better, hoist one
  `resolve_inherited_stats` / `resolve_inherited_traits` pair into
  `spawn_placement_root` and hand every stamp the record it needs, so a fifth
  stamp cannot make this mistake a fifth time. Add the census (templated creatures
  whose shell `DATA` differs from the resolved record's) before claiming a
  magnitude, and add the missing direct test for the stamp.

#### CHAR-2026-09-11-D1-02: no `ROSTER_CASES` entry for the FO76 and Starfield profiles, whose `Stored` model resolves two AVIF EditorIDs falsified only against `Fallout4.esm`
- **Severity**: LOW
- **Dimension**: Ruleset Seam & CHARAL Doctrine (test coverage)
- **Game**: FO76, Starfield
- **Location**: `crates/plugin/tests/parse_real_esm.rs:296-354` (the table);
  `crates/core/src/character/profile.rs:132-146` (the two uncovered profiles);
  `crates/plugin/src/esm/records/actor_value_derive.rs:320-333` (the two keys)
- **Status**: NEW (standing gap; the 2026-08-30 sweep's item 9 passed it over)
- **Source**: n/a — coverage finding, no constant involved.
- **Description**: `CharacterRulesProfile::FALLOUT76` and `::STARFIELD` both carry
  `npc_stats: NpcStatModel::Stored`, so `derive_stored_actor_values` runs for them
  in production and resolves the literal EditorIDs `"Health"` and `"ActionPoints"`
  against that game's own AVIF table. `ROSTER_CASES` has five entries and neither
  of those two is among them, so both EditorIDs — and the header→profile
  classification for both games — are falsified only against `Fallout4.esm`.
  This is the shape #3172 exists to prevent: the #3169 trap (`Illusion` vs the
  authored `AVMysticism`) was exactly "a string that resolves on one master and
  not on another". `test_paths` already ships `FO76_ENV`/`fo76_data_dir` (#3741)
  and `STARFIELD_ENV`/`starfield_data_dir`, so the plumbing to add the two rows
  exists.
- **Evidence**: `ROSTER_CASES` labels are exactly `"FNV" | "FO3" | "FO4" |
  "Skyrim" | "Oblivion"`. `character_rules_profile` (`records/mod.rs:163-165`)
  routes `GameKind::Fallout76 → FALLOUT76` and `GameKind::Starfield → STARFIELD`,
  both of which reach `derive_stored_actor_values`'s
  `index.actor_value_form_id("Health" | "ActionPoints")` lookups. A miss there is
  a silent skip (`if let Some(fid)`), and `stamp_actor_values` only inserts
  `ActorVitals` when the Health key is present — the same failure mode #3481
  measured at 54 undamageable FO4 actors.
- **Impact**: If either game spells those AVIFs differently, every FO76 /
  Starfield actor spawns with no Health key and therefore no `ActorVitals` —
  undamageable, with no diagnostic. Low severity only because neither game has a
  playable actor path yet.
- **Related**: #3172 (the falsification loop), #3169 (the display-name trap),
  #3481 (the undamageable-actor failure mode), #3741 (the FO76 accessor).
- **Suggested Fix**: Add two `RosterCase` rows (`derived_rows: None`,
  `authors_actor_values: true`, empty attribute set) so the existing loop asserts
  the header→profile classification and — with a two-line extension — that
  `"Health"` / `"ActionPoints"` resolve against `SeventySix.esm` and
  `Starfield.esm`.

#### CHAR-2026-09-11-D1-03: `probe_combat_fixture` re-derives actor level as `npc.level.max(1)`, the exact divergence #3081/#3171 rejected twice
- **Severity**: LOW
- **Dimension**: Ruleset Seam & CHARAL Doctrine (the #3171 regression guard, dev-tool side)
- **Game**: FO3 / FNV
- **Location**: `crates/plugin/examples/probe_combat_fixture.rs:64`
- **Status**: NEW
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:61-102` — the
  `effective_actor_level` docstring, which states the rule and names `.max(1)` as
  the rejected form.
- **Description**: Not a fourth *definition* of `effective_actor_level` — the
  function still has exactly one — but it is an inline re-derivation of the same
  decision with the rejected clamp, in a tool whose entire job is to pick combat
  fixtures. It reads `npc.level` raw, so it never consults `ACBS_PC_LEVEL_MULT`
  (`0x0080`): for the 268 FNV records where that bit is set, `level` is a
  fixed-point multiplier (round steps up to 2000), not a level. That value is
  then fed straight into `resolve_inherited_inventory(npc, actor_level, …)` and
  `expand_leveled_form_id(…, actor_level, …)` at lines 65 and 92 — the two
  level-gated filters the docstring explicitly warns about ("feeding that raw
  into a leveled-list filter makes every entry eligible, so the actor always
  draws the top tier").
- **Evidence**:
  ```rust
  // probe_combat_fixture.rs:64-65, 92
  let actor_level = npc.level.max(1);
  let inventory = resolve_inherited_inventory(npc, actor_level, &index);
  expand_leveled_form_id(entry.item_form_id, actor_level, &index, &mut resolved);
  ```
  The file's last touch is `b434e4c0` — the same commit that created the
  `effective_npc_level` copy #3171 later deleted. #3171 fixed the two production
  copies and missed this one.
- **Impact**: Dev-tool only (nothing in `examples/` ships), so no gameplay impact
  — but the probe's *output* is what a human uses to choose the vertical-slice
  combat fixture, and for PC-level-mult creatures it reports the top tier of
  every leveled list. It is also the one remaining place a future reader can copy
  the wrong rule from.
- **Related**: #3081, #3171, #2955.
- **Suggested Fix**: Replace with
  `byroredux_plugin::esm::records::effective_actor_level(npc)` (already `pub`
  and re-exported at `records/mod.rs:48`) and delete the local clamp.

### Candidates verified and dropped

1. **A game-identity branch hiding in the rewritten `npc_spawn.rs`.** Dropped —
   all seven `GameKind` matches there are asset-path / skeleton / idle-KF /
   biped-mask dispatch (NIFAL-side), none selects a stat rule.
2. **A second ruleset construction site left behind by the −144-line surgery.**
   Dropped — the deleted code (`spawn_npc_entity`, `spawn_prebaked_npc_entity`)
   was `#[allow(dead_code)]` wrappers over `NpcSpawnJob` and constructed no
   ruleset; `build_character_ruleset` still has one caller.
3. **`character_rules_profile`'s `match game` as a doctrine violation.** Dropped —
   this is the parser-boundary translation the doctrine *requires* (`GameKind` →
   one data row), called exactly once. Verified it is the only such match.
4. **The save/load path rebuilding a `CharacterRuleset`.** Dropped —
   `registry_completeness_tests.rs:197` classifies it as "immutable game-profile
   rules selected at boot"; no `insert_resource(CharacterRuleset)` exists in
   `save_io`.
5. **`0dcb5cf0` introducing a second stats-resolution path.** Dropped — it calls
   two pre-existing public resolvers in `equip.rs` and adds one private
   `baked_or_shell` helper; no new construction site, no new game branch.
6. **`skill.rs`'s 6-line delta as a semantic change.** Dropped — pure `cargo fmt`
   comment re-indentation inside `mod tests`; the assertions are byte-identical.
7. **A `.max(1)` re-added to `effective_actor_level`.** Dropped —
   `actor/mod.rs:96-102` is byte-unchanged across the delta and still reads
   `npc.level.max(0)` on the plain branch.
8. **Raw plugin-local FormIDs on the actor-value path (the `1ee804c2` analogue).**
   Dropped — `PRPS`, `RNAM`, `CNAM` and the AVIF record header are all remapped;
   checked each individually rather than trusting the module docstring.
9. **`stamp_faction_ranks` not resolving its TPLT chain.** Dropped from *this*
   dimension — `0x0004` "Use Factions" is documented at `actor/mod.rs:444-447` as
   parsed-but-no-consumer-yet, and faction ids are deliberately carried in NPC
   source space (`npc_spawn.rs:74-76`). It is not an actor-value key and not a
   ruleset output. Flagged here only so the next sweep does not re-chase it.
10. **`derived_row_len` drifting past the flat-`Vec` rationale.** Dropped — the
    largest game is 8 rows, and `derived_value` is called per-condition, not
    per-frame-per-actor.
11. **The Skyrim/Oblivion "output keys unfalsifiable" note carried from the
    2026-08-30 sweep.** Partially dropped — `records/tests.rs`'s
    `real_ruleset_falsifiability` module *does* pin both against their real
    masters (`#[ignore]`, opt-in). The residual CI-visible gap is entailed by the
    OPEN #3848 and is not re-filed.
12. **Oblivion/Skyrim rulesets being production-unreachable.** Not re-filed —
    #3848 (OPEN), per the brief.


## Dimension 2 — Derived-Stat Formulas (+ CHARAL-adjacent siblings)

Sweep 2026-09-11, HEAD `8151cded`. Method: every capture document read **first**
(`charal.md` §5–6, then the six per-game rulesets), the expected coefficient for each
formula written down, and only then the Rust opened. Nothing below is carried forward
from `docs/audits/AUDIT_CHARACTER_2026-08-30.md`; that report was read for its dropped
candidates and its 34-row table, then the constants were re-derived independently.

Delta relevance: of this dimension's files only `derived.rs` (+18, doc-only) and
`stealth.rs` (+353) changed. `fallout.rs`, `tes.rs`, `skyrim.rs`, `resistance.rs`,
`combat.rs`, `profile.rs` are byte-unchanged — re-derived anyway, all PASS.

Tests run (sequentially, never concurrent):
`CARGO_BUILD_JOBS=4 cargo test -p byroredux-core --lib character` → **115 passed, 0 failed**;
`… --lib -- stealth:: combat::` → **32 passed, 0 failed**.

---

### Constant verification

| # | Formula/constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FO4 Health | `bilinear(END,4.5, LEVEL,2.5, cross 0.5, bias 77.5).floored().player_only()` — `fallout.rs:133-135` | `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` | `charal-fo4-ruleset.md:88` | PASS |
| 2 | FO4 Action Points | `affine(AGI,10.0,60.0).player_only()` — `fallout.rs:142` | `60 + 10·Agility` | `charal-fo4-ruleset.md:132` | PASS |
| 3 | FO4 Carry Weight | `affine(STR,10.0,200.0)`, ActorGeneral — `fallout.rs:147` | `200 + 10 × Strength`, `fAVD…` ⇒ actor-general | `charal-fo4-ruleset.md:237,245-249` | PASS |
| 4 | FO4 Melee Damage | **no row registered**; `melee_damage_config` returns `None` for FO4 | `×(1 + STR/10)`, but "vanilla `Fallout4.esm` authors no `MeleeDamage` AVIF" | `charal-fo4-ruleset.md:256,276-280` | PASS — correctly absent, pinned by `melee_damage_config_resolves_only_where_the_avif_is_authored` |
| 5 | FO3 Health (ruleset) | `bilinear(END,20.0, LEVEL,10.0, cross 0.0, bias 90.0).player_only()` — `fallout.rs:162` | `90 + END·20 + Level·10` | `charal-fnv-fo3-ruleset.md:93` | PASS |
| 6 | FNV Health (ruleset) | `bilinear(END,20.0, LEVEL,5.0, cross 0.0, bias 95.0).player_only()` — `fallout.rs:201` | `100 + END·20 + (Level−1)·5` ≡ `95 + 20·END + 5·L` | `charal-fnv-fo3-ruleset.md:93` | PASS |
| 7 | FO3 Health (NPC seed) | `NpcHealthCurve { bias 90.0, end_mult 20.0, level_mult 10.0 }` — `profile.rs:94-98` | same row, status "BUILT (player ruleset **+ NPC auto-calc seed**)" | `charal-fnv-fo3-ruleset.md:93` | PASS |
| 8 | FNV Health (NPC seed) | `NpcHealthCurve { bias 95.0, end_mult 20.0, level_mult 5.0 }` — `profile.rs:109-113` | same | `charal-fnv-fo3-ruleset.md:93` | PASS |
| 9 | FO3 Action Points | `affine(AGI,2.0,65.0).capped(85.0).player_only()` — `fallout.rs:168-184` | `65 + 2·AGI` (cap 85) | `charal-fnv-fo3-ruleset.md:94` | PASS (scope deliberately unsourced, #2937, documented at the call site) |
| 10 | FNV Action Points | `affine(AGI,3.0,65.0).capped(95.0).player_only()` — `fallout.rs:207-211` | `65 + 3·AGI` (cap 95) | `charal-fnv-fo3-ruleset.md:94` | PASS (same caveat) |
| 11 | FO3/FNV Carry Weight | `affine(STR,10.0,150.0)`, ActorGeneral — `fallout.rs:49` | `150 + 10·STR`, "actor-general" | `charal-fnv-fo3-ruleset.md:95,106-110` | PASS |
| 12 | FO3/FNV Melee Damage | `affine(STR,0.5,0.0)`, Absolute/additive — `fallout.rs:53` | `STR × 0.5`, "an **additive** bonus" | `charal-fnv-fo3-ruleset.md:97,338-341` | PASS |
| 13 | FO3/FNV Critical Chance | `affine(LUCK,1.0,0.0).capped(10.0)` on the 0–100 scale — `fallout.rs:66` | `Luck × 1%` cap 10 % | `charal-fnv-fo3-ruleset.md:96` | PASS — #2936 convention applied correctly |
| 14 | FO3/FNV Unarmed Damage | `affine(Unarmed,0.05,0.5).ceiled()` — `fallout.rs:71` | `ceil((10 + Unarmed)/20)` ≡ `ceil(0.5 + 0.05·U)` | `charal-fnv-fo3-ruleset.md:98,118-120` | PASS (`RoundMode::Ceil`, correct) |
| 15 | Radiation Resistance | `derive_coeff 2.0`, bias `−2.0`, `resist_cap 85.0`, `clamped_below(0.0)` — `resistance.rs:75-80,106-114` | `(END−1)·2`, cap 85 % | `charal-fnv-fo3-ruleset.md:99,343-357` | PASS |
| 16 | Poison Resistance | `derive_coeff 5.0`, bias `−5.0`, `resist_cap f32::INFINITY` | `(END−1)·5`, "**No documented FO3/FNV cap** … don't invent one" | `charal-fnv-fo3-ruleset.md:100,359-369` | PASS — uncapped expressed as `INFINITY`, not `0` |
| 17 | `damage_multiplier` | `(1 − clamp(r,0,cap)/100).max(0.0)` — `resistance.rs:125-128` | "damage is reduced by this percentage"; ≥100 % = immunity | `charal-fnv-fo3-ruleset.md:343-369` + `resistance.rs` docs | PASS — cannot exceed 1.0, cannot go negative (no heal-on-overresist) |
| 18 | Oblivion Health pool | `affine(END,2.0,0.0).player_only()` — `tes.rs:40` | `2×Endurance` | `charal-oblivion-ruleset.md:455`; `charal.md:314` | PASS |
| 19 | Oblivion Magicka pool | `affine(INT,2.0,0.0).player_only()` — `tes.rs:47` | `Magicka = INT + INT×fPCBaseMagickaMult(1.0)` = `2×INT` | `charal-oblivion-ruleset.md:511-512` | PASS |
| 20 | Oblivion Fatigue pool | four `affine(av,1.0,0.0)` rows summed under one output id — `tes.rs:79-87` | `Strength + Willpower + Agility + Endurance` | `charal-oblivion-ruleset.md:379`; `charal.md:314-318` | PASS — uncapped, unrounded, so the sum is the true total |
| 21 | Oblivion Armor Rating mult | `ARMOR_RATING_SKILL_BIAS 0.35`, `_COEFF 0.0065`, `.as_multiplier()`, ActorGeneral — `tes.rs:61-63,128-149` | `(0.35 + 0.0065 × ArmorSkill)`; "ActorGeneral scope since the source doesn't distinguish player/NPC" | `charal-oblivion-ruleset.md:325,335-345` | PASS (worked values Light 50 → 0.675, Heavy 20 → 0.48 both asserted) |
| 22 | `oblivion_attribute_bonus` | `0→1, 1..=4→2, 5..=7→3, 8..=9→4, _→5` — `tes.rs:190-198` | `+1/+2/+3/+4/+5` by `0 / 1–4 / 5–7 / 8–9 / 10+`, capped, no roll-over | `charal.md:338-339` | PASS |
| 23 | `oblivion_health_gain_per_level` | `0.1 × endurance` — `tes.rs:219` | 10 % of Endurance each level (UESP anchor END 100 → 10) | `charal-oblivion-ruleset.md:456-459` | PASS |
| 24 | `FATIGUE_REGEN_PER_SEC` | `10.0` flat (Endurance coefficient literally absent) — `regen.rs:74` | `Endurance × fFatigueReturnMult(0.0) + fFatigueReturnBase(10.0)` | `charal-oblivion-ruleset.md:390` | PASS — the `0.0` END term correctly collapses, and is documented as doing so |
| 25 | `magicka_regen_per_sec` | `(WIL·0.02 + 0.75) × (MaxMagicka/100)`, `0.0` when stunted or max ≤ 0 — `regen.rs:77-90` | `MagickaRegen = (Willpower × 0.02 + 0.75) × (MaxMagicka / 100)`; Stunted is a binary gate | `charal-oblivion-ruleset.md:523,543-550` | PASS |
| 26 | Skyrim Light Armor rating | `LIGHT_ARMOR_RATING_COEFF 0.004`, bias `1.0`, `.as_multiplier().player_only()` — `skyrim.rs:78,122-133` | `1 + 0.004 × LightArmorSkill` (player); NPC `0.015` explicitly not modelled | `charal-skyrim-ruleset.md:208-220` | PASS |
| 27 | Skyrim Carry Weight | `CARRY_WEIGHT_BIAS 250.0`, `_STAMINA_COEFF 0.5`, `.a_from_base()`, ActorGeneral — `skyrim.rs:85-87,135-143` | `250 + 0.5 × BaseStamina`; temporary Stamina must **not** move it | `charal-skyrim-ruleset.md:655,663-666` | PASS — the only `a_from_base` call site in the workspace, and the only formula sourced as needing it |
| 28 | `SKYRIM_POOL_BASE` | `100.0` — `skyrim.rs:31` | Health/Magicka/Stamina start at 100 | `charal.md:329-331` | PASS |
| 29 | `combat::modified_skill` | `skill + 0.4 * (luck − 50.0)` — `combat.rs:41-43` | `ModifiedSkill = Skill + 0.4×(Luck−50)` (`bias=−20, c_Skill=1, c_Luck=0.4`) | `charal-oblivion-ruleset.md:285,291-295` | PASS |
| 30 | `oblivion_weapon_damage_multiplier` coefficients | `0.5 × (0.75 + 0.005·A) × (0.2 + 0.015·MS)` — `combat.rs:59` | identical, byte-for-byte on both the Blade and Blunt pages | `charal-oblivion-ruleset.md:208-213` | PASS (all four coefficients) |
| 31 | …its bilinear expansion | test asserts bias `0.075` at (0,0,Luck 50) and `1.0625` at (100,100,50) | `bias 0.075, c_STR 0.0005, c_Blunt 0.005625, cross 0.0000375` | `charal-oblivion-ruleset.md:229-233` | PASS — hand-expansion cross-check present |
| 32 | …its `[0,100]` input clamp | `attribute.clamp(0,100)` **and** `modified_skill(..).clamp(0,100)` — `combat.rs:57-58` | "Attribute and ModifiedSkill are constrained between 0 and 100" (Weapon Damage section only) | `charal-oblivion-ruleset.md:255-257` | PASS |
| 33 | `oblivion_hand_to_hand_damage` | `health = 1 + 10.5·(STR/100)·(MS/100)`; `fatigue = 1 + 0.5·health`; **no** clamp — `combat.rs:78-81` | identical; clamp deliberately not carried over (UESP states it only for weapon damage) | `charal-oblivion-ruleset.md:303-304,317-320` | PASS |
| 34 | `detection_score` top level | `att·(sound + visual + detskill/2) − targetskill/2 − 35` — `stealth.rs:283` | identical | `charal-fnv-fo3-ruleset.md:231` | PASS |
| 35 | `TargetSkill` | `Sneak + 5·(TL−DL) + max(50 − 10·TL, 0) − Armor`, `0` when not sneaking — `stealth.rs:274-281` | identical, "0 if not sneaking" | `charal-fnv-fo3-ruleset.md:232-233` | PASS |
| 36 | `DetectorSkill` base | `(10 + 8·Perception) × state` — `stealth.rs:271-272` | `(10 + 8·Perception) × DetectorState` | `charal-fnv-fo3-ruleset.md:234` | PASS |
| 37 | `DetectorState` multiplier **values** | `{0.8, 1.2, 1.0}` — `stealth.rs:171-174` | "`# 0.8 / 1.2 / 1` by AI state" | `charal-fnv-fo3-ruleset.md:234` | PASS (value set only) |
| 38 | `DetectorState` **state→value assignment** | `SleepingOrFightingThisTarget→0.8`, `AlertLostOrFightingOther→1.2`, `Normal→1.0` — `stealth.rs:156-176` | **not in any capture document** — the doc records the three values but never which AI state each belongs to | — | **UNSOURCED — finding D2-01 (NEW)** |
| 39 | `Attenuation` | `((max − d)/max).powi(2)`, MaxDist 2500 in / 5000 out — `stealth.rs:81-85,243` | `((MaxDist − distance)/MaxDist)²`, 2500 in / 5000 out | `charal-fnv-fo3-ruleset.md:235` | PASS |
| 40 | `classify` bands | `< −20` Undetected, `≤ 0` Suspicious, else Detected — `stealth.rs:228-236` | `< −20` undetected, `−20..0` suspicious, `> 0` detected | `charal-fnv-fo3-ruleset.md:238` | PASS — half-open, total, every value in exactly one band |
| 41 | `SoundMultiplier` | `1.6` with LOS / `0.16` without — `stealth.rs:245` | `1.6 if Detector has LOS … 0.16 otherwise` | `charal-fnv-fo3-ruleset.md:251-252` | PASS *(was row 29 UNSOURCED last sweep — now sourced)* |
| 42 | `Sound` | `mult × (movement_sound + 2.0 × action)` — `stealth.rs:252` | `SoundMultiplier × (MovementSound + 2 × ActionSound)` | `charal-fnv-fo3-ruleset.md:253` | PASS *(newly sourced)* |
| 43 | `MovementMultiplier` | `0.0` stationary\|silent-running, `1.5` running, `1.0` walking — `stealth.rs:246-250` | `0` stationary or Silent Running, `1.5` running, `1` otherwise | `charal-fnv-fo3-ruleset.md:254-256` | PASS *(newly sourced)* |
| 44 | `MovementSound` | `(12.0 + weight/2.0) × mult` — `stealth.rs:251` | `(12 + EquippedWeight/2) × MovementMultiplier` | `charal-fnv-fo3-ruleset.md:257` | PASS *(newly sourced)* |
| 45 | `ActionSound` table | Loud 100 / Normal 50 / Silent 10 / None 0 — `stealth.rs:126-133` | identical 4-row table | `charal-fnv-fo3-ruleset.md:269-274` | PASS *(newly sourced)* |
| 46 | `Visual` gate | `0.0` when no LOS **or** invisible — `stealth.rs:254-255` | `0` if no LOS, or Target Invisible/Chameleon | `charal-fnv-fo3-ruleset.md:259-261` | PASS *(newly sourced)* |
| 47 | `nighteye` | `3.0` with the effect, else `1.0` — `stealth.rs:257-261` | `3 if Detector has Night Eye, else 1` | `charal-fnv-fo3-ruleset.md:262` | PASS *(newly sourced)* |
| 48 | `Light` | `1.4 * (light·nighteye).min(100.0)` — `stealth.rs:262` | `1.4 × min(100, LightLevel × nighteye)` — cap **inside** the multiplier | `charal-fnv-fo3-ruleset.md:263` | PASS *(newly sourced)* — `min` order correct, pinned by `light_level_is_capped_before_the_1_4_multiplier` |
| 49 | `VisualMovement` (stationary / walking / running) | `0.0 / 0.01 / 0.21` — `stealth.rs:263-267` | `0` not moving, `0.01` moving not running, `0.21` running | `charal-fnv-fo3-ruleset.md:264-266` | PASS *(newly sourced)* |
| 50 | `VisualMovement` for `SilentRunning` | `0.01` — `stealth.rs:266` | `0.01`, explicitly labelled a **modelling choice the document owns, not the source** | `charal-fnv-fo3-ruleset.md:293-302` | PASS — disclosed, not passed off as sourced; pinned by `visual_movement_matches_the_source_table` |
| 51 | `Visual` composition | `light * (1.0 + visual_movement)` — `stealth.rs:268` | `Light × (1 + VisualMovement)` | `charal-fnv-fo3-ruleset.md:261` | PASS *(newly sourced)* |
| 52 | `ArmorClass` penalty table | Heavy 20 / Medium 10 / Light 0 — `stealth.rs:147-153` | identical 3-row table ("penalty is for armor, not for helmets") | `charal-fnv-fo3-ruleset.md:276-288` | PASS *(newly sourced)* |
| 53 | `eval` structure | `bias + cₐA + c_bB + cross·A·B`, round, then `max(floor).min(cap)`; no allocation, no dispatch — `derived.rs:314-328` | as documented | `derived.rs` § Efficiency | PASS |
| 54 | `eval` — no game-identity branch | `grep -E 'GameKind\|game ==\|master_name\|is_fnv\|is_skyrim'` over `character/`, `combat.rs`, `stealth.rs` → **3 hits, all doc-comment prose** (`mod.rs:53-54`, `profile.rs:3`) | per-game seam is data, not code | `charal.md` §0 doctrine | PASS |
| 55 | `DerivedStatFormula` size + `Copy` | `size_of == 36`, `Copy` asserted by `formula_is_thirty_six_bytes_and_copy` (`derived.rs:354-366`); struct docstring says 36 B at `:23` and `:144` | 36 B since #2939 added `floor: f32` | SKILL.md pin (updated by `8175cb70`) | PASS — the delta's 18 lines were **doc-only** (`cap` field examples, #3766); size unchanged, pin followed |
| 56 | Cap sentinel | uncapped = `f32::INFINITY` (both constructors); `grep 'capped(0' crates byroredux` → **zero hits** | — | `derived.rs:167-183` | PASS — the "cap `0` = clamp-to-zero" trap does not exist; `DerivedStatFormula` has no `Default` |
| 57 | Floor sentinel | unfloored = `f32::NEG_INFINITY`; `clamped_below` used at exactly one site (`resistance.rs:112`, the only negative-bias shape) | `clamped_below(0.0)` is "a defensive domain floor, not a sourced balance number" | `resistance.rs:99-105` | PASS — self-labelled as engine-defensive, not fabricated data |
| 58 | `DerivedInput` sentinels vs real FormIDs | `EsmIndex::actor_value_form_id`'s `usable` closure rejects `form_id == 0` **and** `form_id == u32::MAX` before returning (`index.rs:712`); every production `DerivedInput::actor_value(..)` argument originates there | `0` = unused, `u32::MAX` = level | `derived.rs:56-79`; `index.rs:711-730` | PASS — enforced at the caller, verified by tracing all 12 call sites (the 3 outside `character/` are test fixtures) |
| 59 | `DerivedScope` tagging | Health + AP `PlayerOnly` on FO3/FNV/FO4; Carry Weight / Melee / Crit / Unarmed / both resistances `ActorGeneral`; Oblivion pools `PlayerOnly`, armour `ActorGeneral`; Skyrim Light Armor `PlayerOnly`, Carry Weight `ActorGeneral` | matches every document's own scope annotation | all four captures | PASS |
| 60 | Cross term used only where sourced | non-zero `cross` in exactly **one** shipped row (FO4 Health, `0.5`); every other row passes `cross 0.0` or uses `affine` | only FO4 Health needs it | `charal-fo4-ruleset.md:88` | PASS — no unexplained cross term anywhere |
| 61 | `RoundMode` per formula | `Floor` on FO4 Health only; `Ceil` on Unarmed Damage only; `None` everywhere else | `floor(…)` FO4 Health; `ceil((10+U)/20)` Unarmed Damage; no source states rounding for any other row | `charal-fo4-ruleset.md:88`; `charal-fnv-fo3-ruleset.md:98` | PASS — FO3/FNV Health correctly **un**rounded (their sources give no `floor`) |
| 62 | Round-then-clamp ordering | `eval` rounds, then clamps — `derived.rs:322-327` | — | — | PASS (unobservable): no shipped row has both a rounding mode and a finite cap/floor, so the order cannot change any result |
| 63 | Live consumer: melee bonus is additive | `weapon.damage.max(0.0) + melee_damage_charal_bonus(..)` — `byroredux/src/combat.rs:417` | "an **additive** bonus to Melee Weapon damage" | `charal-fnv-fo3-ruleset.md:338-341` | PASS — added, not substituted (#3092) |
| 64 | Live consumer: scope/kind gate | `condition.rs:520-524` evaluates a derived row only when `scope == ActorGeneral && kind == Absolute` | multiplier rows apply at use-time; player rows must not be applied to NPCs | `derived.rs:111-141` | PASS — both discriminants checked, not just one |

**64 rows checked — 63 PASS, 1 UNSOURCED (row 38, filed below as D2-01), 0 FAIL.**

---

### #3482 closure verdict (stealth sourcing)

**Genuinely closed, not papered over.** Commit `847426fa` ("Fix #3482: capture
stealth.rs's Sound/Visual coefficients and pin them exactly") added **91 lines of actual
source text** to `charal-fnv-fo3-ruleset.md:243-302` — the verbatim `SoundMultiplier` /
`Sound` / `MovementMultiplier` / `MovementSound` / `Visual` / `nighteye` / `Light` /
`VisualMovement` math block plus the `ActionSound` and `Armor` tables — from a
re-read of the same cited fandom *Sneak (Fallout: New Vegas)* page on 2026-08-30. It
also corrected the doc's own false verification-attribution chain in prose
(`:313-324`).

Every one of the previous sweep's six flagged sub-coefficient groups is now backed by a
document line and verified above: **1.6/0.16 LOS** (row 41), **`12.0 + weight/2`**
(row 44), **1.5/1.0/0.0 movement** (row 43), **2.0 action** (row 42) with the
100/50/10/0 `ActionSound` table (row 45), **3.0 night-eye** (row 47), **1.4 light +
`min(100, …)`** (row 48), **0.21/0.01 visual-movement** (row 49). All match the
document exactly. The `stealth.rs` side replaced monotonicity-only coverage with nine
exact-value tests driven off an isolating fixture (`isolated()` / `ISOLATED_BASE = -30`),
so each constant is now individually falsifiable from the repository — that fixture is
the substantive part of the +353 lines.

One constant is explicitly **disclosed as a modelling choice rather than a
transcription** (`MovementState::SilentRunning`'s `VisualMovement = 0.01`), owned by the
document at `:293-302` and by `stealth.rs:93-105`, and pinned by test. That is the
correct handling under the no-guessing policy — visible instead of looking sourced.

**One item the closure missed**: the `DetectorState` state→multiplier *assignment*
(row 38) is the same defect class and is still unfalsifiable from the repository. Filed
as D2-01 below.

---

### Checks performed

1. **Every coefficient verified against its capture document** — 64-row table above,
   built by writing the expected value from the document first. All six per-game
   ruleset documents read in full or by targeted section before any Rust was opened.
   **PASS** (63/64; row 38 UNSOURCED).
2. **Cross-term red flag** — `grep` for non-zero `cross` across all builders: exactly
   one shipped row uses it (FO4 Health, `0.5`), which is the only formula whose source
   has one. Every other row is affine or passes `cross 0.0`. **PASS** (row 60).
3. **`RoundMode` per formula** — `Floor` appears once (FO4 Health, source writes
   `floor(...)`), `Ceil` once (Unarmed Damage, source writes `ceil(...)`), `None`
   elsewhere. FO3/FNV Health are deliberately un-floored: their sources give a bare
   affine with no rounding, so adding one would be an invention. **PASS** (row 61).
   Ordering (round-then-clamp) is unobservable today — no row has both. **PASS** (row 62).
4. **Cap sentinel is `INFINITY`, never `0`** — both constructors default
   `cap = f32::INFINITY` / `floor = f32::NEG_INFINITY`; `grep -rn "capped(0"` over
   `crates/` and `byroredux/` returns **zero hits**; `DerivedStatFormula` derives no
   `Default`, so a zero-valued cap cannot be produced accidentally. Poison Resistance —
   the one stat the document says is uncapped — ships `resist_cap: f32::INFINITY`, not
   `0.0`. **PASS** (rows 16, 56, 57).
5. **Chaining ordering** — `add_fnv_fo3_shared` registers Unarmed Damage keyed on the
   **Unarmed skill** AVIF, so population must write skills before any evaluation.
   `derive_autocalc_actor_values` (`actor_value_derive.rs:365-403`) emits the 7 SPECIAL,
   then every governed skill, then Health, as **one** `Vec<(u32,f32)>` consumed by
   `ActorValues::from_pairs` at spawn — population is a single atomic build, not an
   incremental sequence, so the ordering invariant cannot be violated. Note the
   population path does not itself chain through `ActorValues`: `base_skill` reads
   `class.base_attributes` directly and the Health curve reads `special[2]`, so there is
   no read-before-write window at all. `derived_value` is only ever called later
   (`condition.rs:524`, `byroredux/src/combat.rs:462`). An unpopulated input reads
   `0.0`, documented at `derived.rs:310-312` as "the Bethesda absent-AV default", and
   resolve-or-skip means a row whose input AVIF doesn't resolve is never registered at
   all. **PASS**.
6. **`DerivedInput` sentinels checked at the CALLERS** — traced all 12
   `DerivedInput::actor_value(..)` sites. Nine are in `crates/core/src/character/`
   builders taking a `resolve`-produced id; three (`byroredux/src/combat.rs:706,860`,
   `crates/scripting/src/condition.rs:1805,1812`) are inside `#[cfg(test)]` fixtures.
   Every production id originates from `EsmIndex::actor_value_form_id`, whose `usable`
   closure (`index.rs:712`) rejects `form_id == 0` **and** `form_id == u32::MAX` before
   returning. A real AVIF can therefore never collide with `UNUSED` or `LEVEL`.
   **PASS** (row 58).
7. **`DerivedScope` correctness and containment** — verified every row's scope against
   its document annotation (row 59), and verified the consumer side actually honours it:
   `condition.rs:520-524` gates on `scope == ActorGeneral && kind == Absolute` before
   evaluating, with a dedicated test asserting a Multiplier row falls through to the
   absent-AV default. FO3↔FNV player Health/AP divergence therefore stays contained to
   `PlayerOnly` rows and cannot leak onto NPCs. **PASS**.
8. **`eval` allocation-free, ~5 FMAs, no game branch, `Copy` + 36 B** — `eval` is two
   `read`s, three mults + three adds, one `match` on `RoundMode`, one `max`/`min`
   (`derived.rs:314-328`); no `Vec`, no `Box`, no `dyn`. `grep` for game identity over
   `character/`, `combat.rs`, `stealth.rs` → 3 hits, all doc prose. Size pin is
   `formula_is_thirty_six_bytes_and_copy` asserting `36`, and both docstrings
   (`derived.rs:23`, `:144`) say 36 B. **PASS.** *The delta's 18 lines in `derived.rs`
   are entirely the `cap` field docstring rewrite (#3766) — no field added, no size
   change, pin unchanged.* (Verified by `git diff 64f64480..HEAD -- derived.rs`.)
9. **CHARAL-adjacent siblings** — `combat::modified_skill` (`0.4` Luck coefficient),
   `oblivion_weapon_damage_multiplier` (all four coefficients + the `[0,100]` clamp),
   `oblivion_hand_to_hand_damage` (the `10.5` pure-cross term and the deliberately
   *absent* clamp) verified against `charal-oblivion-ruleset.md` §§ "Melee weapon
   damage" / "The Complete Damage Formula" (rows 29–33). `stealth::detection_score` and
   `classify` verified against § "Sneak Detection (FNV)" (rows 34–52). **PASS** except
   row 38.
10. **Drift reported regardless of consumption** — `combat.rs` still has zero callers
    outside its own tests; `stealth.rs` still has zero callers at all. Both verified in
    full anyway. `stealth.rs`'s own status prose is now accurate post-#3878 (#446 is
    closed at `90e6b068`, `pack.rs` ships, M42 procedures exist) — the capture document
    is not (finding D2-02). **PASS with findings.**
11. **No stray copies of CHARAL coefficients** — `grep` for `77.5`, `10.5 *`, `0.0065`,
    `1.6 *`, `0.16`, `0.015 *`, `0.005 *` across `crates/` and `byroredux/` outside the
    CHARAL files: no hit is a character formula (all are colour/physics/weather
    constants). The formula constants live in exactly one place each, except the
    FO3/FNV Health pair (see dropped candidates). **PASS.**
12. **Tests green** — 115 character + 32 stealth/combat, 0 failed. **PASS.**

---

### Findings

#### CHAR-2026-09-11-D2-01: `DetectorState`'s state→multiplier assignment is unsourced — the same gap #3482 closed for `Sound`/`Visual`, left open one line above it
- **Severity**: LOW
- **Dimension**: Derived Formulas (CHARAL-adjacent sibling `stealth.rs`)
- **Game**: fnv, fo3
- **Location**: `crates/core/src/stealth.rs:156-176` (enum + `multiplier()`); document line `docs/engine/charal-fnv-fo3-ruleset.md:234`
- **Status**: NEW
- **Source**: UNSOURCED-in-code. `charal-fnv-fo3-ruleset.md:234` captures the *values*
  (`DetectorSkill = (10 + 8·Perception) × DetectorState  # 0.8 / 1.2 / 1 by AI state`)
  but no line anywhere in the CHARAL documents says which AI state carries which
  multiplier. `grep -niE "sleep|alert|fighting|DetectorState|AI state"` over the
  document returns only that line plus unrelated Hardcore-mode and prose hits.
- **Description**: `stealth.rs` ships three named detector states with a specific
  assignment, and its enum docstring asserts the semantics in prose — "Sleeping actors
  and actors already fighting their current target are *less* alert (0.8×); actors on
  edge (alert, lost, or fighting someone else) are *more* alert (1.2×)". None of that
  naming or assignment appears in any capture document. This is exactly the defect
  #3482 was filed for and exactly the class of statement its fix converted to captured
  source text for `Sound`/`Visual` — the pass simply stopped at the `DetectorSkill`
  line instead of continuing through it.
- **Evidence**: `stealth.rs:171-174`
  `SleepingOrFightingThisTarget => 0.8, AlertLostOrFightingOther => 1.2, Normal => 1.0`.
  The pinning test `detector_skill_and_state_match_the_source_table` (`:750-769`) asserts
  each pair against a literal in the test itself, so it pins the code to *itself*, not
  to a document — the same circularity the #3482 report identified for the other
  coefficients ("an attribution chain with nothing at the end of it").
- **Impact**: Swapping `0.8` and `1.2` would invert the effect of a detector being
  asleep versus alert, and nothing in the repository could catch it — the monotonicity
  test `alert_detector_state_raises_detection_over_normal` would fail, but only because
  it encodes the same unsourced assumption. No live consumer exists (zero callers), so
  today this is a correctness-of-record problem, not shipped wrong behaviour. It becomes
  gameplay-visible the moment M42 wires an alert-state tick.
- **Related**: #3482 (CLOSED — closed correctly for `Sound`/`Visual`, see the closure
  verdict above); #3878
- **Suggested Fix**: Re-read the same fandom *Sneak (Fallout: New Vegas)* `<math>` block
  the #3482 pass used and capture the three AI-state names beside their multipliers in
  `charal-fnv-fo3-ruleset.md`'s new sub-expression block, then cite that line from the
  enum. If the page names no states, say so explicitly and mark the assignment a
  disclosed modelling choice the way `SilentRunning`'s `VisualMovement` already is.

#### CHAR-2026-09-11-D2-02: `charal-fnv-fo3-ruleset.md` still repeats the three expired blocker claims that #3878 corrected in `stealth.rs`
- **Severity**: LOW
- **Dimension**: Derived Formulas (capture-document rot)
- **Game**: fnv, fo3
- **Location**: `docs/engine/charal-fnv-fo3-ruleset.md:331-336`
- **Status**: NEW
- **Source**: n/a (non-numeric — doc rot, not a constant)
- **Description**: Commit `4195ea41` ("Fix #3878: stealth.rs's deferral condition
  expired — record that, don't repeat it") establishes that all three claims in
  `stealth.rs`'s old status paragraph had expired, and rewrote that paragraph. Its diff
  touches **one file, 33 lines** (`crates/core/src/stealth.rs`). The capture document
  carries a near-verbatim copy of the same paragraph and was not touched, so it still
  reads: *"no ROADMAP milestone exists yet to consume it (M42 'AI packages' … is Tier 7
  and blocked on `PACK` record parsing, #446 … ECS wiring waits for M42)"*.
- **Evidence**: `git show --stat 4195ea41` → `crates/core/src/stealth.rs | 33 +++---`,
  one file. Document `:331-336` versus `stealth.rs:31-43`, which now states the
  opposite: "#446 is CLOSED (`90e6b068`) and `crates/plugin/src/esm/records/misc/pack.rs`
  is ~1,800 lines of shipped PACK parsing; an AI-package evaluator exists … M42 has
  delivered seven procedure runtimes". `git log --oneline -1 90e6b068` confirms
  #446 is closed.
- **Impact**: The capture documents are the designated authority for this layer, and a
  contributor picking up stealth wiring reads the document at least as often as the
  module docstring. The correction's own stated purpose — "a contributor reading this to
  pick up work should not be sent to wait on things that are done" — is defeated for
  exactly the readership the document serves. The fix left the more-read copy stale.
- **Related**: #3878 (CLOSED); #446 (CLOSED); D2-01 (same section)
- **Suggested Fix**: Port `stealth.rs:31-43`'s corrected paragraph into
  `charal-fnv-fo3-ruleset.md:331-336` — unscheduled, not blocked — and drop the
  "waits for M42" closer.

#### CHAR-2026-09-11-D2-03: `charal-skyrim-ruleset.md` still pins `DerivedStatFormula` at 32 bytes and names a test that no longer exists
- **Severity**: LOW
- **Dimension**: Derived Formulas (capture-document rot)
- **Game**: skyrim (the struct is `all`)
- **Location**: `docs/engine/charal-skyrim-ruleset.md:695-699`
- **Status**: NEW (third copy; the first two were fixed)
- **Source**: n/a (non-numeric — a struct-contract claim, not a game constant)
- **Description**: The § Carry Weight build note says the `base_reads: u8` bitfield was
  "packed into the struct's one spare padding byte — so `size_of::<DerivedStatFormula>()`
  stays **exactly 32 bytes**, still enforced by `formula_is_thirty_two_bytes_and_copy`".
  The struct has been 36 bytes since #2939 added `floor: f32`, and
  `formula_is_thirty_two_bytes_and_copy` does not exist anywhere in the workspace — the
  live pin is `formula_is_thirty_six_bytes_and_copy` (`derived.rs:354`). #3485 fixed the
  SKILL.md copy of this claim (`8175cb70`) and `derived.rs`'s own docstrings were already
  correct; this third copy was not swept.
- **Evidence**: `grep -rn "thirty_two_bytes\|exactly 32 bytes" docs crates byroredux` →
  the only live-source hit is `docs/engine/charal-skyrim-ruleset.md:699` (the others are
  archived audit reports and unrelated Vulkan/GRAS byte counts).
  `git show 8175cb70 --stat` shows `.claude/commands/audit-character/SKILL.md` changed,
  this document not.
- **Impact**: This crate treats struct-size assertions as contracts (the pattern
  `resistance.rs:205-214` enforces mechanically, and #2954 was filed for exactly this
  divergence on `Affliction`). A contributor extending `DerivedStatFormula` from the
  Skyrim capture document is told a wrong budget and pointed at a test name that greps
  to nothing.
- **Related**: #3485 (CLOSED — SKILL.md copy fixed); #2939; #2954 (the same
  documented-size-vs-asserted-size divergence class)
- **Suggested Fix**: Change `:695-699` to "packed into a spare flag byte; the struct is
  36 bytes since #2939's `floor: f32`, pinned by `formula_is_thirty_six_bytes_and_copy`".

---

### Candidates verified and dropped

- **"FO3/FNV Health coefficients are encoded twice and can drift."** Structurally true —
  `fallout.rs:162/201` (the player ruleset row) and `profile.rs:94-98/109-113`
  (`NpcHealthCurve`, the NPC auto-calc seed) each carry `{90,20,10}` / `{95,20,5}`
  independently, and no test ties the two together. **Dropped**: both copies are
  numerically correct against `charal-fnv-fo3-ruleset.md:93`, and each is independently
  test-pinned to a worked value from that same line
  (`fnv_and_fo3_share_skill_stats_but_differ_on_health_ap` for the ruleset rows;
  `fallout_profiles_keep_roster_health_and_ruleset_in_lockstep` asserting
  `evaluate(5,2) == 210.0 / 205.0` for the curves). A one-sided edit therefore fails a
  test, which is the property that matters. The capture document itself sanctions both
  consumers on one row ("BUILT (player ruleset **+ NPC auto-calc seed**)"). Recorded so
  the next sweep doesn't re-chase it; worth a cross-assert only if a third copy appears.
- **"FO3/FNV Health is `player_only()` yet the NPC population path applies the same
  curve to NPCs."** Not a defect: the document's own status column for that row reads
  "BUILT (player ruleset + NPC auto-calc seed)", i.e. the seed use is explicitly
  sanctioned, and the two paths are different mechanisms (a `DerivedStatFormula` read at
  query time vs. a value baked into `ActorValues` at spawn). The `player_only()` flag
  correctly stops the *formula* from being evaluated for an NPC at query time
  (`condition.rs:520`); it says nothing about seeding.
- **"An absent input silently reads `0.0` — an accidental zero."** `derived.rs:310-312`
  documents it as "the Bethesda absent-AV default", and resolve-or-skip means a row whose
  input AVIF doesn't resolve is never registered at all, so the `0.0` path only fires for
  a partially-populated actor. The one shape that could turn that into a *negative*
  output — the `(gov−1)·k` resistances — is explicitly floored by `clamped_below(0.0)`
  with the reasoning stated at `resistance.rs:99-105`. Not a finding.
- **"A cap of `0` is read as clamp-to-zero."** Re-verified from scratch: sentinel is
  `f32::INFINITY`, no `capped(0.0)` call site exists workspace-wide, no `Default` derive.
  The trap does not exist. (Also dropped by the 2026-08-30 sweep; re-checked, not
  carried forward.)
- **"`stealth::classify` is ambiguous at the −20 / 0 boundaries."** `< -20.0` →
  Undetected, `<= 0.0` → Suspicious, else Detected: half-open and total, matching
  `charal-fnv-fo3-ruleset.md:238` exactly. Pinned by
  `classify_matches_the_documented_bands`, which asserts both boundary values. Not a
  finding. (Also dropped by the 2026-08-30 sweep.)
- **"`derived.rs` changed in this delta — the 36 B pin may have drifted again."**
  Explicitly checked per the brief's note. `git diff 64f64480..HEAD -- derived.rs` is
  **doc-only**: the entire 18 lines are the `cap` field's example list, rewritten by
  #3766 (the previous sweep's own D2-01 finding). No field added, no layout change;
  `size_of` is still 36 and `formula_is_thirty_six_bytes_and_copy` still asserts 36.
  The previous sweep's only Dimension-2 finding is therefore **fixed and verified**.
- **"`melee_damage_charal_bonus` doesn't check `DerivedScope` before summing."** True of
  that function, but harmless and not a drift: it keys on the resolved `MeleeDamage`
  AVIF, whose only registered row is `ActorGeneral` + `Absolute` on both FO3 and FNV,
  and the row does not exist at all on FO4/TES (`melee_damage_config` returns `None`).
  The generic consumer that *can* see arbitrary rows, `condition.rs`, does gate on both
  `scope` and `kind`. Not a finding.
- **"`b_from_base()` is dead code."** It is unused (only `a_from_base` has a call site,
  Skyrim Carry Weight), but it is the symmetric half of a two-bit field, `const`, and
  zero-cost — and the document's reading-model analysis explicitly leaves the second
  input's mode open. Tech-debt at most, not a Dimension-2 constant defect; not reported.


## Dimension 3 — Leveling & Progression Models

**Repo state**: HEAD `8151cded`, branch `main`. Depth: DEEP, all implemented families.

**Delta**: `leveling.rs`, `skyrim.rs`, `tes.rs`, `components.rs`, `profile.rs` are
**byte-unchanged** since `64f64480` (verified: `git diff 64f64480..HEAD --` on all
five returns empty). Every constant below was nevertheless re-derived from the
capture documents this pass, reading the documents **before** the Rust. The two
documents that *did* move — `charal.md` (+17) and `charal-oblivion-ruleset.md`
(+5) — were diffed line-by-line against the unchanged code; that comparison is
where three of the four findings come from.

**Tests recorded** (read-only; nothing launched):

| Command | Result |
|---|---|
| `CARGO_BUILD_JOBS=4 cargo test -p byroredux-core --lib character::` | **114 passed**, 0 failed, 0 ignored, 639 filtered out |

Includes every leveling guard: `xp_curves_match_wiki_tables`,
`skill_points_match_skill_rate`, `perk_cadence_per_game`,
`oblivion_levels_by_ten_major_skill_ups`, `level_caps_per_game`,
`skyrim_skill_xp_matches_uesp`,
`skyrim_gmst_overlay_reads_only_authored_curve_settings`,
`attribute_bonus_tiers_match_uesp`, `health_gain_per_level_is_ten_percent_of_endurance`,
`skill_xp_cost_matches_uesp_lockpicking`, `pool_base_and_per_level_pick`.

**Findings: 4 — 0 CRITICAL · 0 HIGH · 0 MEDIUM · 4 LOW.** All four are
documentation/capture drift; **no shipped numeric constant is wrong**, and no
mechanism check failed.

### Constant verification

| # | Formula/constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FO4 XP curve | `XpCurve { xp_a 75.0, xp_b 125.0, level_cap 0, SpecialOrPerk }` (`leveling.rs:112-117`) | `XP_to_next(L) = 75·L + 125`; "**No level cap**"; each level grants one point spent on **+1 SPECIAL or one perk rank** | `charal-fo4-ruleset.md:467-487` § XP / level curve — LOCKED | PASS |
| 2 | FO4 curve anchors | test asserts L1 200, L10 875, L22 1775 | "Verified: L1 200, L2 275, L3 350, L10 875, L21 1700, L22 1775" | `charal-fo4-ruleset.md:475` | PASS |
| 3 | FO3 XP curve | `xp_a 150.0, xp_b 50.0, level_cap 20` (`leveling.rs:121-130`) | `XP_to_next(L) = 150·L + 50`; "**Level cap:** FO3 **20** (30 with *Broken Steel*)" | `charal-fnv-fo3-ruleset.md:409-419` | PASS (DLC raise correctly unwired, and the docstring now says so) |
| 4 | FNV XP curve | `xp_a 150.0, xp_b 50.0, level_cap 30` (`leveling.rs:134-143`) | same curve; "FNV **30** (50 with the four add-ons, +5 each)" | same | PASS |
| 5 | FO3 level reward | `SkillPoints { base 10.0, int_mult 1.0, perk_cadence 1 }` | "`SkillPoints { base: 10, int_mult: 1.0 (FO3) …, perk_cadence: 1 (FO3) …}`"; Skill Rate table "**FO3** `10 + Intelligence`" | `charal-fnv-fo3-ruleset.md:428-430`, `:70-76` | PASS |
| 6 | FNV level reward | `SkillPoints { base 10.0, int_mult 0.5, perk_cadence 2 }` | "`int_mult: … 0.5 (FNV)`, `perk_cadence: … 2 (FNV)`"; "**FNV** `10 + Intelligence/2` … odd-INT 0.5 **carries**" | same | PASS — the carry is explicitly the caller's concern in code, matching the document |
| 7 | SPECIAL immutable at FO3/FNV level-up | no SPECIAL arm in `LevelReward::SkillPoints`; only FO4 gets `SpecialOrPerk` | "FO3/FNV grant **no** SPECIAL point at level-up … This is the key `LevelReward` contrast with FO4" | `charal-fnv-fo3-ruleset.md:423-430` | PASS |
| 8 | Oblivion leveling | `SkillUse { major_skill_ups_per_level 10, level_cap 0 }` (`leveling.rs:146-149`) | "a level becomes available after 10 increases in major skills"; no hard cap stated | `charal-oblivion-ruleset.md:787-794` § Leveling — LOCKED | PASS |
| 9 | Oblivion attribute-bonus bands | `0→1, 1..=4→2, 5..=7→3, 8..=9→4, _→5` (`tes.rs:190-198`) | "each selected attribute receives a +1 to +5 bonus based on governed major-skill increases (0, 1–4, 5–7, 8–9, and 10+)" | `charal-oblivion-ruleset.md:790-792` | PASS — all five bands, boundaries inclusive on both ends, capped at +5 with no roll-over (pinned at `tes.rs:346-358`) |
| 10 | `oblivion_health_gain_per_level` | `0.1 × endurance` (`tes.rs:218-220`) | "per-level accrual `0.1×Endurance` rounded down … the page's own worked table (Endurance 98 at level-up → +9, `floor(9.8)`)" | `charal-oblivion-ruleset.md:455-459` | PASS — returns the exact `f32`; the `floor` is explicitly the caller's, matching the doc |
| 11 | `SKYRIM_POOL_BASE` | `100.0` (`skyrim.rs:31`) | "matches `SKYRIM_POOL_BASE=100` … exactly" | `charal-skyrim-ruleset.md:581-587` § Magicka | PASS |
| 12 | Skyrim `pool_pick_gain` | `10.0` (`leveling.rs:158`) | "each time you level up your character, you may add ten points of magicka"; "Each level also grants a 10-point Health/Magicka/Stamina pool pick" | `charal-skyrim-ruleset.md:583-585`, `:717-718` | PASS |
| 13 | Skyrim XP curve | `SkillXp { xp_base 75.0, xp_mult 25.0, level_cap 0 }` (`leveling.rs:154-160`) | "`fXPLevelUpBase` and `fXPLevelUpMult`, whose vanilla values are 75 and 25: XP to the next level is `25 × level + 75`" | `charal-skyrim-ruleset.md:711-721` § XP / level curve — LOCKED | PASS — independently corroborated on real data: `Skyrim.esm` authors `fXPLevelUpBase = 75.0` / `fXPLevelUpMult = 25.0` (measured 2026-09-07, `parse_real_esm.rs:404-407`) |
| 14 | `xp_per_skill_rank` | `1.0`, engine-owned, **no** GMST read | "A skill raised to rank `R` awards `R` character XP; that coefficient is an engine rule, **not** a `fXPPerSkillRank` GMST" | `charal-skyrim-ruleset.md:715-717` | PASS — the settled 2026-08-24 design, correctly implemented; not re-flagged per the skill's standing instruction |
| 15 | `with_gmst` overlay set | requests exactly `["fXPLevelUpBase", "fXPLevelUpMult"]` (`leveling.rs:92-109`) | as row 14 | `leveling.rs:332-346` + `parse_real_esm.rs:397-425` | PASS — **no third GMST has crept back in.** Verified two ways: a repo-wide `grep -rn "fXP" --include='*.rs'` returns only the two names plus their test fixtures, and the recording-probe test asserts the exact ordered pair |
| 16 | `SKYRIM_SKILL_USE_CURVE` | `1.95` (`skyrim.rs:36`) | "The engine's `fSkillUseCurve` is **1.95** for the skill-use progression curve" | `charal-oblivion-ruleset.md:792` (also `charal.md:147`) | PASS on value — but captured only in the **Oblivion** ruleset document, never in the Skyrim one (see D3-04) |
| 17 | `skyrim_skill_xp_to_next` shape | `improve_mult · L^use_curve + improve_offset` (`skyrim.rs:49-57`); anchors 0.25 / 300 / ≈349.13 / ≈1815.5 | **none** — no `charal-*-ruleset.md` contains this formula, these anchors, or the words `SkillImproveMult` / `SkillImproveOffset` | — | **UNSOURCED** → finding D3-04 |
| 18 | `CharacterLevel` shape | `{ level: u16, xp: u32 }` (`components.rs:14-22`) | "`pub struct CharacterLevel { level: u16, xp: f32 /* progress toward next */ }`" | `charal.md:167-175` §4.2 | **MISMATCH** on the `xp` type → finding D3-01 |

**18 rows checked — 16 PASS, 1 UNSOURCED, 1 MISMATCH.** Rows 1–15 are the
previous sweep's 15 rows, re-derived from the documents rather than carried
forward; rows 16–18 are new to this pass.

### Checks performed

1. **Three genuinely different models are three DATA variants, not three
   consumer code paths.** — **PASS.** `LevelingModel` is a three-arm enum
   (`XpCurve` / `SkillUse` / `SkillXp`, `leveling.rs:53-85`). Every shape-aware
   match lives **inside** `leveling.rs`: `xp_to_next` (`:166`),
   `xp_from_skill_rank` (`:181`), `pool_pick_gain` (`:193`), `skill_points`
   (`:205`), `grants_perk_at` (`:221`), `level_cap` (`:240`), `with_gmst`
   (`:92`). Evidence that no consumer matches on the variant: a repo-wide
   `grep -rn "xp_to_next\|xp_from_skill_rank\|pool_pick_gain\|skill_points(\|grants_perk_at\|level_cap()\|with_gmst"`
   over `byroredux/` + `crates/` returns exactly one production call site —
   `crates/scripting/src/condition.rs:675`, `rs.leveling.xp_to_next(level)`
   (`GetXPForNextLevel`, CTDA fn 533) — which never inspects the shape. The
   only other production touch is `profile.rs:189`
   (`ruleset.leveling = ruleset.leveling.with_gmst(gmst)`), also shape-blind.
   Every remaining hit is a test or a `LevelingModel::FNV`-style construction.
2. **`level_cap == 0` means uncapped, handled identically in all three
   variants.** — **PASS, structurally un-divergable.** `level_cap()` is a
   single or-pattern arm over all three variants
   (`leveling.rs:241-245`: `Self::XpCurve { level_cap, .. } | Self::SkillUse { level_cap, .. } | Self::SkillXp { level_cap, .. } => *level_cap`),
   so there is no per-variant branch in which an off-by-one could hide. There
   is still **no production consumer** — `level_cap()` is called only by
   `leveling.rs:307-311` and `profile.rs:266-267`, both tests — so nothing
   applies a cap at runtime and the sentinel cannot be mishandled today. The
   docstrings no longer over-claim (`:236-238`: "DLC-specific raises are not
   applied by the current loader; callers must treat this as the authored base
   cap until that integration lands"), which is the landed #2943 fix; not
   regressed, not re-filed.
3. **Skyrim GMST overlay — the trap.** — **PASS on both halves.**
   (a) `with_gmst` requests exactly `gmst("fXPLevelUpBase")` and
   `gmst("fXPLevelUpMult")` and carries `xp_per_skill_rank`, `pool_pick_gain`
   and `level_cap` through untouched (`leveling.rs:100-106`); the `XpCurve` and
   `SkillUse` arms fall through as `other => other` (`:107`). A repo-wide
   `grep -rn "fXP" --include='*.rs'` finds the two names only in `leveling.rs`
   (the reads + the test fixture), `parse_real_esm.rs` (comments) and
   `index.rs:1997` (a test fixture editor-id) — **no third GMST**. Pinned by
   `skyrim_gmst_overlay_reads_only_authored_curve_settings`
   (`leveling.rs:332-346`), which asserts the exact ordered request list, and
   now also by `parse_real_esm.rs:397-425` (#3923, `c49fe663`, new since the
   last sweep), which recovers the names from `with_gmst` itself via a
   recording probe and asserts both resolve against the shipped `Skyrim.esm`.
   `rank · xp_per_skill_rank` was **not** re-flagged — it is the settled
   engine-coefficient design.
   (b) `08a70f42`'s doc fix **landed and matches the code**: `charal.md:594-602`
   now reads "overlays the authored `fXPLevelUpBase` and `fXPLevelUpMult`
   values with sourced fallbacks — only the level curve is GMST-authored; the
   skill-rank coefficient (`xp_per_skill_rank`) is engine-owned, not a
   `fXPPerSkillRank` GMST (#3221: no shipped Skyrim master defines one; the
   read was withdrawn 2026-08-24)". `grep -rn fXPPerSkillRank docs/engine/`
   now returns only `charal-skyrim-ruleset.md:717` (denying it) and
   `charal.md:600` (denying it). The 2026-08-30 sweep's finding
   `CHAR-2026-08-30-D6-02` is closed and verified closed.
4. **Oblivion banding + the 10-major-skill-ups threshold.** — **PASS.** See
   constant rows 8–9. `oblivion_attribute_bonus` (`tes.rs:190-198`) reproduces
   the document's five bands exactly, with the 10+ band saturating at +5 rather
   than rolling over; `attribute_bonus_tiers_match_uesp` pins both edges of
   every band (0, 1, 4, 5, 7, 8, 9, 10, 30), so a silently-shifted band cannot
   pass. `major_skill_ups_per_level: 10` matches `charal-oblivion-ruleset.md:789`
   and is pinned by `oblivion_levels_by_ten_major_skill_ups`. Neither has a
   production consumer (grep: `major_skill_ups_per_level` appears only in
   `leveling.rs`).
5. **Skyrim base pools `100 H/M/S` with `+10/level`.** — **PASS.** Rows 11–12.
   `pool_base_and_per_level_pick` (`skyrim.rs:237-243`) composes the two
   (`100 + 5×10 = 150`), so a change to either constant breaks the guard.
   Cross-checked against the document's own explicit confirmation sentence,
   which names both symbols.
6. **`Perks` / `PerkRank` — additive ranks, rejection not clamping.** —
   **PASS, #2944's fix is in place and not regressed.** `set_rank`
   (`components.rs:67-80`) is idempotent (raises an existing entry in place,
   never stacks a duplicate) and treats `rank == 0` as a documented no-op;
   `try_set_rank` (`components.rs:90-97`) **rejects** (`return false`) both
   `rank == 0` and `rank > num_ranks` rather than clamping, citing
   `charal-fo4-ruleset.md` § *Perk chart*; `remove` (`:103-115`) exists, which
   it did not at filing time. The population path **does** bypass both —
   `stamp_character_components` builds `Perks { entries: … .collect() }`
   directly (`npc_spawn.rs:209-221`) — but that was measured against shipped
   data in the last sweep and falsified (every one of 10,764 authored `PRKR`
   entries across `Skyrim.esm` + `Fallout4.esm` is rank 1, zero duplicates);
   re-verified as unchanged, carried below as a dropped candidate rather than
   re-filed.
7. **Reachability.** — See the Reachability section. Only `RulesetBuilder::Fallout3`,
   `::FalloutNewVegas` and `::Fallout4` are constructible (`profile.rs:168-186`);
   `OBLIVION` and `SKYRIM` carry `RulesetBuilder::None` (`profile.rs:82-87`,
   `:116-122`) and `build_ruleset` returns `None`. **Existing: #3848 (OPEN)** —
   cited, not re-filed.
8. **Delta-specific (a): do the two moved documents now disagree with the
   unchanged code?** — **PARTIAL.** `charal.md`'s §5 rewrite is *correct*: it
   replaced "Oblivion is now CHARAL-complete end-to-end" with "The Oblivion
   ruleset builder is complete — it is unwired", naming `profile.rs:82-87` and
   `RulesetBuilder::None` exactly as the code has them, and the
   `charal-oblivion-ruleset.md` header gained the matching clause. Both now
   match reality — a genuine improvement, not drift. But three disagreements
   survive or were introduced: `charal.md` §4.2's `xp: f32` (D3-01), the
   Oblivion doc's `AttributeSet::OBLIVION` (D3-02), and `charal.md` §5's
   "level-up leveling-efficiency mechanics are shipped too" against `tes.rs`'s
   own module docstring still calling them deferred (D3-03).
9. **Delta-specific (b): did `847426fa` touch a leveling guard?** — **No.**
   `git show --stat 847426fa` touches six files:
   `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs`,
   `crates/core/src/stealth.rs`, `docs/engine/charal-fnv-fo3-ruleset.md`,
   `docs/smoke-tests/README.md`, `docs/smoke-tests/m48-menu-load.sh`. The four
   overstating guards it fixed were #3430 (m48 smoke test), #3438
   (`sanitize_finite` field coverage), #3462 (`translate_material` copies) and
   #3482 (`stealth.rs` Sound/Visual coefficients). **None is a leveling guard**;
   `leveling.rs`, `skyrim.rs` and `tes.rs` are untouched by it, and its
   `charal-fnv-fo3-ruleset.md` hunk is entirely inside the Sneak section (the
   § *XP / level curve* block is byte-identical —
   `git diff 64f64480..HEAD -- docs/engine/charal-fnv-fo3-ruleset.md` shows no
   XP/level/perk/Skill-Rate line). Dimension 3's guards were not among the
   overstating ones. Independently, the leveling guards are *not* of the
   overstating class: `skyrim_gmst_overlay_reads_only_authored_curve_settings`
   asserts the exact request list rather than the resulting values, and
   `attribute_bonus_tiers_match_uesp` pins both edges of every band.
10. **`grants_perk_at` cadence and its guards.** — **PASS, with an honestly
    labelled approximation.** FO4 `SpecialOrPerk` → every level; FO3/FNV →
    `*perk_cadence != 0 && level.is_multiple_of(u16::from(*perk_cadence))`
    (the `!= 0` guard makes division-by-zero unreachable); Skyrim → every
    level; classic TES → never. Matches each document. The cadence **phase** is
    declared in-code as "the simple modulo; refine per game if a citing pass
    pins an offset" (`leveling.rs:218-219`), which mirrors the capture
    document's own hedge — "**Perk cadence:** FO3 = 1 perk every level; FNV =
    1 perk every other level (well-known; not on the pages pulled so far —
    mark for a citing pass)" (`charal-fnv-fo3-ruleset.md:419-421`). Code and
    document agree about the residual uncertainty; not a finding.
11. **Numeric-safety sweep of the leveling arithmetic.** — **PASS.**
    `f32::from(level: u16)` is lossless; `xp_from_skill_rank` widens a `u16`;
    `skill_points` widens a `u8`; `skyrim_skill_xp_between` uses a half-open
    `(from..to)` range, so an inverted or empty range sums to `0.0` (pinned at
    `skyrim.rs:256-257`). The one narrowing cast on the level population path,
    `effective_actor_level(stats_npc).max(0) as u16` (`npc_spawn.rs:195`), is
    safe: `effective_actor_level` returns `i16` (`actor/mod.rs:96`), so the
    clamped range is `0..=32767`, well inside `u16`.

### Findings

#### CHAR-2026-09-11-D3-01: `charal.md` §4.2 declares `CharacterLevel.xp` as `f32`; the shipped component has always been `u32`
- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: all
- **Location**: `docs/engine/charal.md:167-175` (§4.2) vs `crates/core/src/character/components.rs:14-22`
- **Status**: NEW
- **Source**: `docs/engine/charal.md:170` — "`pub struct CharacterLevel { level: u16, xp: f32 /* progress toward next */ }`". This is the *canonical component spec*; there is no per-game capture value for it, so the mismatch is doc-vs-code, not a wrong game constant.
- **Description**: `charal.md` §4.2 is the design authority for the canonical `CharacterLevel` component and every downstream audit has quoted it verbatim as a Source line (e.g. #2947). It declares `xp: f32`. The component has carried `pub xp: u32` since its first commit (`be3e69d7`, confirmed with `git log -S"pub xp: u32"` — the field was never `f32`, so this is an original transcription error in the doc, not later drift). The code even documents the reasoning the doc lacks: "`u32` is ample — the per-level threshold never approaches `u32::MAX` even at FO4 extremes, and storing per-level progress (not cumulative) keeps it bounded." The mismatch is not cosmetic in one specific way: the value `xp` is compared against is `LevelingModel::xp_to_next`, which returns `f32` (`leveling.rs:166`), so the first leveling runtime to land must introduce a cast the spec says is unnecessary.
- **Evidence**: `grep -rn "xp: f32" docs/ crates/ byroredux/` returns `docs/engine/charal.md:170` and nothing in any `.rs` file; `components.rs:22` is `pub xp: u32,`. The only production readers are `crates/save/src/validate.rs:552` (`if level.xp != 0`) and `byroredux/src/npc_spawn.rs:196` (`xp: 0`), both integer-typed.
- **Impact**: Documentation only today — no leveling runtime exists, so nothing mis-computes. The blast radius is the moment one is written: `charal.md` §4.2 is what a contributor reads to learn the canonical shape, and it disagrees with the struct on the one field a progression system writes every frame. It is also an active source of bad audit citations — §4.2 has already been quoted as authoritative in a filed issue.
- **Related**: #2947 (quotes this exact §4.2 line as its Source); CHAR-2026-09-11-D3-02 and -D3-03 (same class — CHARAL prose disagreeing with byte-unchanged code)
- **Suggested Fix**: Change `charal.md:170` to `xp: u32` and carry over the one-line justification already in `components.rs:19-21`. If `f32` was the intended design, the change belongs in the code with a stated reason — but nothing today wants it.

#### CHAR-2026-09-11-D3-02: `charal-oblivion-ruleset.md`'s header cites `AttributeSet::OBLIVION`, a symbol that does not exist
- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: oblivion
- **Location**: `docs/engine/charal-oblivion-ruleset.md:4-6` vs `crates/core/src/character/tes.rs:103` and `crates/core/src/character/attribute.rs:85-116`
- **Status**: NEW
- **Source**: n/a — this is a symbol-name claim, not a numeric one, so no capture value is required. The correct name is `AttributeSet::TES_CLASSIC` (`attribute.rs:98`).
- **Description**: The Oblivion capture document — the authority Dimension 3 is required to read *before* the Rust — opens by naming the four shipped pieces of the Oblivion core ruleset: "`AttributeSet::OBLIVION`, `SkillSet::OBLIVION`, `LevelingModel::OBLIVION`, `oblivion_attribute_bonus`, `oblivion_health_formula`". Three of the five exist. `AttributeSet::OBLIVION` does not: `attribute.rs` declares exactly four attribute-set consts — `FALLOUT`, `TES_CLASSIC`, `SKYRIM`, `STARFIELD` — and `oblivion_ruleset` builds with `AttributeSet::TES_CLASSIC` (`tes.rs:103`). The name is deliberate and load-bearing: `TES_CLASSIC` signals that Morrowind and Oblivion share one 8-attribute roster, which is exactly the fact a per-game name would hide. `charal.md:336` and `tes.rs:92` both use the right name; this document is the sole outlier.
- **Evidence**: `grep -rn "AttributeSet::OBLIVION" . --include='*.rs' --include='*.md'` returns **one** hit in the entire repository — `docs/engine/charal-oblivion-ruleset.md:4` — and zero in any `.rs` file. The paragraph containing it was edited in this delta (`git diff 64f64480..HEAD -- docs/engine/charal-oblivion-ruleset.md` rewrites the same sentence's tail to add the "unwired" clause), so the stale symbol was read past during that edit.
- **Impact**: Low but real for this audit's own method: an auditor instructed to read the capture document first, then verify the code against it, is handed a symbol that does not compile and must reverse-engineer which set was meant. It also weakly implies a per-game Oblivion roster exists, inviting exactly the duplicate-per-game-const pattern `TES_CLASSIC` was named to prevent.
- **Related**: #3848 (the same sentence's freshly-corrected "unwired" clause); CHAR-2026-09-11-D3-01, -D3-03
- **Suggested Fix**: Replace `AttributeSet::OBLIVION` with `AttributeSet::TES_CLASSIC` in `charal-oblivion-ruleset.md:4` and note in half a clause that the roster is shared with Morrowind, so the next reader does not "fix" it back.

#### CHAR-2026-09-11-D3-03: `tes.rs`'s module docstring still calls the Oblivion per-level Health accrual "the deferred TES leveling-efficiency mechanic" — the same file implements it 200 lines below
- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: oblivion
- **Location**: `crates/core/src/character/tes.rs:16-18` (module doc) vs `crates/core/src/character/tes.rs:200-220` (`oblivion_health_gain_per_level`) and `docs/engine/charal.md:336-341`
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:455-459` — the per-level accrual `0.1×Endurance` is captured and LOCKED, and `docs/engine/charal.md:336-341` (edited in this delta) states "The level-up leveling-efficiency mechanics are shipped too: `oblivion_attribute_bonus(governed_skill_ups)` → +1/+2/+3/+4/+5 … and `oblivion_health_gain_per_level(endurance)` = 10 % of Endurance accrued (and stored) each level".
- **Description**: This is the exact defect class of #2946 ("`leveling.rs` docstring calls `SkillXp` 'a future third variant' and the Oblivion attribute bonus 'deferred' — both shipped"), which is CLOSED. Its fix rewrote `leveling.rs`'s module docstring — which now correctly says the level-up attribute bonuses "are implemented by the TES leveling helpers" — but did not touch `tes.rs`'s, which still reads: "Health's per-level accrual (≈10 % of Endurance each level) is **not** part of the base formula — it is **the deferred TES leveling-efficiency mechanic** (`docs/engine/charal.md` §5), a leveling concern, not a derived pool." The mechanic is not deferred: `oblivion_health_gain_per_level` is a `#[must_use] pub fn` in the same file at `:218`, exported from `mod.rs`, with two tests including the UESP anchor. The file contradicts itself — `:200-201` calls the same thing "the stateful half of the Health pool (leveling-efficiency mechanic, `charal.md` §5)". The pointed-at `charal.md` §5 was updated in *this delta* to say the opposite of what the docstring claims it says.
- **Evidence**: `tes.rs:17` — "it is the deferred TES leveling-efficiency mechanic"; `tes.rs:218-220` — `pub fn oblivion_health_gain_per_level(endurance: u16) -> f32 { 0.1 * f32::from(endurance) }`; `tes.rs:384-389` — `health_gain_per_level_is_ten_percent_of_endurance` asserts the `Endurance 100 → 10.0` UESP anchor and passes (verified in this session's run). `charal.md:340-341` — "The level-up leveling-efficiency mechanics are shipped too".
- **Impact**: Documentation only, but of the kind that causes duplicated logic: the module docstring is the first thing a contributor implementing Oblivion leveling reads, and it tells them the per-level Health accrual is *not yet built*, 200 lines above the function that builds it. That is precisely the re-implementation the project's standing "improve existing code rather than duplicating logic" instruction guards against, and it is the reason #2946 was filed in the first place.
- **Related**: #2946 (CLOSED — same class; its fix reached `leveling.rs` but not `tes.rs`); #3848
- **Suggested Fix**: Replace "the deferred TES leveling-efficiency mechanic" in `tes.rs:17` with a pointer to the sibling that implements it — "the TES leveling-efficiency mechanic, implemented by [`oblivion_health_gain_per_level`] below" — keeping the true half of the sentence ("a leveling concern, not a derived pool") intact.

#### CHAR-2026-09-11-D3-04: the Skyrim skill-XP cost curve is captured in no `charal-*-ruleset.md`, and the last sweep credited it to a Skyrim section that contains no such text
- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: skyrim
- **Location**: `crates/core/src/character/skyrim.rs:33-73` (`SKYRIM_SKILL_USE_CURVE`, `skyrim_skill_xp_to_next`, `skyrim_skill_xp_between`); documents `docs/engine/charal-skyrim-ruleset.md`, `docs/engine/charal-oblivion-ruleset.md:787-794`, `docs/engine/charal.md:147`
- **Status**: NEW (residual of CLOSED #2945 — not a regression; the closed fix landed the *character* XP curve, not the *skill* XP cost curve)
- **Source**: UNSOURCED-in-capture. The value `1.95` **is** captured, but only in the **Oblivion** document (`charal-oblivion-ruleset.md:792`, "The engine's `fSkillUseCurve` is 1.95 for the skill-use progression curve") and in `charal.md:147` (an implementation-summary table row). The cost formula `improve_mult · L^use_curve + improve_offset` and its worked anchors (Lockpicking mult 0.25 / offset 300 → 15→16 ≈ 349.13, 15→20 ≈ 1815.5) appear in **no** capture document at all.
- **Description**: #2945 filed "Skyrim/Oblivion leveling constants are sourced only to `charal.md` implementation prose (circular)" and was closed by adding `charal-skyrim-ruleset.md` § *XP / level curve — LOCKED* (`:711-721`) and `charal-oblivion-ruleset.md` § *Leveling — LOCKED* (`:787-794`). Those sections fully capture the *character*-XP half — rows 8, 12, 13, 14 above are now properly sourced, which is real progress. They do not capture the *skill*-XP half, which is a separate formula living in a separate function: `skyrim.rs:49-57`'s per-skill advancement cost. Its only documentary trace is `charal.md:147`'s parenthetical "(`fSkillUseCurve` 1.95)" — the same implementation-summary prose #2945 ruled circular — plus the function's own docstring. Consequently the `fSkillUseCurve` value a Skyrim reader needs is filed under *Oblivion*, and the formula shape is filed nowhere. The concrete harm has already occurred once: `AUDIT_CHARACTER_2026-08-30.md:306` records row 15's Source as "`charal-skyrim-ruleset.md`" and marks it PASS. `grep -ni "1\.95\|349\|improve\|use.curve" docs/engine/charal-skyrim-ruleset.md` returns nothing relevant — the cited authority does not contain the cited claim, so that PASS was recorded against a source that does not exist.
- **Evidence**: `grep -rn "fSkillUseCurve\|SkillImproveMult\|SkillImproveOffset\|349\.13\|1815" docs/` returns exactly two engine-doc hits — `charal.md:147` and `charal-oblivion-ruleset.md:792` — and zero in `charal-skyrim-ruleset.md`, whose 18 `##` sections (listed via `grep -n "^## "`) contain no skill-advancement-cost section. The code's own docstring (`skyrim.rs:44-48`) does carry the citation: "Source: UESP *Skyrim:Leveling* (Lockpicking 15→16 = `0.25·15^1.95 + 300` ≈ 349.13)", and `skill_xp_cost_matches_uesp_lockpicking` (`skyrim.rs:246-258`) pins both anchors and passes.
- **Impact**: No wrong number ships — `1.95` is supported and `improve_mult`/`improve_offset` are function parameters (the AVIF-authored per-skill values), not hardcoded constants, so there is nothing here for the no-guessing policy to catch. The impact is on the audit method itself: this dimension is contractually required to verify constants against the `charal-*-ruleset.md` captures, and for this formula there is nothing to verify against, which is how a fabricated source citation survived a full deep pass. It also leaves the one Skyrim-specific engine coefficient reachable only through the Oblivion document.
- **Related**: #2945 (CLOSED — the character-XP half of the same gap); `AUDIT_CHARACTER_2026-08-30.md:306` (the mis-citation this produced)
- **Suggested Fix**: Add a short "## Skill advancement cost — LOCKED" section to `charal-skyrim-ruleset.md` transcribing `skyrim.rs:42-48`'s formula, the `fSkillUseCurve = 1.95` value, the Lockpicking 0.25/300 authored pair and both worked anchors, citing UESP *Skyrim:Leveling* — and state explicitly that `improve_mult`/`improve_offset` are per-skill AVIF-authored, not engine constants, so the next sweep does not look for them as hardcoded values.

### Candidates verified and dropped

- **"`Perks` population bypasses `try_set_rank`, so a rank-0 or out-of-range
  `PRKR` entry lands verbatim."** Structurally still true —
  `stamp_character_components` (`npc_spawn.rs:209-221`) builds
  `Perks { entries: … .collect() }` directly, and `npc_spawn.rs` moved 144
  lines in this delta without changing that shape. Falsified against shipped
  data in the last sweep (10,764 authored `PRKR` entries across `Skyrim.esm`
  and `Fallout4.esm`, rank histogram `[(1, N)]`, zero duplicate perk FormIDs),
  so neither the rank-0 ghost nor an over-`num_ranks` rank exists in vanilla
  content. Not re-measured (probe evidence is recent and the parse side is
  unchanged); not re-filed.
- **"A third GMST has crept back into `with_gmst`."** Falsified two
  independent ways (repo-wide `fXP` grep; the recording-probe test's exact
  ordered-list assertion). The withdrawn `fXPPerSkillRank` read is gone from
  the code *and* from all three documents that used to assert it.
- **"`level_cap == 0` is mishandled in one variant / off by one at the cap."**
  Impossible by construction: `level_cap()` is a single merged or-pattern arm
  over all three variants, so there is no per-variant code path to diverge.
  And with no production consumer, no cap is applied at runtime at all.
- **"#2943 regressed — `level_cap()`'s docstring still promises a DLC bump no
  loader performs."** Verified fixed: `leveling.rs:236-238` and `:54-56` now
  say the raises are "a loader concern and are not wired yet" and instruct
  callers to treat the value as the authored base cap. Correct disclosure.
- **"#2944 regressed — `set_rank` still clamps or accepts an over-max rank."**
  Verified fixed: `try_set_rank` rejects `rank == 0` and `rank > num_ranks`
  (`components.rs:90-97`), `set_rank` documents the rank-0 no-op, and `remove`
  now exists.
- **"`grants_perk_at` can divide by zero on a zero `perk_cadence`."** Guarded:
  `*perk_cadence != 0 &&` short-circuits before `is_multiple_of`
  (`leveling.rs:230`). No construction site sets it to 0 in any case.
- **"`skyrim_skill_xp_between` mis-handles an inverted range."** Half-open
  `(from_level..to_level)` yields an empty iterator, summing to `0.0`; pinned
  for both the inverted (20→15) and degenerate (15→15) cases at
  `skyrim.rs:256-257`.
- **"`effective_actor_level(...) as u16` can truncate a large level onto the
  leveling model."** Falsified: the function returns `i16`
  (`actor/mod.rs:96`), so `.max(0) as u16` is exact over `0..=32767`.
- **"Perks are not `TPLT`-resolved — `stamp_character_components` reads
  `npc.perks` while level and background read the resolved `stats_npc` /
  `traits_npc`."** Real asymmetry (`npc_spawn.rs:195-217`), but **dropped as
  unciteable**: no capture document assigns perks to any template-inheritance
  flag, and the `TEMPLATE_FLAG_*` set the resolver honours has no perk bit.
  Asserting that perks *should* follow `Use Stats` would be a guess. Noted here
  so the next sweep does not re-derive it; it belongs to Dimension 5 if a
  source is ever found.
- **"`charal.md` §5's freshly-added Oblivion 'unwired' paragraph overstates or
  understates the code."** Checked clause by clause against
  `profile.rs:82-87` and `build_ruleset`: `RulesetBuilder::None`, `build_ruleset`
  returning `None` before `with_gmst`, and the "no `AVIF` in `Oblivion.esm`"
  claim (independently asserted by
  `records/tests.rs:168` `oblivion_ruleset_resolves_nothing_against_avif_pending_a_legacy_resolver`
  and `parse_real_esm.rs`'s `authors_actor_values: false`). All accurate.

### Reachability (coverage information for Dimension 6, not a bug)

`CharacterRulesProfile::build_ruleset` (`profile.rs:168-186`) has exactly three
constructible arms — `RulesetBuilder::Fallout3`, `::FalloutNewVegas`,
`::Fallout4` — all of which produce `LevelingModel::XpCurve`. `Oblivion`,
`Skyrim`, `Fallout76`, `Starfield` and `NONE` carry `RulesetBuilder::None` and
`build_ruleset` returns `None` before reaching `with_gmst`.

| Family | Leveling model | Constructed in production? |
|---|---|---|
| FO4 | `XpCurve` / `SpecialOrPerk` | **Yes** — `RulesetBuilder::Fallout4` |
| FO3 | `XpCurve` / `SkillPoints{10, 1.0, 1}` | **Yes** — `RulesetBuilder::Fallout3` |
| FNV | `XpCurve` / `SkillPoints{10, 0.5, 2}` | **Yes** — `RulesetBuilder::FalloutNewVegas` |
| Skyrim | `SkillXp` | **No** — `RulesetBuilder::None` |
| Oblivion | `SkillUse` | **No** — `RulesetBuilder::None` |
| FO76 / Starfield | none | **No** — no model defined |

Consequences to carry forward, none of them defects:
`LevelingModel::OBLIVION` / `::SKYRIM`, `oblivion_ruleset`, `skyrim_ruleset`,
`oblivion_attribute_bonus`, `oblivion_health_gain_per_level`,
`skyrim_skill_xp_to_next` / `_between` and `SKYRIM_SKILL_USE_CURVE` are
**correct but unwired** (constant rows 8–11, 13–17 all PASS). Because
`with_gmst`'s only non-identity arm is `SkillXp`, and `SkillXp` is never
constructed, **the GMST overlay is a no-op on every ruleset the engine actually
builds** — it is plumbed, tested, now covered against real `Skyrim.esm` data
(#3923), and unreached. Separately, **nothing in production consumes any
leveling accessor except `xp_to_next`** (one call, `condition.rs:675`):
`level_cap`, `grants_perk_at`, `skill_points`, `pool_pick_gain` and
`xp_from_skill_rank` are test-only, consistent with "no leveling runtime exists
yet". All of this is **Existing: #3848 (OPEN)** and is deliberately not
re-filed.


## Dimension 4 — Pools, Afflictions, Resistances & Reputation

Sweep: `/audit-character`, 2026-09-11, HEAD `8151cded`. Capture documents read
before any Rust (`charal.md` §4.5/§4.6/§4.7/§7.1, `charal-oblivion-ruleset.md`
§Fatigue/§Health/§Magicka, `charal-fnv-fo3-ruleset.md` §Karma/§FNV Reputation/
§Radiation+Poison Resistance, `charal-fo4-ruleset.md` §Radiation Resistance,
`charal-skyrim-ruleset.md` §Disease, `charal-starfield-ruleset.md` §Companion
Affinity).

**Delta in scope**: `regen.rs` +151, `affliction.rs` +29.
`resistance.rs`, `reputation.rs`, `components.rs` (`FactionReputation`) are
byte-unchanged; their constants were still re-derived from the documents, not
carried forward from the 2026-08-30 report.

Tests: `CARGO_BUILD_JOBS=4 cargo test -p byroredux-core character` → **115
passed, 0 failed** (matches the BRIEF's expected baseline).

**Findings: 2, both LOW. 0 CRITICAL / 0 HIGH / 0 MEDIUM.**

---

### Constant verification

| # | Formula/constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | `POOL_REGEN_DT` | `1.0/60.0` | rates stated "per real-time second" → fixed 60 Hz drain | `charal.md` §4.7 | PASS |
| 2 | `FATIGUE_REGEN_PER_SEC` | `10.0` | `FatigueRegen = END × fFatigueReturnMult(0.0) + fFatigueReturnBase(10.0)` = flat 10/sec | `charal-oblivion-ruleset.md` §Fatigue item 1 | PASS |
| 3 | Fatigue Endurance coefficient | **absent** (no END term in code) | `fFatigueReturnMult` ships at `0.0` in vanilla | same | PASS — correctly not modelled |
| 4 | `MAGICKA_REGEN_WILLPOWER_COEFF` | `0.02` | `MagickaRegen = (Willpower × 0.02 + 0.75) × (MaxMagicka/100)` | `charal-oblivion-ruleset.md` §Magicka | PASS |
| 5 | `MAGICKA_REGEN_BASE` | `0.75` | same | same | PASS |
| 6 | `magicka_regen_per_sec` shape | `(wp·0.02 + 0.75)·(max/100)`; `0.0` when `stunted` or `max ≤ 0` | identical; "Stunted Magicka … zeroes regen entirely regardless of Willpower/MaxMagicka" | same | PASS |
| 7 | Oblivion **Health** passive regen | **absent by design** | "Vanilla Oblivion Health has NO passive regeneration at all" (the regen paragraph is `{{OBR}}`-tagged) | `charal-oblivion-ruleset.md` §Health | PASS — correctly unmodelled |
| 8 | Oblivion **Remastered** regen formulas (Fatigue `(AGI×8/100)+12`, Health `(END×0.34/100)+0.16`, Magicka `0.0003·WIL²+0.015·WIL`) | **absent** | explicitly out of compat scope (classic 2006 Gamebryo is the target) | `charal-oblivion-ruleset.md` §Fatigue item 2 / §Health / §Magicka | PASS — correctly not adopted |
| 9 | `MAX_REGEN_SUBSTEPS` | `8` | **none** — engine tuning, not a game rule | — | PASS (labelled unsourced in-code; explicitly *not* "corrected" to physics' 5 — correct no-guessing posture). Cross-checked: `crates/physics/src/world.rs:15 MAX_SUBSTEPS = 5`, `:13 PHYSICS_DT = 1/60`, `:39 SUBSTEP_TIME_BUDGET` — every claim in the doc block is currently true |
| 10 | Radiation resistance `Affliction::RADIATION` | `derive_coeff 2.0`, `resist_cap 85.0`, governing `"Endurance"` | `(END − 1)·2`, capped **85 %**, FO3 == FNV; worked END 5 → 8 % | `charal-fnv-fo3-ruleset.md` §Radiation Resistance | PASS (test asserts 8.0 and the 85 cap) |
| 11 | Poison resistance `Affliction::POISON` | `derive_coeff 5.0`, `resist_cap f32::INFINITY` | `(END − 1)·5`, **no documented FO3/FNV cap** ("don't invent one") | `charal-fnv-fo3-ruleset.md` §Poison Resistance | PASS — uncapped is the sourced answer, not an omission |
| 12 | `fo3_fnv_resistance_formula` negative-bias floor | `.clamped_below(0.0)` | not a document value — SPECIAL domain is 1–10, so `(0−1)·k` is outside it | `charal-fnv-fo3-ruleset.md` (attribute domain) | PASS — labelled in-code as a defensive domain floor, not a balance number (#2939) |
| 13 | `damage_multiplier` curve + cap | `(1 − clamp(r,0,cap)/100).max(0.0)` | "damage is reduced by this percentage"; ≥100 % = immunity | `charal-fnv-fo3-ruleset.md` §Radiation/Poison Resistance + `resistance.rs` module doc | **PASS — cannot exceed 100 %, cannot invert into healing** (floored at 0.0; verified `damage_multiplier(120, ∞) == 0.0`) |
| 14 | FO4 resistance curve | **deliberately absent** | FO4 uses the shared non-linear DR/ER curve; "the wiki gives empirical sample tables only", closed form unsourced | `charal-fo4-ruleset.md` §Radiation Resistance | PASS — correctly not modelled (no-guessing) |
| 15 | Skyrim disease reuse of `AfflictionTable` | **explicitly refused** in `resistance.rs` module doc | "Do not reuse `AfflictionTable` for Skyrim disease without redesigning it — the mechanisms don't match" | `charal-skyrim-ruleset.md` §Disease | PASS |
| 16 | `KARMA_MIN` / `KARMA_MAX` | `-1000` / `1000` | "signed int, starts 0, clamped to [−1000, +1000] (FO3 == FNV, identical)" | `charal-fnv-fo3-ruleset.md` §Karma | PASS |
| 17 | Karma band cut points | `≥750` VeryGood · `≥250` Good · `≥−249` Neutral · `≥−749` Evil · else VeryEvil | +750…+1000 / +250…+749 / −249…+249 / −250…−749 / −1000…−750 | same | PASS — including the asymmetric −249 vs −250 boundary (pinned by `karma_bands_at_exact_boundaries`, 11 exact-boundary assertions) |
| 18 | `clamp_karma` | bounds **both** ends (`< MIN → MIN`, `> MAX → MAX`) | "clamped at both ends" | same | PASS |
| 19 | `REPUTATION_BUMP_POINTS` | `[0, 1, 2, 4, 7, 12]`, index 0 unused, out-of-range → 0 | editor int 1–5 → 1 / 2 / 4 / 7 / 12; "`addreputation … 5` adds 12" | `charal-fnv-fo3-ruleset.md` §FNV Reputation bump table | PASS — all 5 entries + the worked example |
| 20 | `REPUTATION_AXIS_MAX` | `100` | "Each axis caps at 100 in normal play … = the steepest vanilla Range-3, Caesar's Legion" | same | PASS (and consistent: Legion R3 = 100 is exactly reachable) |
| 21 | **13 FNV faction threshold triples — all 39 numbers** | Boomers 8/25/50 · BoS 3/10/20 · Legion 15/50/100 · Followers 8/25/50 · Khans 5/15/30 · Powder Gangers 5/15/50 · NCR 12/40/80 · WGS 2/5/10 · Freeside 11/35/70 · Goodsprings 3/8/15 · Novac 3/10/20 · Primm 5/15/30 · The Strip 6/20/40 | identical, row for row | `charal-fnv-fo3-ruleset.md` §per-faction threshold table | **PASS — 39/39** |
| 22 | **13 `BY_FORM_ID` REPU keys** | `000FFAE8 / 0011E662 / 000F43DD / 00124AD1 / 0011989B / 001558E6 / 000F43DE / 00116F16 / 00129A7A / 00104C22 / 00129A79 / 000F2406 / 00118F61` | identical, and in the document's stated `BY_FORM_ID` order so the two diff line-for-line | same | PASS — 13/13, order preserved as documented |
| 23 | `FactionRepThresholds::range` boundary semantics | `>=` against r3, then r2, then r1, else 0 | "minimum points required for each level" | same | PASS — inclusive minimums, half-open, exactly one range per value |
| 24 | **4×4 standing grid axes** | `STANDING_GRID[infamy][fame]`; `from_ranges(fame, infamy)` indexes `[i][f]`; `classify` → `from_ranges(t.range(fame), t.range(infamy))`; `FactionReputation::standing` passes `(fame, infamy)` | doc table is `Infamy ↓ \ Fame →` | same | **PASS — not transposed.** Verified on the ASYMMETRIC off-diagonal, not the corners: `from_ranges(fame 2, infamy 1) = SmilingTroublemaker` and `from_ranges(fame 1, infamy 2) = SneeringPunk` — the exact pair a transposition swaps. Also checked `(3,2) = DarkHero` and `(0,3) = Vilified`. The component bridge (`components.rs` `standing()`) passes the same order |
| 25 | 16 standing titles | Neutral/Accepted/Liked/Idolized · Shunned/Mixed/SmilingTroublemaker/GoodNaturedRascal · Hated/SneeringPunk/Unpredictable/DarkHero · Vilified/MercifulThug/SoftHeartedDevil/WildChild | identical 16 cells | same | PASS |
| 26 | `ReputationStanding::sentiment` | green/black/red bucketing, `DarkHero`+`SoftHeartedDevil` classed Mixed | **not captured** — the document gives titles + a colour *legend*, never the per-cell colour | — | **UNSOURCED**, but already labelled unsourced in-code with the two judgement-call cells named (#2949, CLOSED). Correct disclosure; **not re-filed** |
| 27 | `AFFINITY_MIN` / `AFFINITY_MAX` | `-1000` / `1100` | "clamps to `[-1000, +1100]` (not Karma's symmetric ±1000)" | `charal.md` §7.1 | PASS — asymmetry preserved |
| 28 | Affinity 7 bands | `≥1000` Idolize · `≥750` Confidant · `≥500` Admiration · `≥250` Friend · `≥0` Neutral · `≥−500` Disdain · else Hatred | "7 bands (Hatred/Disdain/Neutral/Friend/Admiration/Confidant/Idolize) at thresholds `-500/0/250/500/750/1000`" | same | PASS — 6 thresholds, 7 bands, names in order |
| 29 | `affinity_reaction_delta` base deltas | Liked +15, Loved +35, Disliked −15, Hated −35 | "reactions are `±15` (like/dislike) or `±35` (love/hate)" (`TryToModAffinity`) | same | PASS |
| 30 | `AffinityReactionSize` scalars | `0.5 / 1.0 / 1.5` | `CA_Size_{Small,Normal,Large}` = `0.5/1/1.5` | same | PASS |
| 31 | `affinity_passive_gain` | `40.0 − 0.033 × current` | `40 − 0.033·current_affinity`, worked example 500 → +23.5 | same | PASS (`40 − 16.5 = 23.5` ✓; the in-code "worst case at the 1100 cap is +3.7" also checks: `40 − 36.3 = 3.7`) |
| 32 | `clamp_affinity` | bounds **both** ends | — | — | PASS |
| 33 | Starfield affinity (`WantsToTalk +1`, gates 100·N, 30 min / 1 h) | **not implemented** | LOCKED for gates, **PENDING** for reaction deltas (`{{Conversation Key (affinity)}}` never expanded) | `charal-starfield-ruleset.md` §Companion Affinity | PASS — correctly absent; implementing it today would require guessing the 5 reaction deltas |
| 34 | Shipped `AfflictionTable` band thresholds | **none — every table empty** | PENDING for every game; "no citable source has been found yet" | `charal.md` §4.6, `charal-fnv-fo3-ruleset.md` §Radiation Resistance | PASS — mechanism-ahead-of-data, correctly not invented |

**Unsourced-in-code tally**: 2 rows (#9 `MAX_REGEN_SUBSTEPS`, #26 `sentiment`),
both already self-declared as unsourced at their definition site with a named
issue. No silently-unsourced constant found. **Rows 1–8, 10–25, 27–34: PASS.**

---

### Checks performed

1. **The fixed-60 Hz tick — PASS.**
   - *Backlog clamp*: `PoolRegenAccumulator::advance` (`regen.rs:108-117`)
     clamps `self.accumulator` to `MAX_REGEN_SUBSTEPS × POOL_REGEN_DT`
     (0.1333 s) **before** extracting ticks, and subtracts the extracted ticks
     from the already-clamped value — so the discarded backlog cannot roll into
     the following frame either. `accumulator_ticks_at_sixty_hz_and_caps_catchup`
     pins both halves (`advance(10.0) == 8`, then `advance(0.0) == 0`).
   - *Per fixed tick, not per frame*: the system computes
     `elapsed = ticks as f32 * POOL_REGEN_DT` (`:196`) and applies
     `rate * elapsed`. Both rates are linear in time, so a single multiply is
     exactly equivalent to N per-tick applications; no frame-`dt` term reaches
     either pool.
   - *Paused / zero / negative dt cannot spin*: `frame_dt.max(0.0)` (`:109`)
     turns a negative dt into 0 — and, because Rust's `f32::max` returns the
     non-NaN operand, a NaN dt also becomes 0 rather than poisoning the
     accumulator. `+∞` clamps to `max_acc` on the next line. `ticks == 0` then
     returns at `:193` before any query is taken. The accumulator is never
     decremented below zero (`ticks` is derived from it by `floor`).
2. **`PoolRegenConfig` insertion + declared access — PASS on the declaration,
   coverage note on the insertion.**
   - *Insertion*: `PoolRegenConfig` is inserted by **no production path**.
     `oblivion_pool_regen_config` (`tes.rs:159-167`) is the only constructor and
     has zero callers; `build_character_ruleset` returns `None` for Oblivion.
     So the config cannot exist without a live per-game ruleset path — the
     invariant the checklist asks about holds vacuously today. Recorded as
     coverage (`charal.md` §4.7, and the ruleset half is **OPEN #3848**), **not
     re-filed**.
   - *Accumulator*: inserted unconditionally at boot
     (`byroredux/src/boot/world.rs:48`), deliberately, so the config is the only
     outstanding precondition (#2950). Verified.
   - *Declared access vs body*: `boot/schedule/update.rs:303-310` declares
     `reads_resource::<PoolRegenConfig>` + `writes_resource::<PoolRegenAccumulator>`
     + `reads_resource::<CharacterRuleset>` + `writes::<ActorValues>`. The body
     touches exactly those four and nothing else: `try_resource::<PoolRegenConfig>`
     (`:165`), `try_resource_mut::<PoolRegenAccumulator>` (`:188`),
     `try_resource::<CharacterRuleset>` (`:201`), `query_mut::<ActorValues>`
     (`:202`). **Exact match — including after #3483**, which changed
     `CharacterRuleset` from a hard gate to an optional read but did **not**
     change whether it is read, so the declaration stayed correct.
     `scheduler_access_tests.rs:454` (`contract_bearing_exclusives_declare_their_access`)
     pins that the declaration exists and is non-blank.
3. **Affliction diff-and-reapply — PASS.** `reevaluate_affliction`
   (`affliction.rs:159-179`) early-returns when `new_band == old_band`, then
   **reverses the old band first** (`mod_temporary(p.avif_form_id, -p.delta)`
   for every penalty of `bands[old]`) and only then applies the new band's.
   Penalties therefore cannot compound across escalation, de-escalation or a
   cure, and repeated same-band ticks are free. Pinned by
   `reevaluate_applies_penalties_entering_a_band` and siblings.
4. **Band boundaries half-open and total — PASS.** `band_for` selects the
   reached band with the greatest `min_pool` via
   `filter(|b| pool >= b.min_pool).max_by(total_cmp on min_pool)`. `>=` makes
   each boundary belong to the upper band only; below every `min_pool` yields
   `None` (healthy). A value exactly on a boundary lands in exactly one band
   for any table with distinct thresholds.
5. **Resistance curve + cap — PASS.** `damage_multiplier` clamps
   `resist_pct` into `[0, cap]` and then floors `(1 − r/100)` at `0.0`, so a
   resistance ≥ 100 % is immunity and **never a negative multiplier (healing)**.
   Pinned by `damage_multiplier_cuts_and_clamps` (including `(120, ∞) → 0.0`).
   Rad cap 85 → floor 0.15; over-cap 200 clamps to the same 0.15.
6. **Karma / reputation — PASS.** Grid axes verified on the asymmetric
   off-diagonal (table row 24), all 39 faction thresholds and all 13 REPU
   FormIDs verified (rows 21–22), both clamps bound both ends (rows 18, 32),
   bump table verified against the document's own worked example (row 19).
   `FactionReputation::{add_fame,add_infamy}` are monotonic
   (`saturating_add(...).min(REPUTATION_AXIS_MAX)`) with `reset` as the only
   decrement — matching "cannot be lost … scripted resets are not a decrement
   op".
7. **Delta commit `269bab3c` (#3483) — verified clean.** (a) *Does the ungating
   let Fatigue regen run where it should not?* No: the guard is still
   `avs.get(config.fatigue_avif).is_some()`, and `PoolRegenConfig` is per-game
   and resolved from EditorIDs, so only actors carrying the configured Fatigue
   AV are touched. The document states the rate as a GMST pair
   (`fFatigueReturnBase`/`Mult`) with no player-only qualifier, so
   actor-general is the sourced reading. (b) *Access declaration*: unchanged
   and still accurate (check 2). (c) *A second code path with different
   clamping?* No — one `avs.restore` call per pool, unchanged; `restore`
   already floors damage at 0 so no separate max-pool clamp is needed.
   Magicka's no-ruleset path reuses the **same** `base_max` fallback #2932
   installed for player-only rows — one branch, not two. Pinned by
   `both_pools_regenerate_without_a_character_ruleset`.
8. **Delta commit `ea016540` (#3444) — verified clean.** `let config =
   *config_guard; drop(config_guard);` (`:186-187`) genuinely names and drops
   the `ResourceRead` guard, replacing #2153's `let config = *config;`, which
   only shadowed the binding and left the guard alive to end-of-scope. `config`
   is a `Copy` value (`PoolRegenConfig` is `#[derive(Copy)]`, three `u32`s), so
   every later use (`:206`, `:209`, `:210`, `:228`, `:234`) reads the **copy**,
   not the resource — **there is no window in which the config is read after
   the lock is released**, because nothing reads the resource after the drop at
   all. Hold stack at the `ActorValues` acquire is now genuinely 2
   (`CharacterRuleset` → `ActorValues`), the #3441 canonical order. The
   regression test's needle `"drop(config_guard);"` (with semicolon) does not
   collide with the doc comment's backticked `` `drop(config_guard)` `` two
   lines above it, so `src.find` resolves to the real code line — the test is
   not vacuous.
9. **Carry-forward CHAR-2026-08-30-D4-01 — FIXED, not re-filed.** Published as
   **#3767, now CLOSED** (`6ac9caf5`, fix landed in `c604375f`).
   `band_for` no longer uses `rposition` (which returned the *last* matching
   index and was only correct for an ascending table); it now takes the
   `max_by` over `min_pool`, so band order genuinely does not affect the
   answer. The struct docstring's "must be sorted ascending" contract was
   replaced with "band order is not significant", and a new test
   `band_for_ignores_band_order` reverses `stand_in_radiation_table()` and
   asserts the reversed indices. Verified by reading both the diff and the
   current file. **Confirmed fixed.**

### Mechanism checks

- **Affliction wiring.** Still no `AfflictionTable` constructed in production
  and `affliction_tick_system` has no scheduler registration (its own
  save-registry allowlist entry, `registry_completeness_tests.rs:196`, says so:
  "forward-latent: affliction_tick_system has no production scheduler
  registration"). Documented mechanism-ahead-of-data per `charal.md` §4.6 —
  **coverage, not a bug**, per the BRIEF.
- **`damage_multiplier` has no production consumer.** Exported from
  `character/mod.rs:112` and exercised only by tests; combat does not call it
  yet. This is CHARAL's stated state-producing boundary ("neither module
  applies the effect"), not a defect. The *other* half of `resistance.rs` **is**
  live: `fallout.rs:77-82` iterates `Affliction::ALL` and pushes both derived
  resistance rows into the FNV/FO3 ruleset, so rows 10–12 of the constant table
  are shipped values with a real consumer, not dead code.
- **`DerivedScope::PlayerOnly` is effectively unreachable from the regen tick.**
  Oblivion's max-Magicka row is `.player_only()`, and the tick's scope filter
  therefore declines it for **every** actor including the player, falling back
  to `base_max`. This is structural, not an oversight: the player identity lives
  in `PlayerEntity` (`byroredux/src/systems/character.rs:42`), a resource in the
  **binary** crate that `crates/core` cannot reach. Behaviour is correct wherever
  the player's Magicka base layer is populated at chargen. Recorded, not filed.
- **Substep-cap divergence doc block is currently accurate.** Every factual
  claim in `MAX_REGEN_SUBSTEPS`'s docstring was re-checked against
  `crates/physics/src/world.rs`: `MAX_SUBSTEPS = 5` (:15), `PHYSICS_DT = 1/60`
  (:13), and `SUBSTEP_TIME_BUDGET` exists (:39) with no regen analogue. No doc
  rot. `substep_cap_does_not_claim_to_mirror_physics` guards the claim.

---

### Findings

#### CHAR-2026-09-11-D4-01: `#3767`'s fix declared band order insignificant, but `ActiveAffliction` still stores a raw *index* into that `Vec` and `reevaluate_affliction` indexes it unchecked
- **Severity**: LOW
- **Dimension**: Pools, Afflictions, Resistances & Reputation
- **Game**: all
- **Location**: `crates/core/src/character/affliction.rs:66-70` (the new docstring), `:93-104` (`band_for`), `:110-119` (`ActiveAffliction`), `:159-179` (`reevaluate_affliction`)
- **Status**: NEW
- **Source**: n/a — a structural contract, not a numeric claim. (The band *numbers* remain PENDING for every game per `charal.md` §4.6.)
- **Description**: #3767 removed `AfflictionTable`'s "`bands` **must** be sorted ascending by `min_pool`" contract and replaced it with "Band order is not significant; `band_for` selects the highest reached `min_pool`, so authored tables remain correct even when their entries are transcribed in a different order." That is true of the **classifier**, but the per-actor *memory* is order-dependent: `ActiveAffliction.band` is a `Option<usize>` index into `AfflictionTable::bands`, and `reevaluate_affliction` reverses the previous band by direct indexing (`&table.bands[i]`). Under the old sorted contract, index ↔ logical band was a canonical function of the data; under the new one, two tables holding the *same* bands in different order produce different indices for the same band. The old docstring made the index stable by construction; the new one asserts an order-independence the stored state does not have.
- **Evidence**: `affliction.rs:66-69` — "Band order is not significant … authored tables remain correct even when their entries are transcribed in a different order." Against `affliction.rs:167-171`:
  ```rust
  if let Some(i) = old_band {
      for p in &table.bands[i].penalties {
          avs.mod_temporary(p.avif_form_id, -p.delta);
      }
  }
  ```
  Two concrete consequences. (1) *Wrong reversal*: an actor whose `AfflictionStatus` was written against table order `[200, 600]` (index 1 = the 600-rad band) and is re-evaluated against the same bands in order `[600, 200]` reverses the **200**-rad penalties, leaving the 600-rad `temporary_mod` permanently applied — silent, no crash, and `reevaluate_affliction` stays internally consistent while doing it. (2) *Unchecked index*: `table.bands[i]` panics outright if the table is rebuilt with fewer bands while a stale index is held. `AfflictionStatus` is explicitly earmarked for save serialisation (`byroredux/src/save_io/registry_completeness_tests.rs:196` — "classify as gameplay state when activated"), which is exactly the path that carries an index across a table rebuild.
- **Impact**: Latent today — no `AfflictionTable` is constructed in production, `affliction_tick_system` has no scheduler registration, and no code path reorders a table mid-session. The blast radius is the moment real per-game tables land *and* `AfflictionStatus` is saved: the invariant that used to prevent this (sorted-ascending) was just removed, and its removal was documented as making order *safe*, which is the opposite of the guidance a future author needs. `band_for_ignores_band_order` reverses the table between two **stateless** `band_for` calls, so it cannot detect the stateful case.
- **Related**: #3767 (CLOSED — the fix this follows from), CHAR-2026-08-30-D4-01 (the original finding), `charal.md` §4.6, `charal.md` §4.5 (`FactionReputation` keys by FormID rather than index — the shape that does not have this problem)
- **Suggested Fix**: Either key `ActiveAffliction` by something order-stable (the band's `min_pool`, mirroring how `FactionStanding` keys by `repu_form_id` rather than position), or restore an explicit "band order must be stable for the lifetime of any `AfflictionStatus` that references it" clause on the struct and make the reversal `table.bands.get(i)` instead of `table.bands[i]`. Either way, extend `band_for_ignores_band_order` to a *stateful* case: `reevaluate_affliction` into band 1, reverse `table.bands`, re-evaluate, and assert the `temporary_mod` deltas net to the new band's.

#### CHAR-2026-09-11-D4-02: `pool_regen_tick_system` evaluates the max-pool formula at a hardcoded `level = 1`, discarding the `CharacterLevel` it has the entity id to read
- **Severity**: LOW
- **Dimension**: Pools, Afflictions, Resistances & Reputation
- **Game**: all (latent; Oblivion is the only game that would reach it today)
- **Location**: `crates/core/src/character/regen.rs:205` (`for (_entity, avs)`), `:230` (`ruleset.derived_value(config.magicka_avif, avs, 1)`)
- **Status**: NEW
- **Source**: UNSOURCED-in-code. No capture document states that any max-pool formula is level-invariant in general; `charal-oblivion-ruleset.md` §Magicka gives `Magicka = 2×Intelligence` (level-independent), which is why the wrong argument is currently harmless — but the call site is game-agnostic and the `1` carries no comment saying so.
- **Description**: `CharacterRuleset::derived_value(output_avif, avs, level: u16)` (`ruleset.rs:110`) takes the level as a caller-supplied parameter, and `DerivedInput::LEVEL` is a real, shipped input — FNV/FO3 Health is `bilinear(END, LEVEL, …)` (`ruleset.rs:176`, `derived.rs:381-385`). The regen tick passes a literal `1`. Every **other** production `derived_value` call site resolves the real level: `byroredux/src/combat.rs:459-461` does `world.get::<CharacterLevel>(aggressor).map_or(1, |l| l.level)`, and `crates/scripting/src/condition.rs:524` passes a resolved `level`. The regen tick is the sole outlier, and it is not a capability gap — it holds the entity id and discards it as `_entity` at `:205`, while `CharacterLevel` lives in `crates/core` (`character/components.rs:15-27`) and is reachable from there.
- **Evidence**: `regen.rs:205,226-231`:
  ```rust
  for (_entity, avs) in avs_q.iter_mut() {
      ...
      let scoped_max = ruleset.as_ref().and_then(|ruleset| {
          ruleset
              .derived_formula(config.magicka_avif)
              .filter(|f| f.scope == DerivedScope::ActorGeneral)
              .and_then(|_| ruleset.derived_value(config.magicka_avif, avs, 1))
      });
  ```
  versus `byroredux/src/combat.rs:459-462`:
  ```rust
  let level = world
      .get::<CharacterLevel>(aggressor)
      .map_or(1, |level| level.level);
  ruleset.derived_value(melee_damage_avif, &avs, level)
  ```
- **Impact**: Zero today — no shipped max-Magicka row uses `DerivedInput::LEVEL` (Oblivion's is `2×INT`; Skyrim's pools advance by level-up pick, not a LEVEL formula, and the Skyrim ruleset is unreachable anyway per #3848), and the whole system is inert pending `PoolRegenConfig`. It becomes live the first time any game expresses a regenerating pool's maximum as a function of level: every actor's regen rate would then be computed off a level-1 pool, i.e. progressively too slow as the character levels, with no error and no test to catch it. The neighbouring `DerivedScope` filter shows the site is meant to honour the formula's full contract (#2932 went to real lengths for `scope`); the `level` argument next to it is a silent placeholder.
- **Related**: #2932 (the `DerivedScope` half of this same call), #3483 / #3444 (the delta commits that rewrote the surrounding lines without touching the `1`), `charal.md` §7.1 (the caller-supplied `level` parameter is load-bearing there too — scale-to-leader companions pass the *player's* level)
- **Suggested Fix**: Join `CharacterLevel` in the loop — `let level = level_q.get(entity).map_or(1, |l| l.level);` — matching `combat.rs`'s existing `map_or(1, …)` fallback, and add the read to the scheduler access declaration in `boot/schedule/update.rs`. If a hardcoded `1` is deliberate (e.g. "no regenerating pool is level-scaled in any supported game"), state that in a comment and cite it, rather than leaving a bare literal.

---

### Candidates verified and dropped

- **"The 4×4 Fame/Infamy grid is transposed."** Re-checked from scratch against
  the asymmetric off-diagonal (`(fame 2, infamy 1) = SmilingTroublemaker` vs
  `(fame 1, infamy 2) = SneeringPunk`), not the corners. `[infamy][fame]`
  indexing is correct at all three layers — `STANDING_GRID`, `from_ranges`, and
  the `FactionReputation::standing` bridge. Not a finding.
- **"Karma's Neutral/Evil boundary is off by one."** `≥ −249 → Neutral`,
  `≥ −749 → Evil` reproduces the document's `−249…+249` / `−250…−749` split
  exactly, asymmetry included, and `karma_bands_at_exact_boundaries` asserts all
  11 cut points. Not a finding.
- **"#3483's ungating dropped `DerivedScope` enforcement for Fatigue."** No —
  Fatigue never had a formula row to scope-check; its rate is the flat
  `FATIGUE_REGEN_PER_SEC` constant, sourced as a GMST pair with no player-only
  qualifier. The scope filter remains on the Magicka branch, where it belongs.
- **"#3444's drop leaves a window where the config is read after release."**
  Falsified: `PoolRegenConfig` is `Copy`; every post-drop use reads the local
  copy, and the resource is never touched again after `:187`.
- **"#3444's regression test is vacuous because the doc comment above it also
  contains `drop(config_guard)`."** Falsified: the needle includes the trailing
  semicolon (`"drop(config_guard);"`) and the doc comment's occurrence is
  backtick-delimited without one, so `src.find` resolves to the real statement.
- **"The accumulator can spin or overflow on a pathological dt."** Falsified for
  negative (`max(0.0)`), zero (`ticks == 0` early return), NaN (`f32::max`
  returns the non-NaN operand → 0.0) and `+∞` (clamped to `max_acc`).
- **"`MAX_REGEN_SUBSTEPS = 8` is a typo for physics' 5."** Explicitly addressed
  in-code with a test that forbids re-asserting parity; the divergence is
  documented and the physics-side facts re-verified this sweep
  (`crates/physics/src/world.rs:13,15,39`). Correct no-guessing posture, not a
  finding.
- **"`damage_multiplier`'s `clamp(0.0, cap_pct)` panics for a negative cap."**
  Theoretically true (`f32::clamp` panics when `min > max`) but unreachable: the
  only two shipped caps are `85.0` and `f32::INFINITY`, both from
  `Affliction`'s `const` descriptors. Not worth a defensive branch.
- **"`ticks` are consumed before the `ActorValues` query, so a failed query
  loses regen time."** True in the abstract (`:191-192` drains the accumulator,
  `:202` can still bail), but the system is registered `add_exclusive_with_access`,
  so the query cannot fail for contention. Not a finding.
- **"`band_for` is order-dependent for duplicate `min_pool` values."** True —
  `Iterator::max_by` returns the *last* maximum — but a table with two bands at
  the same threshold is degenerate authoring with no defined correct answer.
  Subsumed by D4-01's suggested fix. Not filed separately.
- **"`AfflictionTable` ships empty / `affliction_tick_system` is unregistered."**
  Documented mechanism-ahead-of-data (`charal.md` §4.6). Recorded as coverage
  per the BRIEF; not a bug and not re-filed.
- **"Oblivion/Skyrim rulesets unwired, so `PoolRegenConfig` never lands."**
  **Existing: #3848 (OPEN)** — not re-filed.
- **"`ReputationStanding::sentiment` is unsourced."** True, and already
  disclosed in-code with the two judgement-call cells named (#2949, CLOSED).
  Recorded as constant-table row 26; not re-filed.


## Dimension 5 — Population Boundary (parse → ruleset → actor)

Repo `/mnt/data/src/gamebyro-redux`, branch `main`, HEAD `8151cded`. Read-only pass.
Capture documents read first (`charal.md`, `charal-fnv-fo3-ruleset.md`,
`charal-skyrim-ruleset.md`, `charal-fo4-ruleset.md`) before any code, per BRIEF phase 1.

**Result: 5 findings — 0 CRITICAL, 0 HIGH, 2 MEDIUM, 3 LOW.**
No premise in the dimension prompt turned out to be false; one (the
`npc_spawn.rs` layout) needed correcting in detail — see "Premises checked".

---

## Premises checked before acting on them

| Prompt premise | Verdict |
|---|---|
| `npc_spawn.rs` lost 144 lines and a `byroredux/src/npc_spawn/` directory now exists | TRUE, but the directory is **not** new in this delta — it holds `ai_package.rs`, `resumable.rs`, `tests.rs`. The 144 lines are `211a23cc`'s two deleted shims (−~120) plus `6b73c84d`'s `stamp_creature_attack` (+31). `build_character_ruleset` and all four `stamp_*` fns are still in `npc_spawn.rs`; the spawn tail that calls them is `npc_spawn/resumable.rs:1403-1428` (`spawn_placement_root`). |
| `build_character_ruleset` returns `None` for Oblivion / Skyrim / FO76 / Starfield | TRUE — `CharacterRulesProfile::{OBLIVION,SKYRIM,FALLOUT76,STARFIELD,NONE}` all carry `ruleset: RulesetBuilder::None`, and `build_ruleset` returns `None` on that arm (`crates/core/src/character/profile.rs:82-149,183-191`). |
| `resolve` is `index.actor_value_form_id(editor_id)` | TRUE (`byroredux/src/npc_spawn.rs:231`). |
| FNV/FO3 tag-skill per-level part is a deliberate deferral, its absence is correct | TRUE and still absent — see check 4. |
| `derive_skyrim_actor_values` resolves H/M/S independently through their own AVIFs | TRUE (`actor_value_derive.rs:280-294`). |
| `0dcb5cf0` is squarely in this dimension | TRUE. |
| Dimension 1 already owns the `stamp_creature_attack` TPLT bypass | TRUE — `byroredux/src/npc_spawn.rs:131-145` reads `npc.creature_stats` off the shell while its three siblings resolve. Cited as `CHAR-2026-09-11-D1-01`, **not re-filed**; finding D5-03 below is the *other* half of that fix's surface. |
| The previous sweep's stale "FO3↔FNV ruleset collapse" premise | Confirmed still absent — `character_rules_profile` splits on `hedr_version < 1.0` (`crates/plugin/src/esm/records/mod.rs:156-166`) and `fo3_and_fnv_profiles_build_their_own_distinct_leveling_model` pins it. |

---

## Checks performed

**1. `build_character_ruleset` → `None` must degrade, never fall back to a default ruleset — PASS.**
Sole production caller is `byroredux/src/cell_loader/references/mod.rs:338-344`, guarded
`if try_resource::<CharacterRuleset>().is_none() { if let Some(rs) = build_character_ruleset(..) { insert } }`
— a `None` inserts nothing. Every downstream read is `try_resource`, never `resource`/`resource_or_default`:
`byroredux/src/combat.rs:453`, `crates/scripting/src/condition.rs:500` and `:671`,
`crates/core/src/character/regen.rs:201`. `CharacterRuleset` has **no `Default` impl**
(`crates/core/src/character/ruleset.rs:23-24` derives only `Debug, Clone`), so a
`resource_or_default` fallback is not even expressible. The second half of the concern —
a TES actor receiving Fallout *formulas* — is closed at the profile, not the ruleset:
`CharacterRulesProfile::OBLIVION` carries `npc_stats: NpcStatModel::None`, and
`derive_npc_actor_values`'s `NpcStatModel::None => Vec::new()` arm returns nothing
(`actor_value_derive.rs:223`). `CharacterRulesProfile::default()` is `NONE`, also
`NpcStatModel::None`. Multi-plugin merge is first-non-`NONE`-wins with a `warn!` on
disagreement (`crates/plugin/src/esm/records/index.rs:974-1005`, #3384), so a low-HEDR
third-party plugin cannot re-point an FNV load order at the FO3 profile.

**2. Unresolved EditorID must SKIP its formula, never key one on `0` — PASS.**
`index.actor_value_form_id` (`crates/plugin/src/esm/records/index.rs:711-730`) filters
`avif.form_id != 0 && avif.form_id != u32::MAX` in *both* the exact and the `AV`-prefix
arms, so `Some(0)` is unreachable; `actor_value_lookup_normalizes_av_prefix_and_rejects_null_form_ids`
pins it. Every `push_derived` call site is an `if let (Some(out), Some(input))` resolve-or-skip:
`fallout.rs:48,52,63,70,78-82,130,139,146,159,165`, `tes.rs:107,111,115-123,128,139`,
`skyrim.rs:121,134`. Same shape in the population arms — `actor_value_derive.rs:249,254,286,338,381,393,397`.

**3. Ordering: bases written before dependent derived stats evaluated — PASS (structurally, not by luck).**
The invariant is not enforced by sequence at all, because CHARAL derives **on demand**
(charal.md §6): `ActorValues` is built as one `Vec<(u32,f32)>` and inserted atomically
(`npc_spawn.rs:103-107`), and `GetActorValue`'s derived fall-through
(`crates/scripting/src/condition.rs:481-524`) reads the actor's live `ActorValues` +
`CharacterLevel` at *read* time, after every stamp has run. The FNV `UnarmedDamage`
row (`ceil(0.5 + 0.05·Unarmed)`, `fallout.rs:71`) therefore cannot see a half-populated
component. Actual tail order (`npc_spawn/resumable.rs:1424-1427`):
`stamp_faction_ranks` → `stamp_actor_values` → `stamp_creature_attack` → `stamp_character_components`.
`CharacterLevel` is written *after* `ActorValues`, which would matter only if derivation
were eager — it is not. The one value that *is* computed eagerly, auto-calc Health,
takes its level from `effective_actor_level(stats)` inside the same function
(`actor_value_derive.rs:399`) rather than from the not-yet-written `CharacterLevel`, and
`stamp_character_components` writes `CharacterLevel` from `effective_actor_level(stats_npc)`
resolved through the identical chain with the identical shell level — the two agree by
construction, not by ordering.

**4. FNV/FO3 class auto-calc against `charal-fnv-fo3-ruleset.md` — PASS, and the tag-skill deferral still holds.**
See the constant table below. `derive_autocalc_actor_values` (`actor_value_derive.rs:365-404`)
emits the 7 SPECIAL from `class.base_attributes`, one `base_skill` per roster skill, and
Health — **no `+15` tag bonus and no per-level skill growth term anywhere**
(`grep -n "15\.0\|tag" actor_value_derive.rs` → no match in the derivation body).
The class SPECIAL source is `ClassRecord::base_attributes` (ATTR, not DATA), matching
`actor_value_population`. `fallout_roster_matches_attr_order` pins the positional
contract between `AttributeSet::FALLOUT` and `ClassRecord::base_attributes`.

**5. `setav` / `modav` write the BASE layer — PASS.**
`byroredux/src/commands/actor_value.rs:82-87`: `AvEdit::SetBase => avs.set_base(av, value)`,
`AvEdit::ModPermanent => avs.mod_permanent(av, value)`. `ActorValues::set_base`
(`crates/core/src/ecs/components/actor_values.rs:126-128`) writes `.base` and leaves
permanent/temporary/damage untouched; `mod_permanent` writes `.permanent_mod`. Neither
touches a derived output. No production system recomputes `base` after spawn
(`stamp_actor_values` runs once, at spawn, from `&mut World`), so the edit cannot
silently revert. Derived reads recompute *from* the edited base, which is the documented
intent ("raise Strength and watch Carry Weight recompute", module doc lines 3-6).
`setav` on a *derived* AVIF creates a stored entry that then wins over the formula
(`condition.rs:486-489`) — sticky, not reverting, so still not the failure mode in scope.

**6. Templated NPCs / the 12 `TPLT` flags / commit `0dcb5cf0` — PARTIAL.**
- *Both chains genuinely resolved*: **PASS**. `derive_npc_actor_values` now resolves
  `resolve_inherited_stats` and `resolve_inherited_traits` separately
  (`actor_value_derive.rs:194-215`) and hands each arm the record whose fields it reads —
  `stats` for the signed `ACBS` offsets, `traits` for `race_form_id`
  (`:218`, `:277`, `:282-284`). `stamp_character_components` does the same for
  `Background` (`npc_spawn.rs:182-183, 204-207`), so the pools' race and
  `Background.race_form_id` agree by construction; the test asserts exactly that
  (`skyrim_race_follows_use_traits_while_offsets_follow_use_stats:803-810`). Both
  directions are pinned (`:733` and `:816`).
- *Absent sentinel distinct from a legitimate zero*: **PASS at the two chain endpoints.**
  `baked_or_shell` (`:346-360`) treats `0` as absent on `calculated_health` /
  `calculated_action_points`, matching `NpcRecord`'s own documented sentinel
  (`actor/mod.rs:461-470`, "`0` = absent (no live NPC has 0 base Health)"); the empty
  `PRPS` vector gets the same treatment (`:317-321`). The Skyrim offsets are *not*
  given sentinel treatment, which is correct — `ACBS` is mandatory on `NPC_`, so a
  `0` offset is authored data, not absence. **The known hole is the intermediate
  template**: `baked_or_shell` consults only shell and final resolved record, so a
  mid-chain authored `DNAM` Health is still lost — already filed, **OPEN #4086**, not re-filed.
- *Cycles cannot loop forever*: **PASS.** `resolve_inherited_record`
  (`crates/plugin/src/equip.rs:447-472`) is depth-capped at `TPLT_MAX_DEPTH = 6` with a
  `log::debug!` and a return of the current record; a self-referential or cyclic chain
  terminates after 6 hops. The cap is documented as explicitly serving that purpose
  (`equip.rs:365-372`).
- *Flags respected rather than overwritten*: **FAIL** — only 3 of 12 parsed flags have a
  consumer, and two live population consumers read shell-only fields for categories whose
  flag is parsed. See **D5-01** (Use Traits, race/gender, inconsistent *within one spawn
  tail*) and **D5-02** (Use Factions / Use AI Packages).

**7. `derive_skyrim_actor_values` per-pool independence — PASS in code, test gap.**
`actor_value_derive.rs:280-294` loops the three `(name, starting, offset)` triples and
`continue`s on `index.actor_value_form_id(name).zip(starting) == None`, so a missing
`starting_magicka` or a load order missing the Magicka `AVIF` skips **only** Magicka.
`RaceRecord::{starting_health,starting_magicka,starting_stamina}` are `Option<f32>`
(`actor/mod.rs:564-570`), set only when the parsed `f32` `is_finite() && > 0.0`
(`actor/mod.rs:1588-1600`). The one all-or-nothing gate left is the *race* lookup itself
(`:277-279`), which is unavoidable — all three inputs come from that record. The older
"Health-alone, both-must-be-present" gate is gone. Test coverage of the independence
invariant is incomplete — see **D5-04**.

**Delta commits judged (all five):**
- `0dcb5cf0` — **sound.** Both defects real, both fixed at the right layer, four
  regression tests that falsify (verified by reading each assertion). Residual hole is
  the intermediate template, already OPEN as #4086.
- `6b73c84d` — **value correct, component correct, consumer unreachable.** See **D5-03**.
- `1ee804c2` — **sound, and no population-path regression.** The three parsers gained
  `remap` for `WNAM` / `MODL` / `RNAM` / `XNAM`; none of those feed CHARAL. Every
  FormID the population boundary *does* key on is remapped at parse:
  `race_form_id`, `class_form_id`, `template_form_id`, `factions[].faction_form_id`,
  `actor_value_props[].0` (PRPS AVIF), `perks[].0` (`actor/mod.rs:1018,1022,1091,1128,1379,1415`).
  `index.races` / `index.classes` / `index.actor_values` are global-keyed by
  `read_record_header`, so the lookups are exact on multi-master loads.
- `847426fa` — **no population-path guard was among the four overstating ones.** The four
  were the m48 UI smoke script, `sanitize_finite`'s field list, `translate_material`'s
  copy assertions, and `stealth.rs`'s coefficients (Dimension 2's surface). The CHARAL
  half is the `charal-fnv-fo3-ruleset.md` Sneak capture; nothing in
  `actor_value_derive.rs` / `npc_spawn.rs` / `profile.rs` was touched. Re-derived
  independently: none of D5's own guards is a tautology — each of the 26
  `actor_value_derive` tests asserts a concrete numeric or FormID outcome.
- `211a23cc` — **nothing live depended on the shims.** `grep -rn 'spawn_npc_entity\b|spawn_prebaked_npc_entity\b'`
  over `byroredux/ crates/ tools/` at HEAD returns exactly one hit,
  `npc_spawn/tests.rs:1388`, and it is prose ("Extracted out of the **old** synchronous
  `spawn_npc_entity` wrapper"), not a call. Test count unchanged, as claimed.

---

## Constant verification

| # | Formula/constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | Auto-calc skill base | `SKILL_BASE = 2.0` (`actor_value_derive.rs:132`) | `fAVDSkillBase = 2` | `charal-fnv-fo3-ruleset.md:46-47` | ✅ |
| 2 | Auto-calc primary mult | `SKILL_ATTR_MULT = 2.0` (`:133`) | `…PrimaryBonusMult = 2` | same line | ✅ (unauthored by either master — engine default, disclosed at `:126-131`) |
| 3 | Auto-calc luck mult | `SKILL_LUCK_MULT = 0.5` (`:134`) | `…LuckBonusMult = 0.5` | same line | ✅ (same disclosure) |
| 4 | Skill composition | `2 + 2·gov + ceil(0.5·Luck)` (`:137-139`) | `2 + 2·governing + ceil(Luck/2)`; worked END 5 / Luck 5 → 15 | `charal-fnv-fo3-ruleset.md:46-49` | ✅ (`base_skill_matches_documented_example`) |
| 5 | FO3 NPC Health curve | bias 90, END×20, level×10 (`profile.rs:94-98`) | `90 + END·20 + Level·10` | `charal-fnv-fo3-ruleset.md:93` | ✅ |
| 6 | FNV NPC Health curve | bias 95, END×20, level×5 (`profile.rs:109-113`) | `100 + END·20 + (Level−1)·5` ≡ `95 + 20·END + 5·L` | `charal-fnv-fo3-ruleset.md:93` | ✅ (algebraic identity, pinned at `profile.rs:218-219`) |
| 7 | Tag-skill bonus | **absent** | `+15` flat, FO3 and FNV | `charal-fnv-fo3-ruleset.md:65-67` | ✅ correctly deferred (per-level distribution uncited, `:83-87`) |
| 8 | Carry Weight | `affine(STR, 10.0, 150.0)` (`fallout.rs:49`) | `150 + 10·STR` | `charal-fnv-fo3-ruleset.md:95` | ✅ |
| 9 | Melee Damage | `affine(STR, 0.5, 0.0)` (`fallout.rs:53`) | `STR × 0.5` additive | `charal-fnv-fo3-ruleset.md:97` | ✅ |
| 10 | Critical Chance | `affine(Luck, 1.0, 0.0).capped(10.0)` (`fallout.rs:64-67`) | `Luck × 1%`, cap 10% | `charal-fnv-fo3-ruleset.md:96` | ✅ (0–100 convention, #2936) |
| 11 | Unarmed Damage | `affine(Unarmed, 0.05, 0.5).ceiled()` (`fallout.rs:71`) | `ceil((10 + Unarmed)/20)` | `charal-fnv-fo3-ruleset.md:98` | ✅ (identity) |
| 12 | FO3 Action Points | `affine(AGI, 2.0, 65.0).capped(85.0).player_only()` (`fallout.rs:160-163`) | `65 + 2·AGI` cap 85 | `charal-fnv-fo3-ruleset.md:94` | ✅ formula; scope UNSOURCED and disclosed in-code (#2937, known-open) |
| 13 | FNV Action Points | `affine(AGI, 3.0, 65.0).capped(95.0).player_only()` (`fallout.rs:169-172`) | `65 + 3·AGI` cap 95 | same | ✅ same caveat |
| 14 | FO4 Carry Weight | `affine(STR, 10.0, 200.0)` (`fallout.rs:147`) | `200 + 10·STR` | `charal-fo4-ruleset.md` | ✅ |
| 15 | Skyrim pool composition | `race.starting_X + i16 ACBS offset` (`actor_value_derive.rs:289`) | race start + signed actor offset | `docs/feature-matrix.md:276-280`; layout from OpenMW `esm4/loadrace.cpp:154-170` | ✅ |
| 16 | `RACE.DATA` pool offsets | H@36 / M@40 / S@44 (14+2+8+8+4 prefix, `actor/mod.rs:1570-1600`) | same | OpenMW `loadrace.cpp:154-170`, verified byte-for-byte vs `Skyrim.esm` (NordRace / ElderRace, 2026-08-12) | ✅ |
| 17 | `CREA.DATA` Damage | `i16 @ 8` of a 17-byte block (`actor/mod.rs:241-258`) | same | xEdit `wbDefinitionsFNV.pas` `wbStruct(DATA,…)`, verified against `VCrTier3GiantRadscorpionMedPers` `00167EA7` (damage 60) | ✅ layout; the *reader* is dead — D5-03 |
| 18 | `TPLT_MAX_DEPTH` | `6` (`equip.rs:372`) | n/a — engine choice, justified against measured LVLI depth histogram (max 7) | `equip.rs:334-346, 365-372` | ✅ sourced as a choice, not a guess |

No UNSOURCED row.

---

## Findings

#### CHAR-2026-09-11-D5-01: the spawn tail resolves the `Use Traits` chain for `Background` and the Skyrim pools, and ignores it for every mesh that depends on race
- **Severity**: MEDIUM
- **Dimension**: 5 — Population boundary
- **Game**: all (measurable on Skyrim / FNV / FO3)
- **Location**: `byroredux/src/npc_spawn.rs:836`, `:853`, `:969`; `byroredux/src/cell_loader/references/mod.rs:636`; `byroredux/src/npc_spawn/resumable.rs:434`, `:616`, `:1168`
- **Status**: NEW
- **Source**: `crates/plugin/src/equip.rs:429-438` — `resolve_inherited_traits` is defined as "the NPC record that should supply **race** (and other 'traits' fields)", gated on `TEMPLATE_FLAG_USE_TRAITS`. `crates/plugin/src/esm/records/actor_value_derive.rs:196-207` (`0dcb5cf0` / #3480) establishes that reading race off anything but the traits chain is a defect.
- **Description**: `0dcb5cf0` fixed *one* of the two race readers in the spawn tail. `stamp_character_components` writes `Background { race_form_id: traits_npc.race_form_id }` (`npc_spawn.rs:204-207`) and `derive_skyrim_actor_values` keys the race lookup on the traits chain — but four sibling sites in the same spawn still read the shell's raw `npc.race_form_id`: the `RACE.WNAM` default-skin lookup (`:836`), both `resolve_armor_meshes` race-match arguments (`:853`, `:969`), and the `races.get(&npc.race_form_id)` that supplies the head/body `RaceRecord` handed to `NpcSpawnJob::runtime` (`references/mod.rs:636`). `Gender::from_acbs_flags(npc.acbs_flags)` at `resumable.rs:434/616/1168` is the same shape for the ACBS Female bit — flagged rather than asserted, because no in-repo source states which template flag governs gender.
- **Evidence**:
  ```rust
  // npc_spawn.rs:182-206 — traits chain, for Background
  let traits_npc = resolve_inherited_traits(npc, shell_level, index);
  world.insert(placement_root, Background { race_form_id: traits_npc.race_form_id, .. });

  // npc_spawn.rs:836-853 — shell, for the mesh that renders
  if let Some(race) = index.races.get(&npc.race_form_id) {
      if let Some(skin_fid) = race.default_skin { …
          byroredux_plugin::equip::resolve_armor_meshes(item, gender, npc.race_form_id, index, game)
  ```
  One entity, two different answers to "what race is this actor".
- **Impact**: an `NPC_` with `Use Traits` set whose own `RNAM` differs from its template's gets the wrong race's default-skin `ARMO` and fails the `ARMA.RNAM` race match. `resolve_armor_meshes` returns `Vec::new()` on a race miss — the exact indistinguishable-from-unshipped-mesh failure `1ee804c2`/#3714 describes — so the symptom is an invisible or wrong-race body, plus a head/body `RaceRecord` from a race the actor is not. Measured divergence for this comparison on FNV/FO3 is small and already in-repo: 2/744 (FNV) and 19/337 (FO3) of `Use Traits` actors with a resolvable direct template have an own race that disagrees (`equip.rs:436-438`). The Skyrim count is **not measured** — #3480's 1,180/5,118 figure is a *different* comparison (traits-race vs stats-race), so it is an upper bound, not this finding's number. Severity is MEDIUM rather than HIGH for that reason.
- **Related**: #3480 (CLOSED, `0dcb5cf0`) fixed the actor-value half; #3714 (CLOSED) is the identical "race miss ⇒ empty mesh list" failure mode; `CHAR-2026-09-11-D1-01` is the same class on the stats chain.
- **Suggested Fix**: resolve `traits` once in `spawn_placement_root`/`NpcSpawnJob` construction and thread `traits.race_form_id` into `build_npc_equip_state`, `resolve_armor_meshes` and the `races.get` at `references/mod.rs:636`, exactly as `stamp_character_components` already does. Separately, source which flag governs the ACBS Female bit before touching gender.

#### CHAR-2026-09-11-D5-02: `Use Factions` and `Use AI Packages` are parsed, have live consumers, and are never resolved — the shell's own empty list wins
- **Severity**: MEDIUM
- **Dimension**: 5 — Population boundary
- **Game**: all (FNV/FO3/Skyrim/FO4 all parse `template_flags`)
- **Location**: `byroredux/src/npc_spawn.rs:76-84` (`stamp_faction_ranks`); `byroredux/src/npc_spawn/ai_package.rs:525`, `:537`, `:553`
- **Status**: NEW
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:443-447` — `0x0004` Factions and `0x0020` AI Packages are enumerated as parsed template-inheritance bits with "no consumer yet"; `crates/plugin/src/equip.rs:471-472` — `resolve_inherited_record`'s `flag` parameter is already category-agnostic ("`flag` only gates *whether* to keep following the chain, not how").
- **Description**: the spawn tail honours 3 of the 12 parsed flags (`Use Traits`, `Use Stats`, `Use Inventory`). `stamp_faction_ranks` reads `npc.factions` straight off the shell and returns early when it is empty; `apply_ai_package_behavior` reads `npc.ai_packages` the same way. A `Lvl*` shell that sets `Use Factions` / `Use AI Packages` carries an empty `SNAM` / `PKID` list *by design* — that is what the flag means — so both stamps silently no-op for exactly the actors the flag exists to serve. The `actor/mod.rs` doc's "no consumer yet" is the premise that made this look benign; it describes the *flag* having no consumer, but the underlying **fields** each have a live one.
- **Evidence**:
  ```rust
  // npc_spawn.rs:76-84 — no resolve_inherited_* anywhere in this fn
  fn stamp_faction_ranks(world: &mut World, placement_root: EntityId, npc: &NpcRecord) {
      if npc.factions.is_empty() { return; }
      world.insert(placement_root, FactionRanks::from_pairs(..));
  }
  ```
  Live consumers of what is thereby missing: `GetFactionRank` CTDA
  (`crates/scripting/src/condition.rs:611-620`, returns the `-1.0` not-in-faction
  sentinel when the component is absent), quest-alias fill by faction
  (`crates/scripting/src/scene/quest_alias.rs:358`), the SDK capture bridge
  (`byroredux/src/extensions/capture.rs:336`), and M42.2/M42.9 package selection, whose
  CTDA gating explicitly lists `GetFactionRank` among the gates it evaluates.
- **Impact**: faction-gated dialogue, quest aliases and AI-package selection evaluate a
  structural "not in faction" for templated actors — the same failure shape #3158
  described for `HasPerk` on Skyrim ("every `HasPerk` CTDA evaluated a structural 0.0").
  For AI packages the actor simply gets no ambient behaviour. Population size is
  **unmeasured** — a census needs a full-ESM parse, which this pass is forbidden to run —
  so MEDIUM, not HIGH.
- **Related**: #2956 / #3381 / #3382 / #3480 are the same defect resolved for three other flags; `CHAR-2026-09-11-D1-01` and D5-01 are siblings.
- **Suggested Fix**: add `resolve_inherited_factions` / `resolve_inherited_packages` wrappers over the existing `resolve_inherited_record` (it already takes the flag as a parameter, so this is two three-line fns) and call them from `stamp_faction_ranks` and `apply_ai_package_behavior`. Measure first with a census example so the fix ships with a number.

#### CHAR-2026-09-11-D5-03: `6b73c84d`'s `CreatureAttack` reaches the right component on the right entity and has no reachable reader — `attack_damage`'s only production caller is the player
- **Severity**: LOW
- **Dimension**: 5 — Population boundary
- **Game**: FO3 / FNV
- **Location**: `byroredux/src/combat.rs:129-132`, `:250`, `:374-405`; `byroredux/src/npc_spawn.rs:114-145`; `crates/core/src/ecs/components/creature_attack.rs:1-44`; `docs/feature-matrix.md:271-274`
- **Status**: NEW
- **Source**: n/a (non-numeric). The *value* is sourced and correct — `CREA.DATA` `Damage` is `i16 @ 8` of the 17-byte block per xEdit `wbDefinitionsFNV.pas`, verified byte-for-byte against `VCrTier3GiantRadscorpionMedPers` `00167EA7` (`actor/mod.rs:241-258`); `stamp_creature_attack` reads that field, drops `<= 0`, and writes `CreatureAttack { damage: f32::from(stats.damage) }` on the placement root — all correct.
- **Description**: the fix's *reader* is unreachable. `attack_damage` has exactly one production call site (`combat.rs:250`), inside `combat_input_system`, and its `aggressor` is `world.try_resource::<PlayerEntity>()` (`combat.rs:129-132`). There is exactly one production `HitEvent` producer in the workspace (`combat.rs:251-257`; the file says so itself at `:277-280`, "this slice has exactly one HitEvent producer and it is always player-initiated"), and `grep -rn aggressor` over `byroredux/ crates/` outside `combat.rs` returns only the SDK dispatch *consumer*. `CreatureAttack` is stamped only in `spawn_placement_root`, i.e. never on the player. So no code path can reach `world.get::<CreatureAttack>(aggressor)` with a `Some`. ROADMAP corroborates: NPC combat is named as a subsystem that "doesn't exist in the engine yet" (`ROADMAP.md:1215`).
- **Evidence**:
  ```rust
  // combat.rs:129-132  — the only aggressor there is
  let aggressor = world.try_resource::<PlayerEntity>().and_then(|player| player.0);
  // combat.rs:250
  let damage = attack_damage(world, aggressor);
  // combat.rs:402 — only reachable with aggressor == the player, who never carries one
  if let Some(attack) = world.get::<CreatureAttack>(aggressor) { … }
  ```
  The four new tests all construct the aggressor by hand (`combat.rs:743-746`), so the suite is green while the production path is dead.
- **Impact**: no gameplay change from the fix, and — the part that matters for a future sweep — **no gameplay bug before it either**. Three places now assert a symptom that cannot occur: the commit message and `npc_spawn.rs:118-127` ("a Deathclaw hitting for 8 instead of 125"), `creature_attack.rs:17-22`, and `docs/feature-matrix.md:273-274` ("before that fix every creature in both games attacked for 8"). Creatures do not attack at all. A stale sibling: `CreatureStats::damage`'s own docstring (`actor/mod.rs:279-282`) still says the field is "parsed and left for a future combat consumer", which `6b73c84d` was supposed to close.
- **Related**: #3762 (CLOSED), #3390; `CHAR-2026-09-11-D1-01` (the same fix's TPLT bypass).
- **Suggested Fix**: keep the component and the stamp — both are correct and cheap — but re-word the three claims to say the value is now *available* to a combat consumer rather than that it fixed a live shortfall, and re-point `CreatureStats::damage`'s docstring at `CreatureAttack`. Record the real gap (no NPC/creature aggressor path) where a reader will meet it.

#### CHAR-2026-09-11-D5-04: no test falsifies `derive_skyrim_actor_values`'s per-pool independence — the `Option<f32>` contract is asserted in prose only
- **Severity**: LOW
- **Dimension**: 5 — Population boundary
- **Game**: Skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:1061-1087` (`skyrim_health_skips_when_race_health_is_missing_or_invalid`), `:1030-1059` (`skyrim_pools_are_race_starts_plus_signed_npc_offsets`)
- **Status**: NEW
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:564-570` — the three fields are `Option<f32>` precisely so partial race data degrades one pool at a time; `actor_value_derive.rs:261-262` states the contract ("Each resolves independently through its authored `AVIF`").
- **Description**: the behaviour is correct (check 7), but nothing pins it. The all-pools test uses a race authoring all three; the skip test only ever removes **Health**, and both of its cases assert `is_empty()` — i.e. they exercise the *race-level* all-or-nothing gate, not per-pool independence. Neither direction the contract names is covered: (a) a race with `starting_magicka: None` but Health and Stamina present must still emit two pools; (b) a load order missing only the Magicka `AVIF` must still emit Health and Stamina. A regression to an all-or-nothing gate — the shape this code *used* to have — would keep all 26 tests green.
- **Evidence**: `grep -n "starting_magicka: None" actor_value_derive.rs` matches only inside `skyrim_race_follows_use_traits_while_offsets_follow_use_stats:757-766`, where that race belongs to the template and is deliberately **not** the one the assertions resolve to — so its `None` is never on the output path.
- **Impact**: test-coverage gap only; no shipped wrong value.
- **Related**: #3480 / `0dcb5cf0`; the parallel FO4 sentinel invariant *is* pinned in both directions (`fo4_absent_baked_stats_fall_back_to_the_shell`, `fo4_authored_template_baked_stats_still_win_over_the_shell`).
- **Suggested Fix**: two short tests beside the existing ones — one race with `starting_magicka: None` asserting exactly `[Health, Stamina]`, one index omitting the Magicka `AVIF` asserting the same — each verified to fail if the loop's `continue` is turned into a `return Vec::new()`.

#### CHAR-2026-09-11-D5-05: three CHARAL rows in `NOT_SAVED_BY_DESIGN` claim a boot installation that no site performs, under a header asserting every entry was verified against real insertion sites
- **Severity**: LOW
- **Dimension**: 5 — Population boundary
- **Game**: all
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:180-205`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §4.7 — "`PoolRegenConfig` is inserted only by unit tests, never by a live per-game path (`oblivion_pool_regen_config` builds one, nothing calls it at load)".
- **Description**: the table's own header (`:183-185`) says "Verified 2026-08-05 against real (non-`#[cfg(test)]`) insertion sites for every entry". Three rows contradict that:
  - `("PoolRegenConfig", "immutable game-profile regeneration tuning installed at boot")` — **no production insertion site exists at all**. `grep -rn PoolRegenConfig byroredux/src crates/` returns only the type definition, two scheduler `reads_resource` declarations, two comments, `oblivion_pool_regen_config` (uncalled), and this row.
  - `("CharacterRuleset", "… selected at boot from the source game")` and `("MeleeDamageConfig", "… selected from the game profile at boot")` — both are actually built on the first cell-reference load (`byroredux/src/cell_loader/references/mod.rs:338-359`), not at boot, and not at all for Oblivion/Skyrim/FO76/Starfield.
- **Evidence**: the same table already has the correct idiom for a latent resource and uses it twice — `("AfflictionStatus", "forward-latent: affliction_tick_system has no production scheduler registration…")` and `("FactionReputation", "forward-latent: no production insertion or mutation site exists yet")`. `PoolRegenConfig` is in exactly that state and does not use it.
- **Impact**: doc rot in a guard whose stated purpose is to make "not saved" reasons auditable. A future reader checking whether regen state survives a reload is told a boot install happens; it does not. No runtime behaviour is wrong (the resource's absence already makes `pool_regen_tick_system` a no-op).
- **Related**: #3848 (the unwired Oblivion/Skyrim rulesets, OPEN) is the same "built, unwired" family; charal.md §4.6/§4.7 already record the mechanism-ahead-of-wiring state correctly.
- **Suggested Fix**: re-word `PoolRegenConfig` to the `forward-latent:` idiom the table already uses, and change the two "at boot" phrases to "at first cell load" so they match `references/mod.rs`.

---

## Candidates verified and dropped

1. **`build_character_ruleset` returning `None` could let a default ruleset leak onto a TES actor.** Dropped — `CharacterRuleset` has no `Default` impl, every read is `try_resource`, and the profile's `NpcStatModel::None` arm short-circuits the derivation before the ruleset is even consulted.
2. **`build_character_ruleset` is re-attempted on every cell load for games that return `None`.** Verified true (`references/mod.rs:338-344` re-tests `is_none()` each time) and dropped — the call is a handful of hash lookups on a cold path, and short-circuiting it would need a sentinel resource to distinguish "not built" from "cannot be built". Not a defect.
3. **`stamp_character_components` reads `npc.perks` off the shell while level and race are resolved.** Suspected a fourth `TPLT` divergence. Dropped as **unsourceable**: no capture document and no in-repo source states which template flag governs `PRKR`, and `feedback_no_guessing` forbids picking one. Recorded here so the next sweep does not re-chase it without a source, and so that if a source turns up the site is already located (`npc_spawn.rs:209-222`).
4. **`derive_stored_actor_values` lets the baked `DNAM` Health override a `PRPS`-authored Health** (`from_pairs` is last-write-wins, `actor_values.rs:103-109`, and the baked pair is pushed after the `PRPS` memcpy). Dropped — no capture document states the precedence between an authored `PRPS` Health entry and the engine-baked `DNAM` value, and the commit that touched this area measured the `PRPS` half's impact on vanilla FO4 as zero records. Unsourced, therefore not reportable.
5. **Skyrim pools skip a pool whose composed value is exactly `0.0`** (`actor_value_derive.rs:290`), which could suppress a legitimate zero Stamina. Dropped — consistent with the "unpopulated, not zero" contract the other three arms use, and `starting_*` is already `None` for an authored `0.0` at parse time (`actor/mod.rs:1588-1600`), so the guard is redundant rather than lossy.
6. **`ActorVitals` is inserted only when the Health key is present** (`npc_spawn.rs:100-112`), so an actor with no Health `AVIF` spawns unkillable. Dropped as the *intended* contract and already the subject of the fixed #3481 — the residual case is "this load order authors no Health `AVIF`", which is a missing-master condition, not a population defect.
7. **A self-referential `TPLT` (`template_form_id == form_id`) could spin `resolve_inherited_record`.** Dropped — the depth cap returns the current record after 6 hops with a `debug!`, verified by reading `equip.rs:459-472`.
8. **`actor_value_form_id` is a linear scan over every parsed `AVIF`, called ~22× per auto-calc NPC.** Dropped from this dimension — correctness is fine (check 2); this is a performance question for `/audit-performance`, and the maps are small (hundreds of `AVIF` records).
9. **`character_rules` last-write-wins across a multi-plugin load order could let a low-HEDR `.esp` re-point an FNV load order at the FO3 profile.** Dropped — already fixed as first-non-`NONE`-wins with a disagreement `warn!` (#3384, `index.rs:974-1005`).
10. **`stamp_creature_attack` bypasses the `Use Stats` TPLT resolution its siblings perform.** Real, but **owned by Dimension 1** as `CHAR-2026-09-11-D1-01` — not re-filed. Note for whoever fixes it: `derive_creature_actor_values` is already handed the *resolved* record (`actor_value_derive.rs:222`), so the two functions currently disagree about which record a creature's stats come from, on the same entity, in the same tail.
11. **`baked_or_shell` only consults the two chain endpoints, losing an intermediate template's `DNAM`.** Real, already filed **OPEN #4086**.
12. **Oblivion / Skyrim rulesets unwired.** Already filed **OPEN #3848**; re-verified still true at HEAD (`profile.rs:82-88`, `:119-125`).


## Dimension 6 — Coverage, Documentation & Doctrine Drift
Sweep 2026-09-11 · HEAD `8151cded` · READ-ONLY pass, no cargo invoked (test counts taken from the BRIEF: 115 passed / 0 failed).

**Result: 2 findings — 0 CRITICAL · 0 HIGH · 0 MEDIUM · 2 LOW.** Both are doc rot.
All four 2026-08-30 Dimension-6 findings are **FIXED**; none carries forward.

---

### Coverage matrix

Published in full at the top of this report ("Coverage matrix"). Not repeated here.

### Prior-finding disposition — the four 2026-08-30 Dimension-6 findings

| ID | Verdict | Evidence |
|---|---|---|
| **CHAR-2026-08-30-D6-01** — `charal.md` declares Oblivion "CHARAL-complete end-to-end" | **FIXED** | `grep -rn 'CHARAL-complete' docs/` now returns **only** `.claude/issues/3768/ISSUE.md` (the archived issue). `charal.md:343-349` reads *"**The Oblivion ruleset builder is complete — it is unwired.**"* and names `RulesetBuilder::None`, `profile.rs:82-87` and the missing `AVIF` group. The echo at `charal-oblivion-ruleset.md:7-13` was corrected in the same commit (`8175cb70`, visible in `git diff 64f64480..HEAD -- docs/engine/charal-oblivion-ruleset.md`). Both sites fixed. |
| **CHAR-2026-08-30-D6-02** — `charal.md` §8 item 6 lists `fXPPerSkillRank` among the overlaid GMSTs | **FIXED** | `charal.md:594-602` now reads *"overlays the authored `fXPLevelUpBase` and `fXPLevelUpMult` values … only the level curve is GMST-authored; the skill-rank coefficient (`xp_per_skill_rank`) is engine-owned, not a `fXPPerSkillRank` GMST (#3221 … withdrawn 2026-08-24)"*. Matches `leveling.rs:92-109`, which requests exactly those two settings and passes `xp_per_skill_rank` through untouched. Fixed by `08a70f42`. |
| **CHAR-2026-08-30-D6-03** — the skill's own Dimension-5 checklist asserts a removed FO3↔FNV ruleset collapse | **FIXED** | `grep -n 'Fallout3NV' .claude/commands/audit-character/SKILL.md` → **no match**. The Dimension-5 checklist's last bullet is now the `derive_skyrim_actor_values` per-pool-independence check (SKILL.md:369-379); no bullet anywhere asks an auditor to verify the collapse premise. |
| **CHAR-2026-08-30-D6-04** — `feature-matrix.md` has no row or prose for the `CREA` stat model | **FIXED** | `docs/feature-matrix.md:253` now carries a *"Creature (`CREA`) actor-value population at spawn"* row (Oblivion ✗ `CREA.DATA` layout unsourced / FO3 ✓ / FNV ✓ / rest n/a), and `:267-275` adds the prose naming `NpcStatModel::CreatureData`, the 1,578 FNV + 533 FO3 counts, the deliberately-omitted aggregate skills, and the `CreatureAttack` routing for `DATA.Damage` (#3762). |

**Zero carry-forwards.** No 2026-08-30 D6 finding survives.

---

### SKILL.md self-audit

Every path, symbol, number, line citation and commit hash in
`.claude/commands/audit-character/SKILL.md` was re-checked against HEAD.

**Paths — 28/28 exist.** Every file named in Scope, the CHARAL-adjacent-siblings paragraph,
the Population-boundary paragraph and all six dimension entry-point lists resolves, including
the ones most likely to have moved in the 2026-09-09 split: `byroredux/src/boot/world.rs`
(named in Dim 4 — the `PoolRegenConfig` comment really is at `:40-48`),
`byroredux/src/cell_loader/references/mod.rs` (`build_character_ruleset`'s sole caller, `:342`)
and `crates/plugin/tests/parse_real_esm.rs`. **No stale path.**

**Symbols — 24/24 resolve.** `skyrim_gmst_overlay_reads_only_authored_curve_settings`
(`leveling.rs:332`), `formula_is_thirty_six_bytes_and_copy` (`derived.rs:354`),
`pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier`, `ROSTER_CASES`,
`assert_rosters_resolve`, `effective_actor_level`, `derive_skyrim_actor_values`,
`build_character_ruleset`, `melee_damage_config`, `modified_skill`,
`oblivion_weapon_damage_multiplier`, `oblivion_hand_to_hand_damage`, `detection_score`,
`classify`, `push_derived`, `with_gmst`, `pool_pick_gain`, `SKYRIM_POOL_BASE`,
`NpcHealthCurve`, `NpcStatModel`, `RulesetBuilder`, `with_attributes`/`with_skills`/
`with_derived`. The retired `effective_npc_level` is correctly described as deleted — its only
two remaining occurrences are historical prose in `actor/mod.rs:90` and
`actor_value_derive.rs:558`, no definition. **No stale symbol.**

**Numbers — both pins correct.**
- `DerivedStatFormula` = **36 B**, `Copy`: `derived.rs:359` asserts `size_of == 36`; SKILL.md:190-192
  says 36 B and explains the 32→36 growth. The prior 32 B pin (#3485) is gone from SKILL.md.
  The sweep for further 32 B copies found exactly one survivor,
  `charal-skyrim-ruleset.md:699` — **already filed this sweep as CHAR-2026-09-11-D2-03**, not re-filed.
  No third copy exists anywhere in `docs/engine/charal*.md`, `docs/feature-matrix.md` or SKILL.md.
- Module count = **14**: `affliction attribute components derived fallout leveling profile regen
  reputation resistance ruleset skill skyrim tes`, all fourteen indexed as ``[`name`]`` in the
  `mod.rs` docstring, mechanically pinned by `mod_docstring_indexes_every_sub_module`
  (`mod.rs`, `assert_eq!(declared.len(), 14)`) plus
  `mod_docstring_points_at_the_charal_adjacent_siblings`.

**Line citation — correct.** `crates/core/src/character/derived.rs:353-359` is exactly
`#[test]` (353) … `assert_eq!(…, 36)` (359).

**Commit hashes — 4/4 resolve.** `9e44a0dd` (Fix #3171/#3172), `b434e4c0`
(centralize actor value profiles), `1d0c5d4b`, `a1327227` (Fix #3390 …).

**Known-open claims — all three still true.** Tag-skill per-level formula still absent (not
approximated); FO3↔FNV player Health/AP still `.player_only()` with no player stat entity;
no VATS runtime. Dim-3's "do NOT re-flag the `fXPPerSkillRank` split" instruction matches the
code (`with_gmst` requests exactly the two authored curve settings, no third crept back).
Dim-5's "`build_character_ruleset` returns `None` for Oblivion / Skyrim / FO76 / Starfield" is
exact against `profile.rs`. Dim-2's "these are consumer-less today" for `crates/core/src/combat.rs`
and `crates/core/src/stealth.rs` is **still true** — verified by grepping every caller of
`modified_skill` / `oblivion_weapon_damage_multiplier` / `oblivion_hand_to_hand_damage` /
`detection_score`: zero hits outside each module's own tests.

**Two sub-finding-threshold notes (recorded, not filed):**
1. SKILL.md:60-62 says of the two adjacent siblings *"each module's own docstring explains the
   boundary and cites `charal.md` §7"*. True for `stealth.rs:16`; `combat.rs:1-9` explains the
   boundary but cites `charal-oblivion-ruleset.md`, not `charal.md` §7. A half-clause
   overstatement that misdirects no audit effort.
2. The Dimension-6 **entry-points** line does not list `docs/feature-matrix.md`, although
   checklist item 4 requires cross-checking it. Cosmetic; the checklist carries the instruction.

**Verdict: SKILL.md is clean.** Zero stale paths, zero stale symbols, zero stale numbers, zero
stale line citations, zero stale known-open claims. The one claim I could not verify from source
alone is Dim-1's measured *"30 `NPC_` records are non-multiplier with `level <= 0`"* on
`FalloutNV.esm` — verifying it needs an ESM parse the BRIEF forbids; marked **UNVERIFIED**, not
disputed.

---

### Checks performed

| # | Check | Verdict | Evidence |
|---|---|---|---|
| 1 | Coverage matrix built for all seven families × six columns | **PASS** (published above) | `profile.rs:72-186`, `leveling.rs:112-159`, `push_derived` census, `boot/schedule/update.rs:304-311`, `boot/world.rs:40-48` |
| 2 | Every capture document has an implementation or an explicit "not implemented" note | **PASS** | All 6 captures exist. Every `##` section in all six carries a status marker (LOCKED / PENDING / BUILT / "out of CHARAL scope"). FO76's header (`:6-9`) and Starfield's §Skills (`:18`) + `charal.md` §8 item 8 + §9 explicitly record the absent implementation; `feature-matrix.md:248-256` mirrors it. **No silent scope loss.** |
| 3 | `mod.rs` docstring matches the live module list (14) | **PASS** | 14 `pub mod` declarations, 14 ``[`name`]`` index entries; pinned by `mod_docstring_indexes_every_sub_module` (hard `assert_eq!(…, 14)`) and by `mod_docstring_points_at_the_charal_adjacent_siblings` |
| 4 | `docs/feature-matrix.md` character/progression rows cross-checked against code | **PASS on content, 1 stale path (D6-02)** | `:248-292`. Ruleset-wired row matches `RulesetBuilder`; NPC-population row matches `NpcStatModel`; `CREA` row present (D6-04 fixed); Skyrim row now "Health+Magicka+Stamina" (#3484 fixed); regen/affliction uniformly `✗ inert` and the prose states both mechanisms correctly. Only defect: the `boot.rs` path at `:291`. |
| 5 | Naming / vocabulary drift vs `translate` / `canonical` / `resolve` | **PASS** | Full fn-name census of `crates/core/src/character/` (58 public fns), `npc_spawn.rs` (24) and `actor_value_derive.rs` (8): the verb set is `build_*_ruleset` / `resolve` / `derive_*` / `stamp_*` / `eval`. `stamp_*` is uniform across all four population writers (`stamp_actor_values`, `stamp_character_components`, `stamp_faction_ranks`, `stamp_creature_attack`), and `CreatureAttack` / `MeleeDamageConfig` mirror `PoolRegenConfig`'s established "pre-resolve once, consume by id" noun shape. No competing verb invented. |
| 6 | Prior D6-01 disposition | **FIXED** | see table above |
| 7 | Prior D6-02 disposition | **FIXED** | see table above |
| 8 | Prior D6-03 disposition | **FIXED** | see table above |
| 9 | Prior D6-04 disposition | **FIXED** | see table above |
| 10 | SKILL.md self-audit (paths / symbols / numbers / line cites / hashes / known-opens) | **PASS** | see SKILL.md self-audit above |
| 11 | Further copies of the stale 32 B pin beyond D2-03's | **PASS — none** | `grep -rn '32 B\|32 bytes\|32-byte\|thirty-two'` over `docs/engine/charal*.md`, `docs/feature-matrix.md`, SKILL.md → exactly one hit, `charal-skyrim-ruleset.md:699`, already filed as D2-03 |
| 12 | Further copies of the stale-symbol class beyond D3-02's `AttributeSet::OBLIVION` | **FAIL → finding D6-01** | Mechanical `Type::CONST` sweep of all CHARAL docs against the whole Rust tree found `SkillSet::FALLOUT_FO3_FNV` at `charal.md:146` and `:307` |
| 13 | Doctrine: no per-game branch introduced by any doc-described mechanism | **PASS** (Dim 1 owns the code check) | `build_ruleset` is the single `match self.ruleset` on a data row; every doc describes the seam without asserting a branch |

---

### Constant verification

| # | Formula / constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | `DerivedStatFormula` size | 36 B (`derived.rs:359`) | 36 B (SKILL.md:190; `derived.rs:23,144` module docs) | SKILL.md Dim 2 / `derived.rs` | **PASS** |
| 2 | CHARAL sub-module count | 14 `pub mod` | 14 (SKILL.md Dim 6; `mod.rs` test) | SKILL.md Dim 6 | **PASS** |
| 3 | `SkillSet::OBLIVION` roster size | 21 `SkillDef` | "21 governed" (`charal.md:306`) | `charal.md:306` | **PASS** |
| 4 | `SkillSet::SKYRIM` roster size | 18 `SkillDef` | "18 ungoverned" (`charal.md:307`) | `charal.md:307` | **PASS** |
| 5 | FO3 skill roster size | `SkillSet::FALLOUT3` = 13 | "15 = FO3 ∪ FNV" under a symbol that no longer exists | `charal.md:307` | **FAIL → D6-01** |
| 6 | FNV skill roster size | `SkillSet::FALLOUT_NV` = 13 | same bad row | `charal.md:307` | **FAIL → D6-01** |
| 7 | `AttributeSet::FALLOUT` / `TES_CLASSIC` sizes | 7 / 8 | 7 SPECIAL / 8 attributes (`charal.md:74-75`) | `charal.md:74-75` | **PASS** |
| 8 | Derived rows per family | FO3 8 · FNV 8 · FO4 3 · Oblivion 8 · Skyrim 2 | 6–10 flat-`Vec` rationale (`charal.md` §5 / SKILL.md Dim 1) | SKILL.md Dim 1 | **PASS** |
| 9 | Skyrim GMST overlay set | exactly `["fXPLevelUpBase","fXPLevelUpMult"]` (`leveling.rs:101-102`) | same two, and explicitly **not** `fXPPerSkillRank` | `charal.md:594-602`, `charal-skyrim-ruleset.md:711-720` | **PASS** (D6-02 fix confirmed) |
| 10 | `LevelingModel::SKYRIM` curve / pool pick | `25·L+75`, `pool_pick_gain 10.0`, `xp_per_skill_rank 1.0` | `25·L+75`, +10 pool, 1 XP/rank | `charal-skyrim-ruleset.md` § XP/level curve; `mod.rs` docstring | **PASS** |

No row in this dimension is UNSOURCED.

---

### Findings

#### CHAR-2026-09-11-D6-01: `charal.md` still documents the deleted `SkillSet::FALLOUT_FO3_FNV` and its merged 15-skill roster, at two sites
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**: `docs/engine/charal.md:146` and `docs/engine/charal.md:307`
- **Status**: NEW
- **Source**: `crates/core/src/character/skill.rs:166-205` — the two shipped rosters, `SkillSet::FALLOUT3` (13 skills, `AVBigGuns` present) and `SkillSet::FALLOUT_NV` (13 skills, `AVBigGuns` deliberately absent, `SmallGuns`/`Throwing` keyed by EditorID not display name). Split from the merged set by #3169 (`.claude/issues/3169/ISSUE.md:39`: *"split `SkillSet::FALLOUT_FO3_FNV` into `FALLOUT3` / `FALLOUT_NV` and corrected FNV's `Guns` → …"*).
- **Description**: `charal.md` is the layer spec and the first document `/audit-character` Phase 1 loads. Two of its rows still name a constant that no longer exists and state a roster size that was never right after the split. `:305-308` reads *"Shipped rosters: `SkillSet::OBLIVION` (21 governed), `SkillSet::SKYRIM` (18 ungoverned), `SkillSet::FALLOUT_FO3_FNV` (15 = FO3 ∪ FNV, SPECIAL-governed) and `SkillSet::NONE` (FO4/FO76)"* — three of the four entries are correct and verified (21 / 18 / `NONE`), which is what makes the fourth read as authoritative. `:146` repeats the symbol in the AUTHORED-vs-ENGINE-SUPPLIED table's "Skill → governing attribute map" row (*"canonical `SkillSet` rosters shipped (OBLIVION / SKYRIM / FALLOUT_FO3_FNV)"*). The union framing is also substantively wrong now: the whole point of #3169 was that FO3 and FNV do **not** share a roster — FNV drops `BigGuns` and adds `Throwing`, so `|FO3 ∪ FNV| = 14`, not 15, and neither shipped roster is the union.
- **Evidence**:
  ```
  $ grep -rn FALLOUT_FO3_FNV --include='*.rs' .      # no match in any Rust file
  $ grep -n FALLOUT_FO3_FNV docs/engine/charal.md
  146:| **Skill → governing attribute** map | … (OBLIVION / SKYRIM / FALLOUT_FO3_FNV); …
  307:`SkillSet::FALLOUT_FO3_FNV` (15 = FO3 ∪ FNV, SPECIAL-governed) and `SkillSet::NONE`
  $ # roster sizes, counted from skill.rs
  OBLIVION 21   SKYRIM 18   FALLOUT3 13   FALLOUT_NV 13
  ```
- **Impact**: Same class as CHAR-2026-09-11-D3-02 (`charal-oblivion-ruleset.md`'s `AttributeSet::OBLIVION`) and CHAR-2026-09-11-D2-03 (`charal-skyrim-ruleset.md`'s 32 B pin) — and it lands in the **parent** spec rather than a child capture, so a contributor who greps the symbol finds nothing and a contributor who reads the prose learns a merged roster that the code deliberately abolished. Prior sweeps have twice had to re-falsify an FO3↔FNV-collapse premise from stale documentation (last sweep's D6-03 was the SKILL.md copy); this is the third surviving copy of the same retired fact and the only one still live. Documentation-only: no code reads either line.
- **Related**: #3169 (the split); CHAR-2026-09-11-D2-03, CHAR-2026-09-11-D3-02 (the same incomplete symbol/number sweep, filed this sweep by Dimensions 2 and 3); CHAR-2026-08-30-D6-03 (the SKILL.md copy, since fixed)
- **Suggested Fix**: At `:307` replace with *"`SkillSet::FALLOUT3` (13, SPECIAL-governed) and `SkillSet::FALLOUT_NV` (13 — `BigGuns` dropped, `Throwing` added; keyed by `AVIF` EditorID, not display name)"*, and at `:146` replace `FALLOUT_FO3_FNV` with `FALLOUT3 / FALLOUT_NV`. Both are one-line edits.

#### CHAR-2026-09-11-D6-02: four CHARAL-scope references to `byroredux/src/boot.rs` survive the boot-module split — including one in a file edited in this very delta
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `docs/feature-matrix.md:291`; `crates/core/src/character/regen.rs:145`; `crates/core/src/stealth.rs:38`; `crates/core/src/combat.rs:15`
- **Status**: NEW
- **Description**: `byroredux/src/boot.rs` no longer exists — the module is now the directory `byroredux/src/boot/` (`mod.rs`, `cli.rs`, `registries.rs`, `world.rs`, `schedule/`). Commit `f9f64cc1` ("docs(audit): re-point 30 stale path refs at the 2026-09-09 split targets") and `8175cb70` repointed `charal.md:263` from `byroredux/src/boot.rs` to `byroredux/src/boot/schedule/update.rs` — correctly — but four sites in the same CHARAL scope were missed, and three of them are Rust docstrings rather than Markdown, which is presumably why a docs-only sweep did not reach them. Each names a *specific* thing to go look at, so each sends the reader to a file that is not there:
  - `feature-matrix.md:291` — *"its required `PoolRegenConfig` resource is inserted only inside unit tests, never in `boot.rs`"* → the insertion site would be `byroredux/src/boot/world.rs`.
  - `regen.rs:145` — *"inserted unconditionally at boot (`byroredux/src/boot.rs`, `build_world`)"* → `build_world` lives in `byroredux/src/boot/world.rs:25`, and the accumulator really is inserted at `:48`. `regen.rs` is one of the six files in this sweep's delta (151 lines changed), so the stale path was carried through an edit.
  - `stealth.rs:38` — *"`ambient_ai_package_system`, registered unconditionally as a `Stage::Update` exclusive in `byroredux/src/boot.rs`"* → `byroredux/src/boot/schedule/update.rs`.
  - `combat.rs:15` — *"`combat_input_system` + `combat_damage_system` (two `Stage::Update` exclusives, `byroredux/src/boot.rs`)"* → same target.
- **Evidence**:
  ```
  $ ls byroredux/src/boot.rs
  ls: cannot access 'byroredux/src/boot.rs': No such file or directory
  $ ls byroredux/src/boot/
  cli.rs  mod.rs  registries.rs  schedule  world.rs
  $ grep -rn 'boot\.rs' docs/engine/charal*.md docs/feature-matrix.md \
        crates/core/src/character/ crates/core/src/combat.rs crates/core/src/stealth.rs
  docs/feature-matrix.md:291:  … never in `boot.rs`; `affliction_tick_system`
  crates/core/src/combat.rs:15:  … (two `Stage::Update` exclusives, `byroredux/src/boot.rs`), …
  crates/core/src/character/regen.rs:145: …   at boot (`byroredux/src/boot.rs`, `build_world`).
  crates/core/src/stealth.rs:38: …  unconditionally as a `Stage::Update` exclusive in `byroredux/src/boot.rs`);
  ```
- **Impact**: Low but systematic. Three of the four are the *wiring* explanation for the two CHARAL systems this dimension's matrix reports as unwired — i.e. exactly the pointers a contributor picking up "wire regen" or "wire affliction" would follow first, and the one thing they need is where the resource insertion goes. `regen.rs:145` additionally names `build_world`, which does exist, so the reader gets a half-true citation rather than an obviously-dead one. Counting only the CHARAL slice understates the pattern — the same dead path appears in `docs/engine/npc-spawn-ai-packages.md` (×4), `docs/engine/launcher.md` (×3) and `docs/engine/m47-2-design.md`; that wider cleanup is `/audit-tech-debt`'s, not this sweep's.
- **Related**: `f9f64cc1` (the incomplete repoint sweep); `8175cb70` (which fixed the fifth copy, `charal.md:263`); memory *Session 34/35 Layout* (the general stale-path-translation hazard)
- **Suggested Fix**: Repoint all four: `feature-matrix.md:291` and `regen.rs:145` → `byroredux/src/boot/world.rs`; `stealth.rs:38` and `combat.rs:15` → `byroredux/src/boot/schedule/update.rs`. Consider extending `f9f64cc1`'s sweep to `*.rs` docstrings, which is where all three code-side copies hid.

---

### Candidates verified and dropped

1. **"`mod.rs`'s docstring has drifted from the module list."** Dropped. 14 `pub mod`, 14 index entries, two mechanical regression tests (`mod_docstring_indexes_every_sub_module` asserts the count *and* per-module presence; `mod_docstring_points_at_the_charal_adjacent_siblings` pins the `crate::combat` / `crate::stealth` pointer). Re-derived by hand, not taken from the prior report.
2. **"The `mod.rs` docstring's attribute-roster parenthetical omits `AttributeSet::STARFIELD`."** Real but below threshold, dropped. `mod.rs` says *"the per-family attribute roster (`FALLOUT` SPECIAL, `TES_CLASSIC`, and Skyrim's deliberately empty set)"* while `attribute.rs:116` also ships `STARFIELD` (also empty). It is an illustrative parenthetical, not the mechanically-pinned index (which is over modules), and `charal.md:293` enumerates all four correctly. No reader is misled about anything actionable.
3. **"FO76 is silent scope loss — a 126-line capture with LOCKED constants and no code."** Dropped: the note exists and is explicit. `charal-fo76-ruleset.md:6-8` states *"Not yet in the CHARAL §8 rollout order"* and `:124` repeats *"Not blocking (FO76 isn't in the §8 rollout order)"*; `feature-matrix.md:248-256` gives FO76 `✗` for ruleset and `~ stored, unverified` for population. The checklist's failure condition is *no implementation **and** no note*; the note is present in both the capture and the matrix.
4. **"Starfield is silent scope loss — 174 lines of capture, no code."** Dropped, same reason and better documented: `charal.md` §8 item 8 names `SkillSet::STARFIELD` / `LevelingModel::STARFIELD` / `starfield_ruleset()` as "not yet buildable", §9 records the two unpublished research blockers (XP curve, category-spend thresholds), and `charal-starfield-ruleset.md:18` marks the roster LOCKED / curve PENDING. Verified that none of those three symbols exists in code, i.e. the doc's claim is exactly right.
5. **"`charal.md:148`'s XP/level-curve row still says `pending` although §8 item 6 says the curve is GMST-overlaid."** Dropped — this was a live contradiction in the 2026-08-15 sweep and is no longer one. `:148` names the **Fallout** GMSTs (`iXPBase`, `iXPLevelUpBase`), and `LevelingModel::with_gmst` (`leveling.rs:92-109`) matches **only** the `SkillXp` variant, i.e. Skyrim — every `XpCurve` model falls through `other => other` with its constants untouched. The Fallout XP curve genuinely is still pending; the two statements are about different games.
6. **"`charal.md:419` cites a nonexistent `LevelingModel::ScaleToLeader`."** Dropped — the surrounding sentence is *"this does **NOT** need a new `LevelingModel::ScaleToLeader` arm at all"*, an explicit correction of an earlier prediction. The symbol is named in order to be rejected.
7. **"`byroredux/src/combat.rs` is the sole production consumer of `MeleeDamageConfig` + `CharacterRuleset::derived_value` but SKILL.md's scope omits it."** Dropped as not-silent. Confirmed the coupling is real (`byroredux/src/combat.rs:10, 443-463`) and that no `/audit-*` skill claims the file — but `.claude/commands/_audit-common.md:182` carries it as an explicit **"Gameplay slice (P2)"** ownership-gap row, naming `/audit-ecs` + `/audit-runtime` as nearest owners and calling it "highest-value gap on this list". A registered gap is coverage information, not scope loss, and re-filing it here would duplicate that register.
8. **"SKILL.md Dimension 2's 'these are consumer-less today' claim about `combat.rs`/`stealth.rs` has expired."** Dropped — re-measured, still true. Every caller of `modified_skill`, `oblivion_weapon_damage_multiplier`, `oblivion_hand_to_hand_damage` and `detection_score` is inside the defining module's own `#[cfg(test)]` block. `crates/core/src/combat.rs:11-26` documents the state accurately, including that a *different* consumer (`byroredux/src/combat.rs`) shipped and deliberately does not route through it.
9. **"A third stale 32 B pin exists somewhere."** Dropped — swept `docs/engine/charal*.md`, `docs/feature-matrix.md` and SKILL.md; exactly one hit (`charal-skyrim-ruleset.md:699`), already filed as CHAR-2026-09-11-D2-03. SKILL.md's own pin is the corrected 36 B.
10. **"Regen / affliction being unwired on all seven families is an unreported gap."** Dropped as already-documented coverage, not a defect: stated at `boot/schedule/update.rs:282-292`, `regen.rs:138-150`, `charal.md:262-266`, `feature-matrix.md:287-292` and `save_io/registry_completeness_tests.rs:196`. Recorded in the matrix above; the ruleset half is #3848 (OPEN).
11. **"Vocabulary drift: `stamp_*` is a competing verb set."** Dropped. `stamp_*` is uniform across all four population writers and is the population-boundary verb, distinct by design from the layer's `translate`/`resolve`/`derive`. `CreatureAttack` and `MeleeDamageConfig` follow `PoolRegenConfig`'s existing noun shape. Nothing new invented in this delta.

## Known-Open Register

The three deferred items this skill names, plus the standing OPEN issue.
Confirmed **not** re-filed by any dimension:

| Item | Disposition this sweep |
|---|---|
| **FNV/FO3 tag-skill per-level formula undocumented, deliberately deferred** (CLAS SPECIAL lives in `ATTR`, not `DATA`) | Confirmed still absent rather than guessed at (Dim 5 check 4). Correct state; not filed. |
| **FO3↔FNV divergent *player* Health/AP, deferred with the player actor** | Confirmed still deferred; `DerivedScope::PlayerOnly` tagging keeps the deferral contained to player stats (Dim 2 row 33). Not filed. |
| **VATS runtime does not exist; only the AP formulas are in CHARAL** | Confirmed. AP formulas verified (Dim 2 rows 6–7); no AP pool, time-pause, limb health or hit-chance roll exists. Not filed. |
| **`skyrim_ruleset` / `oblivion_ruleset` production-unreachable** | **Existing: #3848 (OPEN).** Cited as Existing by Dimensions 3, 4, 5 and 6; re-filed by none. |
| **`AfflictionTable` ships empty / `affliction_tick_system` unregistered** | Documented mechanism-ahead-of-data (`charal.md` §4.6), recorded in the coverage matrix at five sites. Coverage, not a defect; not filed. |

### Prior-sweep findings — disposition

All seven findings from `docs/audits/AUDIT_CHARACTER_2026-08-30.md` were
published as issues and are now **CLOSED**. Each was verified against current
code this pass rather than trusted:

| Prior finding | Issue | Verified this sweep |
|---|---|---|
| D2-01 `cap` field docstring named the pre-#2936 fractional Crit cap | #3766 | **FIXED** — `derived.rs`'s 18-line delta is exactly this doc correction |
| D4-01 `band_for` required a sorted `bands` with nothing enforcing it | #3767 | **FIXED** — `rposition` → `max_by`, contract rewritten, `band_for_ignores_band_order` added. D4-01 below is a *new, narrower* consequence, not a restatement |
| D5-01 `CREA` `DATA.Damage` dropped at the boundary | #3762 | **FIXED in part** — value and component correct and byte-verified; two residual gaps are D1-01 (TPLT bypass) and D5-03 (unreachable reader) |
| D6-01 `charal.md` declared Oblivion "CHARAL-complete end-to-end" | — | **FIXED** by `8175cb70` |
| D6-02 `charal.md` §8 listed `fXPPerSkillRank` | #3221 | **FIXED** by `08a70f42` |
| D6-03 SKILL.md Dim-5 asserted a removed FO3↔FNV ruleset collapse | — | **FIXED** — `grep Fallout3NV SKILL.md` → no match |
| D6-04 `feature-matrix.md` had no `CREA` row | — | **FIXED** — row at `:253`, prose at `:267-275` |

Also CLOSED and verified in place: #3482 (stealth sub-coefficients — see the
Executive summary), #3483, #3444, #3485, #3484, #3878, #3390.

## Cross-audit routing

Per `_audit-common.md`, these belong to sibling skills and are **not** filed here:

- **Component storage / shape** (`ActorValues`, `Perks`, `CharacterLevel`
  storage backends) → `/audit-ecs`.
- **AVIF / CLAS / `NPC_` / `RACE` / `CREA` sub-record byte accounting and FormID
  remap** → `/audit-esm` Dim 3 (remap) and Dim 4 (per-record schemas). D5-02's
  *parsing* of `Use Factions` / `Use AI Packages` is correct; only the
  *resolution* at the population boundary is filed here.
- **CTDA condition evaluation** (`GetActorValue`, `GetFactionRank`,
  `GetXPForNextLevel`) → `/audit-scripting`.
- **Scheduler access declarations** → `/audit-concurrency` Dim 4. The specific
  `pool_regen_tick_system` declaration was verified here (Dim 4) and PASSES.
- **`byroredux/src/combat.rs` ownership** — a registered gap row in
  `_audit-common.md:182`; the melee-damage pipeline has no owning skill. Not
  silent scope loss, but worth an owner.

## Process notes

- Six dimension agents, max 3 concurrent, each given the capture documents
  **before** the code per Phase 1 item 6.
- Every dimension's scratch file was cross-checked against its returned summary
  before merging (per *feedback_audit_suite_nested_agent_relay*); the D1-01 /
  D5-03 conflict was found that way and resolved by direct verification in the
  merge, not by trusting either agent.
- No census probe was left in the tree; `git status` is clean. No
  `byroredux` process was started (per *feedback_no_parallel_engine_launch*),
  and no `--ignored` ESM-parsing test was run (per *plugin_ignored_tests_oom*).
- One claim is recorded as **UNVERIFIED** rather than asserted: Dimension 1's
  magnitude estimate for D1-01 (how many templated creatures' shell `DATA`
  actually differs from their resolved record's) needs an ESM parse that the
  memory budget did not permit. The finding states the bound it can support
  (815/1578 FNV and 399/533 FO3 creatures are templated) and explicitly leaves
  the exact affected count open.
