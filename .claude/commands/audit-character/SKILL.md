---
description: "Deep audit of CHARAL — the per-game character ruleset → canonical ActorValues/Level/Perks layer: derived-stat formulas, leveling models, skill/attribute rosters, regen + affliction + reputation systems, population boundary"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# Character / CHARAL Audit

Audit `crates/core/src/character/` — the per-game **character ruleset**
translation tier. CHARAL translates *rules*, not data: each game's attribute /
skill / perk / leveling model is resolved into one canonical representation over
`ActorValues`, so the gameplay runtime never branches on the source game.

This subsystem had **no owner** before this skill. `/audit-ecs` checks the shape
of its components; nothing checked its *numbers*. That is the whole risk: CHARAL
is the one place where a wrong constant changes gameplay silently — no crash, no
validation-layer error, no failing test unless someone wrote the test.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crate slice**: `crates/core/src/character/`
- `crates/core/src/character/ruleset.rs` — `CharacterRuleset`, the per-game
  `Resource` seam (attribute roster + skill roster + flat derived table +
  leveling model).
- `crates/core/src/character/profile.rs` — `CharacterRulesProfile`, the single
  per-game policy row selected at the parser boundary (`NpcStatModel`,
  `RulesetBuilder`, `NpcHealthCurve`) — the construction seam Dimension 1's
  doctrine check and Dimension 3's coverage matrix both depend on.
- `crates/core/src/character/derived.rs` — `DerivedStatFormula`, `DerivedInput`,
  `DerivedOutput`, `DerivedScope`, `RoundMode`: the fixed-layout bilinear form
  every Bethesda derived stat reduces to.
- `crates/core/src/character/leveling.rs` — `LevelingModel`
  (`XpCurve` / `SkillUse` / `SkillXp`), `LevelReward`.
- `crates/core/src/character/attribute.rs` / `skill.rs` — `Attribute`,
  `AttributeSet`, `SkillDef`, `SkillSet`, `ResolvedSkill`.
- `crates/core/src/character/fallout.rs` / `tes.rs` / `skyrim.rs` — the three
  family implementations (`fallout3_ruleset`, `falloutnv_ruleset`,
  `fallout4_ruleset`, `oblivion_ruleset`, `skyrim_ruleset` + their helpers).
- `crates/core/src/character/regen.rs` — `PoolRegenAccumulator`,
  `PoolRegenConfig`, `pool_regen_tick_system` (the fixed-60 Hz pool tick).
- `crates/core/src/character/affliction.rs` / `resistance.rs` —
  `AfflictionTable`, `AfflictionStatus`, `AfflictionBand`, `AvPenalty`,
  `affliction_tick_system`, `Affliction`, `damage_multiplier`.
- `crates/core/src/character/reputation.rs` — `KarmaBand`,
  `ReputationStanding`, `AffinityBand`, `FactionRepThresholds`.
- `crates/core/src/character/components.rs` — `CharacterLevel`, `Perks`,
  `PerkRank`, `Background`, `FactionReputation`, `FactionStanding`.
- Substrate: `crates/core/src/ecs/components/actor_values.rs`,
  `crates/core/src/ecs/components/perk_list.rs`,
  `crates/core/src/ecs/components/faction_ranks.rs`.

