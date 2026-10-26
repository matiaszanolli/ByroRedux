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

- `crates/core/src/character/` (16 modules, `mod_docstring_indexes_every_sub_module` pins the count):
  `ruleset.rs` (`CharacterRuleset`, the per-game `Resource` seam), `profile.rs` (`CharacterRulesProfile` —
  the single per-game policy row: `NpcStatModel`, `creature_stats`, `body_condition_base`, `vital_pools`,
  `RulesetBuilder`, and since #5921 `3162b21b8` `OverEncumberedRule`, `awards_kill_experience`,
  `essential_death_time_secs`, `BarterModel`, `PerkEntryPointDialect`; later `radiation_gauge` `e9b7bcead`,
  `level_cap_raises` #6227 `cafd3ccab`, `crime`), `derived.rs` (`DerivedStatFormula`/`DerivedInput`/`DerivedOutput`/`DerivedScope`/
  `RoundMode`), `leveling.rs`, `attribute.rs`/`skill.rs`, `fallout.rs`/`tes.rs`/`skyrim.rs` (per-family
  ruleset builders), `regen.rs`, `affliction.rs`/`resistance.rs`, `reputation.rs`, `crime.rs` (`CrimeModel` — Oblivion and Skyrim rows; `CrimeKind`, `BountyScope`), `components.rs`.
  Substrate: `crates/core/src/ecs/components/{actor_values,faction_ranks}.rs`.
- CHARAL-adjacent (not submodules): `crates/core/src/combat.rs` (Oblivion damage math),
  `crates/core/src/stealth.rs` (FO3/FNV sneak detection) — consumer-less or thin today; verify anyway.
