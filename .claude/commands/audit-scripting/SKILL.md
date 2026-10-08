---
description: "Deep audit of the M30/M47 scripting runtime — AST→ECS recognizer chain (decline-on-unmodeled), fragment/quest-stage dispatch, ECS event/timer/condition/trigger systems, cell-loader script attach, scene/package/cinematic playback, legacy ObScript quest execution (M47.3), extender-compat provider layer. Compiler frontends (.pex decompiler, .psc parser) are /audit-papyrus"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Scripting Runtime Audit (M30 / M47.0–M47.3)

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for shared protocol.

Audit `crates/scripting/` and its engine-side wiring: the AST→ECS **recognizer chain** whose load-bearing
invariant is *decline-on-any-unmodeled-term*, fragment / quest-stage dispatch, the ECS event runtime, the
cell-loader attach path, scene/package/cinematic playback, and the two legacy/compat execution layers
(compiled ObScript, Papyrus provider calls). **A partial or approximate lowering is worse than none**: an
inert unrecognized script is safe; a wrongly-lowered one corrupts game state with no fallback (the scripting
analogue of NIFAL's no-fabrication rule).

**Split**: the `.pex` reader/decompiler (`crates/pex`) and the `.psc` lexer/parser (`crates/papyrus`) are
`/audit-papyrus` (untrusted-input bounds, decompiler soundness, depth caps). `crates/hkx` parsing →
`/audit-parsers` (this skill keeps only cinematic *playback* of decoded clips, Dim 5). `crates/sdk` →
`/audit-tooling` (this skill keeps the provider/extension *seam*, Dim 7). AI-package procedure runtimes and
`systems/combat_ai.rs` → `/audit-gameplay`; scheduler/lock shape in general → `/audit-ecs`,
`/audit-concurrency`.

**Architecture**: Orchestrator; each dimension is a Task agent (max 3 concurrent).

## Scope

`crates/scripting/src/` (~45k LOC): `translate/` (`mod`, `source`, `archetype`, `compose`, `effects`, `tables`,
`recognizers/{quest_stage_gate,rumble,two_state_activator}`); `fragment.rs` + `fragment/{effects,populate,
state,systems}.rs`; `quest_stages.rs`, `globals.rs`, `vm_state.rs`, `events.rs`, `cleanup.rs`, `timer.rs`,
`recurring_update.rs`, `condition.rs`, `trigger.rs`, `player_control.rs`, `equipment.rs`, `registry.rs`;
`scene.rs` + `scene/{playback,quest_alias}.rs`, `package.rs`, `dialogue.rs`, `cinematic.rs`, `combat.rs`
(`FactionRelations`, `AiCombatState`), `magic.rs` (`SpellCatalog`/`SpellList`/`RaceSpells`, #4415), `story_manager.rs` (#5366 SM dispatch: `SmTree`, `StoryEvent`, `story_manager_dispatch_system`), `load_order.rs`
(`LoadOrderIdentity`, slot↔`PluginId` bridge); `obscript{,_runtime,_vm,_quests}.rs`; `papyrus_provider/`,
`compatibility.rs`; `papyrus_demo/` (reference scripts/test fixtures; `ScriptRegistry` is the pre-Skyrim
`SCRI`→`SCPT` extension point — boot no longer seeds it). Engine side: `byroredux/src/cell_loader/references/
{attach,synth_child}.rs`, `cell_loader/{spawn,exterior,unload}.rs`, `asset_provider/script.rs`,
`systems/{cinematic,npc_dialogue,forcegreet,story_events}.rs`, `commands/quest.rs` (`sm.event`, `dialogue.forcegreet`), `boot/schedule/`, plus `crates/plugin/src/esm/records/{index,
script_instance}.rs` (VMAD retention; decode is `/audit-esm`).

Ground truth: `docs/engine/scripting.md`, `m47-0-design.md`, `m47-2-design.md` (recognizer + "no opcode
semantics guessed"), `m47-2-recognizer-scaling.md` (corpus: handler vs fragment populations, decline-the-tail),
`m47-3-quest-alias-design.md` ("Remaining subsystem boundary" lists the bounded alias follow-ups: Created
Object fills, true `LCTN`, reference collections, unloaded-world search, injected
packages/spells/keywords overlays — real gaps, don't re-file as discoveries), crate docstrings
(`translate/mod.rs`, `fragment.rs`, `cleanup.rs` marker house rules).

**Known-open — cite, don't re-file** (verified 2026-10-08 with `gh issue view`): none owned here. #4113/#4115
are open but live in `crates/papyrus`/`crates/pex` (`/audit-papyrus`). Closed: #3817 (cinematic retention reversible, `63bf3347f`), #4415 (magic runtime incl. the racial pair, `9813af435`), #5366 Phases 0-4 (Story Manager), #4334 (once-only
persistence, `ReferenceScriptState`), #4190, #4116.
Not gaps: the M47.1 condition resolvers and fragment lowerer are implemented and live-verified; `MoveTo`
default-materialized shapes lower (#3487); the QUST VMAD property table is wired.

**Instruments**: `crates/scripting/examples/{fragment_coverage,mq101_conformance,extender_preflight,
quest_fragment_populate,dump_stage_fragment_effects}.rs` (need game data; `fragment_coverage` counts fragments through
`Effect::is_placeholder` primitives separately), `docs/smoke-tests/m47-triggers.sh` (spawn+attach gate on
Skyrim data via `--scripts-bsa`), `tests/pex_recognize_e2e.rs` (`#[ignore]`d, needs Skyrim SE data).

## Parameters / Extra Fields / Severity

`--focus` default all 7 · `--depth shallow` (API contracts + decline/bounds invariants) | `deep` (per-frame
ECS lifecycle, lock traces). Finding fields: **Dimension**: Recognizer Chain | Fragment Dispatch & Locks |
Event Runtime | Attach & Reference Identity | Scene/Package/Cinematic | Legacy ObScript | Provider/Extender Layer ·
**Untrusted-Input**: Yes | No (Yes on any path decoding mod-supplied bytes — `.pex` derivatives, `SCDA`,
provider manifests).
Escalations over `_audit-severity.md`: recognizer/effect emits on an **unmodeled** term instead of declining,
or a wrong AST is accepted → HIGH; ECS lock held across a second resource/component mutation (deadlock) →
HIGH; transient marker not drained / drained out of stage order (re-fires every frame, or a frame late) →
HIGH; panic/OOB/unbounded alloc or recursion on mod-supplied bytes → HIGH; `feature-matrix.md`/comment rot → LOW.

## Phase 1: Setup

`mkdir -p /tmp/audit/scripting`; dedup `gh issue list --repo matiaszanolli/ByroRedux --limit 300 --json number,title,state,labels > /tmp/audit/scripting/issues.json`;
read the newest `docs/audits/AUDIT_SCRIPTING_*.md` (older reports use the pre-split 8-dimension numbering:
old 5→Dim 1/2, old 6→Dims 2/3, old 7→Dim 4, old 8→Dim 5) and diff against it; read `translate/mod.rs`,
`fragment.rs`, `cleanup.rs` docstrings and `m47-2-design.md` §"Risks & mitigations" to separate designed
declines from defects. Delta-first: `git log --since=<last report> --format='%h %cs %s' -- crates/scripting byroredux/src/cell_loader/references byroredux/src/asset_provider/script.rs byroredux/src/systems/cinematic.rs`.
Run `cargo test -p byroredux-scripting` and `cargo test -p byroredux --bin byroredux boot::schedule`.

## Phase 2: Dimensions

### Dimension 1: Recognizer Chain & Effect Lowering (decline-on-unmodeled — highest yield)
Paths: `crates/scripting/src/translate/**`, `crates/scripting/src/papyrus_provider/{catalog,lower_call,lower_program}.rs` (seam)
First step: `cargo test -p byroredux-scripting -- translate::`; `git log --since=<last report> -- crates/scripting/src/translate/effects.rs`
Guard (E2): `every_effect_primitive_bounds_its_argument_count` (`effects.rs` — every `prim_*` bounds `args`,
so an over-arity call from a modded `.pex` declines) and `unrecognized_script_is_a_silent_miss`; aim at what
they cannot see:
- **Decline enforcement**: `classify_guard_atom(atom, ..)?` propagates `None` on any atom no `GUARD_PRIMITIVES`
  entry claims (no `if let Some` that drops a `None`); `split_and` deliberately does **not** split `||`, so a
  disjunction declines rather than lowering half. Flat-sequence effect model: `Stmt::ExprStmt →
  classify_effect(..)?`; locals bind only via `bind_local` (quest / object / player / plain, else decline).
  Exactly **two** control-flow exceptions: `Stmt::While` only via `lower_3d_loaded_wait` (OR-tree of
  `!<actor>.Is3DLoaded()` + exactly one positive `Utility.Wait`), and `Stmt::If` only as `Effect::Conditional`
  (no `elseif`; every atom an exact `GetStageDone == 0.0|1.0`; branches lowered against **cloned** `Scope`s;
  no `Wait`/`WaitForActors3DLoaded` inside — `has_latent`). Widening either is a decline-invariant
  regression. `lower_statements` recursion is capped (`MAX_CONDITIONAL_DEPTH` = `MAX_STMT_DEPTH`, the smaller
  upstream cap).
- **Lower only what is modeled, name it from the declared signature.** For each primitive compare against the
  engine class's real Papyrus signature (decompile the vanilla `.pex` — e.g. `faction.pex`, `game.pex`):
  parameters must carry their *declared* names (#4322 `SetInChargen` flags were invented; #4318
  `SetEnemy` lowered `abSelfIsNeutralToOther`/`abOtherIsNeutralToSelf` then discarded them, so "mutually
  neutral" was recorded as hostility). A primitive that lowers an argument and drops it, or accepts a
  non-literal for a default-only parameter, is a finding; the pattern is "accept only the literal-default form,
  decline the rest" (`MoveTo`: ≤ `MOVE_TO_MAX_ARGS`=6 with offsets `0.0`, rotation flags at defaults;
  `AddItem`: literal `abSilent`). Post-2026-09-19 primitives to hold to this: `StopCombat`, `AddSpell` (accepts
  and drops a literal `abVerbose`), `RemoveSpell` (#4414/#4415), `AddRaceSpells`/`RemoveRaceSpells` (unqualified argless call onto the player; apply re-applies/clears exactly the spawn-stamped `RaceSpells` set, a scripted `AddSpell` survives the clear, `9813af435`), `SetUnconscious` (`prim_set_unconscious`,
  `[abUnconscious = true]`, player receiver declines; #5017 `3c08cfe50`).
- **Receiver/hole binding never defaults to form-id 0**: `QuestRef::{OwningQuest, SelfRef, Property}` must
  fully resolve (`OwningQuest` needs `ctx.owning_quest`; `SelfRef` on a REFR declines); `ObjectRef` has no
  bare-receiver case (`receiver_object` rejects `self`; object locals resolve through
  `scope.object_locals`, alias-bound via `SceneActorBindings`); a receiver the runtime would *drive* must
  not be the player (`StartCombat`, #4323). `PlayerRef` (0x14) resolves inside the shared
  `resolve_entity_by_global_form_id` (#4694) — a local 0x14 special-case elsewhere is drift. `Disable`/`Enable` classify the receiver through the same
  alias-aware `receiver_object` as their siblings (an alias-bound marker must not decline alone).
- **Placeholders are coverage with a hole**: `Effect::is_placeholder` (`ShowRaceMenu`, `RequestSave`,
  `SetHudCartMode` — state nothing reads) must be reported separately by coverage harnesses (#4328/#4372);
  any *new* effect whose runtime arm only records a value needs listing there and a pin
  (`only_the_counter_only_primitives_are_placeholders`).
- **Chain order**: `RECOGNIZERS` = per-script first (`two_state_activator`, `rumble`), generic second
  (`quest_stage_gate`); first match wins, all-`None` = silent miss; a new generic recognizer must not shadow a
  per-script one. `quest_stage_gate` declines when the condition quest and the `SetStage` target disagree.
  `rumble` extracts literal property values only. Fidelity gate: `recognizes_da10_and_reproduces_hand_builder`
  (`.psc` side) **and** `da10_pex_reproduces_hand_builder_byte_for_byte` (`.pex` side, `#[ignore]`d, Skyrim SE
  data) — the `.psc` half alone never touches the decompiler; run both.
- **Provider catalog is consulted after the canonical table** (`classify_effect_with_providers`: primitives
  first, #3934 — an installed alias must never change how vanilla lowers); lowering must only accept
  provider barriers the dispatcher can serve (`servable_catalog()` gates on `callback.is_some()`;
  `every_lowering_seam_reads_the_servable_catalog` in `asset_provider/script.rs`).
- **`translate_pex` clean-`None` on bad bytes and on panic**: `catching_panics` wraps the whole
  parse→decompile→analyze→lower sequence (`a_decompile_panic_is_a_silent_none`); it cannot recover OOM/stack
  overflow (those need the bounds `/audit-papyrus` checks). `CanonicalEvent::from_papyrus` is
  case-insensitive; `Unknown` means "no consumer", never a wildcard.
**Output**: `/tmp/audit/scripting/dim_1.md`

### Dimension 2: Fragment Dispatch, Quest Stages & Lock Nesting
Paths: `crates/scripting/src/{fragment.rs,fragment/**,quest_stages.rs,globals.rs}`, `byroredux/src/boot/schedule/`
First step: `cargo test -p byroredux-scripting -- fragment:: quest_stages::`; `cargo test -p byroredux --bin byroredux boot::schedule`
Guards: `boot/schedule/mod.rs` — `scene_and_fragment_dispatch_chain_stays_in_dependency_order`,
`fragment_activation_order_tests`, `every_parallel_system_declares_everything_it_acquires`,
`papyrus_provider_system_…`/`legacy_obscript_load_order_system_declares_everything_it_acquires`,
`build_scheduler_reports_zero_access_conflicts`; and `fragment/tests.rs::{nested_lock_contract_documents_
apply_effect_itself, the_nested_lock_residual_list_names_every_type_apply_effect_acquires}` (source scans over
`fragment::SOURCES`: `apply_effect`'s doc block must name every type its per-family helpers acquire).
- **Nested-lock safety is exclusive scheduling.** `apply_effect` (now delegating to
  `apply_{global,inventory,placement,scene,lock,player_control,vehicle_cinematic,ai_combat}_effect` +
  `apply_quest_scoped_effect`, #4340) runs under one `resource_2_mut::<QuestStageState, QuestObjectiveState>()`
  taken in `apply_fragment_guard_free`; `QuestStageFragments` is cloned before any quest lock (no read→write
  nesting). Safe only because every quest-resource system is `add_exclusive` — moving
  `quest_fragment_dispatch_system`, `scene_fragment_dispatch_system`, or any sibling to the parallel lane, or
  adding a nested component/resource lock, needs the ABBA analysis re-derived (`apply_effect`'s doc block is
  the running inventory — re-read it, do not trust a count). Reverse-order acquirers (`Inventory`/`Transform`/
  `Globals` first, quest resources second) on any path are the finding.
- **`apply_effects` is an explicit cursor stack** (#3935) — no per-`Conditional` recursion or program clone; an
  unresolvable guard `QuestRef` declines the whole `Conditional` (neither branch; #3785,
  `apply_effects_declines_conditional_with_unresolvable_guard`), with a `warn!`.
- **Cascade**: `MAX_CASCADE = 64` bounds only fragment-emitted (`is_cascade`) `SetStage`s, not authored
  ingress; FIFO `VecDeque`; overflow warns; a no-op re-set (`previous == new`) does not cascade.
- **Objective transitions** (#5153): `QuestObjectiveState` mutators push a bounded `QuestObjectiveEvent` only on
  false→true flips (`objective_mutators_emit_transitions_only`); the queue is serde-skipped presentation
  (drained by the app frame into announcements) — a repeat/un-flip that emits is the regression.
- **Quest journal**: `QuestStageState` keeps a sequenced event journal (`QUEST_EVENT_RETENTION` = 16 384) with
  three fixed subscribers (`SCENE_QUEST_EVENT_SUBSCRIBER`, `FRAGMENT_QUEST_EVENT_SUBSCRIBER`, `TERMINAL_QUEST_EVENT_SUBSCRIBER`), each polling its own cursor
  (`missed_events > 0` ⇒ the subscriber must resync from canonical state). `set_stage` keeps `stages_done`
  history (`GetStageDone(37)` stays true after 40). The journal is authoritative; `QuestStageAdvancedBatch` is a
  compatibility mirror written through `push_quest_stage_advances` (one `SparseSetStorage` slot: a bare
  `insert` drops same-frame producers, #3277). A consumer that reads both must dedup the mirror as a multiset
  (as `quest_fragment_dispatch_system` does — otherwise a double advance) and must not claim (`poll`) the
  journal while it has nothing to consume (#3012); a new consumer picks a subscriber constant, not a private cursor.
- **Populate**: `populate_quest_fragments_from_pex` merges all of a stage's `Fragment_N` effect chains in VMAD
  order and *replaces* the installed chain (re-population must not duplicate); `QuestStageFragments::vmad`
  is registered so `Property`-targeted effects resolve — a decline with a populated VMAD is a regression.
  `SceneFragments` is one binding per `(scene, event)`.
- **Lock & enable ledgers** (`ReferenceLockState`, `ReferenceEnableState`, FormID-keyed, saved): a scripted
  `SetLocked`/`SetLockLevel` records the component's **full outcome** (level + key), never a partial delta that
  outranks the authored XLOC (#4329); alias-bound receivers come back to a FormID via `entity_global_form_id`;
  non-resident references still record where the ledger can hold them (#4330; `Enable`/`Disable` do). Third
  sibling `ReferenceScriptState` (#4334, save v26): fire-once triggers park their script there and the
  recognizer's spawn closure consults it so a reload does not re-arm. A "simplification" that keys any of the
  three ledgers by entity is the regression.
- `Effect::StartCombat` also removes the actor's `AmbientEngagement` (moved into `crates/scripting/src/combat.rs`
  beside `AiCombatState`) in its own guard scope, so a scripted fight is never auto-dropped by the ambient
  disengage (#5046 `fef67f3c3`); `Effect::SetUnconscious` writes `ActorControlState` (mutually exclusive with
  restrained) and ends the actor's combat — both acquisitions belong in `apply_effect`'s lock inventory.
- `Effect::AddSpell`/`RemoveSpell` apply through `magic::{add_spell,remove_spell}` (`SpellList` + permanent
  `ActorValues` modifier, add/undo symmetric — `add_and_remove_spell_apply_and_undo_permanent_modifiers`); both
  types are nested under the quest lock, so they belong in `apply_effect`'s lock inventory.
- `Effect::SetGlobalValue` writes `Globals` (saved, `resolve_property_form_id` — a GLOB is never alias-bound).
  `QuestAliasReadinessGate`: advances only when `is_running`, `stage < only_below_stage`, and
  `!get_stage_done(target)` (idempotent); one gate per quest (upsert).
**Output**: `/tmp/audit/scripting/dim_2.md`

### Dimension 3: Event Runtime — Markers, Timers, Conditions, Triggers
Paths: `crates/scripting/src/{events,cleanup,timer,recurring_update,condition,trigger,vm_state,player_control}.rs`, `crates/scripting/src/papyrus_demo/quest_advance.rs`
First step: `cargo test -p byroredux-scripting -- cleanup:: condition:: trigger:: recurring_update:: quest_advance`
Guards: `cleanup.rs::every_drained_marker_is_a_documented_pattern_a_marker`, `cleanup_removes_all_event_types`;
`boot/schedule/mod.rs::{transient_cleanup_is_the_last_late_exclusive, activation_flush_is_scheduled_before_every_activate_event_consumer, extension_event_adapters_run_before_transient_cleanup}`.
- **Marker lifetime — two sanctioned patterns** (module doc of `cleanup.rs` is authoritative): **A** = listed in
  `event_cleanup_system` for markers with no single owner (last system in `Stage::Late`); **B** = drained
  *unconditionally at the head* of the one owning consumer (no early return before the drain, or the marker
  strands and re-fires forever). A marker in neither is the bug — cross-check every `world.insert` of a
  marker type against the drain lists, and check each Pattern-B owner's first statements. **Multi-producer
  markers must merge, not overwrite** (`SparseSetStorage` = one slot per entity): `QuestStageAdvancedBatch`
  (`push_quest_stage_advances`), `HitEvent` (two producers; same-frame overwrite fixed in #4324 — re-verify
  after any new producer), `ActivateEvent` (Update-stage consumer order pinned by the flush test, which now
  lists `container_loot_system` and `mg07_on_activate_dispatch` (#4712/#4116); the Late-stage
  `npc_dialogue` selection system is a further consumer — it must stay before `event_cleanup_system`).
- **Two-phase lock discipline**: `timer_tick_system`, `trigger_detection_system`,
  `recurring_update_tick_system` collect under one `query_mut`, `drop()`, then acquire the marker-insert
  `query_mut`; two component-mut locks at once forces the TypeId-sorted contract (deadlock vector).
- **CTDA OR-precedence** (`condition.rs::evaluate`): consecutive `or_next` conditions form a block that binds
  tighter than the AND chain (`A AND B OR C AND D` = `A AND (B OR C) AND D`); empty list → `true`; guards
  `or_precedence_quirk_*`, `and_chain_short_circuits_on_first_false`. Function catalog per the module-doc
  table (indices verified against TES5Edit); `GetIsID` compares the Run-On's **base** object
  (`SceneAliasCandidate::base_form_id`), not the placed ref (#5041 `1816bbc14`,
  `get_is_id_matches_run_on_base_object_not_placed_reference`); `GetActorValue` reads through
  `CharacterRuleset::actor_value` (the shared derived+modifier composer, #5042 — `/audit-character` Dim 4); unknown functions return the documented safe default, `RunOn`
  resolution declines (condition fails) on an unresolvable target rather than defaulting to the subject; a
  wrong sentinel flips a condition. `crates/plugin/src/consumables.rs` borrows fn-586 semantics by profile
  (`/audit-character` Dim 1).
- **Triggers** (`trigger_detection_system`): edge-triggered (outside→inside); `occupant_inside: Option<bool>`
  with `None` = never checked (first tick never fires for the player — a player loaded inside a volume must
  not fire); `contains` math (Sphere `half_extents.x` radius; OBB via `rotation.inverse()`);
  `OnTriggerEnterEvent.triggerers: Vec<EntityId>` — no consumer may read `[0]` only
  (`preserves_all_actors_entering_one_volume_in_the_same_frame`); actors tracked in `TriggerOccupancyState`
  keyed `(trigger, actor)` and pruned each frame (`retain(observed)`); only tethered horses use
  `intersects_sphere` (`TETHERED_HORSE_TRIGGER_RADIUS`); `became_ready_inside` is a level condition that stops
  once the target stage is done. `actor_quest_trigger_is_in_sequence` (gate) and
  `scene_trigger_actor_approach_system_inner` (router, Dim 5) answer "which BaseForm trigger stage is next"
  separately — verify they agree via the shared `base_form_advance_is_eligible`/`scene_phase_awaited_stage`
  (#4333) on cross-quest phase waits and centerless triggers.
- `quest_advance_system` unifies `ActivateEvent` + `OnTriggerEnterEvent` on `QuestAdvanceOnActivate` with a
  three-way `ActivatorGate` (`Any`/`PlayerOnly`/`BaseForm`), evaluated per `(entity, triggerer)` pair;
  nothing may deliver both signals to one entity in one frame. `disable_reference_after_advance` (from
  `disableWhenDone`) records into `ReferenceEnableState`; both fire-once variants park in `ReferenceScriptState`
  (`fire_once_advances_park_the_reference_script_state`, #4334).
- `recurring_update_tick_system`: no fire on the registering frame/zero dt; once per interval; overshoot fires
  once; `UnregisterForUpdate` in a handler terminates cleanly. `QuestAliasReadinessGate` timing: after
  `quest_alias_refresh_system`, before `scene_playback_system`.
**Output**: `/tmp/audit/scripting/dim_3.md`

### Dimension 4: Engine Attach Path & Reference Identity
Paths: `byroredux/src/cell_loader/references/{attach,synth_child,mod}.rs`, `byroredux/src/cell_loader/{spawn,exterior,unload}.rs`, `byroredux/src/asset_provider/script.rs`, `crates/plugin/src/esm/records/index.rs`
First step: `cargo test -p byroredux --bin byroredux -- cell_loader::references reference_enable_gate`; `grep -rn 'spawn_logical_quest_reference' byroredux/src`
Untrusted-Input: Yes — `.pex` bytes come from possibly-modded archives.
- **Silent-miss everywhere**: no `--scripts-bsa`, VMAD absent, `.pex` missing (`extract_pex` → `None`), decode/
  decompile failure, recognizer miss — every branch is `continue`/`return`, never `unwrap`/`expect`.
  `--scripts-bsa` is repeatable, **first-listed-archive-hit wins** (the inverse of mod-manager priority:
  list overrides first); `pex_archive_path` normalizes to `scripts\<lowercase>.pex` (a wrong prefix/separator =
  every `.pex` misses, zero attach, no error).
- **Script sources**: `base_record_script_instance` (VMAD on ACTI/CONT/NPC/CREA, item family, statics family
  incl. STAT/MSTT/FURN/DOOR/LIGH/…, TERM; pins per family) and legacy `base_record_script` (SCRI incl. the
  statics family); REFR's own VMAD is merged by `dedup_vmad_scripts` (names differing only by case collapse
  to one attach). Verifying coverage against only the first few accessor arms re-derives a closed gap.
- **Every identity-only actor/reference stub must attach scripts.** All `spawn_logical_quest_reference`
  callers outside `synth_child.rs` — the persistent-worldspace loop in `cell_loader/exterior.rs` and
  `materialize_scene_actor_alias_stubs` in `asset_provider/script.rs` — go through
  `spawn_logical_quest_reference_with_scripts` (attaches once per `reference_form_id`, #4112/#4331; a source
  pin rejects the bare spelling). Residual (documented in the helper): a stub followed by the home cell
  streaming in leaves scripts on both — stub/real reconciliation (#2664's half) is unbuilt. New spawn paths
  that call the bare spawner are the recurrence.
- **Canonical identity stamping** (`stamp_quest_reference`: `FormIdComponent` + `SceneAliasCandidate` +
  `mark_scene_actor_bindings_dirty`) is applied at every synthetic REFR path, gated on the primary synth child
  (`synth_idx == 0`) so a SCOL/PKIN fan-out registers one candidate per authored alias; the no-mesh fallback
  still spawns a `Transform`/`GlobalTransform` entity.
- **Enable/lock gates** consult the FormID ledgers *before* content is built: `reference_is_disabled` is asked
  once per REFR ahead of every branch in `load_references_budgeted` (actor jobs, LIGH-only/fxlight placements,
  invisible triggers — #4326/#4327), `placement_is_disabled` in `spawn_placed_instances` after the root but
  before mesh/collider/light, `scripted_lock_override` at the `Locked` stamp. A disabled actor/trigger keeps
  identity + scripts but no body/volume. Regression = a spawn path that never reaches a gate, or the gate
  moved render-side (`AnimatedVisibility` is honored in static but not skinned rendering). Guard:
  `cell_loader/reference_enable_gate_tests.rs`.
- **XPRM → `TriggerVolume`** (`trigger_volume_from_primitive`): bounds are z-up **half**-extents (do not
  divide by 2), permute `[x, z, y]` with `.abs()`, REFR scale baked into a world-space volume, sphere radius in
  `half_extents.x`, shape `1 → Box`, `3 → Sphere`, else `None`
  (`trigger_volume_from_{box,sphere}_primitive_*`); invisible (MODL-less) trigger REFRs compose world space once.
- The `M47.2 scripts:` cell-load summary counters (`recognized`, `trigger volumes`) must be wired or
  `m47-triggers.sh` is vacuous; exterior stub loops report attach counts. `commands/quest.rs`
  (the `quest.*` and `scene.*` console commands, incl. `quest.effects`) stays read-only over `&World` except the explicit start/stage
  commands.
**Output**: `/tmp/audit/scripting/dim_4.md`

### Dimension 5: Scene / Package / Dialogue / Cinematic Playback
Paths: `crates/scripting/src/{scene.rs,scene/**,package.rs,dialogue.rs,cinematic.rs}`, `byroredux/src/systems/{cinematic,npc_dialogue}.rs`, `byroredux/src/cell_loader/unload.rs`, `byroredux/src/asset_provider/animation.rs`
First step: `cargo test -p byroredux-scripting -- scene:: package:: dialogue:: cinematic::`; `cargo test -p byroredux --bin byroredux -- cinematic strip_ active_tether`
The M47.2 MQ101 cart sequence is the first scripted sequence that drives *animation*, not just ECS state.
- **Scene alias substrate**: `SceneActorBindings` `(quest, alias) → EntityId` is what `resolve_object`,
  `RunOn::QuestAlias`, and `SceneShowCommand` all resolve through (one entry point — a debug view must not use
  a different path); an unfilled alias returns `None`, never fabricates an entity;
  `apply_alias_injections`/`QuestAliasInjectionState` (permanent inventory-grant ledger, saved) must not
  double-grant after load. **Refresh pre-filters must be exact supersets**: `refresh_scene_actor_bindings`
  visits per-fill `CandidateIndex` buckets and skips/filters condition-only fills via `subject_requirement`
  (`ConditionFunction::reads_run_on_entity` must stay exhaustive; `run_on_identity` is shared with the
  evaluator) — a bucket or requirement that excludes a candidate the full fill test or `evaluate` would admit silently
  unbinds an alias (`subject_requirement_never_excludes_a_passing_subject`,
  `condition_only_identity_fill_binds_what_the_full_scan_would`).
- **NPC dialogue selection** (`systems/npc_dialogue.rs`, Late exclusive): consumes the player's `ActivateEvent`
  on an NPC that `running_quests_binding_entity` names (owned DIALs via `DialRecord::quest_refs` — QSTI and,
  for Skyrim, QNAM). Candidates are **Player-category** topics with an INFO that passes `select_first_info` for
  this NPC (actor subject, player target); a qualifying **Blocking** DLBR entry opens the conversation and its
  INFO's TCLT links replace the list; otherwise the list is the **Top-Level** starting topics, and mid-branch
  children are reached only through links (#5037/#5045 `7ab87c0fb`; FO3/FNV branch-less topics without the DIAL
  Top-level flag are link-only, #5224 `305363122`). It stamps `NpcDialogueTopic` on exactly one NPC (the
  surface keys on `DialogueSurfaceState::npc`, #5038); both entry points gate on `player_can_act` and the shared
  `npc_refuses_dialogue` (dead / `AiCombatState` / unconscious), also applied by the interaction Talk arm
  (#5043, `1d31ced2b`). UI re-selection goes through `select_topic_by_form_id` and must re-check live
  ownership. **Spoken-line fragments** (#5152, `c59600138`): `DialogueInfoFragments` (TIF_ INFO VMAD, rebuilt
  from plugin data, unsaved) dispatches the line's OnBegin at selection and the outgoing line's OnEnd on
  selection change / close via `apply_spoken_info_fragment` — the same guard-free executor + journal polling
  as the quest/scene dispatchers (`the_spoken_lines_fragments_advance_the_stage`). Greetings and line
  lifetime (#5367 `14cff35ae`): `open_conversation` falls back to the master's generic greeting topic
  (`GREETING` FO3/FNV, `DialogueGenericHello` Skyrim) when no owned quest topic qualifies; `select_info`
  collects every passing candidate — a passing non-random INFO keeps priority, an all-Random pool rolls through
  the seeded `DialogueRandomState`, spoken Say-Once INFOs are disqualified through the saved
  `DialogueSpokenInfoForms` ledger (FORMAT_MAJOR 33); a spoken Goodbye closes on the presentation estimate. FO3/FNV
  INFOs honour their own owning quest (QSTI, #5271 `dbc07e8f0`). **Force-greet**
  (`systems/forcegreet.rs`, `ForceGreetDirective` bridge): FO3/FNV Dialogue procedure (`PKDD` topic on
  `PackRecord::dialogue_topic`) and the Skyrim `ForceGreet` procedure-tree leaf (#5367 `287214103`) walk the NPC
  to the player and open the topic through the same selection path without activation; vanilla installs these from
  quest scripts, so `dialogue.forcegreet` is the only installer today (smoke `dt1`/`dt2`). Every opened
  conversation raises an `AHEL` story event. Talk-candidate filtering (`populate_candidates`, `byroredux/src/interaction.rs`)
  takes each alias-resource guard alone and builds bulk sets per storage (`running_quest_bound_entities`;
  #5025 `2a1a7a362` closed two ABBA cycles, #5109 `83d52707e`) — a guard nest reintroduced there reddens the
  lock-order lane. The egui response page itself is `/audit-tooling`.
- **Story Manager** (`story_manager.rs`, `docs/engine/story-manager.md`; #5366 Phases 0-4, live gate `docs/smoke-tests/sm1-story-manager.sh`):
  `install_story_manager` folds the parsed `SMBN`/`SMEN`/`SMQN` map into an `SmTree` (children in authored `SNAM`
  chain order; a sibling cycle must still terminate — `sibling_cycle_builds_and_dispatch_terminates`).
  `story_manager_dispatch_system` (exclusive, Update; `StoryClock` synced from `GameTimeRes` first) drains the
  `StoryEvent` marker (Pattern B) and walks the event mnemonic's `SMEN` root: node CTDA via the shared M47.1
  evaluator with `RunOn::EventData` resolved over the positional R1/R2/L1/L2 slots, a passing quest node starts
  through the canonical `QuestStageState` lifecycle (never a private start path), `FromEvent` aliases fill from
  the slots via `StoryEventAliasFill` consumed by `refresh_scene_actor_bindings`. `DNAM` policies: random parents
  shuffle (`StoryManagerRng`), a processed node without Shares Event consumes the event, pools round-robin with
  `RNAM` reset windows in `StoryManagerNodeState` — the only saved SM resource (`SmTree`/cursors/clock/RNG are
  not-saved-by-design, so a quickload cannot re-fire spent radiants). Producers: `KILL` (combat death site),
  `CLOC` (`systems/story_events.rs`, LCTN-granularity key), `AHEL` (every opened conversation), `sm.event` console;
  `SCPT`/crime/crafting families have no producer (documented gaps, not findings).
- **Marker patterns** for this domain (Pattern B, drained at consumer head): `Scene{Start,Stop}Request`,
  `SceneActionCompletionBatch`, `DialoguePresentationEventBatch`, `DialogueLineCompletionBatch`,
  `ScenePackage*Batch`, `EvaluatePackageRequest`, `MotionTypeChangeRequest` (tail-drains exactly what it
  snapshotted). Pattern-A `SceneEventBatch`/`SceneFragmentInvocationBatch` feed
  `scene_fragment_dispatch_system` (Dim 2 order guard). SCEN package `Activate` leaf ordering vs `ActivateEvent`
  consumers is pinned by the schedule tests.
- **Playback lifecycle**: `idle_clip_playback_system` starts a scoped player once per serial and drains the
  request (`idle_request_starts_scoped_animation_player_once_per_serial`); `cinematic_root_motion_system` applies
  then drains the delta (`cart_exit_root_motion_moves_and_orients_actor_then_drains_delta` — an undrained
  delta launches the actor); `cinematic_animation_event_system`/`behavior_completion_events` ignore unknown
  annotations safely and a missing completion event cannot deadlock a quest stage;
  `vehicle_attachment_system`/`scripted_motion_type_system` flip motion type with the same `Keyframed`
  discipline `npc_spawn` uses (physics half → `/audit-physics`); `cinematic_horse_route_system` drives the
  tethered cart along the horse's XLKR marker chain from `PackageTargetRegistry` (markers are not render
  entities). Clip conversion (`convert_hkx_clip`: unmatched bones reported not dropped; Z-up→Y-up exactly
  once; deterministic candidate order in `idle_animation_candidates`) — decode itself is `/audit-parsers`.
- **Router vs gate**: `scene_trigger_actor_approach_system_inner` (registered via
  `make_scene_trigger_actor_approach_system`, persistent scratch) routes an offscreen actor to the lowest-stage
  reachable `BaseForm` trigger; the gate in `trigger.rs` decides whether it may fire. They must share one
  eligibility predicate (#4333) and agree on cross-quest phase waits and centerless triggers. Lock order:
  the `SceneRegistry` read guard is held across the scan and must not invert the canonical scene→quest order
  (drop before quest resources). Cinematic systems take `Transform` before `ActorCinematicState`/
  `HorseTetherState` (#4546, `docs/engine/ecs.md` canonical chain; `BYRO_LOCK_ORDER_CHECK=1` is the gate).
- **Cinematic retention** (`cinematic_retained_entities` computed once per unload batch;
  `strip_retained_cell_root(world, victims, retained)` per cell, scoped to `retained ∩ victims`): walk is
  transitive over `Children` (`active_tether_retains_horse_cart_rider_and_hierarchy`); stripping never
  orphans an entity from indexes that expect a `CellRoot`; the retention set's lifetime is bounded since
  #3817 `63bf3347f`: `cinematic_horse_route_system` releases the tether at the XLKR chain's terminal marker, and
  released entities that lost their `CellRoot` to a mid-tether unload queue on `CinematicReAdoption` (unsaved,
  session-local ids) for the streaming step to stamp onto the loaded exterior cell root containing them — un-rooted
  entities are despawn-immune, so a non-draining list is residency leak. Pinned by two lifecycle tests in
  `systems/cinematic.rs`; a release path that strips `vehicle`/`cart_seat` would break the awaited-event exit.
- Player `CinematicPresentationState`/`PlayerControlState` flags are saved; a flag a consumer never reads is
  a placeholder (Dim 1) — `disable_saving` is read by `SaveCommand`; `hud_cart_mode` had no reader at #4372.
**Output**: `/tmp/audit/scripting/dim_5.md`

### Dimension 6: Legacy ObScript Execution (Oblivion/FO3/FNV; M47.3 quest scripts)
Paths: `crates/scripting/src/{obscript,obscript_runtime,obscript_vm,obscript_quests}.rs`, `byroredux/src/cell_loader/references/attach.rs` (`attach_scpt_script`, `obscript_dialect_for`), `byroredux/src/asset_provider/script.rs` (`install_quest_scripts` call)
First step: `cargo test -p byroredux-scripting -- obscript`; `cargo test -p byroredux --bin byroredux -- attach::tests`; `git log -p --since=<last report> -- crates/scripting/src/obscript_vm.rs`
Untrusted-Input: Yes — `SCDA` bytecode comes from mod-supplied plugins. Distinct from the (unbuilt) `.psc`-side
SCTX frontend (`ScriptSource::Obscript` placeholder).
- **`obscript_vm.rs` (pure interpreter) bounds discipline**: statement framing `[op][len][payload]`,
  `0x1c` explicit-caller escapes, implicit-caller statement calls (line opcode = command id), RPN expression
  tokens, `[u16 count]` typed arguments, `If/ElseIf/Else` nesting with `Return`-inside-arm early exit,
  1-based SLSD local / SCRO reference resolution. Every `read_u16`/`checked_add`/span end must be bounds-checked
  and malformed input must yield `BlockOutcome::Malformed`, never panic (`truncated_bytecode_reports_malformed`;
  `installed_legacy_masters_have_structurally_valid_scda` is the real-data check, `#[ignore]`d). Recursion is
  capped by `MAX_OBSCRIPT_VM_NESTING` = 32 on both `exec_if_chain` ↔ `run_arm_body` and nested `X` argument
  decode (`decode_args_at_depth`); guards `excessive_nested_if_chains_are_malformed`,
  `excessive_nested_x_arguments_are_rejected` — a new recursive path that skips the depth parameter is the
  regression. The operand stack in
  `eval_expr`/`apply_operator` must not grow unbounded from a lying token count.
- **Command IDs are empirical** (aligned SCTX↔compiled stream on vanilla `Oblivion.esm`; 2 349/2 393 scripts
  decode): `SetStage` 0x1039, `GetStage` 0x103a, `GetStageDone` 0x103b, `StartQuest`/`StopQuest` 0x1036/0x1037,
  `GetQuestRunning` 0x1038, `Message`/`MessageBox`, `GetSecondsPassed`, `GetButtonPressed` (−1). Anything else
  is a **traced no-op returning 0, counted per quest in `ObScriptQuestTimers.unknown_commands`, never faked**
  — a new command that
  returns a guessed value is a finding (*feedback_no_guessing*); dialect tables (xNVSE vs xOBSE opcode
  numbering, `ObscriptDialect`) stay separate because the same number names unrelated commands.
- **`obscript_quest_tick_system`** (exclusive `Stage::Update`, vanilla `fQuestProcessInterval` 5 s cadence):
  runs only *running* quests' GameMode block through `QuestCommandHost`, which lowers onto the same
  `QuestStageState`/`Globals` and emits the same stage events as the fragment path (so Dim 2's journal and
  cascade rules apply). Timers/effect ring (`ObScriptQuestTimers`, `ObScriptEffectLog`, cap 64) are unsaved by
  design — cadence restarts on load and script locals are re-executed fresh per run (documented M47.3
  residual, not silent save loss); `StartQuest`/`StopQuest` attach/detach the script loop consistently.
  Phase-2 gaps (object-script blocks, Message UI, per-quest quest-delay time, actor-state functions) are
  documented in ROADMAP M47.3 — not new findings. The 255-script corpus gate in `obscript_quests.rs`
  (`#[ignore]`d) now resolves Oblivion.esm via `test_paths::oblivion_esm()`, panics under
  `BYROREDUX_REQUIRE_GAME_DATA=1` instead of returning green, and runs in `.github/workflows/real-data-gates.yml`
  (#5088 `1377d3463`).
- **Legacy load-order/extender path**: `attach_scpt_script` resolves SCPT via `ScriptRegistry` (logs an error
  when the resource is absent); dialect selection is by *profile row*, pinned by
  `obscript_dialect_follows_the_profile_not_the_game_kind`; source-less FNV/FO3 bytecode is decoded
  structurally (`decode_extender_calls`, only registered xNVSE/xOBSE opcodes) and FO3 must not apply the
  xNVSE table (`source_less_fo3_scpt_does_not_apply_xnvse_opcode_table`). Arguments/bytes obey the SDK caps
  (`MAX_SCRIPT_CALL_BYTES`, `MAX_SCRIPT_STRING_BYTES`, `MAX_LEGACY_OBSCRIPT_NESTING`).
**Output**: `/tmp/audit/scripting/dim_6.md`

### Dimension 7: Provider / Extender-Compatibility Layer (SKSE, JContainers, StorageUtil, ModEvent seam)
Paths: `crates/scripting/src/{papyrus_provider/**,compatibility.rs}`, `crates/scripting/tests/extender_compat_e2e.rs`, `byroredux/src/extensions/`, `crates/pex/src/call_sites.rs` (consumer)
First step: `cargo test -p byroredux-scripting -- papyrus_provider compatibility`; `cargo test -p byroredux --bin byroredux -- boot::schedule::system_access_declaration_tests`
This layer (~10k LOC scripting-side incl. tests) was a reported coverage gap through 2026-09-14 (only its *seams* were
audited). Its SDK half (`crates/sdk`: StorageUtil/JContainers verbs, typed script values, limits, manifests)
is `/audit-tooling`; audit here the runtime that consumes it.
- **Untrusted-input**: preflight/lowering run on `.pex`-derived data and provider manifests. `compatibility`
  caps its script table (`MAX_COMPATIBILITY_SCRIPTS`, `truncated()` must surface); lowering recursion is capped
  (`MAX_PROVIDER_HANDLER_NESTING` = 32, `MAX_PROVIDER_FRAGMENT_BARRIERS` = 64 — cap-before-descend); value sizes
  obey `MAX_SCRIPT_ARRAY_ELEMENTS`/`MAX_SCRIPT_CALL_BYTES`/`MAX_PENDING_PAPYRUS_MOD_EVENTS`. `execute.rs` has
  `unreachable!` arms guarded by "validated …" invariants — each must be truly established by lowering.
  The `catching_panics` net (#3948) wraps the translate/lower path; check whether runtime execution in
  `papyrus_provider_system` has any equivalent (none found 2026-10-05: no `catch_unwind` under `papyrus_provider/`).
- **Decline-on-unmodeled at the seam**: a provider call that cannot be typed declines the whole fragment/
  handler (`lower_provider_call` errors → `None`), it never lowers a prefix and silently drops the tail; a
  `PapyrusProviderRuntime::default()` with a non-empty catalog but `callback: None` is not servable (Dim 1).
- **Continuation persistence**: `PapyrusProviderContinuationQueue` and `FragmentExecutionQueue` are saved
  resources; continuations must stay principal-owned and typed (no raw closures or process-local handles).
  Locals holding `ScriptValue::Entity(EntityRef)` are session-scoped (`world_generation`) — they are dropped
  after load by `drop_entity_bound_continuations` (#4139; `/audit-save` Dim 5 owns the ordering); any new
  saved shape here is a `FORMAT_MAJOR` question (`/audit-save` Dim 2).
- **Lock/access**: `papyrus_provider_system` and `legacy_obscript_load_order_system` declare what they acquire
  (the schedule tests scan bodies to 3 call hops); `extension_*` adapters run before `event_cleanup_system`
  (`extension_event_adapters_run_before_transient_cleanup`). Any new SDK-state exposure that nests a resource
  lock against `QuestStageState`/`Inventory`/etc. is a Dim 2-class finding.
- **Provider resolvers/callbacks** (`set_papyrus_provider_{runtime,entity_resolver,form_resolver,
  mod_event_publisher}`, `set_extension_script_function_invoker`, `set_legacy_obscript_content_catalog`) are
  installed by boot/extension setup; an uninstalled resolver must degrade to a decline, not a panic.
- Census/instrument: `crates/scripting/examples/extender_preflight.rs`; docs: `feature-matrix.md`/ROADMAP M47.2
  rows must acknowledge this layer (SCR-ORCH-2026-09-06-01).
**Output**: `/tmp/audit/scripting/dim_7.md`

## Phase 3: Merge

Combine `/tmp/audit/scripting/dim_*.md` into `docs/audits/AUDIT_SCRIPTING_<TODAY>.md`: **Executive Summary**
(shipped vs deferred; findings by severity; **untrusted-input verdict** for the runtime-side decoders —
`SCDA`, provider manifests — must be NO panic/OOB/OOM/overflow; frontends' verdict lives in the newest
`AUDIT_PAPYRUS_*`) · **Decline-Invariant Audit** (every recognizer/composer/effect decline point ×
verified-conservative vs leaks-a-partial-lowering) · **Runtime Lifecycle Matrix** (marker drain coverage;
two-phase lock-drop per system; cascade bound; edge-trigger seed; CTDA OR-precedence; journal subscribers) ·
**Findings** (CRITICAL first, deduplicated) · **Future-Phase Readiness** (invariants pinned for ObScript
phase 2, alias follow-ups). Dedup ownership: marker-drain coverage = Dim 3 (pointers from others);
`translate_pex` clean-`None` = Dim 1 (pointer from Dim 4); half-extent convention = Dim 4; cinematic router/
gate agreement = Dim 5 (pointer from Dim 3).

## Phase 4: Cleanup

`rm -rf /tmp/audit/scripting`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_SCRIPTING_<TODAY>.md` (domain label `scripting`; add `quests` for
QUST/alias findings and the matching `game:*` for one title's scripts).
