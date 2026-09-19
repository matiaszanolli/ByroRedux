---
description: "Deep audit of CHARAL — the per-game character ruleset → canonical ActorValues/Level/Perks layer: derived-stat formulas, leveling models, skill/attribute rosters, regen + affliction + reputation systems, population boundary"
argument-hint: "--focus <dimensions> --game <name> --depth shallow|deep"
---

# Character / CHARAL Audit

Audit `crates/core/src/character/` — the per-game **character ruleset** translation tier. CHARAL
translates *rules*, not data: each game's attribute / skill / perk / leveling model resolves into one
canonical representation over `ActorValues`, so the gameplay runtime never branches on the source game.
`/audit-ecs` checks the *shape* of its components; nothing else checks its *numbers*. A wrong constant
changes gameplay silently — no crash, no validation error, no failing test unless someone wrote one.

**Architecture**: Orchestrator; each dimension runs as a Task agent (max 3 concurrent). Read
`.claude/commands/_audit-common.md` (layout, methodology, dedup, finding format) and
`.claude/commands/_audit-severity.md` first; do not duplicate them here.

## Scope

- `crates/core/src/character/` (14 modules, `mod_docstring_indexes_every_sub_module` pins the count):
  `ruleset.rs` (`CharacterRuleset`, the per-game `Resource` seam), `profile.rs` (`CharacterRulesProfile` —
  the single per-game policy row: `NpcStatModel`, `creature_stats`, `body_condition_base`,
  `RulesetBuilder`), `derived.rs` (`DerivedStatFormula`/`DerivedInput`/`DerivedOutput`/`DerivedScope`/
  `RoundMode`), `leveling.rs`, `attribute.rs`/`skill.rs`, `fallout.rs`/`tes.rs`/`skyrim.rs` (per-family
  ruleset builders), `regen.rs`, `affliction.rs`/`resistance.rs`, `reputation.rs`, `components.rs`.
  Substrate: `crates/core/src/ecs/components/{actor_values,perk_list,faction_ranks}.rs`.
- CHARAL-adjacent (not submodules): `crates/core/src/combat.rs` (Oblivion damage math),
  `crates/core/src/stealth.rs` (FO3/FNV sneak detection) — consumer-less or thin today; verify anyway.
- **Population boundary**: `byroredux/src/npc_spawn.rs` (`build_character_ruleset`, the only ruleset
  construction site) + `npc_spawn/{resumable,ai_package}.rs`, `crates/plugin/src/esm/records/
  actor_value_derive.rs`, `crates/plugin/src/esm/records/actor/mod.rs` (`effective_actor_level`),
  `crates/plugin/src/equip.rs` (`resolve_inherited_*`), `byroredux/src/cell_loader/references/`,
  `byroredux/src/inventory.rs` (`attach_to_player` stamps the player's `ActorValues`/`ActorVitals`),
  `byroredux/src/commands/{actor_value,condition}.rs`.
- Handoff: gameplay writers of `ActorValues` (consumables, restoration, combat — `byroredux/src/
  {inventory,combat}.rs`, `systems/restoration.rs`) belong to `/audit-gameplay`; CHARAL owns only whether
  they respect the layer contract (Dim 4). Save schema of `ActorValues`/`Perks` → `/audit-save`.

**Ground truth — read the captures BEFORE the Rust constants** (reading code first turns verification
into confirmation): `docs/engine/charal.md`, then `charal-{fnv-fo3,fo4,oblivion,skyrim,fo76,starfield}-
ruleset.md` — **the authority for every constant**; a coefficient no capture supports is a finding
(*feedback_no_guessing*). Memory: *actor_value_population*, *class_system*, *tes_character_rules*,
*vats_system*, `perk_system`.

**Known-open — do NOT re-file** (confirm still true):
- FNV/FO3 **tag-skill per-level** formula undocumented, deferred; CLAS SPECIAL lives in `ATTR`, not `DATA`.
- FO3↔FNV divergent *player* Health/AP (#2937): every affected row ships `.player_only()`, disclosed in
  place and pinned; formula decision deferred pending master-name disambiguation.
- VATS runtime (AP pool/regen, time-pause, limb health, hit roll) absent; only AP *formulas* exist.
- Regen: `PoolRegenConfig` has **zero production insertion sites** (`pool_regen_tick_system` is
  registered and early-returns on every game); `affliction_tick_system` has no scheduler registration; no
  runtime level-up system (`level_cap()` has no consumers). Verify still true; a change is a wiring
  event (Dim 5 doc sweep).
- Oblivion `RulesetBuilder::None` is deliberate (no AVIF pre-FO3 → no resolver); pinned by
  `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate`. FO76/Starfield: captures, no builders.
- Open from the 2026-09-19 report — cite, do not re-file: #4452 (`melee_damage_charal_bonus` skips the
  `DerivedScope` check), #4453 (FO76/Starfield `Stored` rows unsourced), #4454 (Skyrim NPC pool: 2 of 3
  capture terms), #4455/#4459-#4463 (doc rot), #4456 (`AfflictionTable` tie-break), #4457 (TPLT hoist),
  #4137 (six `template_flags` bits with no consumer), #4232 (`effective_actor_level` returns 0 verbatim).
  Verify with `gh issue view` before citing.