- **Population boundary**: `byroredux/src/npc_spawn.rs` (`build_character_ruleset`, the only ruleset
  construction site) + `npc_spawn/resumable/`, `npc_spawn/ai_package.rs`, `crates/plugin/src/esm/records/
  actor_value_derive.rs`, `crates/plugin/src/esm/records/actor/mod.rs` (`effective_actor_level`),
  `crates/plugin/src/equip.rs` (`resolve_inherited_*`), `byroredux/src/cell_loader/references/`,
  `byroredux/src/inventory.rs` (`attach_to_player` stamps the player's `ActorValues`/`ActorVitals`/`CharacterLevel`/
  `Background`/`SpellList`/`RaceSpells` and reads `CharacterRulesProfile::vital_pools`), `byroredux/src/combat.rs` (`melee_damage_charal_bonus`, `grant_experience`),
  `byroredux/src/commands/{actor_value,condition}.rs`, `byroredux/src/crime.rs` (installs the `CrimeModel` resource from
  `index.character_rules.crime()`), `crates/scripting/src/{crime_faction,jail}.rs` (crime-faction fines, Oblivion jail).
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
- FO3/FNV AP (and Crit/Melee/Unarmed) NPC scope is unsourced: those rows ship `.player_only()` as the
  documented conservative choice (#2937, closed as documented), disclosed in place and pinned.
- VATS runtime (AP pool/regen, time-pause, limb health, hit roll) absent; only AP *formulas* exist.
- Regen: `PoolRegenConfig` has **zero production insertion sites** (`pool_regen_tick_system` is
  registered and early-returns on every game); `affliction_tick_system` has no scheduler registration.
  Verify still true; a change is a wiring event (Dim 5 doc sweep). Level-up is no longer absent: M47.2's
  `grant_experience` (`byroredux/src/combat.rs`, shared by Fallout kill experience, `levelup xp` and Skyrim
  `Game.AdvanceSkill` rank-ups via `xp_from_skill_rank`) rolls levels through `LevelingModel::roll_levels` up to
  `level_cap()` and banks the rewards into the saved `LevelUpPoints` (Dim 3/4) — Fallout `skill_point_reward`,
  Skyrim a perk point + a pool pick (#5916 `612bd179e`); only classic-TES `SkillUse` has no XP-driven roll.
- Oblivion `RulesetBuilder::None` is deliberate (no AVIF pre-FO3 → no resolver); pinned by
  `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate`. FO76/Starfield: captures, no builders.
- Open (verified 2026-10-24) — cite, do not re-file: #4137 (six `template_flags` bits with no consumer),
  #4232 (`effective_actor_level` returns 0 verbatim). #4415 (magic runtime) is closed (`9813af435`). The 2026-09-19 report's
  #4452-#4457/#4459-#4463 are all closed — regression checks now. Verify with `gh issue view` before citing.

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
First step: `grep -rn 'GameKind\|\.game ==\|game_kind' crates/core/src/character byroredux/src/npc_spawn* crates/plugin/src/esm/records/actor_value_derive.rs byroredux/src/{encumbrance,barter,combat}.rs crates/scripting/src/{bleedout,perk_entry_points}.rs`
- **Doctrine**: the per-game seam is *data in the tables*, never a branch in a consumer. Report every
  `game ==` / `GameKind` / master-name test in CHARAL population or consumers (weight = `/audit-nifal`'s
  render-time per-game branch). Precedents: FO3/FNV body-condition seeding was a raw `GameKind` gate until
  it became `CharacterRulesProfile.body_condition_base` (#4447); five M47.2 consumers (`install_encumbrance_rules`,
  `BarterRules::from_index`, `install_experience_rules`, bleedout's `engine_defaults`, `PerkEntryTable`) matched on
  `GameKind` until #5921 `3162b21b8` moved each table onto a profile row. Every row is an exhaustive struct
  literal (a new profile cannot compile without stating each); pin `per_game_rule_rows_are_stated_on_every_profile`.
- **Profile-as-oracle**: `CharacterRulesProfile` is the only FO3-vs-FNV discriminator (both are
  `GameKind::Fallout3NV`); non-character consumers (`cell_loader/references/attach.rs::obscript_dialect_for`,
  `crates/plugin/src/consumables.rs` fn-586 gate, and the #5921 rule-row readers above) borrow it. The duty is documented and pinned (#4448) —
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
- **Profile rows must be sourced**: a row for an unwired family may not claim a wire format without a
  capture line (FO76/Starfield were reset to `NpcStatModel::None`, #4453); the same holds for `vital_pools`
  (per-profile roster since #4679 — a consumer-side `GameKind` pool list is the regression); the "blocked, not forgotten" comment pattern
  (Oblivion arm) is the template; Starfield's pool roster is `("O2", "Oxygen")`, the AVIF EDID its master
  authors (#5044 `9bac2d87b` — an unresolvable `"O2"` EDID once silently dropped the bar). Pins: `fallout_profiles_keep_roster_health_and_ruleset_in_lockstep`,
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
  every consumer that evaluates for an arbitrary entity must check scope
  (`crates/scripting/src/condition/eval.rs` and, since #4452,
  `melee_damage_charal_bonus` do). `PlayerOnly` rows are evaluated for the
  player only (stamped #4674, refreshed per frame since #5039) — verify no NPC path reaches them. FO3/FNV
  shared-row scopes cite the GECK *Stats Tab — NPC* page (#5240 `3826c2792`,
  `fo3_fnv_geck_npc_page_scopes_the_shared_derived_rows`); Melee Damage is annotated CONFLICTED and kept
  actor-general deliberately — do not re-file the flip without the settle bar the capture names.
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
  variants with one consumer-side match; `level_cap()==0` = uncapped identically in all three. The roll is
  `LevelingModel::roll_levels` (#5918 `118edb1e6`): `capped = level_cap() > 0 && level >= level_cap()`, each
  level costs `xp_to_next` rounded UP to ≥ 1 whole XP (a `0 < cost < 1` curve once truncated to a zero-XP level
  and spun the main thread forever), and it stops at `u16::MAX` (`roll_levels_is_bounded_and_exact_for_odd_curves`);
  the cap off-by-one has a consumer pin, `experience_roll_stops_at_the_fallout_new_vegas_level_cap`
  (#5925 `dc259eef9`). Fallout rewards: `LevelingModel::skill_point_reward` is the one reward query
  (`skill_points` and `grants_perk_at` delegate to it since #5925; FO3 `10 + INT`, perk every level; FNV
  `10 + INT/2`, perk every other level; FO4 `SpecialOrPerk` → `(0, perk)`; `None` for the TES models, Skyrim's
  per-level perk + `pool_pick_gain` pick banked by `grant_experience` into `LevelUpPoints::pool_picks`, #5916
  `612bd179e`) banks into `LevelUpPoints` (`character/components.rs`; the half point rides in
  `skill_point_fraction` via `grant_skill_points`; pin `skill_point_rewards_follow_each_fallout`), with INT read
  through the *Base* view (base + `set_override`, not current — #5917 `9c087f15e`) and perk hook 10 (Adjust
  Gained Skill Points) composed at the consumer, not in the ruleset. `levelup` spends skill points on the AV
  *base* and perk points only under the PERK's authored `num_ranks` (`PerkEntryTable::num_ranks`; #5924
  `c3239fd55`). `with_gmst` requests exactly `["fXPLevelUpBase","fXPLevelUpMult"]` on Skyrim and
  `["iXPBumpBase","iXPBase","iMaxCharacterLevel"]` on the Fallout curves (#5915 `8791be5fe` — the cap is
  overlaid only when authored and ≥ 1, so `BrokenSteel.esm` lifts FO3's 20 to 30; pins
  `skyrim_gmst_overlay_reads_only_authored_curve_settings`, `fallout_gmst_overlay_reads_curve_and_cap`);
  `xp_per_skill_rank` is a fixed engine coefficient *by design* (do not re-flag). **Cap raises** (#6227
  `cafd3ccab`): `CharacterRulesProfile::build_ruleset(resolve, gmst, plugin_files)` applies `LevelingModel::raise_cap` *after*
  the GMST overlay, summing `level_cap_raises` (FNV only: `DeadMoney`/`HonestHearts`/`OldWorldBlues`/`LonesomeRoad`, +5 each, matched
  on the loaded file's stem from `EsmIndex::plugin_files`; no master authors it — `iMaxCharacterLevel` is in `FalloutNV.esm` alone);
  an uncapped model stays uncapped (`raise_cap_lifts_only_a_capped_curve`). Oblivion attribute-bonus banding + 10-major-skill-ups; Skyrim pool base 100, +10/level
  (`SKYRIM_POOL_BASE`, `pool_pick_gain`); `Perks::try_set_rank` rejects out-of-range rather than clamping.
- **Regen tick**: fixed-60 Hz (`POOL_REGEN_DT`), backlog clamped (`MAX_REGEN_SUBSTEPS`), zero-dt cannot
  spin; the scheduler access declaration matches the body (`/audit-concurrency` Dim 4 owns the rule).
  Absence of `PoolRegenConfig` insertion is known-open — verify the gap, not a comment describing it.
- **Affliction** = diff-and-reapply (remove previous band's penalty before applying the new one — a
  missing removal compounds); bands half-open and consistent; `band_for` vs `band_by_key` tie-break (#4456).
- **Resistance/reputation**: `damage_multiplier` clamps `[0, cap]`, never negative (no healing);
  `STANDING_GRID` axes `[infamy][fame]` (transpose = plausible wrong standing); karma/affinity clamp both
  ends; per-faction `FactionRepThresholds` rows match the capture row-for-row.
- **Crime** (`crime.rs`): `CrimeModel::OBLIVION` / `SKYRIM` are profile rows (`crime()`; `None` on FO3/FNV/FO4/FO76/Starfield — FO3/FNV
  crime is faction-based and unmodelled), installed as a resource by `byroredux/src/crime.rs`, never a consumer `GameKind` match. Check each
  default against its cited CS/UESP line (`fCrimeGoldSteal` 0.5, attack 40, murder 1000, prison divisor 100, `rush_bounty` 500 =
  Oblivion.esm's authored `iCrimeGoldAttackMin`); `theft_rounds_up` (Oblivion up, min 1 septim; Skyrim down) and the saturating
  `as i32` casts; `with_gmst` ignores non-finite/negative values. Pins `a_theft_fines_half_the_value_rounded_up_and_never_less_than_one`,
  `skyrim_rounds_a_theft_down_and_keeps_the_bounty_per_crime_faction`. Trespass 5 / pickpocket 25 are the associated consts, used by
  `crates/scripting/src/crime_faction.rs::faction_fine` only where a faction's Crime tab does not author them.
- Every constant here needs a **Source**; regen rates and band thresholds are exactly the numbers typed
  from memory.
**Output**: `/tmp/audit/character/dim_3.md`

### Dimension 4: Population Boundary (parse → ruleset → actor) — highest yield
Paths: `byroredux/src/npc_spawn*`, `crates/plugin/src/esm/records/{actor_value_derive,actor/mod}.rs`, `crates/plugin/src/equip.rs`, `byroredux/src/inventory.rs`, `byroredux/src/cell_loader/references/`
First step: `cargo test -p byroredux --bin byroredux resolve_inherited_call_sites_are_enumerated_and_pinned; git log --since=<last report> --format='%h %cs %s' -- <Paths>`
- **One derivation, one implementation** (the class that keeps recurring): `effective_actor_level` lives
  once in `actor/mod.rs` and is `.max(0)` on the non-multiplier branch — a second copy or a `.max(1)` (also
  in `examples/`) is the regression (#3171/#4095); `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_
  multiplier` calls it through the plugin crate. TPLT template inheritance: FO4 authors a per-flag `TPTA`
  Template Actors array (`NpcRecord::template_actors: [u32; 13]`, 0 = use the `TPLT` default) and
  `walk_inherited_records` picks the flag's own entry when non-zero (#5498 `99c22a8db`; FO4-only data, no game
  branch — pinned by `crates/plugin/src/equip.rs` tests). Since #4457 the population boundary resolves
  every category once through `ResolvedNpc::resolve` (`crates/plugin/src/equip.rs`; M47.2 added the
  `base_data` terminal for "Use Base Data", `TEMPLATE_FLAG_USE_BASE_DATA`, which carries Essential/Protected —
  decoded per game in `parse_npc` into `NpcRecord::essential`/`protected` since #5921, so `ActorProtection::from_record`
  takes no game)
  and consumers read the resolved type; the guard pins production `resolve_inherited_*` calls per file (two left, both pre-spawn
  race boundaries: `cell_loader/references/mod.rs` and `player_body.rs`, the latter joined in #5047
  `c20dcdb04`, which also moved the player's level/Background/outfit reads onto the Use-Stats/Use-Traits/
  Use-Inventory terminals) — a new direct call or a raw `npc.<field>` read is the recurrence
  (fix-by-addition made it worse in #4086). Verify the enumerated count moved only deliberately, and that new reads honor `template_flags` rather than overwriting inherited values (six bits still have no consumer: #4137).
- **Base-FormID readers of Use-Traits fields** (#6228 `b76a74e45`): sex/voice/race for readers that hold a base FormID rather than a
  spawn's `ResolvedNpc` go through `equip::traits_terminal` — the dialogue voice key (`systems/dialogue_voice.rs`), `ActorBaseTraits`
  (`GetIsSex`/`GetSex`/`GetVoiceType`/`GetLeveledActorBase`, which is its own arm now, not an alias of `GetActorBase`), the gendered
  animation subgraph (`crates/scripting/src/anim_subgraph.rs`). `GetActorBase().GetSex()` deliberately stays the shell's. The
  `resolve_inherited_call_sites_are_enumerated_and_pinned` test also asserts all three call `traits_terminal(`; a new reader of
  `NpcRecord` sex/voice/race that reads the placed shell is the recurrence.
- **Writers vs stamps (silent no-op class)**: for every writer that gates on a component
  (`consume_item`/`restoration_system`/drowning gate on player `ActorValues`+`ActorVitals`), find a
  *production* insert of that component on the entity class the writer targets; unit tests hand-insert and
  hide the gap (#4458: fixed by `attach_to_player`; recurred as #5413 `212a71dba` — the player body path skips
  `stamp_spell_list`, so `RaceSpells` was absent and the scripted `AddRaceSpells`/`RemoveRaceSpells` pair no-oped
  until `attach_to_player` stamped it from `resolve_racial_spells`). The player now carries `CharacterLevel`/`Background`
  from the resolved Player record (#4678); `CharacterLevel` is saved since M47.2's kill experience (v46),
  which retired #2947's `validate_progression_state` guard; `LevelUpPoints` is saved too (`38539eee8`). The
  player's `FactionRanks` is re-derived from the record (Use-Factions terminal) on load, not saved (#5058
  `be3cd9468`); scripted edits (`FactionRanks::set_rank` / `remove`, Papyrus `SetFactionRank` /
  `RemoveFromFaction`) are instead recorded by reference FormID in the saved `ScriptedFactionChanges` ledger
  (`b3556ef6c`) and replayed on spawn and load — a direct write to `FactionRanks` that bypasses the ledger is
  lost on load.
- **Magic modifiers (#4415, closed)**: `byroredux_scripting::magic::apply_constant_modifiers` adds constant-spell
  amounts to an `ActorValues` *permanent modifier* at spawn and on `AddSpell`/`RemoveSpell`. Check it never
  writes base, that add/remove are symmetric, and that no other writer *overwrites* `permanent_mod`
  (`set_permanent` replaces the whole constant-spell sum — the pre-#5412 `SetBase` reroute did exactly that).
  Composition is `current = base + set_override + permanent + temporary − damage`. **Derived-value composition** (#5042 `c9254beb8`):
  `CharacterRuleset::actor_value` is the one composer — an authored carried base (`ActorValue::base_authored`,
  set only by `set_base`) wins; otherwise an actor-general `Absolute` row supplies the base and the entry's
  modifier + damage layers compose on top (a modifier-only placeholder entry once read as the bare modifier:
  Finesse CritChance 5). `GetActorValue` (`crates/scripting/src/condition/eval.rs`) and
  `melee_damage_charal_bonus` must both call it; a consumer reading `derived_value` or the raw entry alone is
  the regression. The composer derives its layer term as `value.current() - value.base`, never a hand-summed
  subset, so a new layer in `ActorValue::current` cannot be missed (#5706 `ad26650ec` — the derived branch had
  dropped `set_override`; pin `set_override_composes_onto_the_formula`). **Player pools** (#5039): the player's `PlayerOnly` Health/AP base is a *cache* of the
  formula, refreshed by `CharacterRuleset::refresh_player_only_bases` (allocation-free) at stamping, after
  `attach_to_player`'s constant spells, and every frame by `player_derived_stats_system`
  (`byroredux/src/systems/character.rs`, Update) — the one documented exception to computed-on-demand
  (`docs/engine/charal.md` §6); a refresh must preserve the damage, `set_override` and modifier layers.
- `build_character_ruleset` returns `None` for Oblivion/FO76/Starfield; every caller must treat `None` as
  "no CHARAL for this game", never "use the default ruleset" (Fallout formulas on a TES actor).
- `resolve` = `index.actor_value_form_id(editor_id)`: an unresolved EditorID skips its formula
  (`push_derived` resolve-or-skip), never registers one keyed on `0`.
- Ordering: base attributes + skills written before dependent derived stats; FO3/FNV class auto-calc
  `skill = 2 + 2×SPECIAL + ceil(Luck/2)` implemented, tag-skill part still *absent* not guessed.
  FO3/FNV NPC Health is the **NPC** curve, not the player row (#5238 `c71eeed80`): `NpcHealthCurve` in
  `NpcStatModel::ClassAutoCalc` = `NPC_` `DATA` Base Health + `fAVDNPCHealthEnduranceMult`·(END + offset) +
  `fAVDNPCHealthLevelMult`·(L + `level_anchor`), measured 5 / 5 / −1 / −1. `with_gmst` overlays the Endurance
  offset and the two mults only: the level anchor is an engine-hardcoded −1 (the player formula's `Level − 1`;
  no level-offset GMST exists in either exe), so a retuned Endurance offset moves only the Endurance term
  (#5340 `5ed49ed5c`, `the_level_anchor_is_engine_hardcoded_and_never_overlaid`). Pins
  `fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples` — all 15 infobox samples since #5342 `274ba13c5`, which
  also restored the player Health row beside the NPC row in the capture — and the `#[ignore]`d
  `fo3_fnv_named_npc_health_matches_the_authored_curve`. The player-curve constants (bias 90/95, END·20)
  on an NPC path are the regression. Non-auto-calc NPCs (ACBS auto-calculate bit clear, ~40% of the FO3/FNV
  census) seed their authored `NPC_` `DATA` SPECIAL (`NpcRecord::data_attributes`) instead of the class `ATTR`,
  falling back to the class for a `DATA`-less shell; skills still derive from the SPECIAL, the DNAM per-skill
  half is unparsed (#5338 `4957536ba`).
- Skyrim `derive_skyrim_actor_values`: Health/Magicka/Stamina resolve **independently** (race base +
  signed ACBS offset, own AVIF; `RaceRecord.starting_*` are `Option<f32>`) — a missing pool suppresses
  only itself. The capture's class/level term is deferred and must be disclosed in place (#4454). It also
  emits the 18 skills (`7c740eca0`, `derive_skyrim_skills`): auto-calc NPCs = `SKYRIM_BASE_SKILL` 15 + race
  bonus + the class's `skyrim_skill_weights` share of 8 points/level (`skyrim_class_skill_points`, UESP *CLAS*;
  capped at 100; skills and pools share one `distribute_by_weight::<N>` routine since #6157 `f520d8d77` — equal weights break by
  descending actor-value index in both, so a divergent second copy is the regression); a non-auto-calc NPC uses its `DNAM` `skill_values` as written (race bonus not added — the
  combination is undocumented, so absent, not guessed). `SKYRIM_SKILLS` names the AVIF editor ids; Illusion is
  `AVMysticism` (`a8dc9e8e6`) — an unresolved skill name drops silently, so the all-18 check
  `vanilla_skyrim_skill_names_all_resolve_and_a_vanilla_npc_derives_them` (`#[ignore]`d) is the guard.
  Morality is seeded from the `AIDT` Responsibility byte (`ActorAiData::responsibility`, Skyrim-only decode) by
  `derive_resolved_actor_values` unless the stat model already supplied it (#5927 `a1eece82a` — `GetBribeAmount`
  once priced every NPC at Morality 0; `skyrim_spawn_seeds_morality_from_inherited_aidt_responsibility`).
- **One ceiling composition**: `ActorValue::undamaged()` (base + set_override + permanent + temporary) is the
  single ceiling; `current()` is `undamaged() − damage`. HUD bar fractions, debug vitals and the SDK projection
  read through it (the SDK folds `set_override` into base — `ActorValueState` has no override slot); a reader that
  re-sums layers by hand is the regression (#5508 `627149b62`, `undamaged_includes_every_layer_but_damage`).
- `setav`/`modav` write the *base* component, not a derived output the next tick recomputes — except the
  player's `PlayerOnly` derived pools: `setav`/SDK `SetBase` there route through
  `CharacterRuleset::is_player_derived_pool` into the `set_override` layer (`ActorValues::set_override`, composed
  total = formula base + value + any constant-spell modifier), because `refresh_player_only_bases` would revert a
  base write in one tick (#5239 `2464a52d7`; re-layered by #5412 `a614eb273` — the first destination,
  `set_permanent`, erased carried ability bonuses and made their removal subtract twice;
  `set_override_survives_the_player_refresh_and_keeps_permanent_mod`). Sourcing per `charal.md` §6: the Oblivion
  CS SetActorValue note (FO3/FNV inherit it unverified) plus the FO4 CK `SetValue` "modifiers are left intact".
  Non-player targets and actor-general stats keep `set_base`.
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