**CHARAL-adjacent siblings** (in scope, but NOT submodules of
`crates/core/src/character/` — CHAR-D6-05 / #2962): `crates/core/src/combat.rs`
(classic Oblivion combat-damage math — `modified_skill`,
`oblivion_weapon_damage_multiplier`, `oblivion_hand_to_hand_damage`) and
`crates/core/src/stealth.rs` (FO3/FNV sneak-detection — `detection_score`,
`classify`). Both read `ActorValues` as inputs but evaluate at
combat/stealth-resolution time against transient per-hit state that never
lives in `ActorValues` — each module's own docstring explains the boundary
and cites `charal.md` §7. They previously sat outside every audit skill's
declared scope entirely; Dimension 2 now covers their constants explicitly
(see below).

**Population boundary** (Dimension 5 — outside the crate):
`byroredux/src/npc_spawn.rs` (`build_character_ruleset` — the ONLY construction
site) and the parse-side feed `crates/plugin/src/esm/records/actor_value_derive.rs`
+ `crates/plugin/src/esm/records/actor/mod.rs`. Console consumers:
`byroredux/src/commands/actor_value.rs` (`setav` / `modav`) and
`byroredux/src/commands/condition.rs`.

**Ground truth — read before auditing, in this order**:
1. `docs/engine/charal.md` — the layer spec and its doctrine (data seam, not
   code seam; single sink).
2. The six per-game ruleset captures — `docs/engine/charal-fnv-fo3-ruleset.md`,
   `charal-fo4-ruleset.md`, `charal-oblivion-ruleset.md`,
   `charal-skyrim-ruleset.md`, `charal-fo76-ruleset.md`,
   `charal-starfield-ruleset.md`. **These are the authority for every constant.**
   A coefficient in code that no capture document supports is a finding, full
   stop (`feedback_no_guessing`).
3. Project memory: `actor_value_system`, *actor_value_population*,
   *class_system*, `perk_system`, `perk_entry_points`, *tes_character_rules*,
   *vats_system*.

**Known-open, do NOT report as new**:
- FNV/FO3 **tag-skill per-level** formula is undocumented and deliberately
  deferred (*actor_value_population*). CLAS SPECIAL lives in `ATTR`, not `DATA`.
- FO3↔FNV divergent *player* Health/AP needs master-name disambiguation and is
  deferred with the player actor (see `build_character_ruleset`'s docstring).
- VATS runtime (AP pool/regen, time-pause, limb health, hit-chance roll) does
  not exist yet; only the AP *formulas* are in CHARAL (*vats_system*).

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers. Default: all 6.
- `--game <name>`: restrict constant-verification to one family
  (`fnv` / `fo3` / `fo4` / `oblivion` / `skyrim`). Default: every implemented one.
- `--depth shallow|deep`: `shallow` = structure + doctrine; `deep` = verify every
  coefficient against its capture document. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Ruleset Seam | Derived Formulas | Leveling & Progression |
  Pools, Afflictions & Reputation | Population Boundary | Coverage & Doctrine
- **Game**: which family's numbers the finding concerns (or `all`).
- **Source**: the capture-document line the correct value comes from — required
  for any numeric finding. A numeric finding with no source is not reportable.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--game`, `--depth`.
2. `mkdir -p /tmp/audit/character`.
3. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/character/issues.json`.
4. Read the most recent `docs/audits/AUDIT_CHARACTER_*.md` if one exists;
   otherwise scan the ECS and per-game reports for `ActorValues` / CHARAL
   findings — that is where duplicates live.
5. `cargo test -p byroredux-core character` and record the counts.
6. **Load the capture documents first.** Do not read the Rust constants before
   the documents; reading code first anchors you to whatever it says and turns
   verification into confirmation.

## Phase 2: Launch Dimension Agents

### Dimension 1: Ruleset Seam & CHARAL Doctrine
**Entry points**: `crates/core/src/character/ruleset.rs` — `CharacterRuleset`,
`new`, `with_attributes`, `with_skills`, `with_derived`, `push_derived`;
`crates/core/src/character/profile.rs` — `CharacterRulesProfile`, the per-game
policy row the doctrine check depends on; `crates/core/src/character/mod.rs`
(the re-export surface)
**Checklist**:
- **The doctrine check.** The per-game seam must be *data in the tables*, never a
  branch in a consumer. Grep every consumer of `CharacterRuleset` for a match on
  game identity (`GameKind`, master name, a `game ==` compare) and report each —
  that is a CHARAL violation with the same weight `/audit-nifal` gives a
  render-time per-game branch.
- **Single sink.** `build_character_ruleset` (`byroredux/src/npc_spawn.rs`) is
  documented as the only construction site. Verify no second site builds a
  ruleset or bypasses it by writing derived stats directly into `ActorValues`.
- The derived table is a flat `Vec<(u32, DerivedStatFormula)>` scanned linearly,
  justified by N ≈ 6–10. Verify N is still in that range for every implemented
  game; a game that grows to dozens invalidates the data-structure rationale
  (report as a documented trade-off, not a bug, unless it is on a hot path).
- Output keys are **AVIF FormIDs in global space**. Verify they are remapped ids
  (`/audit-esm` Dim 3), not raw plugin-local ones — a raw id here silently
  targets the wrong actor value in a multi-plugin load.
- `attributes` / `skills` membership is ENGINE-SUPPLIED; the FormIDs each
  resolves to are AUTHORED. Verify that split holds: a hardcoded FormID in the
  roster is a finding, and so is an engine-supplied *count* derived from parsed
  data.
- **Effective actor level has exactly ONE implementation (regression guard, #3171, `9e44a0dd`).** `effective_actor_level` lives in `crates/plugin/src/esm/records/actor/mod.rs`, beside the `NpcRecord` whose overloaded `level` field it decodes; both call sides import it. It has been duplicated **twice** already — `inventory.rs` (deleted by #3081) and *effective_npc_level* in `actor_value_derive.rs` (deleted here) — and the second copy carried the exact `.max(1)` divergence #3081 had explicitly rejected six weeks earlier, surviving because #2955's regression test only ever called the original. Measured consequence on FalloutNV.esm: **30 `NPC_` records are non-multiplier with `level <= 0`**, and for those `derive_autocalc_actor_values` evaluated the Health curve at level 1 while `stamp_character_components` wrote `CharacterLevel { level: 0 }` on the same entity, and `resolve_inherited_stats` filtered `LVLN` tiers at a third level — a `Use Stats` shell could take its ActorValues from one source record and its `CharacterLevel`/`Background` from another. The rule is `.max(0)`, **not** `.max(1)`: a plain `level` of 0 is not a "record carries none" sentinel the way `calc_min` is, so forcing it to 1 invents data. A fourth copy, or a re-added `.max(1)`, is the regression — `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` calls it through the plugin crate so a new copy has something to fail against.
- **Rosters are falsified against real masters, not against fixtures (#3172).** `ROSTER_CASES` + `assert_rosters_resolve` (`crates/plugin/tests/parse_real_esm.rs`) run the existence loop over every shipped master for every roster and every derived-row *output* key. Before this, only `SkillSet::FALLOUT_NV` had a real-data loop; the other four and all output keys leaned on hand-written `full()` fixtures that enumerate the same strings the builders pass — so they could not falsify a roster at all. A new roster or derived output added without a `ROSTER_CASES` entry is untested by construction.
**Output**: `/tmp/audit/character/dim_1.md`

### Dimension 2: Derived-Stat Formulas (the highest-value numbers)
**Entry points**: `crates/core/src/character/derived.rs` — `DerivedStatFormula`,
`eval`, `DerivedInput::{UNUSED, LEVEL, actor_value}`, `DerivedOutput`,
`DerivedScope`, `RoundMode`; the per-game tables in
`crates/core/src/character/fallout.rs`, `tes.rs`, `skyrim.rs`
**Checklist**:
- **Verify every coefficient against its capture document.** For each formula:
  bias, `c_a`, `c_b`, cross term, cap, and rounding mode. Cite the document line
  for each. The reference shape is FO4 Health
  (`floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)`) — the only one needing the cross
  term; every other captured formula is affine, so a non-zero cross term
  elsewhere is a red flag worth chasing.
- `RoundMode`: floor vs round vs truncate changes the result by one point at
  every boundary. Verify each formula's mode against its source; Bethesda mixes
  them and "obviously it's round" is exactly the guess this audit exists to catch.
- Caps: a cap of `0` must mean *uncapped*, not *clamped to zero*. Verify the
  sentinel handling in `eval` and that no per-game table relies on the opposite
  reading.
- **Chaining ordering.** A derived stat may read another actor value (FNV Unarmed
  Damage ← Unarmed skill ← SPECIAL). The deriver must populate base attributes +
  skills into `ActorValues` **before** evaluating dependents; the ordering is the
  resolution mechanism, there is no dependency graph. Verify the population path
  (Dim 5) actually establishes that order, and that a formula whose input is not
  yet populated reads a *documented* default rather than an accidental zero.
- `DerivedInput` sentinels: `0` = unused, `u32::MAX` = level. Verify no code path
  can construct a real FormID equal to either (the constructor documents the
  caller guarantee — check the callers, not just the constructor).
- `DerivedScope`: player-only vs actor-general stats. The FO3/FNV player
  Health/AP divergence is deferred *because* of this split — verify the scope
  tagging is correct so the deferral stays contained to player stats.
- `eval` is documented as allocation-free and ~5 FMAs. Verify no branch on game
  identity crept in, and that `DerivedStatFormula` is still `Copy` + 36 B
  (grew from 32 B when `clamped_below`'s `floor: f32` and the `base_reads: u8`
  bitfield landed under #2939; pinned by `formula_is_thirty_six_bytes_and_copy`,
  `crates/core/src/character/derived.rs:353-359`).
- **CHARAL-adjacent siblings** (CHAR-D6-05 / #2962): verify
  `crates/core/src/combat.rs`'s `modified_skill` (the `0.4` Luck coefficient),
  `oblivion_weapon_damage_multiplier` (four coefficients), and
  `oblivion_hand_to_hand_damage` (the Strength×Skill cross-term) against
  `docs/engine/charal-oblivion-ruleset.md`'s "The Complete Damage Formula" /
  "Melee weapon damage" sections, same discipline as any table in `fallout.rs`
  / `tes.rs` / `skyrim.rs`. Verify `crates/core/src/stealth.rs`'s
  `detection_score` / `classify` against `charal-fnv-fo3-ruleset.md`'s
  "Sneak Detection (FNV)" section. These are consumer-less today (no combat/
  stealth-resolution system reads them yet) — report drift as a normal
  finding regardless; deferred consumption is not a reason to skip
  verification.
**Output**: `/tmp/audit/character/dim_2.md`

### Dimension 3: Leveling & Progression Models
**Entry points**: `crates/core/src/character/leveling.rs` — `LevelingModel`
(`XpCurve`, `SkillUse`, `SkillXp`), `LevelReward`;
`crates/core/src/character/skyrim.rs` — `skyrim_skill_xp_to_next`,
`skyrim_skill_xp_between`, `SKYRIM_SKILL_USE_CURVE`, `SKYRIM_POOL_BASE`;
`crates/core/src/character/tes.rs` — `oblivion_attribute_bonus`,
`oblivion_health_gain_per_level`, `oblivion_health_formula`,
`oblivion_magicka_formula`, `oblivion_fatigue_formulas`;
`crates/core/src/character/components.rs` — `CharacterLevel`, `Perks`, `PerkRank`;
`crates/core/src/character/profile.rs` — `NpcHealthCurve`/`NpcStatModel`, the
per-game coverage matrix's construction seam
**Checklist**:
- Three genuinely different models — Fallout XP curve, classic-TES skill-use,
  Skyrim skill-XP — must be three data variants, not three code paths in the
  consumer. Verify the consumer side is one match on the enum at one place.
- `level_cap == 0` means uncapped in all three variants. Verify the sentinel is
  handled identically everywhere (an off-by-one at the cap is a hard wall the
  player hits at exactly one level).
- Skyrim: `xp_mult·level + xp_base` is sourced to UESP `fXPLevelUpBase` /
  `fXPLevelUpMult` and IS resolved from parsed GMSTs (`with_gmst` overlays both
  when present). `rank · xp_per_skill_rank` is different: `xp_per_skill_rank`
  is a deliberate fixed engine coefficient, not authored — the code no longer
  reads a *fXPPerSkillRank* GMST at all (withdrawn 2026-08-24; see
  `docs/engine/charal-skyrim-ruleset.md` and
  `skyrim_gmst_overlay_reads_only_authored_curve_settings` in
  `crates/core/src/character/leveling.rs`). **Do not re-flag this split as an
  authoring gap** — it is the settled, verified design: only the level curve
  is GMST-authored, the skill-rank coefficient is engine-owned. Do verify the
  overlay still requests exactly `["fXPLevelUpBase", "fXPLevelUpMult"]` and no
  third GMST creeps back in.
- Oblivion: the level-up modifier is `+1..5` driven by governing-skill increases
  (*tes_character_rules*). Verify the banding table and the 10-major-skill-ups
  threshold; `oblivion_attribute_bonus` is where a wrong band silently changes
  every level-up in the game.
- Skyrim base pools `100 H/M/S` with `+10/level` on the chosen pool
  (*tes_character_rules*) — verify `SKYRIM_POOL_BASE` and `pool_pick_gain`.
- `Perks` / `PerkRank`: rank stacking and the perk-entry-point model
  (`perk_system`, `perk_entry_points`) — verify ranks are additive per perk and
  that a rank beyond the perk's declared max is rejected, not clamped silently.
- Which models are *reachable*? Only FO4 and FO3NV rulesets are constructed today
  (Dim 5). Oblivion and Skyrim rulesets exist but have no construction site —
  audit them for correctness, and state plainly in the report that they are
  unwired. That is coverage information, not a bug.
**Output**: `/tmp/audit/character/dim_3.md`

### Dimension 4: Pools, Afflictions, Resistances & Reputation
**Entry points**: `crates/core/src/character/regen.rs` —
`PoolRegenAccumulator`, `PoolRegenConfig`, `pool_regen_tick_system`,
`POOL_REGEN_DT`, `FATIGUE_REGEN_PER_SEC`, `MAGICKA_REGEN_BASE`,
`MAGICKA_REGEN_WILLPOWER_COEFF`, `magicka_regen_per_sec`;
`crates/core/src/character/affliction.rs` — `AfflictionTable`,
`AfflictionStatus`, `AfflictionBand`, `AvPenalty`, `ActiveAffliction`,
`affliction_tick_system`, `reevaluate_affliction`;
`crates/core/src/character/resistance.rs` — `Affliction`, `damage_multiplier`;
`crates/core/src/character/reputation.rs` — `karma_band`, `clamp_karma`,
`affinity_band`, `affinity_reaction_delta`, `affinity_passive_gain`,
`clamp_affinity`, `reputation_bump_points`, `FactionRepThresholds`,
`ReputationStanding`
**Checklist**:
- **The fixed-60 Hz tick.** `pool_regen_tick_system` runs on
  `PoolRegenAccumulator` at `POOL_REGEN_DT`, deliberately decoupled from the
  frame rate and mirroring the physics accumulator. Verify: the accumulator
  clamps its backlog (an unbounded catch-up after a long stall regenerates a full
  pool instantly), regen is applied per fixed tick not per frame, and a paused /
  zero dt cannot spin.
- `PoolRegenConfig` holds per-game *resolved AVIF ids*. Verify it is only
  inserted once a live `CharacterRuleset` exists (see the comment in
  `byroredux/src/boot.rs`), and that the declared resource access in the
  scheduler matches what the system actually touches — `/audit-concurrency` Dim 4
  owns the general rule; verify this specific declaration here.
- Affliction is a **diff-and-reapply** driver: pool → threshold band → SPECIAL
  penalty, with per-actor active-band memory. Verify the reapply removes the
  previous band's penalty before applying the new one — a missing removal
  compounds penalties every band change and looks like a slow stat drain.
  Verify band boundaries are half-open and consistent (a value exactly on a
  boundary must land in exactly one band).
- `damage_multiplier` / resistance derivation: verify the curve shape and its
  cap against the capture documents; a resistance that can exceed 100 % turns
  damage into healing.
- Karma / reputation: `KarmaBand` and the FNV Fame/Infamy 4×4
  `ReputationStanding` grid. Verify the grid's axes are not transposed
  (Fame×Infamy vs Infamy×Fame produces a plausible-looking wrong standing) and
  that `clamp_karma` / `clamp_affinity` bound both ends.
- Every constant in this dimension needs a **Source** field. Regen rates and
  band thresholds are exactly the kind of number that gets typed from memory.
**Output**: `/tmp/audit/character/dim_4.md`

### Dimension 5: Population Boundary (parse → ruleset → actor)
**Entry points**: `byroredux/src/npc_spawn.rs` — `build_character_ruleset` and
the actor-spawn tail that writes `ActorValues` / `CharacterLevel` / `Perks`;
`crates/plugin/src/esm/records/actor_value_derive.rs`;
`crates/plugin/src/esm/records/actor/mod.rs`;
`byroredux/src/cell_loader/references/mod.rs` (the `CharacterRuleset`
resource lookup); `byroredux/src/commands/actor_value.rs`;
`crates/core/src/character/profile.rs` — `CharacterRulesProfile`, the row
`derive_skyrim_actor_values` and its siblings are selected from
**Checklist**:
- `build_character_ruleset` returns `None` for Oblivion / Skyrim / FO76 /
  Starfield. Verify every caller handles `None` as "no CHARAL for this game" and
  degrades gracefully — not as "use the default ruleset", which would apply
  Fallout formulas to a TES actor.
- `resolve` is `index.actor_value_form_id(editor_id)`: an EditorID→FormID lookup.
  Verify an unresolved EditorID skips its formula (the `push_derived`
  resolve-or-skip form) rather than registering a formula keyed on `0`.
- Ordering (from Dim 2): base attributes and skills must be written into
  `ActorValues` before dependent derived stats are evaluated. Trace the actual
  spawn tail and confirm the order; this is a *sequence* invariant with no type
  to enforce it.
- FNV/FO3 class auto-calc (`skill = 2 + 2×SPECIAL + ceil(Luck/2)` per
  *actor_value_population*): verify the implemented half, and confirm the
  tag-skill per-level part is still *absent* rather than guessed at.
- `setav` / `modav` write live values. Verify they write the base component, not
  a derived output that the next tick recomputes (a console command whose effect
  silently reverts is worse than one that fails).
- Templated NPCs: the NPC_ record's 12 template-inheritance flags
  (*actor_record_structure*) decide which stat groups come from the template.
  Verify CHARAL population respects the flags rather than overwriting inherited
  values.
- `derive_skyrim_actor_values` (2026-08-24) resolves Health, Magicka, and
  Stamina independently — each is its own `RACE.DATA` starting value plus a
  signed `NPC_.ACBS` offset, resolved through its own authored AVIF. This
  replaced an older all-or-nothing gate that returned Health alone and only
  when both the Health AVIF and `starting_health` were present. Verify the
  per-pool independence holds: a race missing `starting_magicka` (or a load
  order missing the Magicka AVIF) must not suppress Health/Stamina, and vice
  versa — `RaceRecord.starting_magicka`/`starting_stamina` are `Option<f32>`
  precisely so partial race data degrades one pool at a time, not the whole
  NPC.
**Output**: `/tmp/audit/character/dim_5.md`

### Dimension 6: Coverage, Documentation & Doctrine Drift
**Entry points**: the six `docs/engine/charal-*-ruleset.md` captures vs the code;
`docs/engine/charal.md`; `crates/core/src/character/mod.rs` docstring
**Checklist**:
- Build the coverage matrix: game family × {ruleset implemented, ruleset wired,
  derived stats implemented, leveling model implemented, regen wired, affliction
  wired}. Publish it even with zero findings — it is the artifact that tells the
  next milestone what is actually missing.
- Every capture document should have a corresponding implementation or an
  explicit "not implemented" note. A capture with no implementation and no note
  is silent scope loss.
- The `mod.rs` docstring enumerates the sub-module roles. Verify it matches the
  live module list (`affliction`, `attribute`, `components`, `derived`,
  `fallout`, `leveling`, `profile`, `regen`, `reputation`, `resistance`,
  `ruleset`, `skill`, `skyrim`, `tes` — 14 modules,
  `mod_docstring_indexes_every_sub_module` pins the count) — this docstring is
  the entry point every future contributor reads first, and it names files.
- Cross-check `docs/feature-matrix.md` for character/progression rows; it is
  documented as lagging the code, so treat a lag as doc rot to report, not as a
  missing feature.
- Naming/vocabulary drift: CHARAL's verbs are `translate` / `canonical` /
  `resolve`, matching NIFAL/EXAL/PHYSAL/WATAL. Flag a new sibling concept that
  invents a different vocabulary.
**Output**: `/tmp/audit/character/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/character/dim_*.md`.
2. Combine into `docs/audits/AUDIT_CHARACTER_<TODAY>.md`:
   - **Executive Summary** — findings by severity; which families' constants were
     actually verified against capture documents (and which were not).
   - **Constant Verification Table** — formula/constant × code value × document
     value × verdict. Any row without a document value is `UNSOURCED` and is
     itself a finding.
   - **Coverage Matrix** — from Dim 6.
   - **Findings** — grouped by severity, deduplicated.
   - **Known-Open Register** — restate the three deferred items and confirm they
     were not re-filed.
3. Cross-audit dedup: component storage/shape → `/audit-ecs`; AVIF/CLAS/NPC_
   parsing → `/audit-esm` Dim 4; CTDA condition evaluation → `/audit-scripting`;
   scheduler access declarations → `/audit-concurrency` Dim 4.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/character`
2. Inform the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_CHARACTER_<TODAY>.md`
   (domain label: `character`; add the matching `game:*` — CHARAL findings are almost
   always specific to one title's ruleset).