## Parameters

`--focus <dims>` (default all 5) · `--game fnv|fo3|fo4|oblivion|skyrim` (restrict constant checks) ·
`--depth shallow|deep` (`shallow` = structure + doctrine; `deep` = every coefficient vs its capture; default deep).

## Extra Per-Finding Fields

- **Dimension**: Ruleset Seam | Derived Formulas | Progression & Pools | Population Boundary | Coverage & Doctrine
- **Game**: family the finding concerns (or `all`). **Source**: the capture-document line the correct value
  comes from — **required for any numeric finding**; a numeric finding with no source is not reportable.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/character`; dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/character/issues.json`.
2. Read the newest `docs/audits/AUDIT_CHARACTER_*.md`; its fixed findings are regression checks.
3. `cargo test -p byroredux-core character` (record counts). Load the capture documents before any Rust constant.

## Phase 2: Dimensions

Delta-first: for each dimension run `git log --since=<last report date> --format='%h %cs %s' -- <Paths>`;
skim dimensions whose Paths did not change (constants only move when a capture or formula commit lands).

### Dimension 1: Ruleset Seam & Doctrine
Paths: `crates/core/src/character/{ruleset,profile,mod}.rs`, `byroredux/src/npc_spawn.rs`
First step: `grep -rn 'GameKind\|\.game ==\|game_kind' crates/core/src/character byroredux/src/npc_spawn* crates/plugin/src/esm/records/actor_value_derive.rs`
- **Doctrine**: the per-game seam is *data in the tables*, never a branch in a consumer. Report every
  `game ==` / `GameKind` / master-name test in CHARAL population or consumers (weight = `/audit-nifal`'s
  render-time per-game branch). Precedent: FO3/FNV body-condition seeding was a raw `GameKind` gate until
  it became `CharacterRulesProfile.body_condition_base` (#4447).
- **Profile-as-oracle**: `CharacterRulesProfile` is the only FO3-vs-FNV discriminator (both are
  `GameKind::Fallout3NV`); non-character consumers (`cell_loader/references/attach.rs::obscript_dialect_for`,
  `crates/plugin/src/consumables.rs` fn-586 gate) borrow it. The duty is documented and pinned (#4448) —
  verify each consumer still has a test proving the discriminator is the *profile row*, and that a new
  borrower adds one.
- **Single sink**: `build_character_ruleset` is the only construction site; no second site builds a
  ruleset or writes derived stats straight into `ActorValues`.
- Derived table is a flat `Vec<(u32, DerivedStatFormula)>` scanned linearly (N ≈ 3–10): report growth to
  dozens as a documented trade-off unless on a hot path. Output keys are **remapped global AVIF FormIDs**
  (`/audit-esm` Dim 3), not plugin-local ids.
- `attributes`/`skills` *membership* is engine-supplied, the FormIDs they resolve to are authored: a
  hardcoded FormID in a roster, or an engine count derived from parsed data, is a finding. Guard:
  `ROSTER_CASES` + `assert_rosters_resolve` (`crates/plugin/tests/parse_real_esm.rs`, `#[ignore]`d
  real-master corpus gate) — a new roster/derived output without a `ROSTER_CASES` entry is untested by
  construction.
- **Profile rows must be sourced**: a row for an unwired family may not claim a wire format
  (`NpcStatModel::Stored`, #4453) without a capture line; the "blocked, not forgotten" comment pattern
  (Oblivion arm) is the template. Pins: `fallout_profiles_keep_roster_health_and_ruleset_in_lockstep`,
  `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst`.
**Output**: `/tmp/audit/character/dim_1.md`

### Dimension 2: Derived-Stat Formulas
Paths: `crates/core/src/character/{derived,fallout,tes,skyrim,resistance}.rs`, `crates/core/src/{combat,stealth}.rs`
First step: `cargo test -p byroredux-core character::derived character::fallout character::tes character::skyrim`
- **Every coefficient vs its capture** (bias, `c_a`, `c_b`, cross term, cap, `RoundMode`), citing the
  document line. FO4 Health `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` is the only cross-term row; a
  non-zero cross term elsewhere is a red flag. Rounding modes are per-source (FO4 sourced; FO3/FNV ship
  `None`, exact on their integer domain) — "obviously round/floor" is the guess this audit catches (#4451).
- **Sentinels**: uncapped cap is `f32::INFINITY` (no table may rely on cap-0 = uncapped);
  `DerivedInput` `0` = unused, `u32::MAX` = level — no path may build a real FormID equal to either.
- **Scope contract**: `DerivedScope` (player-only vs actor-general) is what contains the #2937
  deferral. A row with no scope annotation in its capture is *unsourced* (#4450: documented + pinned);
  every consumer that evaluates for an arbitrary entity must check scope (`crates/scripting/src/
  condition.rs` does; `melee_damage_charal_bonus` does not — #4452).
- **Chaining**: SPECIAL + skills must be in `ActorValues` before dependents evaluate (order is the
  mechanism, no dependency graph); an unpopulated input reads a *documented* default, not accidental 0.
- `eval` stays allocation-free, no game branch; `DerivedStatFormula` `Copy` + 36 B
  (`formula_is_thirty_six_bytes_and_copy`).
- **Siblings**: `combat.rs` (`modified_skill` Luck 0.4, `oblivion_weapon_damage_multiplier`,
  `oblivion_hand_to_hand_damage` cross-term) vs `charal-oblivion-ruleset.md` "Complete Damage Formula";
  `stealth.rs` (`detection_score`/`classify`) vs `charal-fnv-fo3-ruleset.md` "Sneak Detection". Perspective
  traps: UESP's "Opponent…" names are attacker-relative; the code reads the *wearer's* own skill (#4449).
  Verify regardless of consumer-less status.
**Output**: `/tmp/audit/character/dim_2.md`

### Dimension 3: Progression, Pools, Afflictions & Reputation
Paths: `crates/core/src/character/{leveling,regen,affliction,reputation,components,skyrim,tes}.rs`
First step: `cargo test -p byroredux-core character::leveling character::regen character::affliction character::reputation`
- **Leveling**: three models (Fallout `XpCurve`, classic-TES `SkillUse`, Skyrim `SkillXp`) are enum *data*
  variants with one consumer-side match; `level_cap()==0` = uncapped identically in all three (no consumer
  exists yet — the off-by-one duty is future). Skyrim `with_gmst` requests exactly
  `["fXPLevelUpBase","fXPLevelUpMult"]`; `xp_per_skill_rank` is a fixed engine coefficient *by design*
  (do not re-flag). Oblivion attribute-bonus banding + 10-major-skill-ups; Skyrim pool base 100, +10/level
  (`SKYRIM_POOL_BASE`, `pool_pick_gain`); `Perks::try_set_rank` rejects out-of-range rather than clamping.
- **Regen tick**: fixed-60 Hz (`POOL_REGEN_DT`), backlog clamped (`MAX_REGEN_SUBSTEPS`), zero-dt cannot
  spin; the scheduler access declaration matches the body (`/audit-concurrency` Dim 4 owns the rule).
  Absence of `PoolRegenConfig` insertion is known-open — verify the gap, not a comment describing it.
- **Affliction** = diff-and-reapply (remove previous band's penalty before applying the new one — a
  missing removal compounds); bands half-open and consistent; `band_for` vs `band_by_key` tie-break (#4456).
- **Resistance/reputation**: `damage_multiplier` clamps `[0, cap]`, never negative (no healing);
  `STANDING_GRID` axes `[infamy][fame]` (transpose = plausible wrong standing); karma/affinity clamp both
  ends; per-faction `FactionRepThresholds` rows match the capture row-for-row.
- Every constant here needs a **Source**; regen rates and band thresholds are exactly the numbers typed
  from memory.
**Output**: `/tmp/audit/character/dim_3.md`

### Dimension 4: Population Boundary (parse → ruleset → actor) — highest yield
Paths: `byroredux/src/npc_spawn*`, `crates/plugin/src/esm/records/{actor_value_derive,actor/mod}.rs`, `crates/plugin/src/equip.rs`, `byroredux/src/inventory.rs`, `byroredux/src/cell_loader/references/`
First step: `cargo test -p byroredux --bin byroredux resolve_inherited_call_sites_are_enumerated_and_pinned; git log --since=<last report> --format='%h %cs %s' -- <Paths>`
- **One derivation, one implementation** (the class that keeps recurring): `effective_actor_level` lives
  once in `actor/mod.rs` and is `.max(0)` on the non-multiplier branch — a second copy or a `.max(1)` (also
  in `examples/`) is the regression (#3171/#4095); `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_
  multiplier` calls it through the plugin crate. TPLT template inheritance: each population read goes
  through a `resolve_inherited_*` helper (`crates/plugin/src/equip.rs`), currently ~10 independent
  chain-walk sites (#4457, open); the guard enumerates the call sites — a new site or a raw `npc.<field>`
  read is the recurrence (fix-by-addition made it worse in #4086). Verify the enumerated count moved only
  deliberately, and that new reads honor `template_flags` rather than overwriting inherited values (six bits still have no consumer: #4137).
- **Writers vs stamps (silent no-op class)**: for every writer that gates on a component
  (`consume_item`/`restoration_system`/drowning gate on player `ActorValues`+`ActorVitals`), find a
  *production* insert of that component on the entity class the writer targets; unit tests hand-insert and
  hide the gap (#4458: fixed by `attach_to_player`). Player `CharacterLevel`/`Background` remain
  deliberately absent — re-check each new consumer against that.
- `build_character_ruleset` returns `None` for Oblivion/FO76/Starfield; every caller must treat `None` as
  "no CHARAL for this game", never "use the default ruleset" (Fallout formulas on a TES actor).
- `resolve` = `index.actor_value_form_id(editor_id)`: an unresolved EditorID skips its formula
  (`push_derived` resolve-or-skip), never registers one keyed on `0`.
- Ordering: base attributes + skills written before dependent derived stats; FO3/FNV class auto-calc
  `skill = 2 + 2×SPECIAL + ceil(Luck/2)` implemented, tag-skill part still *absent* not guessed.
- Skyrim `derive_skyrim_actor_values`: Health/Magicka/Stamina resolve **independently** (race base +
  signed ACBS offset, own AVIF; `RaceRecord.starting_*` are `Option<f32>`) — a missing pool suppresses
  only itself. The capture's class/level term is deferred and must be disclosed in place (#4454).
- `setav`/`modav` write the *base* component, not a derived output the next tick recomputes.
**Output**: `/tmp/audit/character/dim_4.md`

### Dimension 5: Coverage, Documentation & Doctrine Drift — second-highest yield
Paths: `docs/engine/charal*.md`, `crates/core/src/character/mod.rs` docstring, `docs/feature-matrix.md`, `ROADMAP.md`
First step: `grep -rn 'not yet reachable\|oblivion_pool_regen_config\|when a live .CharacterRuleset' crates byroredux docs ROADMAP.md`
- **Coverage matrix** (publish even with zero findings): family × {ruleset implemented, wired, derived
  stats, leveling model, regen wired, affliction wired}. Re-derive wiring from `profile.rs` arms + workspace
  greps, never from prose. Today: FO3/FNV/FO4/Skyrim wired, Oblivion blocked, FO76/Starfield absent; regen
  and affliction inert everywhere; "leveling model" is a model, not a runtime.
- **Wiring events rot prose**: after any commit that wires/unwires a builder or resource, grep for the old
  claim in the `mod.rs` docstring, the capture Status columns (code ahead of capture is doc rot too),
  `docs/engine/charal.md`, `docs/feature-matrix.md`, `ROADMAP.md`, and code comments describing an
  *intended* insertion site. `mod_docstring_indexes_every_sub_module` checks module *names* only.
- Cite symbols, not line ranges, in docs (`CharacterRulesProfile::OBLIVION`, not `profile.rs:82-87`).
- Every capture document has an implementation or an explicit "not implemented" note; a capture with
  neither is silent scope loss. Vocabulary stays `translate`/`canonical`/`resolve` (NIFAL/EXAL/PHYSAL/WATAL).
**Output**: `/tmp/audit/character/dim_5.md`

## Phase 3: Merge

Combine `/tmp/audit/character/dim_*.md` into `docs/audits/AUDIT_CHARACTER_<TODAY>.md`: **Executive
Summary** (severity counts; which families' constants were verified against captures) · **Constant
Verification Table** (formula × code × document × verdict; a row with no document value is `UNSOURCED` and
itself a finding) · **Coverage Matrix** (Dim 5) · **Findings** by severity, deduplicated · **Known-Open
Register** (restate the items above; confirm none re-filed). Cross-audit routing: component shape →
`/audit-ecs`; AVIF/CLAS/NPC_ parsing → `/audit-esm` Dim 4; CTDA evaluation → `/audit-scripting`;
scheduler access → `/audit-concurrency` Dim 4; gameplay writers → `/audit-gameplay`.

## Phase 4: Cleanup

`rm -rf /tmp/audit/character`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_CHARACTER_<TODAY>.md` (domain label `character`; add the matching
`game:*` — CHARAL findings are almost always one title's ruleset).
