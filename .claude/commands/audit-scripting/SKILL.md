---
description: "Deep audit of the M30/M47 scripting runtime — AST→ECS recognizer chain (decline-on-unmodeled), the M47.2 Papyrus VM that runs what the lowering declines (budget, reference/alias scripts, update/event registrations), fragment/quest-stage dispatch, ECS event/timer/condition/trigger systems, cell-loader script attach, scene/package/cinematic playback, legacy ObScript quest execution (M47.3), extender-compat provider layer. Compiler frontends (.pex decompiler, .psc parser) are /audit-papyrus"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Scripting Runtime Audit (M30 / M47.0–M47.3)

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for shared protocol.

Audit `crates/scripting/` and its engine-side wiring: the AST→ECS **recognizer chain** whose load-bearing
invariant is *decline-on-any-unmodeled-term*, the **Papyrus VM** (`fragment/vm/`, M47.2) that interprets the
fragments / reference / alias scripts the lowering declines — its own contract is *check ahead of time, run whole
or not at all* (`check_function` / `check_reference_script`; a native that declines its actual values stops the
run, it never guesses) — fragment / quest-stage dispatch, the ECS event runtime, the cell-loader attach path,
scene/package/cinematic playback, and the two legacy/compat execution layers (compiled ObScript, Papyrus
provider calls). **A partial or approximate lowering is worse than none**: an inert unrecognized script is safe;
a wrongly-lowered one corrupts game state with no fallback (the scripting analogue of NIFAL's no-fabrication rule).
Both fragment paths (lowered `Effect` list, VM `Effect::Interpret`) end in the same `apply_effects` runtime, so
Dim 2's lock rules cover both.

**Split**: the `.pex` reader/decompiler (`crates/pex`) and the `.psc` lexer/parser (`crates/papyrus`) are
`/audit-papyrus` (untrusted-input bounds, decompiler soundness, depth caps). `crates/hkx` parsing →
`/audit-parsers` (this skill keeps only cinematic *playback* of decoded clips, Dim 5). `crates/sdk` →
`/audit-tooling` (this skill keeps the provider/extension *seam*, Dim 7). AI-package procedure runtimes and
`systems/combat_ai.rs` → `/audit-gameplay`; scheduler/lock shape in general → `/audit-ecs`,
`/audit-concurrency`.

**Architecture**: Orchestrator; each dimension is a Task agent (max 3 concurrent).

## Scope

`crates/scripting/src/` (~118k LOC incl. tests; re-measure with `find crates/scripting/src -name '*.rs' | xargs wc -l`): `translate/` (`mod`, `source`, `archetype`, `compose`, `effects`, `tables`,
`recognizers/{quest_stage_gate,rumble,two_state_activator}`); `fragment.rs` + `fragment/{effects,populate,
state,systems}.rs` with the per-family appliers in `fragment/effects/{resolve,deferred,inventory,placement,scene,lock,
player_control,vehicle_cinematic,ai_combat,quest_scoped,evaluate,guards,quest_vars,actor_values,actor_lists,
location_alias,reference_state,player_stats,messages}.rs` (#5468 split; #6142 `b269744f2` moved the older inline
`apply_effect` arms into the last seven and `evaluate_guard`'s into `guards.rs`); the **Papyrus VM family**
(M47.2): `fragment/vm/` (`interp.rs` the interpreter, `checker.rs` + `natives/` the `check_*` ahead-of-time checks; `vm.rs` is a ~136-LOC hub), `fragment/budget.rs` (per-frame
statement budget, #5763), `fragment/dispatch.rs` (`run_papyrus_call` / `flush_advances`, the one budget→defer→apply
tail every Papyrus-call system shares, #5797 `e27c93a97`), `fragment/{reference,alias_scripts,owners}.rs` (reference / alias script instances and
event delivery), `fragment/{updates,distance,events,hits,anim_events}.rs` (registration families),
`fragment/{package,perk,terminal}.rs` (PACK/PERK/TERM fragments); `quest_stages.rs`, `globals.rs`, `vm_state.rs`,
`events.rs`, `cleanup.rs`, `timer.rs`, `recurring_update.rs`, `condition/`, `trigger.rs`, `player_control.rs`,
`equipment.rs`, `registry.rs`; `scene.rs` + `scene/{playback,quest_alias}.rs`, `package.rs`, `dialogue.rs`
(incl. `DialogueSurfaceState`, `DialogueQuestConditions`), `cinematic.rs`, `combat.rs`
(`FactionRelations`, `AiCombatState`), `magic.rs` (`SpellCatalog`/`SpellList`/`RaceSpells`, #4415), `story_manager.rs` (#5366 SM dispatch: `SmTree`, `StoryEvent`, `story_manager_dispatch_system`), `load_order.rs`
(`LoadOrderIdentity`, slot↔`PluginId` bridge); `animation_graph.rs` (placed-object behavior graphs); the M47.2
**native-state stores** the VM's primitives write and conditions read — one small saved module each (CK page quoted
in its module doc; each is a `FORMAT_MAJOR` question for `/audit-save`): `quest_variables.rs`, `location.rs`,
`bleedout.rs`, `messages.rs`, `keywords.rs`, `relationships.rs`, `faction_changes.rs`, `alias_overrides.rs`,
`alias_collections.rs`, `created_references.rs`, `perk_entry_points.rs`, `destruction.rs` and the sibling
`activate_parents`/`crime_*`/`jail`/`guards`/`followers`/`map_markers`/`menus`/`ownership`/`scanning`/`skills`/`speechcraft`/
`*_requests` files (subsystems migrated by #5802 export their own `install(world)` + `register_saved(registry)`;
`build_save_registry` calls each at its old row position — the schema fingerprint is order-sensitive); the M47.3 ObScript family
`obscript{,_runtime,_vm,_quests,_results,_objects,_cmd,_effects,_actions,_actor_values,_anim,_speech,_state,_ledger,_more}.rs`
+ `obscript_more/` with its sibling runtimes `magic_effects.rs`
(caster runtime), `magic_shaders.rs`, `script_packages.rs`, `challenges.rs` (Dim 6); `registration.rs` (the crate's
`register`, #5806); `papyrus_provider/`,
`compatibility.rs`; `papyrus_demo/` (reference scripts/test fixtures; `ScriptRegistry` is the pre-Skyrim
`SCRI`→`SCPT` extension point — boot no longer seeds it). Engine side: `byroredux/src/cell_loader/references/
{attach,synth_child}.rs`, `cell_loader/{spawn,exterior,unload,created,graph_nodes}.rs` + `cell_loader/exterior/{persistent,apply_job}.rs` (#6271 split of the exterior hub; `created.rs` places/spawns
Created Object "At" references; `graph_nodes.rs` keeps the NiNode tree of graph-animated props), `asset_provider/script.rs`
(`attach_vm_reference_script`, `check_running_quest_alias_scripts`, `ProviderScriptLibrary`, `DecompiledPexCache`),
`asset_provider/{object_graph,actor_graph}.rs`,
`systems/{cinematic,npc_dialogue,forcegreet,story_events,scripted_sound,trap_hits,projectiles,archetype_idle}.rs`
(consumers of the VM's request markers) and the M47.3 ObScript request consumers
`systems/{script_requests,script_queries,revive,knockdown,flee,magic_shader,head_look}.rs`, `commands/quest.rs` (`sm.event`, `dialogue.forcegreet`), `boot/schedule/`, plus `crates/plugin/src/esm/records/{index,
script_instance}.rs` (VMAD retention; decode is `/audit-esm`).

Ground truth: `docs/engine/scripting.md` (the M47.2 sections — "The Papyrus VM", "The frame budget", "Reference
scripts on the VM", the registration families — carry each primitive's CK-page contract and its "Not modelled"
list; those lists are documented gaps, not findings), `story-manager.md`, `m47-0-design.md`, `m47-2-design.md` (recognizer + "no opcode
semantics guessed"), `m47-2-recognizer-scaling.md` (corpus: handler vs fragment populations, decline-the-tail),
`m47-3-quest-alias-design.md` (its "Remaining subsystem boundary" predates M47.2: Created Object "At"/"In"
fills, the `LCTN` runtime + location aliases, `FromEvent` fills and script-filled `RefCollectionAlias`es have
since shipped; still real gaps — world-query search across unloaded data, condition/fill-type collection fills,
injected packages/spells/keywords/names/voice-type/combat overlays — don't re-file as discoveries), crate docstrings
(`translate/mod.rs`, `fragment.rs`, `fragment/vm/`, `fragment/budget.rs`, `cleanup.rs` marker house rules).

**Known-open — cite, don't re-file** (verified 2026-10-27 with `gh issue view`): #5764 (interpreter deferral ~0.8 µs a delivery; a value-called function or an
array/struct-holding frame cannot suspend — the budget-split residual; its idle-walk half shipped as `11685934d`,
Dim 2),
#5586 (`examples/mq101_conformance/::run` oversized — debt, not
runtime), #5802 (hub-file growth; subsystems are migrating to their own `install` + `register_saved`, sliced in
`bde3478db`/`e619dbbaf` — `/audit-tech-debt`). #4113 is open but lives in `crates/papyrus`/`crates/pex`
(`/audit-papyrus`). Closed since the 2026-10-24 sync: #5646 (`GetRandomPercent` drew `pick(101)`, 0..=100; the CK/GECK say
0-99 — now `pick(100)`, pinned by `get_random_percent_is_an_integer_in_zero_to_ninety_nine`, `6701086fd`), #5647 (the
one-slot `StoryEvent` marker lost a second same-frame event on one entity — all three engine producers, `CLOC`/`AHEL`/`KILL`,
now go through `story_manager::raise_story_event`: the first event on an entity takes the marker, a later one queues on
`PendingScriptStoryEvents`; `two_events_on_one_entity_in_a_frame_both_dispatch`, `eca0c4709`), #6139 (the ObScript
command chain became the `obscript_cmd.rs` table and `ObScriptCommandHost::call` was split, `bd4cabf81`..`d5d13edcc`),
#6142 (`apply_effect` / `evaluate_guard` / `check_call` / `guard_reading` / `call_statement` split by family,
`b269744f2`), #6221 (`GetIsPlayerBirthsign`, `GetAnimAction` and every `IsPlayerActionActive` index but the tracked
jump (6) return `Action::Declined` instead of a constant, `c1b2b1d12`,
`untracked_state_declines_instead_of_answering_a_constant`), #6226/#6281 (script sounds go through the bounded
voice cache, skip decode with no device and honour SNDD attenuation and the 2D/Menu bits; ObScript `PlaySound` has no
source reference — `NON_LOCATIONAL_SOURCE` — and is heard at the `AudioListener`, the player body only when no
listener exists, `f04c3433d`), #6333/#6335 (per-frame cost: `collect_due` asks the stub question per due instance,
`4f6cac254`; `StoryLocationMemo`, `80471eb78`), #6338 (`resolve_entity_by_global_form_id` answers from the
`FormIdPool` generation-keyed index `entity_with_local`, `ab3a604bf`), #6365 (effect handlers open through
`live_caller` / `open_actor_effects`, `bf27df84c`, `handlers_open_through_the_live_caller_helpers`), #6287 (the scalar
`npc_refuses_dialogue` agrees with the bulk gather, `aa1016f3c`), #6286 (the alias inventory grant folds into the
form's row, `f3c8461fc`). Closed since the 2026-10-21 sync: #5750 (`GetIsVoiceType` catalogued — 426 Skyrim/FO4, 427 FO3/FNV, each foreign in
the other family; true for the speaker's VTCK voice type or any member of an FLST of them, the leveled base's terminal
traits via #6228 `b76a74e45` — plus `IsCommandedActor` 700, a documented 0.0 since the engine has no summon/reanimate,
so Skyrim's Hello greeting selects by voice again — `20d01cd6c`) and its FO3/FNV sibling #6251 (ten functions the
ObScript getters already answered catalogued so the fail-closed gates stop declining them — `f57c75616`). Closed since the 2026-10-20 sync: #5778 (Oblivion's quest-level dialogue gates catalogued —
254 `GetIsPlayableRace`, 365 `GetPlayerInSEWorld` (Oblivion-only, foreign elsewhere), 70 `GetIsSex`, 310
`GetInWorldspace`; the census now declines 1 quest / 9 INFOs, the one gated by uncatalogued fn 313 — `71e488a26`).
Closed since the 2026-10-15 sync: #5682 (the menu entry carries the INFO that admitted it and selection reuses it, so a fn-77 gate is
rolled once — `8daa24105`, `selected_info_is_not_re_evaluated_after_menu_filtering`; the click half #6007
`f7c8f02a7`, `a_topic_click_speaks_the_info_the_menu_offered`), #5752 (`select_info` declines an INFO whose CTDA
set holds any term `condition::is_modeled` rejects instead of evaluating it against `0.0` — `bd85de5ea`,
`uncatalogued_info_condition_is_declined_instead_of_matching_zero`), the Dim 6 ObScript follow-ups #5787 (`a5e5346d0`,
tests below), #5788 (module doc refreshed, `fecdd043a`), #5789 (`%0.f` precision, `1502fbd3d`), #5790
(`GetButtonPressed` answer global, `173f9e155`). Closed earlier: #5608
(the `ScriptTimer`/`TimerExpired` pair has no production producer or consumer — docs made honest, `bef345396`;
FO4 `OnTimer` rides `fragment/updates.rs`), #5681 (fragment `Utility.Wait` capped at `MAX_PROVIDER_WAIT_SECONDS`,
`FragmentExecutionQueue` capped at `MAX_FRAGMENT_CONTINUATIONS` = 4 096, `ef130cd6c`), #5734 (quest-level dialogue
conditions gate their quest's INFOs, `6709eb65e`), #5763 (interpreter frame budget + `ReferenceScriptIndex`,
`195e3b4ef`), #3817 (cinematic retention reversible, `63bf3347f`), #4415 (magic runtime incl. the racial pair, `9813af435`), #5366 (Story Manager, Phases 0-4 all shipped), #5367 (dialogue
trees: greeting/line lifetime/FO3-FNV force-greet `14cff35ae`, Skyrim force-greet `287214103`) and its #5397
Random-stack follow-up (`0436bea5c`), #4334 (once-only persistence, `ReferenceScriptState`), #4190, #4116.
Not gaps: the M47.1 condition resolvers and fragment lowerer are implemented and live-verified; `MoveTo`
default-materialized shapes lower (#3487); the QUST VMAD property table is wired; a Papyrus interpreter exists
(`fragment/vm/`) — "no VM" findings are stale; structs are modelled on the VM for Fallout 4 (the VM header's "Not
modelled: structs" line is stale, `Value::Struct` exists).

**Instruments**: `crates/scripting/examples/{fragment_coverage,mq101_conformance,extender_preflight,
quest_fragment_populate,dump_stage_fragment_effects,refr_script_coverage,alias_script_coverage,refr_script_props,
terminal_fragment_census,cndf_census,sm_tree_dump,dump_pex_script,obscript_tools}.rs` (need game data; `fragment_coverage` counts fragments through
`Effect::contains_placeholder` primitives separately; `refr_script_coverage` with `VM_REASONS=1` lists why the VM declines
a reference script; `sm_tree_dump` prints a Story Manager event's tree in evaluation order; `obscript_tools
names <ESM> all` measures ObScript call-site coverage, `census`/`avcodes` re-derive block and actor-value ids),
`cargo test -p byroredux-scripting --release --lib -- --ignored --nocapture vm_cost` (interpreter cost bench,
`fragment/vm/tests.rs`), `docs/smoke-tests/m47-triggers.sh` (spawn+attach gate on
Skyrim data via `--scripts-bsa`), `docs/smoke-tests/m47-3-obscript.sh` (ObScript object blocks live on FNV), `tests/{pex_recognize_e2e,location_aliases_real_data,perk_entry_points_real_data}.rs`
(`#[ignore]`d, need game data).

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
declines from defects. Delta-first: `git log --since=<last report> --format='%h %cs %s' -- crates/scripting byroredux/src/cell_loader/references byroredux/src/asset_provider/script.rs byroredux/src/systems/cinematic.rs byroredux/src/systems/npc_dialogue.rs byroredux/src/systems/forcegreet.rs byroredux/src/systems/story_events.rs`.
Run `cargo test -p byroredux-scripting` and `cargo test -p byroredux --bin byroredux boot::schedule`.

## Phase 2: Dimensions

### Dimension 1: Recognizer Chain & Effect Lowering (decline-on-unmodeled — highest yield)
Paths: `crates/scripting/src/translate/**`, `crates/scripting/src/papyrus_provider/{catalog,lower_call,lower_program}.rs` (seam)
First step: `cargo test -p byroredux-scripting -- translate::`; `git log --since=<last report> -- crates/scripting/src/translate/` (`effects.rs` is a thin hub over `translate/effects/`; never scope to the hub file)
Guard (E2): `every_effect_primitive_bounds_its_argument_count` (`translate/effects/tests.rs` — every `prim_*` bounds `args`
with a real *upper* bound or a real call to a bounded helper, comments stripped, row-for-row against the table, #5977
`51f30c95c`; so an over-arity call from a modded `.pex` declines) and `unrecognized_script_is_a_silent_miss`; aim at what
they cannot see:
- **Decline enforcement — two guard paths.** *Handler recognizers* (`compose.rs`): `classify_guard_atom(atom, ..)?`
  propagates `None` on any atom no `GUARD_PRIMITIVES` entry claims (no `if let Some` that drops a `None`);
  `split_and` splits `&&` only, so a `||` declines rather than lowering half
  (`split_and_flattens_conjunction_keeps_disjunction_whole`). *Fragment lowerer* (`translate/effects/lower.rs`, guards in `translate/effects/guards.rs`):
  `classify_condition` folds an `If`/`While` condition into `FragmentGuard`s — quest/objective/global/quest-variable
  reads, actor/scene/location/map-marker atoms, run-time `Compare`, and (M47.2) `Any` for `||` (`A || (B && C)`;
  a negated `||` is a conjunction, a nested disjunction flattens). Any atom no guard claims returns `None` and the
  whole fragment declines (then Dim 2's VM check gets a chance — a decline here is never a partial lowering).
  Flat-sequence effect model: `Stmt::ExprStmt → classify_effect_with_providers(..)`, then the quest-script inline /
  `quest_vmad_statement` fallbacks, else decline; locals bind only via `bind_local` (quest / object / player /
  plain, else decline); a named call argument declines instead of binding positionally (#5068). Control flow is
  closed to: `Stmt::While` only via `lower_3d_loaded_wait` (OR-tree of `!<actor>.Is3DLoaded()` + exactly one
  positive `Utility.Wait`) or `lower_wait_while` (any modelled guard + exactly one positive literal `Wait` →
  `Effect::WaitWhile`), and `Stmt::If` only as `Effect::Conditional` (`ElseIf` chains nest as the else branch,
  bounded by the same depth cap; branches lowered against **cloned** `Scope`s, and a branch that re-binds a local
  declared before the `If` declines — `Scope::rebound_by`, #5632 `ed6998c51`; a branch may wait since the cursor
  stack suspends mid-branch, but `has_latent_tail` still refuses a latent effect where nothing can resume it —
  quest scope, speaker line, run-time locals). Bare `Return` lowers only as the body's final
  statement at depth 0 (#5631), and an assignment to a bare identifier that is no declared local declines
  (#5638, `rebinding_a_pre_branch_local_inside_a_branch_declines`, `assignment_to_a_script_variable_declines`)
  — except where the VMAD is the quest's own and the name is one of its script variables
  (`vmad_is_quest && is_script_variable`, M47.2; only Bool/Int/Float declarations count — a non-numeric one
  declines rather than reading `0.0`, #5978 `e802d1705`). A compound assignment (`n += 2`) to a declared local
  declines (#5976 `686885652`); Int `/` lowers only when an operand is provably Float and `SetValueInt` only on an
  integral value (#5974 `4cf0e087f` — `ValueExpr` arithmetic is untyped f32); a truncating global read
  (`GetValueInt`, `GetValue() as Int`) lowers in a guard only as exact bounds on the stored float and declines in a
  value, where the VM runs it (#6215 `a16053fbb`). Widening any of these is a decline-invariant
  regression. `lower_statements` recursion is capped (`MAX_CONDITIONAL_DEPTH` = `MAX_STMT_DEPTH`, the smaller
  upstream cap), and one fragment's lowering across inlined quest-script callees shares `MAX_LOWERED_STATEMENTS`
  = 10 000 (#5970 `07f2a6561` — `MAX_INLINE_DEPTH` alone let N^4 expansion through).
- **Lower only what is modeled, name it from the declared signature.** For each primitive compare against the
  engine class's real Papyrus signature (decompile the vanilla `.pex` — e.g. `faction.pex`, `game.pex`):
  parameters must carry their *declared* names (#4322 `SetInChargen` flags were invented; #4318
  `SetEnemy` lowered `abSelfIsNeutralToOther`/`abOtherIsNeutralToSelf` then discarded them, so "mutually
  neutral" was recorded as hostility). A primitive that lowers an argument and drops it, or accepts a
  non-literal for a default-only parameter, is a finding; the pattern is "accept only the literal-default form,
  decline the rest" (`MoveTo`: ≤ `MOVE_TO_MAX_ARGS`=6; the three offsets and `abMatchRotation` must be literals and
  are *carried* into `Effect::MoveTo` since `24755f1d7` — the player's teleport still declines a non-zero offset /
  `!match_rotation`, and Starfield `abRotateOffset = true` declines; `AddItem`: literal `abSilent`, parsed and not
  applied). The M47.2 primitive wave (~60 `M47.2:` commits adding `prim_*` / `classify_native_*` pairs) quotes the
  CK page in each module doc — an argument dropped or defaulted beyond what that page says is the finding.
  Post-2026-09-19 primitives to hold to this: `StopCombat`, `AddSpell` (accepts
  and drops a literal `abVerbose`), `RemoveSpell` (#4414/#4415), `AddRaceSpells`/`RemoveRaceSpells` (unqualified argless call onto the player; apply re-applies/clears exactly the stamped `RaceSpells` set, a scripted `AddSpell` survives the clear, `9813af435`; the player's own stamp is `attach_to_player`'s, #5413 `212a71dba` — before it both were silent no-ops on the only actor they target), `SetUnconscious` (`prim_set_unconscious`,
  `[abUnconscious = true]`, player receiver declines; #5017 `3c08cfe50`). Also held to it since 2026-10-10:
  `Lock` takes the compiled two-parameter shape (`abLock`, literal `abAsOwner` parsed and not applied; a
  non-literal or third argument declines, #5653 `2ce808553`), `SetObjectiveDisplayed` parses `abForce` through
  `bool_arg` (#5652 `9777c41a2`), `Enable`/`DisablePlayerControls` fill absent arguments from their *own*
  default tables (`ENABLE_PLAYER_CONTROLS_DEFAULT` vs `DISABLE_PLAYER_CONTROLS_DEFAULT`, #5651 `5e2fc3c8e`), and
  `SetMotionType` integer literals lower only where every game agrees (just `1` = Dynamic; Skyrim/FO4 constants
  differ, the named `Motion_*` property read still lowers; #5633 `92a1782fe`,
  `literal_motion_types_lower_only_where_every_game_agrees`; the game-aware VM accepts Skyrim's raw 1/4/5/7 via
  `ScriptGame` and stops the run on the inertia variants and other games' numbers, `2517782fc`). Since 2026-10-17:
  Skyrim's `DamageActorValue`/`RestoreActorValue` (and `*AV`) lower the amount's magnitude — a literal folded, a
  run-time amount through `ValueExpr::Abs` (CK: negatives convert to positive; FO4/Starfield `*Value` keep the sign,
  #5973 `3f32efc04`); `SetEssential`/`SetProtected` lower only on an ActorBase receiver, FO4/Starfield's per-instance
  Actor/alias spellings decline to the VM (#5972 `40345ea0d`); the set-stage trigger recognizers fall back to the
  script's declared bool initializers for omitted VMAD properties (#5969 `f417c3f64`).
- **Receiver/hole binding never defaults to form-id 0**: `QuestRef::{OwningQuest, SelfRef, Property}` must
  fully resolve (`OwningQuest` needs `ctx.owning_quest` and lowers only from `Self.GetOwningQuest()` — an
  alias property's `GetOwningQuest()` names another quest, #5634 `221befd61`; `SelfRef` on a REFR declines); `ObjectRef` has no
  bare-receiver case (`receiver_object` rejects `self`; object locals resolve through
  `scope.object_locals`, alias-bound via `SceneActorBindings`); a receiver the runtime would *drive* must
  not be the player (`StartCombat`, #4323). `PlayerRef` (0x14) resolves inside the shared
  `resolve_entity_by_global_form_id` (#4694; every other id answers from `FormIdPool::entity_with_local`, a
  generation-keyed index rebuilt only when `FormIdComponent` membership moves, first duplicate wins as the old scan did,
  #6338 `ab3a604bf`) and its inverse `entity_global_form_id` returns 0x14 for the player
  (#5979 `a6e66a08c`, which deleted the per-writer copies) — a local 0x14 special-case elsewhere is drift. A VMAD
  `None` object property resolves to absent, never form 0 (`ScriptInstance::object_form_id`, #5971 `34cc809ae`). `Disable`/`Enable` classify the receiver through the same
  alias-aware `receiver_object` as their siblings (an alias-bound marker must not decline alone).
- **Placeholders are coverage with a hole**: `Effect::is_placeholder` (`ShowRaceMenu`, `RequestSave`,
  `SetHudCartMode`, `AddAchievement` — state nothing reads) must be reported separately by coverage harnesses
  (#4328/#4372), nested ones included (`Effect::contains_placeholder` / `visit_nested` walk `Conditional` arms and
  `QuestScope` bodies, #5975 `fd70bf557`); any *new* effect whose runtime arm only records a value needs listing
  there and a pin (`only_the_counter_only_primitives_are_placeholders`).
- **Chain order**: `RECOGNIZERS` = per-script first (`two_state_activator`, `rumble`), generic second
  (`quest_stage_gate`); first match wins, all-`None` = silent miss; a new generic recognizer must not shadow a
  per-script one. `quest_stage_gate` declines when the condition quest and the `SetStage` target disagree, and arms only the
  handler in effect in the script's **boot state** (the `Auto State`'s handler, else script scope; handlers
  only in non-auto states are ignored — #5639 `e1b7e31e7`, `advance_handler_must_be_in_the_boot_state`).
  `rumble` extracts literal property values only. Fidelity gate: `recognizes_da10_and_reproduces_hand_builder`
  (`.psc` side) **and** `da10_pex_reproduces_hand_builder_byte_for_byte` (`.pex` side, `#[ignore]`d, Skyrim SE
  data) — the `.psc` half alone never touches the decompiler; run both.
- **Provider catalog is consulted after the canonical table** (`classify_effect_with_providers`: primitives
  first, #3934 — an installed alias must never change how vanilla lowers); lowering must only accept
  provider barriers the dispatcher can serve (`servable_catalog()` gates on `callback.is_some()`;
  `every_lowering_seam_reads_the_servable_catalog` in `asset_provider/script.rs`), and only as many as the runtime can
  cross — the four fragment entry points count the longest path's barriers and decline at
  `MAX_PROVIDER_FRAGMENT_BARRIERS` (#6022 `893ad4838`). A script that mixes provider and non-provider handlers no
  recognizer claims declines whole (`MixedScript`, #6021 `45c28ad66`) — the VM attach is suppressed once a provider
  program attaches, so a half-lowered script left the rest inert.
- **`translate_pex` clean-`None` on bad bytes and on panic**: `catching_panics` wraps the whole
  parse→decompile→analyze→lower sequence (`a_decompile_panic_is_a_silent_none`), now split as `decompile_pex`
  (reference-independent, cached per script) + `translate_decompiled_with_providers` (per reference, #5999
  `f28930504`), and the VM's `check_reference_script`/`check_alias_script` (#6000 `586290057`,
  `the_vm_script_checkers_run_inside_the_panic_net`); it cannot recover OOM/stack
  overflow (those need the bounds `/audit-papyrus` checks). `CanonicalEvent::from_papyrus` is
  case-insensitive; `Unknown` means "no consumer", never a wildcard.
**Output**: `/tmp/audit/scripting/dim_1.md`

### Dimension 2: Fragment Dispatch, Quest Stages & Lock Nesting
Paths: `crates/scripting/src/{fragment.rs,fragment/**,quest_stages.rs,quest_variables.rs,globals.rs}`, `byroredux/src/boot/schedule/`
First step: `cargo test -p byroredux-scripting -- fragment:: quest_stages::`; `cargo test -p byroredux --bin byroredux boot::schedule`
Guards: `boot/schedule/mod.rs` — `scene_and_fragment_dispatch_chain_stays_in_dependency_order`,
`fragment_activation_order_tests`, `hit_event_consumers_run_after_every_producer`,
`every_parallel_system_declares_everything_it_acquires`,
`papyrus_provider_system_…`/`legacy_obscript_load_order_system_declares_everything_it_acquires`,
`build_scheduler_reports_zero_access_conflicts`; and `fragment/tests.rs::{nested_lock_contract_documents_
apply_effect_itself, the_nested_lock_residual_list_names_every_type_apply_effect_acquires}` (source scans over
`fragment::SOURCES`: `apply_effect`'s doc block must name every type its per-family helpers acquire).
- **Nested-lock safety is exclusive scheduling.** `apply_effect` (`fragment/effects.rs`, ~120 lines: every arm
  delegates to `apply_{quest_vars,inventory,placement,scene,lock,player_control,vehicle_cinematic,messages,
  reference_state,player_stats,location_alias,actor_values,actor_lists,ai_combat}_effect`
  + `apply_quest_scoped_effect` in `fragment/effects/*.rs`, #4340/#5468/#6142; `SetGlobalValue` lives in `quest_vars.rs`, and
  `evaluate_guard` delegates to `evaluate_{quest,actor,world}_guard` in `guards.rs`; the scans read those bodies through
  `fragment::SOURCES`, which `sources_completeness` pins to every production file) runs under one `resource_2_mut::<QuestStageState, QuestObjectiveState>()`
  taken in `apply_fragment_guard_free`; `QuestStageFragments` is cloned before any quest lock (no read→write
  nesting). Safe only because every quest-resource system is `add_exclusive` — moving
  `quest_fragment_dispatch_system`, `scene_fragment_dispatch_system`, or any sibling to the parallel lane, or
  adding a nested component/resource lock, needs the ABBA analysis re-derived (`apply_effect`'s doc block is
  the running inventory — re-read it, do not trust a count; since #5986 `9712354c9` it states the real invariant —
  *everything* under the paired guards, VM natives and the `FragmentExecutionQueue` write included, relies on
  exclusive scheduling — and the scan follows the helper bodies, `nested_helper_bodies`: `apply_effects`, `defer`,
  inventory transfers, evaluators, resolvers, equipment emitters, plus the `has`/`query_2_mut_mut`/`resource_2_mut`
  forms). Reverse-order acquirers (`Inventory`/`Transform`/
  `Globals` first, quest resources second) on any path are the finding. The stage/objective *pair* itself is taken
  TypeId-sorted everywhere — `resource_2_mut` for writes, `World::try_resource_2` for the guard-free getter
  (`evaluate_guard_guard_free`, #6101 `824c620a5`; TypeIds move with the feature set, so a code-order pair reddens the
  lock-order lane in only some builds) — two sequential `resource::<…>()` reads of the pair are the recurrence.
- **`apply_effects` is an explicit cursor stack** (#3935) — no per-`Conditional` recursion or program clone; an
  unresolvable guard `QuestRef` declines the whole `Conditional` (neither branch; #3785,
  `apply_effects_declines_conditional_with_unresolvable_guard`), with a `warn!`.
- **Cascade**: `MAX_CASCADE = 64` bounds only fragment-emitted (`is_cascade`) `SetStage`s, not authored
  ingress; FIFO `VecDeque`; overflow warns; a no-op re-set (`previous == new`) does not cascade. A quest started
  with no Start Up Stage reports `QuestStageAdvanced::stage_set == false`, so no stage fragment runs (CK *Quest
  Stages Tab*); `is_run_once` gates the radiant-reset on `start_quest` (#5406). Stopping a quest or scene goes
  through the one `QuestStageState::stop_with_shutdown` (a running quest advances to its shut-down stage first;
  #5812 `32bebaa19`, `stop_with_shutdown_advances_only_a_running_quest`) — a private stop tail is drift.
- **Objective transitions** (#5153): `QuestObjectiveState` mutators push a bounded `QuestObjectiveEvent` only on
  false→true flips (`objective_mutators_emit_transitions_only`); the queue is serde-skipped presentation
  (drained by the app frame into announcements) — a repeat/un-flip that emits is the regression.
- **Quest journal**: `QuestStageState` keeps a sequenced event journal (`QUEST_EVENT_RETENTION` = 16 384) with
  four fixed subscribers (`SCENE_QUEST_EVENT_SUBSCRIBER`, `FRAGMENT_QUEST_EVENT_SUBSCRIBER`, `TERMINAL_QUEST_EVENT_SUBSCRIBER`, and
  `OBSCRIPT_STAGE_QUEST_EVENT_SUBSCRIBER` for `obscript_stage_result_system`, Dim 6), each polling its own cursor
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
  recognizer's spawn closure consults it so a reload does not re-arm. Fourth sibling `ReferenceMotionTypeState`
  (local FormID → `MotionType`, #6237 `6765e60eb`): `Effect::SetMotionType` (lowering and VM) records it through the
  deferred batch and `cell_loader::spawn::scripted_motion_type` applies it to every collider a placement spawns — a
  `RigidBodyData` column cannot carry it, because collision entities never hold a `FormIdComponent`
  (`reference_motion_type_state_survives_save_load_round_trip`,
  `a_scripted_motion_type_is_applied_to_the_placements_rebuilt_colliders`). A "simplification" that keys any of the
  four ledgers by entity is the regression.
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
- **Papyrus VM contract** (`fragment/vm/`, `docs/engine/scripting.md` "The Papyrus VM"): populate installs
  `Effect::Interpret` only through `interpret_fallback`, i.e. when `check_function` accepts the fragment ahead of
  time — against typed placeholders, in every state's version down the `extends` chain, collecting each script the
  run needs (`the_check_takes_what_the_lowering_declines`); reference and alias scripts go through
  `check_reference_script` / `check_alias_script` (every handler of `DELIVERED_EVENTS` / `DELIVERED_ALIAS_EVENTS`
  must run whole; both lists are built from one `SHARED_REFERENCE_EVENTS` table, the reference list appending the
  documented `REFERENCE_ONLY_EVENTS`, #6158 `fa719a9d5`) and each delivery re-checks via `handler_runs` against the installed scripts. A run that still
  meets a native declining its *actual* values stops and logs — never a guessed value (`feedback_no_guessing`).
  Int-typed natives (`GetStage`, `GetItemCount`, `GetRelationshipRank`, `GetLevel`, `RandomInt`, a truncated
  `GetValueInt`) return `Value::Int` and a typed local keeps its declared numeric type, so `i / 10` and string
  concatenation match the compiled Int/Float split the decompiler erased (#6216 `8dee19ed4`).
  Natives are not interpreted here: `classify_native_{effect,value,guard}` re-use the lowering's primitive table
  and `apply_effects` applies the result, so Dim 1's per-primitive rules and this dimension's lock inventory apply
  to every VM native unchanged. Bounds: `MAX_STEPS` = 100 000 statements per *logical* run (the budget's
  `carried_steps` keeps it cumulative across yields), `MAX_DEPTH` = 16 script-function nesting, a `Utility.Wait`
  past `MAX_PROVIDER_WAIT_SECONDS` stops the run (`waits_past_the_shared_cap_stop_the_run`, #5681), the continuation
  queue holds `MAX_FRAGMENT_CONTINUATIONS` = 4 096, enforced at every push (`FragmentExecutionQueue::push` /
  `extend_capped` drop the newest tail — zero-delay deferrals, custom-event deliveries and yields used to bypass it,
  #5984 `73fddb4b8`). Allocation caps (#5791 `5c3c5758d`): `MAX_ARRAY_ELEMENTS` / `MAX_STRING_BYTES` reuse the SDK's
  `MAX_SCRIPT_ARRAY_ELEMENTS` (512) / `MAX_SCRIPT_STRING_BYTES` (4 KiB) and stop the run in `Expr::New`, array
  literals, `Add`/`Insert` and both string-concat sites; a stored script variable follows at most `MAX_STORED_DEPTH`
  = 8 levels (a struct cycle `s.m = s` stops instead of overflowing the stack, #5983 `9018a5523`). The *checks* run
  under `catching_panics` (#6000), the run itself does not (populate does); the
  production `unreachable!`/`expect` arms in `fragment/vm/interp.rs` each rest on a "loaded/checked above" invariant — re-verify.
  Script variables live in the saved `QuestScriptVariables` (numbers) / `QuestScriptObjects` (objects, strings,
  arrays, structs) keyed by the *owner id* — a quest FormID, a reference FormID, or an `AliasScriptOwners` id in the
  `ALIAS_OWNER_BASE` (0xFC00_0000) range, which no plugin can occupy since regular slot allocation stops at 0xFB
  (#5988 `0edc61bbf`) — so they outlive the cell unload; keying any of them by entity is the
  regression (same rule as Dim 2's ledgers).
- **Frame budget** (`fragment/budget.rs`, #5763): `VmFrameBudget` = `DEFAULT_FRAME_BUDGET` (10 000 statements) per
  frame, `RUN_START_COST` (16) per run, renewed by `event_cleanup_system` — a budget nothing renews kills the
  interpreter after one frame. A run that has not started defers onto `FragmentExecutionQueue` with zero delay; a
  run in progress yields between statements exactly as a `Utility.Wait(0)` would (`VmResume.carried_steps`); a due
  `OnUpdate` the budget cannot start is left due, not queued; a matching one-shot `RegisterForHitEvent` fires and its
  run defers rather than being dropped (#5987 `746214f29`); `fragment_continuation_system` only advances the
  clocks while `budget::exhausted`. Every Papyrus-call system goes through the shared tail
  `fragment/dispatch.rs::run_papyrus_call` (budget → `defer` → `apply_fragment_guard_free`) and `flush_advances`
  (#5797 — hits, distance, package, perk and terminal ran unbudgeted before it); a new system that hand-rolls the
  tail is the recurrence. Unsplittable (documented residual, open as #5764): a statement inside a function called for its value
  and any frame holding an array/struct local — they finish in-frame, charged to the budget. Guards:
  `a_budget_is_spent_and_renewed`, `a_run_the_budget_cuts_short_resumes_where_it_stopped`,
  `several_runaway_handlers_stay_inside_the_frame_budget`; cost bench `vm_cost_*` (`#[ignore]`d).
  `ReferenceScriptIndex` (one hash lookup for "is this reference loaded, which scripts") is rebuilt at the first
  lookup after an attach and re-validates the entity it finds — a despawned/reused id reads as not loaded.
  `fragment/reference.rs::collect_deliveries` returns before walking the attached scripts on a frame that raised none
  of its sources (`OnDeathEvent` — drained first, ahead of the early-out — bleedout/combat/player-location changes,
  `OnCellLoadEvent`, `OnInitEvent`, `ActivateEvent`, `OnTriggerEnterEvent`, `HitEvent`; #5764 part, `11685934d`): a new
  delivery source not added to that gate is silently never delivered.
- **Delivery systems** (all exclusive `Update`; order in `boot/schedule/update/`): (`obscript_object_system`, Dim 6) →
  `check_alias_scripts_dispatch` →
  `alias_script_event_system` → `reference_script_event_system` (ahead of quest dispatch so a stage a script sets
  runs its fragment the same frame; drains `OnDeathEvent` at its head; every marker but `HitEvent`) → `activate_parent_relay_system` →
  `animation_event_system` → `scripted_sound_system` → `quest_fragment_dispatch` → `quest_terminal_stage_system` →
  `fragment_continuation_system` → `script_update_dispatch` → `distance_event_system` → … →
  `pending_trap_hit_flush_system` → `reference_script_hit_system` (reference/alias `OnHit`, split out by #5991
  `e4810c125` because projectile and trap hits land after the frame pass) → `hit_event_system` →
  `obscript_hit_system`/`obscript_combat_system` — every `HitEvent` consumer after every producer. Registration families (`fragment/{updates,distance,events,
  hits,anim_events}.rs`): per script *instance*, saved, delivered to the registrant only; a quest's registrations
  end when it stops, a reference's wait while it is unloaded; a registration is accepted only when its handler
  passes the VM check, so a delivery never meets an unchecked body (a registration whose handler cannot run is
  skipped without failing the registering code). PACK / PERK / TERM fragments (`fragment/{package,perk,terminal}.rs`)
  are *host-reported* (`report_package_end` on a Travel arrival, `report_perk_entry` / `report_terminal_selection`
  from the `perk.run` / `terminal.select` console commands — no engine producer for the activation popup or terminal
  screen yet, a documented gap).
**Output**: `/tmp/audit/scripting/dim_2.md`

### Dimension 3: Event Runtime — Markers, Timers, Conditions, Triggers
Paths: `crates/scripting/src/{events,cleanup,timer,recurring_update,condition,trigger,vm_state,player_control}.rs`, `crates/scripting/src/papyrus_demo/quest_advance.rs`, the marker producers/consumers in the M47.2 request modules (`*_requests.rs`, `havok_impulse.rs`, `activate_parents.rs`)
First step: `cargo test -p byroredux-scripting -- cleanup:: condition:: trigger:: recurring_update:: quest_advance`
Guards: `cleanup.rs::every_drained_marker_is_a_documented_pattern_a_marker`,
`every_documented_pattern_b_marker_drains_in_its_named_consumer` (row-checks each Pattern-B row — `OnDeathEvent`,
`HavokImpulseRequest`, `MotionTypeChangeRequest` and the seven ObScript request markers — `OnResetEvent`,
`PickIdleRequest`, `FleeRequest`, `PushActorRequest`, `ReviveRequest`, `UseWeaponRequest`, `ResetInventoryRequest` —
included — against its consumer's production text only, #5996 `11bb9f1c3`, #6267 `cf7380e27`; `OnResetEvent` drains
ahead of `collect_due`'s no-instances early returns), `cleanup_removes_all_event_types`;
`boot/schedule/mod.rs::{transient_cleanup_is_the_last_late_exclusive, activation_flush_is_scheduled_before_every_activate_event_consumer, extension_event_adapters_run_before_transient_cleanup}`.
- **Marker lifetime — two sanctioned patterns** (module doc of `cleanup.rs` is authoritative): **A** = listed in
  `event_cleanup_system` for markers with no single owner (last system in `Stage::Late`); **B** = drained
  *unconditionally at the head* of the one owning consumer (no early return before the drain, or the marker
  strands and re-fires forever). A marker in neither is the bug — cross-check every `world.insert` of a
  marker type against the drain lists, and check each Pattern-B owner's first statements. **Multi-producer
  markers must merge, not overwrite** (`SparseSetStorage` = one slot per entity): `QuestStageAdvancedBatch`
  (`push_quest_stage_advances`), `HitEvent` (one row per target: the NPC melee producer holds a strike on an
  already-hit target, #4324; projectile hits ride the `ProcessTrapHit` queue whose flush applies the same hold,
  #5860 `ea28a35b8` — re-verify after any new producer), `ActivateEvent` (Update-stage consumer order pinned by the flush test, which now
  lists `container_loot_system` and `mg07_on_activate_dispatch` (#4712/#4116), plus
  `reference_script_event_system`; the Late-stage `npc_dialogue` selection system is a further consumer — it must
  stay before `event_cleanup_system`); `NativeActivation` rides an `ActivateEvent` raised by `Activate(akActivator,
  true)` and makes the Papyrus `OnActivate` delivery and the `BlockActivation` gate stand aside (CK
  *Activate - ObjectReference*) — every script-side `OnActivate` consumer (lowered quest-advance, rumble, MG07,
  provider, WASM extension, VM) reads `papyrus_activations` (`ActivateEvent` minus `NativeActivation`, #5994
  `6f7dd2213`), so a raw `ActivateEvent` read in a script consumer is the regression (the last two,
`two_state_activator_system` and `legacy_obscript_load_order_system`, moved over in #6218 `36d1a2c2c`); `OnDeathEvent` is Pattern B, drained at the head of
  `reference_script_event_system` (a Late-stage script kill is delivered next frame, not lost to cleanup).
  `event_cleanup_system` also renews `VmFrameBudget` (Dim 2) and drains `NativeActivation`; its module-doc table
  lists the request markers the engine-side consumers drain (e.g. `HavokImpulseRequest` →
  `scripted_havok_impulse_system`, which snapshots, drains exactly those and early-returns only when there are none —
a missing `HavokImpulseRules` drains the snapshot before returning, #6009 `f00bfd6e5`; a resource check placed after
the resolve and before the drain is the recurrence).
- **Two-phase lock discipline**: `timer_tick_system` (no production code starts a `ScriptTimer` or reads
  `TimerExpired` — #5608; FO4 `OnTimer` is `fragment/updates.rs`), `trigger_detection_system`,
  `recurring_update_tick_system` collect under one `query_mut`, `drop()`, then acquire the marker-insert
  `query_mut`; two component-mut locks at once forces the TypeId-sorted contract (deadlock vector).
- **CTDA OR-precedence** (`condition/eval.rs::evaluate`): consecutive `or_next` conditions form a block that binds
  tighter than the AND chain (`A AND B OR C AND D` = `A AND (B OR C) AND D`); empty list → `true`; guards
  `or_precedence_quirk_*`, `and_chain_short_circuits_on_first_false`. Function catalog per the module-doc
  table (indices verified against TES5Edit; a function is added as one `condition/catalog.rs` `ROWS` entry, in `CATALOG` order; `CATALOG` was 24 at #5380 `395d2d831`, which added 56 `GetQuestRunning`,
  74 `GetGlobalValue`, 77 `GetRandomPercent` — the last rides `StoryManagerRng`, a documented 50.0 with none
  installed, and draws an integer 0..=99 as the CK says — `pick(100)`, #5646 `6701086fd`; the module-doc table and a row-count pin
  (`the_module_doc_table_lists_the_whole_catalog`) track the catalog, #5648 `2ed7f8ec3`. `GetQuestRunning`
  reads `is_running` alone — a stopped/completed/failed quest is not running, #5636 `aac92c6ca`; the plugin-side
  `param1_is_form_id` load-order remap mirrors the catalog by index, with rows for 56/71/74 and an exhaustive
  mirror test, #5637 `722c06a76`; M47.2 grew `CATALOG` to 48, #5778 `71e488a26` to 52 and #5750/#6251/the crime
getters to 77 (the array length is the live count; #6251 `f57c75616` added 35 `GetDisabled`, 47 `GetItemCount`
(a form list sums its members), 64 `GetIsCreature`, 84 `GetDeadCount`, 131 `GetPCIsSex`, 289 `IsInCombat`, FNV's
420/421 `GetObjectiveCompleted`/`Displayed`, 431 `GetHealthPercentage`, 546 `GetQuestCompleted`; #5750 `20d01cd6c`
426/427 `GetIsVoiceType` and 700 `IsCommandedActor`; the crime commits the `GetCrimeGold`/`GetCrime`/`IsGuard`/
`GetPCFame`-family reads, whose state is `crime_*`/`jail`/`guards` — `/audit-gameplay`; #5778 added 254 `GetIsPlayableRace` over `PlayableRaces`, 365 `GetPlayerInSEWorld`, 70 `GetIsSex`, 310
`GetInWorldspace`, whose `param_1` joins the plugin remap) — 629 `GetVMQuestVariable`, the
  location family 181/359/360/562/563/565/566/567/605 plus the cleared / keyword-data location reads (250, 598 and the
  `GetKeywordDataFor*` rows), 576 `GetEventData` (xEdit's packed `function:member` in `param_1`, the form in
  `param_2`), 560 `HasKeyword`, 403 `GetRelationshipRank`, 237 `GetIsGhost` and the essential/protected/
  bleedout/teammate reads — those that need a context read `ConditionContext` (owning quest's aliases, a
  location-alias fill's `location_subject`) through `evaluate_contextual` (`reads_context`), and
  `param2_is_form_id` remaps 605/181's (and 576's) second parameter; 180/610 stay unmapped (Oblivion/FNV collisions); fn 0 is deliberately **unmapped** (xEdit: `GetWantBlocking` on TES5/FO4, no index 0 on
  TES4/FO3/FNV — the short-lived `GetButtonPressed` mapping was removed by #5431; `GetButtonPressed` exists
  only as an ObScript script function in `obscript_quests`)); `GetIsID` compares the Run-On's **base** object
  (`SceneAliasCandidate::base_form_id`), not the placed ref (#5041 `1816bbc14`,
  `get_is_id_matches_run_on_base_object_not_placed_reference`); `GetActorValue` reads through
  `CharacterRuleset::actor_value` (the shared derived+modifier composer, #5042 — `/audit-character` Dim 4), its
  authored enum index translated to the loaded AVIF by `condition_actor_value_form` (FO3/FNV GetAV table, Skyrim UESP
  indices; unreadable on Oblivion, which has no AVIFs — #5989 `d1dd5e030`); unknown functions return the documented safe default — a
  display/selection default only: a consumer that *acts* on a pass must decline on any `Unknown` term first
  (Story Manager nodes, #5380; the ambient force-greet install's `package_conditions_fully_modeled`, #5376;
  dialogue INFOs, #5752; scene/phase starts, quest-log entries, objective targets, base-form trigger advances and
  consumable branches through `evaluate_modeled`, #5993 `9b6dcf872`;
  every such check goes through the one game-aware `condition::is_modeled` — fn 79 `GetQuestVariable` reads
  `ObScriptQuestLocals` on Oblivion/FO3/FNV and stays unmodeled on FO4/Starfield where 79 is another function,
  `f67d82b3c`; catalogued indices another game reuses — 125, 131, 403, 426/427, 431, 449, 459, 533, 550, 555, 630, and 365 everywhere but Oblivion (`IsChild`
  there, #5778) — are unmodeled there (`FOREIGN_INDICES`, per-game rows), #5990 `9f418b4fa`; an untranslatable `GetActorValue` index is unmodeled), and
  a new intrusive consumer that trusts the fail-open pass is the finding; `RunOn`
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
  once the target stage is done. `GetTriggerObjectCount` counts through `trigger_occupants` (player included; an
  empty candidate list clears stale pairs, #5992 `06f6382fe`). `actor_quest_trigger_is_in_sequence` (gate) and
  `scene_trigger_actor_approach_system_inner` (router, Dim 5) answer "which BaseForm trigger stage is next"
  separately — verify they agree via the shared `base_form_advance_is_eligible`/`scene_phase_awaited_stage`
  (#4333) on cross-quest phase waits and centerless triggers.
- `quest_advance_system` unifies `ActivateEvent` + `OnTriggerEnterEvent` on `QuestAdvanceOnActivate` with a
  three-way `ActivatorGate` (`Any`/`PlayerOnly`/`BaseForm`), evaluated per `(entity, triggerer)` pair, and the first
  passing triggerer of a source wins — one advance per volume per frame, so two actors entering together cannot run
  the stage fragment twice past `onlyOnce` (#5995 `6a85756d6`);
  nothing may deliver both signals to one entity in one frame. `disable_reference_after_advance` (from
  `disableWhenDone`) records into `ReferenceEnableState`; both fire-once variants park in `ReferenceScriptState`
  (`fire_once_advances_park_the_reference_script_state`, #4334).
- `recurring_update_tick_system`: no fire on the registering frame/zero dt; once per interval; overshoot fires
  once; `UnregisterForUpdate` in a handler terminates cleanly. `QuestAliasReadinessGate` timing: after
  `quest_alias_refresh_system`, before `scene_playback_system`.
**Output**: `/tmp/audit/scripting/dim_3.md`

### Dimension 4: Engine Attach Path & Reference Identity
Paths: `byroredux/src/cell_loader/references/{attach,synth_child,mod}.rs`, `byroredux/src/cell_loader/{spawn,exterior,unload}.rs`, `byroredux/src/cell_loader/exterior/persistent.rs`, `byroredux/src/asset_provider/script.rs`, `crates/scripting/src/fragment/{reference,alias_scripts}.rs` (the attached component + check), `crates/plugin/src/esm/records/index.rs`
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
  to one attach; a reference that lists a script with fewer properties than its base record's copy keeps its own
  values and inherits the rest — `a_reference_inherits_the_base_copys_properties_it_does_not_set`).
  **Attach order**: the `.pex` is extracted and decompiled once per script name (`decompiled_pex` →
  `DecompiledPexCache`, compatibility preflight recorded on first sight; #5999 `f28930504`), then
  `translate_decompiled_with_providers` runs per reference against the shared AST and yields a provider program and/or a recognized
  archetype (each attaches when present); only when neither claimed the script does
  `attach_vm_reference_script` (`asset_provider/script.rs`) run, which declines a reference with no FormID (a
  non-primary SCOL/PKIN child — the VM reaches scripts by FormID, #5998 `838ce679d`) and attaches
  `PapyrusReferenceScripts` when `check_reference_script` accepts every handler of the delivered events (verdict
  cached per script name in `VmReferenceScriptChecks`; a script a recognizer models gets no VM instance, so VM
  calls into it decline). Quest alias scripts are checked lazily by `check_running_quest_alias_scripts` —
  `ALIAS_CHECKS_PER_FRAME` = 4 quests and `ALIAS_SCRIPT_CHECKS_PER_FRAME` = 8 distinct uncached scripts per frame
  (#6001 `b7c04b44b`; a stopped quest resumes next frame), only for running quests — never at cell load. A legacy
  `SCRI` reference attaches its ObScript object script (`attach_object_script`, Dim 6) in the same `attach.rs` arm
  set. Verifying coverage against only the first few accessor arms re-derives a closed gap.
- **Every identity-only actor/reference stub must attach scripts.** All `spawn_logical_quest_reference`
  callers outside `synth_child.rs` — the persistent-worldspace loop in `cell_loader/exterior/persistent.rs` and
  `materialize_scene_actor_alias_stubs` in `asset_provider/script.rs` — go through
  `spawn_logical_quest_reference_with_scripts` (attaches once per `reference_form_id`, #4112/#4331; the source
  pin in `cell_loader/references/source_pin_tests.rs` rejects the bare spelling and scans production text only,
  #5650 `a7e05dd7b`). Identity-only entities get `OnInit` alone: `attach_script_for_refr` takes `has_3d` and inserts
  `OnCellLoadEvent` (→ `OnInit`/`OnCellAttach`/`OnLoad`/`OnCellLoad`) only with real 3D, else `OnInitEvent`; every
  `spawn_logical_quest_reference` site pairs with `attach_identity_reference_script` and a `Disable()`d trigger passes
  `!placement_disabled` (#5997 `a44d4a20d`, `attach_script_for_refr_sends_the_load_events_only_to_entities_with_3d`)
  — a stub that receives `OnCellLoadEvent` runs its `OnLoad` twice. The persistent loader also stamps
  `UnloadedReferenceStub` on a stub whose home cell is not loaded, so ObScript `GameMode` skips it like a
  `RemoteSceneActorStub` while its event blocks still answer (#6220 `335994b4c`). Residual (documented in the helper): a stub followed by the home cell
  streaming in leaves scripts on both — stub/real reconciliation is unbuilt and untracked (the helper doc calls it
  *#2664's half*, but #2664 closed on the stamping/`Transform` half; no open issue owns it). New spawn paths
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
  moved render-side (`AnimatedVisibility` is honored in static but not skinned rendering). An XESP child resolves
  through `reference_enabled_from`: the chain is followed (cycle-capped) to its first parent-less reference, whose
  scripted Enable/Disable beats its authored flag, flipping once per inverted hop; a child cannot be toggled
  independently (CK *Reference*), and a parent-disabled child keeps a content-withheld root with identity + scripts
  (#6077 `22ff0e12e`). Guard:
  `cell_loader/reference_enable_gate_tests.rs`.
- **XPRM → `TriggerVolume`** (`trigger_volume_from_primitive`): bounds are z-up **half**-extents (do not
  divide by 2), permute `[x, z, y]` with `.abs()`, REFR scale baked into a world-space volume, sphere radius in
  `half_extents.x`, shape `1 → Box`, `3 → Sphere`, else `None`
  (`trigger_volume_from_{box,sphere}_primitive_*`); invisible (MODL-less) trigger REFRs compose world space once.
- **Spawn/unload hooks the VM reads** (M47.2): `spawn.rs` stamps `EditorLocation` (the spawn pose, for
  `MoveToMyEditorLocation`), `ExplicitOwner` / `ExplicitEncounterZone` from the REFR, and re-applies scripted
  faction edits (`reapply_scripted_faction_changes`) after the actor job so record-derived membership never undoes
  them; `unload.rs` calls `raise_unload_events` on the victims *before* despawn (`OnCellDetach` / `OnUnload`) — but a
  save load's own teardown must not deliver them: `byroredux/src/save_io/live_load.rs` calls `discard_pending_unload_deliveries` and
  `rebaseline_actor_watch` (no cross-timeline `OnUnload` / `OnEnterBleedout` / `OnCombatStateChanged` /
  `OnLocationChange`; #6083 `4b8e62d87`, `a_load_discards_the_unload_events_its_own_teardown_raised`). An
  evicted reference's `TwoStateActivator` / `ScriptVariables` (ObScript object locals included), `ScriptTimer` /
  `RumbleOnActivate` (#6085 `3b9060701`) and its `ScriptPackage` slot are parked in the eviction row
  (`cell_loader/reference_state.rs`, `/audit-gameplay`) and put back over the defaults the re-attach stamps, before the
  `OnLoad` marker is consumed (#6071/#6072 `611769b16`). A new spawn path that skips these is the recurrence.
- The `M47.2 scripts:` cell-load summary counters (`recognized`, `trigger volumes`) must be wired or
  `m47-triggers.sh` is vacuous; exterior stub loops report attach counts. `commands/quest.rs`
  (the `quest.*` and `scene.*` console commands, incl. `quest.effects`) stays read-only over `&World` except the explicit start/stage
  commands.
**Output**: `/tmp/audit/scripting/dim_4.md`

### Dimension 5: Scene / Package / Dialogue / Cinematic Playback
Paths: `crates/scripting/src/{scene.rs,scene/**,package.rs,dialogue.rs,cinematic.rs,story_manager.rs,location.rs,animation_graph.rs}`, `byroredux/src/systems/{cinematic,npc_dialogue,forcegreet,story_events}.rs`, `byroredux/src/cell_loader/unload.rs`, `byroredux/src/asset_provider/animation.rs`
First step: `cargo test -p byroredux-scripting -- scene:: package:: dialogue:: cinematic:: story_manager::`; `cargo test -p byroredux --bin byroredux -- cinematic strip_ active_tether npc_dialogue forcegreet story_dispatch_and_forcegreet`
The M47.2 MQ101 cart sequence is the first scripted sequence that drives *animation*, not just ECS state.
- **Scene alias substrate**: `SceneActorBindings` `(quest, alias) → EntityId` is what `resolve_object`,
  `RunOn::QuestAlias`, and `SceneShowCommand` all resolve through (one entry point — a debug view must not use
  a different path); an unfilled alias returns `None`, never fabricates an entity;
  `apply_alias_injections`/`QuestAliasInjectionState` (permanent inventory-grant ledger, saved) must not
  double-grant after load. **Refresh pre-filters must be exact supersets**: `refresh_scene_actor_bindings`
  visits per-fill `CandidateIndex` buckets and skips/filters condition-only fills via `subject_requirement`
  (`ConditionFunction::reads_run_on_entity` and `is_deterministic` must stay exhaustive — a stochastic
  function like `GetRandomPercent` is never folded into a once-for-everyone value, #5645; `run_on_identity`
  is shared with the evaluator) — a bucket or requirement that excludes a candidate the full fill test or `evaluate` would admit silently
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
  `npc_refuses_dialogue` (dead / `AiCombatState` / unconscious — the whole `collect_unconscious` set, so also knocked
  down, fleeing or `SetActorsAI 0`, #6287 `aa1016f3c`), also applied by the interaction Talk arm
  (#5043, `1d31ced2b`) and by `default_activation_blocked` (a script `BlockActivation`, M47.2). `DialogueSurfaceState`
  lives in `crates/scripting/src/dialogue.rs` (re-exported by `npc_dialogue.rs`). UI re-selection goes through `select_topic_by_form_id` and must re-check live
  ownership. Each menu entry records the INFO that admitted it and a click on a listed topic speaks that INFO —
  conditions are evaluated once per offer, never re-rolled at selection (#5682 `8daa24105`, #6007 `f7c8f02a7`); an
  unlisted topic is evaluated fresh. The Late selection system's head ends the open conversation (OnEnd, selection,
  voice) once `npc_refuses_dialogue` holds for the partner, and `begin_barter` applies the same refusal (#5867
  `ca3caf411`). **Spoken-line fragments** (#5152, `c59600138`): `DialogueInfoFragments` (TIF_ INFO VMAD, rebuilt
  from plugin data, unsaved) dispatches the line's OnBegin at selection and the outgoing line's OnEnd on
  selection change / close via `apply_spoken_info_fragment` (the speaking NPC is passed as `akSpeaker`, M47.2) — the same guard-free executor + journal polling
  as the quest/scene dispatchers (`the_spoken_lines_fragments_advance_the_stage`); every native leave path
  (Close, Escape, the Inventory key) funnels through `App::leave_dialogue_page` → `end_open_conversation`, so
  the OnEnd dispatches on all of them (#5409 `14b0d33f2`). Greetings and line
  lifetime (#5367 `14cff35ae`): `open_conversation` falls back to the master's generic greeting topic
  (`GREETING` Oblivion/FO3/FNV, `DialogueGenericHello` Skyrim — resolved once per load order onto
  `EsmIndex::generic_greeting_form`, the map scan kept only as the hand-built-index fallback, #5426
  `411d0eeaf`) when no owned quest topic qualifies; `select_info`
  first gates each candidate through its owning quest's own condition list (`DialogueQuestConditions`, the CK
  *Quest Conditions* tab, #5734 `6709eb65e`) — memoised per quest per call, an uncatalogued function fails the gate
  closed (a hijacker gated only by functions we cannot read must not win on priority; on Oblivion that once declined 87 % of
  INFOs until #5778 `71e488a26` catalogued the gate functions — re-run `condition/tests.rs::oblivion_quest_level_dialogue_gates_census`, `#[ignore]`d, after any catalog change), and the list is cloned out and its guard released before `evaluate` (the `QuestStageState` ↔
  `DialogueQuestConditions` cycle, `8e4c6e40a`); an INFO whose own CTDA set holds any term `is_modeled` rejects is
  declined outright (#5752 `bd85de5ea`); then `select_info`
  collects every passing candidate — the first wins, and a Random first INFO rolls within its positional
  Random stack (ended by the next passing non-Random INFO or a Random End, #5397 `0436bea5c`) through the seeded
  `DialogueRandomState`; position is the merged INFO chain (override INFOs fold by FormID after their PNAM
  predecessor, #5375 `168b81322`, one pass per override since #5644 `0c2bd4d6d` — the merge itself is `/audit-esm`). Spoken Say-Once INFOs are disqualified
  per actor through the saved `DialogueSpokenInfoForms` ledger — `(speaker reference FormID, INFO)` pairs since
  FORMAT_MAJOR 38 (Skyrim ENAM/legacy-DATA flags now decode; #5300 `f18224b02`; re-selecting the same INFO ends
  the outgoing line first); a spoken Goodbye closes on
  the presentation estimate. FO3/FNV
  INFOs honour their own owning quest (QSTI, #5271 `dbc07e8f0`). **Force-greet**
  (`systems/forcegreet.rs`, `ForceGreetDirective` bridge): FO3/FNV Dialogue procedure (`PKDD` topic on
  `PackRecord::dialogue_topic`) and the Skyrim `ForceGreet` procedure-tree leaf (#5367 `287214103`; a concrete package's leaf and topic are
  sought on its own tree first, then on its PKCU template — 334 of 339 Skyrim.esm force-greet packs are template
  instances, #5511 `72e863d80`) walk the NPC to the player and open the topic through the same selection path
  without activation. The walk goes through `navmesh_path::cached_walk_step` and honours KCC-blocked: a mover
  held against an obstacle for `LOCOMOTION_STUCK_REPICK_SECS` gives up the approach and opens in place (#5452
  `6316719ed`), and the step is gated on the shared `ai_locomotion_enabled()` kill switch (the in-radius open
  stays live and the directive stays pending, #5430 `bd366e300`). Two installers: the
  `dialogue.forcegreet` console door (any Dialogue package, unconditionally — the drivable one, smoke `dt1`/`dt2`)
  and FO3/FNV ambient package selection, which installs only from a fully-modeled, player-targeted (PTDT 0x14 or
  none) Conversation (`PackRecord::dialogue_type` 0) — #5376 `4e58f241d`, `AmbientBehavior::from_package` in
  `byroredux/src/npc_spawn/ai_package.rs` (`/audit-gameplay`), pinned by `dialogue_ambient_install_declines_unmodeled_offtarget_sayto`;
  the Skyrim tree dialect has no ambient installer. A failed open consumes the directive (no per-frame retry);
  `forcegreet_system` holds every directive while `player_can_act` is false and holds one while the player talks
  to another NPC (consumed if already talking to the greeter) — #5392 `16544d4c9`,
  `dead_player_holds_the_forcegreet_directive`, `open_conversation_holds_or_consumes_the_forcegreet_directive`.
  Pass 2 re-reads the open conversation partner per open (`open_conversation_partner`), so a second same-tick
  greeter holds instead of stripping the first's line (#5642 `c0c9c9d1b`,
  `two_same_frame_force_greeters_open_one_conversation`).
  Walk steps write `Transform` (not `GlobalTransform`) and `PhysicsWorld` is held for the step alone (#5373/#5371
  `f0c683ef1`). Every opened conversation raises an `AHEL` story event; `raise_hello_story_event` resolves the
  location *before* taking the `StoryEvent` guard and the open sites scope their `LoadedCellIndex` read to a clone
  (#5372 `1c15a2270` — the `LoadedCellIndex → StoryEvent → LoadedCellIndex` cycle). The `npc_dialogue_selection`
  Access row declares the cross-file voice and AHEL hops, and the declaration scan lists `dialogue_voice.rs`,
  `story_events.rs` and the condition evaluator (`evaluate_function`, 18 storages/resources incl. the
  `StoryManagerRng` write) explicitly (#5414 `9d8fd4cc9`, #5649 `197a9e245` — the same-file callee walk cannot
  follow a hop into another file). A replaced or closed line cuts its voice (`stop_line_voice` →
  `AudioWorld::stop_sounds_for`; #5432 `40b77828b`). Talk-candidate filtering (`populate_candidates`, `byroredux/src/interaction.rs`)
  takes each alias-resource guard alone and builds bulk sets per storage (`running_quest_bound_entities`;
  #5025 `2a1a7a362` closed two ABBA cycles, #5109 `83d52707e`) — a guard nest reintroduced there reddens the
  lock-order lane. The egui response page itself is `/audit-tooling`.
- **Story Manager** (`story_manager.rs`, `docs/engine/story-manager.md`; #5366 Phases 0-4, live gate `docs/smoke-tests/sm1-story-manager.sh`):
  `install_story_manager` folds the parsed `SMBN`/`SMEN`/`SMQN` map into an `SmTree`. `SNAM` names the
  **previous** sibling (xEdit): the head is the `SNAM == 0` (or dangling) member and the fold follows the inverse
  edges, authored top-down (#5385 `846e4a6dd`, `snam_previous_semantics_walk_head_first`; every member of a
  sibling group is placed — groups replay in FormID order, not one SNAM chain, #5405 `76a927f8a`; the earlier same-parent
  check held in both directions, so a direction pin must be asymmetric); a sibling cycle must still terminate
  (`sibling_cycle_builds_and_dispatch_terminates`).
  The tree build is linear in sibling-group size and the walk runs on an explicit stack capped at
  `MAX_WALK_DEPTH` = 64 (#5643 `b79f0d2eb`, `a_huge_sibling_group_builds_and_keeps_every_member`,
  `a_hugely_nested_tree_walks_without_overflowing_the_stack`).
  `story_manager_dispatch_system` (exclusive, Update; `StoryClock` synced from `GameTimeRes` first) drains the
  `StoryEvent` marker (Pattern B) and `PendingScriptStoryEvents` (script-raised `SCPT`, below) and walks the event mnemonic's `SMEN` root: a node whose CTDA set holds any
  uncatalogued function **declines before evaluation** — neither starts nor consumes, the walk continues to its
  siblings (#5380 `395d2d831`, `unknown_condition_function_declines_the_node`; the Unknown→0.0 default once
  passed `== 0` gates vacuously) — otherwise node CTDA runs through the shared M47.1
  evaluator with `RunOn::EventData` resolved over the positional R1/R2/L1/L2 slots, a passing quest node starts
  through the canonical `QuestStageState` lifecycle (never a private start path), `FromEvent` aliases fill from
  the slots via `StoryEventAliasFill` consumed by `refresh_scene_actor_bindings`, which probes slot liveness
  (despawned id, or `Dead` without ALLOW_DEAD → `StoryManagerEventUnavailable`, never a stale bind; #5419
  `1dfdeb4f9`, `from_event_alias_skips_stale_and_dead_slot_entities`) and honours the shared reserved set — a slot
  entity another alias reserved stays unbound unless the alias is Allow Reserved (#6006 `e4afa2b0d`). `DNAM` policies: random parents
  shuffle (`StoryManagerRng`, whose write guard is taken for the draw alone — `GetRandomPercent` fn 77 re-enters
  it during `evaluate`, #5628 `faa00de2a`; the `StoryManagerNodeState` pool marks are written after the
  `QuestStageState` guard drops, a source pin keeping them out of the stages-held region, #6003 `738fbbd5d`), a non-sharing quest node consumes the event only when its own pool
  queued a start candidate (a passing node with an exhausted pool no longer swallows the event, #5404
  `76a927f8a`), pools round-robin with
  `RNAM` reset windows (hours, decoded through xEdit's 1/24 scale, #5386) in `StoryManagerNodeState`. Saved SM
  resources: `StoryManagerNodeState` and `StoryEventAliasFill` (the CLOC location cursor is session-local and reset on every save load, #5435 `557ead21d`; #5394 `03e51dfad`, column added at FORMAT_MAJOR 35 — the R1/R2
  slots persist as reference FormIds and re-resolve among resident candidates, session `EntityId`s serde-skipped;
  `story_event_alias_fill_survives_save_load_as_reference_forms`); `SmTree`/cursors/clock/RNG are
  not-saved-by-design, so a quickload cannot re-fire spent radiants. Update ordering (CLOC producer → dispatcher →
  `quest_alias_refresh_system`, ambient installer → `forcegreet_system`) is pinned by
  `story_dispatch_and_forcegreet_update_ordering_is_pinned` (`byroredux/src/scheduler_access_tests.rs`, #5415).
  Producers: `KILL` (`raise_kill_story_event`, called from all four live death producers — combat, NPC and
  player drowning, SDK actor-value batch — with the session LCTN as L1; corpse restores deliberately do not
  raise it, #5434 `c85d50356`; the per-frame CLOC key derivation early-outs on an empty `SmTree`, #5447, and otherwise runs once per cell context through `StoryLocationMemo`, #6335 `80471eb78`),
  `CLOC` (`systems/story_events.rs`, LCTN-granularity key), `AHEL` (every opened conversation), `sm.event` console, and
  **`SCPT`**: `Keyword.SendStoryEvent(akLoc, akRef1, akRef2, aiValue1, aiValue2)` lowers to `Effect::SendStoryEvent`,
  which queues onto `PendingScriptStoryEvents` once the quest-state guards drop (a queue, not a per-entity marker, so
  several raised in one frame all dispatch — `queued_script_events_in_one_frame_all_dispatch`; the engine producers
  `CLOC`/`AHEL`/`KILL` share the one-slot `StoryEvent` marker through `raise_story_event`, whose second event on the same
  entity in a frame queues on `PendingScriptStoryEvents` too — #5647 `eca0c4709`,
  `two_events_on_one_entity_in_a_frame_both_dispatch`); `SCPT` nodes route on condition fn 576 `GetEventData`
  (K1/L1/L2 `GetIsID`, location `HasKeyword`, V1/V2 `GetValue`), and a non-optional `FromEvent` alias with an empty
  slot makes its quest ineligible so the walk continues (`a_required_from_event_alias_without_its_slot_fails_the_start`);
  crime/crafting/`LEVL` families have no producer, and `MNAM`/`XNAM` (Num quests to run / Max concurrent, named
  #5420) are unconsumed — one quest starts per node (documented gaps in `story-manager.md` §8, not findings).
- **Locations & location aliases** (`location.rs`, `docs/engine/scripting.md` "Locations and location aliases"):
  `install_location_table` flattens `LCTN` + each cell's `XLCN` into `LocationTable`; a cell-loaded entity's current
  location is its `CellRoot`'s cell location, the player's is published as `PlayerLocation` by
  `story_change_location_system` only where the load order authors locations (FO3/FNV/Oblivion keep #5447's
  no-per-frame-work contract). `ALLS` aliases fill in list order with the reference aliases inside
  `refresh_scene_actor_bindings` (Specific / Reference-Alias / External / Find Matching), the binding is saved
  (`LocationAliasBindings`), a Find Matching alias never picks a cleared location unless flagged Allow Cleared.
  Reference-alias fills read *this pass's* location bindings, not the committed ones (#6002 `2424b119c`), and a
  Location Alias Reference fill is scoped to its bound location (listed by it under the ref type, or standing in it;
  declines while the location alias is unbound — #6004 `f52a048f3`).
  Cleared state is `LocationClearState`, fed by `note_actor_death` on the one death path every kill / start-dead
  actor / save load funnels through — a new death path that skips `reconcile_dead_actor` never clears a location.
- **Placed-object animation graphs** (`animation_graph.rs`, `asset_provider/object_graph.rs`,
  `object_graph_step_system`, `docs/engine/scripting.md` "Animation graphs of placed objects" / "Behavior graph
  depth"): `PlayAnimation` sends an event to the reference's `ObjectAnimationGraph`; an event takes the state's own
  transition (else a wildcard one; `DISABLED` transitions never fire), enters the new state and starts its clip on
  the object's `AnimationPlayer` (a lazily decoded clip restarts only the graphs whose pose it changes,
  `start_if_pose_changed`, #6005 `d55940d0a`); the clip's trigger events are scheduled in real time and raised by
  `object_graph_step_system` (bounded by `MAX_TRIGGERS_PER_STEP` = 1 024; a saturated step drops still-due one-shots
  too, not just periodic ones, and reports once through the runaway latch — mutually re-arming zero-length triggers
  used to spend the cap every frame forever, #6010 `26e8afb52`,
  `one_shots_that_re_arm_each_other_are_dropped_once_a_step_saturates`), and graph-raised events reach `OnAnimationEvent` through registrations
  (`fragment/anim_events.rs`). FO4 actors run their own graph (`asset_provider/actor_graph.rs`,
  `systems/graph_locomotion.rs`) — `/audit-gameplay`.
  The behavior-graph decode is `/audit-parsers`; collider/motion halves `/audit-physics`.
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
  (drop before quest resources); the router's conditioned advances are evaluated in a pre-pass *before* the
  `QuestStageState` guard (the evaluator re-reads it for the GetStage family; source-pinned, #6008 `22ba1f5fe`). Cinematic systems take `Transform` before `ActorCinematicState`/
  `HorseTetherState` (#4546, `docs/engine/ecs.md` canonical chain; `BYRO_LOCK_ORDER_CHECK=1` is the gate).
- **Cinematic retention** (`cinematic_retained_entities` computed once per unload batch;
  `strip_retained_cell_root(world, victims, retained)` per cell, scoped to `retained ∩ victims`): walk is
  transitive over `Children` (`active_tether_retains_horse_cart_rider_and_hierarchy`); stripping never
  orphans an entity from indexes that expect a `CellRoot`; the retention set's lifetime is bounded since
  #3817 `63bf3347f`: `cinematic_horse_route_system` releases the tether at the XLKR chain's terminal marker, and
  released entities that lost their `CellRoot` to a mid-tether unload queue on `CinematicReAdoption` (unsaved,
  session-local ids) for the streaming step (`retry_cinematic_readoption`) to stamp onto the loaded exterior cell
  root containing them — un-rooted entities are despawn-immune, so a non-draining list is residency leak. Only
  **parentless** members queue and a root's adoption stamps its whole `Children` subtree (a local `Transform` is
  not a world position, #5384 `faf8e5682`); the player's subtree is never queued or cell-owned, and
  `purge_cinematic_retention_state(world, ctx)` releases the still-pending roots' whole `Children` closure
  (minus the player rig) through `release_entities` at session replacement (#5379 `203be9ed4`; the closure and
  the render-side release, #5490 `99cd9fd6c`). `release_finished_tethers` clears `vehicle`, so `ExitCart`'s exit
  heading falls back to the rider's own rotation — equal only because `vehicle_attachment_system` last wrote
  `vehicle.rotation * vehicle_local_rotation` (`released_rider_exit_heading_matches_the_attached_heading`, #5461).
  Pins: `systems/cinematic.rs` (the #3817 lifecycle pair plus `parented_subtree_rides_the_roots_adoption`,
  `player_rider_is_never_queued_or_cell_adopted`) and `cell_loader/unload.rs`
  (`pending_readoption_victims_are_the_whole_subtree_closure`, `pending_readoption_victims_spare_the_player_rig`); a release path
  that strips `vehicle`/`cart_seat` would break the awaited-event exit.
- Player `CinematicPresentationState`/`PlayerControlState` flags are saved; a flag a consumer never reads is
  a placeholder (Dim 1) — `disable_saving` is read by `SaveCommand`; `hud_cart_mode` had no reader at #4372.
**Output**: `/tmp/audit/scripting/dim_5.md`

### Dimension 6: Legacy ObScript Execution (Oblivion/FO3/FNV; M47.3 quest + object scripts)
Paths: `crates/scripting/src/obscript{,_runtime,_vm,_quests,_objects,_cmd,_actions,_actor_values,_anim,_speech,_state,_ledger,_more}.rs`, `crates/scripting/src/obscript_more/`, `crates/scripting/src/obscript_effects/`, `crates/scripting/src/{magic_effects,magic_shaders,script_packages,challenges}.rs`, `byroredux/src/cell_loader/references/attach.rs` (`attach_scpt_script`, `obscript_dialect_for`, `attach_object_script` call), `byroredux/src/asset_provider/script.rs` (`install_quest_scripts` / `install_object_scripts` / `install_item_scripts` calls), the request consumers `byroredux/src/systems/{script_requests,script_queries,revive,knockdown,flee,magic_shader,head_look}.rs`
First step: `cargo test -p byroredux-scripting -- obscript`; `cargo test -p byroredux --bin byroredux -- attach::tests`; `git log -p --since=<last report> -- crates/scripting/src/obscript_vm.rs`
Untrusted-Input: Yes — `SCDA` bytecode comes from mod-supplied plugins. Distinct from the (unbuilt) `.psc`-side
SCTX frontend (`ScriptSource::Obscript` placeholder).
- **`obscript_vm.rs` (pure interpreter) bounds discipline**: statement framing `[op][len][payload]`,
  `0x1c` explicit-caller escapes, implicit-caller statement calls (line opcode = command id), RPN expression
  tokens, `[u16 count]` typed arguments, `If/ElseIf/Else` nesting with `Return`-inside-arm early exit,
  1-based SLSD local / SCRO+SCRV reference resolution — compiled reference indices are 1-based into the
  *interleaved* SCRO+SCRV list (`ScriptRef`; `SCHR.num_refs` counts both; an unresolved explicit caller is
  declined, #5629). Remote variables (`Quest.var`, `ref.var`) are read and written by ordinal through the synthetic
  `CMD_GET_REMOTE_VARIABLE` / `CMD_SET_REMOTE_VARIABLE` host calls (`72fd96dfe`;
  `remote_variables_read_and_write_by_ordinal_on_the_vanilla_bytes`); `exec_remote_set` declines and counts
  (`CMD_DECLINED`) a malformed or unresolvable target, never storing it as a ref assignment (#5630). Reference
  variables hold exact form ids (`VmState::ref_locals`). Argument payload reads (#5655 `291af4528`) and an
  `If`/`ElseIf` condition (#6016 `9a7298283`) are bounded by the statement's own payload, not the script buffer.
  Every `read_u16`/`checked_add`/span end must be bounds-checked
  and malformed input must yield `BlockOutcome::Malformed`, never panic (`truncated_bytecode_reports_malformed`;
  `installed_legacy_masters_have_structurally_valid_scda` is the real-data check, `#[ignore]`d). Recursion is
  capped by `MAX_OBSCRIPT_VM_NESTING` = 32 on both `exec_if_chain` ↔ `run_arm_body` and nested `X` argument
  decode (`decode_args_at_depth`); guards `excessive_nested_if_chains_are_malformed`,
  `excessive_nested_x_arguments_are_rejected` — a new recursive path that skips the depth parameter is the
  regression. The operand stack in
  `eval_expr`/`apply_operator` must not grow unbounded from a lying token count.
- **Command and block IDs are empirical** — recovered by aligning each script's `SCTX` with its compiled stream over
  the vanilla masters (`obscript_tools`), never from a spec. The quest core (`SetStage` 0x1039, `GetStage` 0x103a,
  `GetStageDone` 0x103b, `StartQuest`/`StopQuest` 0x1036/0x1037, `GetQuestRunning` 0x1038, `GetSecondsPassed`
  0x100c, `Message` 0x1059, `GetButtonPressed` 0x101f, `GetSelf` 0x10ce) carries a per-game evidence table in the
  `obscript_quests.rs` module doc (#6015 `3f78f9fe1`): all agree across the three games except `MessageBox`
  0x1000, which no Fallout script compiles to, so FO3/FNV decline it. Getters run through the M47.1 evaluator (id =
  `0x1000` + CTDA index); the wider surface is split by family — `obscript_effects.rs` (the effect bridge:
  commands lowered to the Papyrus `Effect` their namesake builds and applied through `apply_fragment_guard_free`, so
  Dim 1's per-primitive rules and Dim 2's lock inventory cover them), `obscript_actions.rs` (reference-bound
  `Enable`/`Disable`/`MoveTo`/`EVP`/`AddScriptPackage` onto the existing sinks), `obscript_actor_values.rs` (GECK AV
  codes → AVIF, checked by `obscript_tools avcodes`), `obscript_anim.rs`, `obscript_speech.rs`, and
  `obscript_more.rs` and its four sibling files. Every one of those commands is a **row of the command table**
  (`obscript_cmd.rs`, #6139: id, `Games`, `needs_caller`, handler; each family file exports its own `*_COMMANDS`
  slice, listed in `FAMILIES`); the host asks the table, not a `match cmd` chain, so an id claimed twice in one
  game is `duplicate_rows`' test failure, never a shadowed arm, and one id that names different commands per family
  (`0x116b`, `0x116c`, `0x10db`) is two rows told apart by the loaded game (`Game::of`: Oblivion, Fallout, or
  `Other`, which runs only the `Games::Any` rows). The `FALLOUT` / `OBLIVION` census lives in
  `obscript_more/tests.rs`, checked against the rows both ways. Only the VM's own statements
  (`ObScriptCommandHost::call`: `Set`, quest variables, `RemoveMe` / `DropMe`, `GetSecondsPassed`) stay outside it.
  Handlers that act on the calling actor open through
  `obscript_effects.rs::{live_caller, open_actor_effects}` (decline when the caller is not loaded; #6365 `bf27df84c`,
  source guard `handlers_open_through_the_live_caller_helpers`) — a hand-spelled `caller_of` + `live` prologue is the
  recurrence. Anything unmodelled
  is a **traced no-op returning 0, counted per owner in `ObScriptQuestTimers.unknown_commands`, never faked**
  — a command that returns a guessed value, or answers state the engine does not keep, is a finding
  (*feedback_no_guessing*; #6221 `c1b2b1d12` turned `GetIsPlayerBirthsign`, `GetAnimAction` and every
  `IsPlayerActionActive` index but the tracked jump into `Action::Declined`, pinned by
  `untracked_state_declines_instead_of_answering_a_constant`; `obscript_more` answers only from a named page, engine state or the ledger); the
  remaining unmodelled tail and each command's blocker are tabled in `docs/engine/scripting.md` (M47.3 phase 3) —
  not new findings. Dialect tables (xNVSE vs xOBSE opcode
  numbering, `ObscriptDialect`) stay separate because the same number names unrelated commands.
- **Message / MessageBox** (`5182f7817`): `decode_message` parses the `[u16 length]` text, the set/if-format variable
  list and the button strings out of the statement's own argument region (untrusted: every length is checked against
  `end`; a payload that fails to decode is declined and counted `CMD_DECLINED`, never shown half-decoded). The
  decoded `ObScriptMessage` reaches the host's `message`, which substitutes the variables (`substitute_numbers`
  reads width before precision, so `%0.f` / `%2.0f` give zero places, #5789 `1502fbd3d`; `[+- 0#]` flags, minimum
  field width and `%e`/`%g` per the CS *MessageFormatting* template, #6014 `a4e524c9c` — an unparsed spec used to
  shift every later argument; mod width/precision clamped, #6011) and queues it through
  `messages::show_obscript_message`, which drops any unread answer (the CS: −1 while a box is open, #6013
  `cbee28109`); `GetButtonPressed` consumes the last answered box from one global slot (`messages::take_button`, any
  quest's first call gets it — CS wiki contract, #5790 `173f9e155`, `obscript_button_answer_is_consumed_globally_once`),
  −1 otherwise.
- **`obscript_quest_tick_system`** (exclusive `Stage::Update`): runs only *running* quests' GameMode block — due
  quests in FormID order, never `QuestStageState`'s `HashMap` order (#6013 `cbee28109`; two quests polling
  `GetButtonPressed` or cross-`SetStage`ing in one tick raced per process) — each on
  `quest_script_interval` — the authored `QUST` delay (FO3/FNV), overridden by a runtime `SetQuestDelay` (saved in
  `ObScriptQuestLocals`), either one ≤ 0 meaning the INI default `QUEST_SCRIPT_INTERVAL_SECS` = 5 s (GECK
  *SetQuestDelay*). It runs through `ObScriptCommandHost` (generic over its owner — a quest, a placed reference or an
  inventory item), which lowers onto the same
  `QuestStageState`/`Globals` and emits the same stage events as the fragment path (so Dim 2's journal and
  cascade rules apply). Timers/effect ring (`ObScriptQuestTimers`, `ObScriptEffectLog`, cap 64) are unsaved by
  design — elapsed time restarts on load (documented residual, not silent save loss). Quest-script **locals persist**
  between GameMode runs in the saved `ObScriptQuestLocals` (seeded into each run's `VmState`, stored back after the
  run; an unassigned slot reads 0) and condition fn 79 `GetQuestVariable` reads them on Oblivion/FO3/FNV only
  (`f67d82b3c`; the game split and the cross-tick read are pinned since #5787 `a5e5346d0` by
  `get_quest_variable_uses_obscript_locals_only_for_legacy_game_paths` and
  `quest_game_mode_local_is_read_on_the_next_tick_and_by_condition_79`); `StartQuest`/`StopQuest` attach/detach the
  script loop consistently. The 255-script corpus gate in `obscript_quests.rs`
  (`vanilla_oblivion_quest_scripts_execute_and_setstage`, `#[ignore]`d) resolves Oblivion.esm via
  `test_paths::oblivion_esm()`, panics under `BYROREDUX_REQUIRE_GAME_DATA=1` instead of returning green, and runs in
  `.github/workflows/real-data-gates.yml` (#5088 `1377d3463`).
- **Result scripts** (`obscript_results.rs`, `6db47b0e4`): the compiled streams a FO3/FNV/Oblivion `QUST` stage item and
  a dialogue `INFO` carry inline (`QuestStageLogEntry::result_script`, `result_script_begin`/`_end`).
  `obscript_stage_result_system` (exclusive `Update`, after `obscript_quest_tick_system`) polls the quest journal as
  its own subscriber (`OBSCRIPT_STAGE_QUEST_EVENT_SUBSCRIBER`, Dim 2) and runs every stage item whose conditions pass
  (GECK: all of them, not the first) plus the stages they set, up to `MAX_STAGE_SCRIPT_ROUNDS` = 16
  (`a_stage_cycle_is_bounded`); an INFO's Begin/End scripts run with the speaker as implicit reference. Documented
  deviation: the calling script finishes before the stage's items run (the original ran them inside the
  `SetStage` statement). Same decline rules as the other hosts: an `If` whose condition reads a declined getter runs no
  arm. Any new `SCDA` path here is untrusted input (above).
- **Sound commands** (`systems/scripted_sound.rs`, `crates/scripting/src/sound_requests.rs`): `PlaySound` is
  non-locational (GECK) — its request carries `NON_LOCATIONAL_SOURCE` (form 0, never a placement) and plays at the
  `AudioListener` entity, the player body only when none exists (`listener_or_player`; #6226 `f04c3433d`,
  `a_non_locational_play_is_heard_at_the_listener`); `PlaySound3D` and a Papyrus `Sound.Play` keep their source, and a
  record's 2D bit takes the listener too. All script plays use the byte-budgeted voice cache and skip decode on an
  inactive `AudioWorld` (#6281 `41b53ab41`); the audio engine itself is `/audit-audio`.
- **Ledger honesty** (#6190 `7e867cf47`): a command whose value no system, gate or UI reads back declines instead of
  storing it in the saved `ObScriptLedger` and reporting done (`AddTopic`, `ModDisposition`, `SetQuestObject`,
  `SetItemValue` were removed; `GetTalkedToPC` is written by `apply_selection`). Mod-supplied accumulators saturate
  (`ModCrimeGold`/`ModDisposition`, #6187 `50e743a56`). The compiled and source `If`/`ElseIf`/`Else` line forms share one
  `parse_conditional` recursion that enforces `MAX_LEGACY_OBSCRIPT_NESTING` once (#6159 `3c553b7ca`).
- **Object and item scripts** (`obscript_objects.rs`, M47.3 phase 2; live gate `docs/smoke-tests/m47-3-obscript.sh`
  on FNV): a placed reference whose base carries an `SCRI` gets an `ObScriptInstance` at attach; blocks run on the
  events the engine already raises — `GameMode` every frame per loaded scripted reference (not an `UnloadedReferenceStub` / `RemoteSceneActorStub`, #6220; the
  residency question is asked per due instance, #6333 `4f6cac254`), `OnLoad`, `OnDeath`,
  `OnActivate`, the trigger trio, `OnHit`/`OnHitWith`, `OnStartCombat`/`OnCombatEnd`, the package blocks,
  `SayToDone`, the item blocks `OnAdd`/`OnDrop`/`OnEquip`/`OnUnequip`, `OnReset` (a cell's pending reset lives for
  its whole budgeted load — each pass marks the scripted references that appeared since, once each — and the loaders'
  `obscript_state::cell_load_finished` makes the next pass retire it, scriptless or cancelled cells included; #6102
  `2efed9843`; the worldspace persistent CELL's apply job retires it too, #6219 `a99b2f461`, and a save load's own
  reload drops the resets it raised via `discard_pending_cell_resets`, #6238 `078df54e1`,
  `live_load_drain_discards_the_cell_resets_its_reload_raised`), `OnDestructionStageChange`, and
  the scripted-effect blocks in `magic_effects.rs`. A block written for one reference (`Begin OnTrigger player`)
  runs only for it (`BlockInfo`). **`OnActivate` replaces default processing**: `has_activate_block` feeds
  `default_activation_blocked` (the same gate Papyrus `BlockActivation` uses) and the script's own `Activate`
  queues the native activation — but an `OnActivate` run that declined any command or came back malformed lets the
  default go ahead (a door nothing can open is the failure mode). Block ids (`BLOCK_*` in `obscript_vm.rs`) differ
  between Oblivion and the Fallout pair for 0x1a/0x1b (`OnTriggerActor`/`OnTriggerMob` vs enter/leave) — a block
  read in the wrong game's meaning is the finding. Object locals live in the saved `ScriptVariables` and ride the
  eviction row (#6071, Dim 4). Systems: `obscript_object_system` before `reference_script_event_system` (which drains
  `OnDeathEvent`), `obscript_hit_system`/`obscript_combat_system` after every `HitEvent` producer,
  `obscript_destruction_system`, the Late `obscript_item_system`; `BYRO_NO_OBSCRIPT_OBJECTS` is the kill switch. New
  saved state (`ObScriptLedger`, the `obscript_state` stores, `magic_effects` — each `register_saved`) is a
  `FORMAT_MAJOR` question for `/audit-save`.
- **Legacy load-order/extender path**: `attach_scpt_script` resolves SCPT via `ScriptRegistry` (logs an error
  when the resource is absent); dialect selection is by *profile row*, pinned by
  `obscript_dialect_follows_the_profile_not_the_game_kind`; source-less FNV/FO3 bytecode is decoded
  structurally (`decode_extender_calls`, only registered xNVSE/xOBSE opcodes) and FO3 must not apply the
  xNVSE table (`source_less_fo3_scpt_does_not_apply_xnvse_opcode_table`). Arguments/bytes obey the SDK caps
  (`MAX_SCRIPT_CALL_BYTES`, `MAX_SCRIPT_STRING_BYTES`, `MAX_LEGACY_OBSCRIPT_NESTING`); the compatibility analysis
  caps findings per script (`MAX_COMPATIBILITY_FINDINGS_PER_SCRIPT`) and retained entries per registry
  (`MAX_COMPATIBILITY_RETAINED_ENTRIES`), logging the first truncation (#6024 `fcead3b9d`).
**Output**: `/tmp/audit/scripting/dim_6.md`

### Dimension 7: Provider / Extender-Compatibility Layer (SKSE, JContainers, StorageUtil, ModEvent seam)
Paths: `crates/scripting/src/{papyrus_provider/**,compatibility.rs}`, `crates/scripting/tests/extender_compat_e2e.rs`, `byroredux/src/extensions/`, `crates/pex/src/call_sites.rs` (consumer)
First step: `cargo test -p byroredux-scripting -- papyrus_provider compatibility`; `cargo test -p byroredux --bin byroredux -- boot::schedule::system_access_declaration_tests`
This layer (~8.7k LOC scripting-side: `papyrus_provider/` + `compatibility.rs`, incl. tests) was a reported coverage gap through 2026-09-14 (only its *seams* were
audited). Its SDK half (`crates/sdk`: StorageUtil/JContainers verbs, typed script values, limits, manifests)
is `/audit-tooling`; audit here the runtime that consumes it.
- **Untrusted-input**: preflight/lowering run on `.pex`-derived data and provider manifests. `compatibility`
  caps its script table (`MAX_COMPATIBILITY_SCRIPTS`, `truncated()` must surface); lowering recursion is capped
  (`MAX_PROVIDER_HANDLER_NESTING` = 32 — an `elseif` chain counts its clause index into the depth so lowering and
  the per-dispatch `validate_provider_statements` agree, #5656; `MAX_PROVIDER_FRAGMENT_BARRIERS` = 64 —
  cap-before-descend); a literal `Utility.Wait` is capped at `MAX_PROVIDER_WAIT_SECONDS` (one game day) at
  lowering so a mod cannot park immortal continuations in the bounded 4 096-slot queue, #5657 `05d8d5864` — the
  same bound is shared by the fragment lowerer, the VM and `FragmentExecutionQueue`, #5681 `ef130cd6c` (Dim 2);
  `execute_statements` runs on an explicit cursor stack, not a recursion per executed `If` (#5635 `afa5186a1`,
  20 000 sequential `If`s on a 256 KiB stack; nesting depth is threaded through receiver chains too, and saved
  `Wait` values obey the same cap, #6025); `papyrus_provider_system` groups ModEvent registrations by entity
  once per frame and only when events are pending (#5640 `34a47ba81`). ModEvent runtime (#6018–#6025, `2f4b59041`):
  registrations whose registrant no longer carries a provider program are pruned (entity ids are never
  reclaimed), delivery runs under `MAX_PAPYRUS_MOD_EVENT_DELIVERIES_PER_FRAME` = 1 024 with the rest left queued in
  order, owner FormIDs resolve per entity on demand, and a failing provider call keeps the registrations collected
  before it. A reference's provider `OnInit` runs once, its first-init flag saved in `QuestScriptVariables` under
  the reference FormID (#6017 `0b1f1545f`). Value sizes
  obey `MAX_SCRIPT_ARRAY_ELEMENTS`/`MAX_SCRIPT_CALL_BYTES`/`MAX_PENDING_PAPYRUS_MOD_EVENTS` (256) /
  `MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS` (4 096). `execute.rs` has
  `unreachable!` arms guarded by "validated …" invariants — each must be truly established by lowering.
  The `catching_panics` net (#3948) wraps the translate/lower path; check whether runtime execution in
  `papyrus_provider_system` has any equivalent (none found 2026-10-20: no `catch_unwind` under `papyrus_provider/`, nor around the VM's run in `fragment/vm/interp.rs` — only the VM's ahead-of-time checks joined the net, #6000).
- **Decline-on-unmodeled at the seam**: a provider call that cannot be typed declines the whole fragment/
  handler (`lower_provider_call` errors → `None`), it never lowers a prefix and silently drops the tail — nor
  lowers a script's provider handlers while leaving its other handlers inert (`MixedScript`, #6021, Dim 1); a
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
