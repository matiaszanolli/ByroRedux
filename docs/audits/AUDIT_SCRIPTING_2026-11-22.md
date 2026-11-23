**HEAD**: `6e6fdb58b` (audited) · **Baseline**: `AUDIT_SCRIPTING_2026-11-15.md` (`66941d0c0`) · **Audited**: all seven dimensions (each has commits since the baseline: Dim 1 3, Dim 2 28, Dim 3 14, Dim 4 13, Dim 5 7, Dim 6 25, Dim 7 10) · **Unchanged since baseline (skimmed)**: `quest_stages.rs`, `quest_variables.rs`, `globals.rs`, `papyrus_provider/ir.rs`

# Scripting Runtime Audit: 2026-11-22

## Executive summary

| Severity | New |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 7 |
| LOW | 18 |

Process:
- Five read-only Sonnet sub-agents traced the seven dimensions (Dims 1+5 shared one). None ran cargo.
- Every HIGH and MEDIUM finding was re-checked against the code before it was included here. One was graded down: SCR-D2-03, MEDIUM to LOW, because it needs 16 boxes already queued.
- The engine was not launched.
- Baseline tests at HEAD:
  - `byroredux-scripting` lib: 1476 passed, 11 ignored. The integration targets are all real-data and ignored.
  - bin `boot::schedule`: 28 passed.

What changed since the baseline:
- M48's latent `Message.Show()` (#6667) is the delta's main new surface, and it carries most of the findings (SCR-D2-01..05).
- The #7243 explicit-`EvaluatePackage` hold release has two holes:
  - SCR-D3-01: the request can be drained before the ambient pass sees it.
  - SCR-D5-01: ObScript `SetPackDuration` reaches the explicit path.
- All findings of the 2026-11-15 report that sit in these paths are fixed and are now regression checks:
  - SCR-2026-11-15-01/02/03;
  - the 2026-10-17 report's SCR-D7-01..09;
  - #7281.

**Untrusted-input verdict** (runtime-side decoders: `SCDA` bytecode, provider manifests, `.pex`-derived data):
- No panic, OOB or overflow path was found. Message decode, the statement and argument bounds, and the provider caps all hold.
- One unbounded-growth item: SCR-D6-03. The unknown-operator log-once list is process-global, uncapped and scanned linearly. It is bounded by the distinct garbage tokens in the installed plugins, so it is MEDIUM rather than HIGH.
- The frontend verdict (`.pex` reader, `.psc` parser) is in the newest `AUDIT_PAPYRUS_*`.

## Decline-invariant audit

- **Recognizers and composer:** unchanged since the baseline apart from the let-chain refactor (153f611a7), which changed no decline point.
- **Fragment lowerer:** the one delta is M48 `ShowRaceMenu`, and it **leaks a partial lowering**. FO4's `akMenuTarget` / spouse arguments are dropped rather than declined (SCR-D1-01, HIGH). The placeholder list and its pin agree; the docs that still call `ShowRaceMenu` a placeholder do not (SCR-D1-02).
- **VM checks:** `check_function` accepts a value-`Show()` without marking the frame as waiting (SCR-D2-02). A run can therefore stop mid-way after raising the box, which breaks "run whole or not at all". No other check regressed.
- **ObScript command table:** no `Games::Any` row shadows an Oblivion opcode. Untracked state still declines. One ledger-honesty breach (SCR-D6-04).

## Runtime lifecycle matrix

| Invariant | State |
|---|---|
| Marker drain coverage (table below) | verified, with two caveats: `EvaluatePackageRequest` has two consumers (SCR-D3-01), and the Pattern-B row text for the request markers is stale (SCR-D3-04) |
| Two-phase lock drop (tick systems) | verified; no new violation |
| Quest-pair lock nesting | holds because every holder is exclusive. The value-`Show()` path breaks the deferral rule (SCR-D2-04), and the `apply_effect` lock doc misses `Sneaking` (SCR-D2-06) |
| Cascade bound / journal subscribers | unchanged |
| Edge-trigger seed, CTDA OR-precedence | unchanged; `CATALOG` 106, 26 VM-bridged getters |
| Frame budget / continuation resume | **drifted**: a budget-deferred `MessageAnswered` resume loses its answer (SCR-D2-01) |
| Attach gates (enable/lock/`skipped_at_home`) | verified on every loader |
| Provider layer caps | verified |

### Marker-drain coverage

Every `world.insert` / `query_mut().insert` of a marker type was cross-checked against its drain site. No marker is in neither pattern.

| Marker | Pattern | Drain site |
|---|---|---|
| ActivateEvent, NativeActivation, DefaultActivation | A | `cleanup.rs:106-108`. DefaultActivation is new in 180080de7; producer `door.rs:95`; consumer `vm_state.rs:127-145`. |
| HitEvent, HitShots, HitParts | A | `cleanup.rs:109-111`. Every producer row fills `arrow` / `ignores_resistance`. |
| SplashEvent, RippleEvent, TimerExpired, AnimationTextKeyEvents, OnUpdateEvent | A | `cleanup.rs:112-115` |
| QuestStageAdvancedBatch | A | `cleanup.rs:118`. The single writer `push_quest_stage_advances` (`quest_stages.rs:823`) extends. |
| CameraShakeCommand, ControllerRumbleCommand, UiMessageCommand | A | `cleanup.rs:119-121` |
| SceneEventBatch, SceneFragmentInvocationBatch | A | `cleanup.rs:122-123`; also drained at the head of `scene_playback_system` for direct invocations. |
| OnTriggerEnterEvent, OnTriggerLeaveEvent, OnCellLoadEvent, OnInitEvent | A | `cleanup.rs:130-133`. Enter and Leave merge (`trigger.rs:400-433`, `cinematic.rs:1317`, `obscript_more/actions.rs:91`). |
| EquipmentEventBatch, ItemEventBatch | A | `cleanup.rs:134-135`. `emit_*` extend-or-insert (`equipment.rs:61-67,99-103`). |
| SceneStart/Stop/ActionCompletion, DialoguePresentation/LineCompletion, ScenePackageEvent/Completion | B | Heads of `scene_playback_system` (`playback.rs:556-572`), `scene_dialogue_system` (`dialogue.rs:593-596`), `scene_package_system` (`package.rs:684-688`). |
| EvaluatePackageRequest | B (two consumers, see D3-01) | `scene_package_system`; also read by `ambient_ai_package_system`. |
| ReleasePackageHold | B | `ambient_ai_package_system`, `ai_package.rs:1388-1397`, before every early return. Pinned by `request_drain_tests.rs:113`. |
| StoryEvent | B | `story_manager_dispatch_system` head (`story_manager.rs:764-768`) |
| TwoStateTransitionBatch | B | `two_state_activator_system` head (`vm_state.rs:149`). `door_motion_system` is registered immediately before it, so a batch is played the same or next frame. |
| OnDeathEvent | B | `collect_deliveries(Frame)` (`reference.rs:646-653`). `obscript_object_system` only reads and is scheduled before it. |
| OnResetEvent | B | `collect_due`, before its early returns |
| HavokImpulseRequest, MotionTypeChangeRequest, PickIdleRequest, FleeRequest, PushActorRequest, ReviveRequest, UseWeaponRequest, ResetInventoryRequest | B | `take_requests` at the head of each system (`cinematic.rs:152,417,470`, `flee.rs:87`, `script_requests.rs:47,239`, `revive.rs:32`, `knockdown.rs:65`). `HavokImpulseRequest` producers sum (`havok_impulse.rs:136-143`). |

## Findings

### SCR-D1-01: Fallout 4 `Game.ShowRaceMenu(akMenuTarget, 0, …)` lowers on `uiMode` alone and drops a non-player `akMenuTarget`; the engine's race menu then edits the player

- **Severity**: HIGH (per the skill's escalation: a lowering that accepts an unmodeled term instead of declining. Exposure is low: vanilla FO4 passes `None` or the player.)
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: Yes (the call arrives from a decompiled mod `.pex`; the value is the receiver expression)
- **Location**: `crates/scripting/src/translate/effects/prims/player_control.rs:120-146` (`prim_show_race_menu`); runtime `crates/scripting/src/fragment/effects/deferred.rs:998-1016` (`flush_cinematic_presentation`)
- **Status**: NEW (the umbrella #6667, "M48 Phase 5: interactive race and sex selection menu", is open but is about the UI, not the recognizer)
- **Description**: The FO4 CK page (`falloutck-uesp-wiki/(main)/ShowRaceMenu - Game.wiki`) gives `akMenuTarget` as "The primary actor to be modified by the race menu. Default: None (use the Player)". `akMenuSpouseFemale` and `akMenuSpouseMale` are "used in the character generation version of the race menu", which is mode 0. The new primitive rejects only a *literal* in the reference slots (`Int/Float/Bool/StringLit`). Any identifier, property or call (a companion property, `Self.GetActorRef()`) passes, and so does `Game.GetPlayer()`. Mode 0 then lowers to the argument-less `Effect::ShowRaceMenu`. Its runtime pushes `MenuRequest { menu: "race", .. }` and the character-creation page edits the player. The doc comment justifies the drop with "with no editor they are not read". That holds for modes 1-4 (S4, no editor) but not for mode 0, where an editor opens. The primitive-level rule is: accept only the literal-default form of a parameter, decline the rest. The target and spouse slots accept `None` or a player expression and decline anything else.
- **Evidence**:
  ```rust
  // player_control.rs:130-135
  let literal = |slot: usize| {
      args.get(slot).is_some_and(|arg| matches!(arg.value.node, Expr::IntLit(_) | Expr::FloatLit(_) | Expr::BoolLit(_) | Expr::StringLit(_)))
  };
  if [0, 2, 3, 4].into_iter().any(literal) { return None; }
  ...
  // :140-145
  0 => Some(Effect::ShowRaceMenu),
  mode @ 1..=4 => Some(Effect::ShowAppearanceMenu { mode: mode as u8 }),
  ```
  The new test `fallout4_show_race_menu_lowers_on_its_mode` (`translate/effects/tests.rs`, near line 3139) passes only `None` in slots 0, 2, 3 and 4. No test covers a non-`None` reference.
- **Impact**: A mod `.pex` that calls `Game.ShowRaceMenu(SomeCompanion, 0)` gets a lowered effect. The player's face and sex are edited instead of the companion's, with no VM fallback, because the recognizer claimed the call. A wrongly lowered script corrupts state and an inert one does not.
- **Related**: #6667, #4328 (placeholder accounting). For 1-4 the target is also dropped, but that effect is a documented placeholder (`Effect::is_placeholder`).
- **Suggested Fix**: Lower only when slot 0 is absent, `None` or a player expression (`Game.GetPlayer()` or the player-alias property, through the helper the other player-receiver primitives use), and decline otherwise. Do the same for the spouse slots at mode 0 until the page shows a spouse. Add a decline test with a non-`None` actor argument.

### SCR-D2-01: A `Message.Show()` continuation re-queued by the frame budget loses the player's answer (resumes with button 0)
- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/systems.rs:90-99` and `:151-166`; `crates/scripting/src/messages.rs:191-197`
- **Status**: NEW (feature tracker Refs #6667; no open issue covers it)
- **Description**: `fragment_continuation_system` resolves a `MessageAnswered` continuation by calling `take_papyrus_answer`, which *removes* the answer from `PapyrusMessageAnswers::answered`, then stores the button in `state.answer` and pushes the run onto `ready`. If the interpreter frame budget is exhausted before this run's turn, the `ready` loop re-queues every remaining run with `..pending`, which keeps `resume_when: MessageAnswered { ticket }`. On the next frame `take_papyrus_answer` finds neither an answered entry nor a waiting ticket and returns the "box was lost" default `Some(0)`, and the system overwrites `state.answer` with it. The real answer is gone and the script branches on button 0.
- **Evidence**:
  ```rust
  // systems.rs:90
  FragmentResumeCondition::MessageAnswered { ticket } => match crate::messages::take_papyrus_answer(world, *ticket) {
      Some(button) => { ... state.answer = Some(button); ... ready.push(pending); }
  // systems.rs:151 (budget exhausted -> re-queue keeps resume_when)
  let mut waiting = vec![PendingFragmentExecution { remaining_seconds: 0.0, ..pending }];
  // messages.rs:191 — second call, answer already removed:
  if let Some(button) = answers.answered.remove(&ticket) { return Some(button); }
  (!answers.waiting.contains(&ticket)).then_some(0)
  ```
- **Impact**: Silent wrong branch of a quest script (MQ101's `If TempChooseSidesMessage.Show() == 0`) whenever the answer lands in a frame whose budget (10 000 statements) runs out before the run is reached — i.e. under exactly the load the budget exists for. Violates "never guess a value". No test covers a budget-cut ready `MessageAnswered` run (`message_show_waits_for_its_answer_and_branches_on_it` runs with a fresh budget).
- **Related**: #5763 (budget), #6667; `a_budget_is_spent_and_renewed`.
- **Suggested Fix**: When the answer is taken, also set the pending's `resume_when` to `DelayElapsed` (the answer already lives in the continuation's `resume.answer`), so a re-queue cannot re-poll the ticket. Add a test that exhausts the budget between answer and resume.

### SCR-D2-02: The VM check does not treat `Message.Show()` as a wait, so a function called for its value (or with an array local) raises the box, then the run stops
- **Severity**: MEDIUM
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/vm/checker.rs:1099-1114` (check arm), `crates/scripting/src/fragment/vm/interp.rs:274-281` (`call_function`), `:1192-1224` (`message_show`); `crates/scripting/src/fragment/vm/natives/game_utility.rs:267-272` (the `Utility.Wait` arm it should mirror)
- **Status**: NEW (Refs #6667; the value-called-wait residual of #5764 is the related documented gap, but that issue covers *statements inside* such functions, not a check hole)
- **Description**: `Utility.Wait` is accepted only as a statement and sets `self.waits = true`, so `call_waits` rejects any function that waits when it is called for its value ("a wait in a function called for its value"). The new `Message.Show()` arm returns `Some(Kind::Num)` without setting `waits`. A helper such as `Int Function Ask() Return Msg.Show() EndFunction` called as `If Ask() == 0` therefore passes `check_function`. At run time the callee's `exec_block` turns the `AWAITS_MESSAGE` stop into `Flow::Suspend`, `call_function` rejects a suspension, and the run ends with `stop("a wait in a function called for its value")` — after `show_message_awaiting` already queued the box and `awaits_message = Some(ticket)`. The same applies to a frame holding an array/struct local (`frame_state` → "an array local across a wait"), which the check also never sees for `Show`.
- **Evidence**:
  ```rust
  // checker.rs:1104-1113 — no `self.waits = true`
  if arg_kinds.len() <= 9 && arg_kinds.iter().all(numeric) { return Some(Kind::Num); }
  // game_utility.rs:267 — the Wait arm
  if statement && matches!(arg_kinds, [..]) { self.waits = true; return Some(Kind::None); }
  // interp.rs:279
  Flow::Suspend(..) => return stop("a wait in a function called for its value"),
  ```
- **Impact**: Breaks the VM contract "check ahead of time, run whole or not at all": the player is shown a modal box whose answer is discarded and the script aborts mid-run (earlier effects of the run already applied). Secondary: `awaits_message` is never cleared on a stopped run, so a later `Utility.Wait` suspending in the same `apply_effects` call (several `Interpret` effects merged in one stage) is built with `resume_when: MessageAnswered { ticket: <stale> }` instead of `DelayElapsed` (effects.rs:400). The orphaned ticket stays in `waiting` until the 64-ticket cap.
- **Related**: SCR-D2-03, #5764.
- **Suggested Fix**: Set `self.waits = true` in the checker arm (so `call_waits` declines value-called callers and the check fails ahead of time), and clear `deferred.awaits_message` when `run_fragment` ends in a stop.

### SCR-D3-01: An explicit EvaluatePackage raised between `ambient_ai_package_system` and `scene_package_system` is invisible to the ambient pass, and its Must Complete release is lost
- **Severity**: MEDIUM
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/package.rs:685-688` (drain), `byroredux/src/npc_spawn/ai_package.rs:1381-1397,1507` (read), `byroredux/src/boot/schedule/update/quest_scene/package_fragments.rs:12` and `scene_actions.rs` (order)
- **Status**: NEW. Related closed issues: #7243 (the release marker), #2652 (the "observe before the SCEN drain" ordering).
- **Untrusted-Input**: No
- **Description**: `EvaluatePackageRequest` has two consumers. `ambient_ai_package_system` only reads it (`requested`), and `scene_package_system` snapshots and drains it. The cleanup.rs table lists only the second, as a Pattern B single owner. The ambient pass sees the request only if the request was raised before it in the same Update, or in a later stage of the previous frame. Update order is `ambient_ai_package_system` (package_fragments), then `package_fragment_system`, `perk_*`, `terminal_fragment_system`, `scripted_conversation_system`, `forcegreet`, `obscript_quest_tick_system`, `obscript_quest_menu_mode_system`, `obscript_stage_result_system`, and then `scene_package_system` (scene_actions, first row). Any `EvaluatePackage` raised in that window is drained the same frame by `scene_package_system`. The next ambient pass has `requested` empty.
  - **Which producers are in the window**: Quest and stage-result ObScript (`obscript_actions::evaluate_package`, reachable through `obscript_quests.rs:703` and the command table). Skyrim PF_/PRKF_/TERM_ fragments (`Effect::EvaluatePackage`, `fragment/effects/ai_combat.rs:184`).
  - **What is lost**: The actor is not forced due, so the re-evaluation waits for the next in-game-minute gate. The `ReleasePackageHold` marker (#7243) is taken by the next ambient pass and discarded, because `release_hold.contains(&actor)` is consulted only for actors in `due`. A Must Complete package therefore keeps its hold, which is the exact #7243 symptom on this path.
- **Evidence**:
  - `crates/scripting/src/package.rs:685-688`:
    ```rust
    let reevaluate: HashSet<EntityId> = snapshot::<EvaluatePackageRequest>(world).into_keys().collect();
    drain::<EvaluatePackageRequest>(world);
    ```
  - `ai_package.rs:1381-1397`: `requested` is a read-only collect. `release_hold` is collected, then every marker is removed immediately, before the `due` gate.
  - `package_fragments.rs:12`: `scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);`
  - `quest_scene.rs` calls the segments in the order `package_fragments`, `forcegreet`, `obscript_quests`, `scene_actions`.
  - The pin `an_explicit_evaluate_package_ends_a_must_complete_hold` (`ai_package.rs:3128-3153`) calls `request_package_evaluation` and then `ambient_ai_package_system` back to back. It never runs `scene_package_system` between them, and `ambient_ai_package_system_takes_its_release_marker` only checks the drain.
- **Impact**: An `EVP` from a quest or stage-result script, or from a package/perk/terminal fragment, drops the explicit-request semantics. For ordinary packages the effect is a delay of up to one in-game minute (about 2 s at the default time scale). For a Must Complete package the release is permanently lost, so the actor stays on the held package. That is exactly #7243's symptom, which is therefore only fixed for producers outside the window.
- **Related**: #7243, #7264, the `EvaluatePackageRequest` row in the cleanup.rs table (it is not single-owner).
- **Suggested Fix**: Make the marker single-owner for real. Either schedule `ambient_ai_package_system` after `scene_package_system` and let it take both `EvaluatePackageRequest` and `ReleasePackageHold`, with the scene pass only snapshotting. Or stamp a "seen by ambient" side set that gates the scene pass's drain. Add a schedule-order test (`BuiltSchedule::position`) plus a world test that runs both systems in schedule order.

### SCR-D5-01: ObScript `SetPackDuration` calls the explicit-`EvaluatePackage` entry point, so since #7243 it releases a Must Complete hold and drops a running script package, and it never applies the duration

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic (the package seam; the command itself is Dim 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_more/state.rs:373-382` (`cmd_1122`, table row `state.rs:772`); the semantics change is in `crates/scripting/src/package.rs:252-258` (`request_package_evaluation`)
- **Status**: NEW (adjacent: D6-04 `forms` is a write-only sink behind "kept" commands, which is the generic store-and-never-read class; D3-01 is the marker-ordering bug. Neither mentions this command.)
- **Description**: #7243 made `request_package_evaluation` the *explicit-EVP* path. After `drop_script_package(world, actor, true)` it raises `ReleasePackageHold`, so the next ambient selection ignores the running package's Must Complete extension. Its doc says the internal re-evaluations that must keep the hold go through `queue_package_evaluation`. `cmd_1122` is ObScript `SetPackDuration`. Its comment says the value is "kept; the ambient AI re-evaluates the actor when it elapses". Yet it calls `request_package_evaluation` at once, before storing the number. The CS page (`cs-uesp-wiki/(main)/SetPackDuration.wiki`) says it "Sets the duration of the actor's current package to iDuration (in seconds of gametime)". It describes no re-evaluation and no cancelling.
  - Both side effects are unrelated to a duration change. A running script package (`AddScriptPackage` then `SetPackDuration`) is dropped by `drop_script_package(.., only_running = true)`. A Must Complete ambient package with no Done state loses its hold.
  - The stored value (`ObScriptLedger::set_value(0x1122, ..)`) has no reader (`rg 0x1122 crates byroredux` finds only the table row and the id census in `obscript_more/tests.rs`). So the package is never shortened.
  - The unit comment says minutes and the CS page says seconds, so even the stored number is mislabelled.
- **Evidence**:
  ```rust
  // state.rs:373-382
  // CS *SetPackDuration <minutes>*: the current package's duration — kept; the ambient AI re-evaluates
  // the actor when it elapses.
  fn cmd_1122(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
      match caller(world, ctx) {
          Some((form, actor)) => {
              crate::package::request_package_evaluation(world, actor);
              store(world, cmd, form, num(args, 0))
  ```
  ```rust
  // package.rs:252-258
  pub fn request_package_evaluation(world: &World, actor: EntityId) -> bool {
      crate::script_packages::drop_script_package(world, actor, true);
      if let Some(mut releases) = world.query_mut::<ReleasePackageHold>() { releases.insert(actor, ReleasePackageHold); }
      queue_package_evaluation(world, actor)
  ```
  The other `request_package_evaluation` callers are the explicit spellings: `Effect::EvaluatePackage` (`fragment/effects/ai_combat.rs:184`), `RefCollectionAlias.EvaluateAll` (`vm/natives/collections.rs:170`, "Calls EvaluatePackage on all the actors"), ObScript `EVP` (`obscript_actions.rs:139`) and `ResetAI` (`obscript_effects/combat.rs:171`, whose GECK text says packages are "re-evaluated"). Only `SetPackDuration` is a non-EVP caller.
- **Impact**: An Oblivion/FO3/FNV script that shortens its actor's current package ends a running script package, or a held Must Complete package, instead. The command reports Done, so no decline records the gap. The tick that follows is the very package change the script did not ask for.
- **Related**: #7243, #7224, D6-04, D3-01
- **Suggested Fix**: Either decline `SetPackDuration` (the value has no consumer) or implement the duration on the actor's running package. In both cases drop the `request_package_evaluation` call. If an immediate re-evaluation is wanted, use `queue_package_evaluation`, which keeps the hold.

### SCR-D6-01: Object scripts' `Begin MenuMode` blocks never run on non-pausing menu-mode frames
- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_objects.rs:287-295` (doc claim) and `:429`; `byroredux/src/boot/schedule/update/quest_scene/obscript_objects.rs:8-20`; `byroredux/src/menu_stack.rs:179-183`; `byroredux/src/app_events.rs:951-956`
- **Status**: NEW (introduced by 3ef0a2cb2, 2026-11-16, M48 Phase 1 / #6653; not in the 11-15 baseline)
- **Description**: GUI-EVT-006 made `collect_due` skip every object `GameMode` block while the registry holds any menu-mode menu, and added `obscript_object_menu_mode_system` to run `MenuMode` blocks instead. The doc says it is "Scheduled beside [`obscript_object_system`], and run by hand on the frames an open menu pauses the simulation". Only the second half is true. The scheduler registers `obscript_object_system` and `mark_cell_reset` only; the menu-mode system is called solely from `run_menu_mode_frame`, which `run_paused_frame` calls on frames whose simulation is paused. The quest sibling `obscript_quest_menu_mode_system` is registered in `obscript_quests.rs:23`; the object one is not. Oblivion/FO3/FNV `DialogMenu` is `MENU_MODE | CLAIMS_INPUT` with no `PAUSES_GAME` (the 3ef0a2cb2 commit message says "the dialogue menu keeps the world running"), so during every conversation object `GameMode` is suppressed and object `MenuMode` never runs.
- **Evidence**:
  ```rust
  // obscript_objects.rs:429
  BLOCK_GAME_MODE if !menu_mode && !is_stub(entity) => runs.push(Run { block, who: None }),
  // boot/schedule/update/quest_scene/obscript_objects.rs:15-19  (the whole registration)
  scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_state::mark_cell_reset);
  scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_object_system);
  // menu_stack.rs:179-183 — the only non-test caller
  pub(crate) fn run_menu_mode_frame(world: &World, dt: f32) { ... obscript_object_menu_mode_system(world, dt); ... }
  // menu_registry/tests.rs:57
  assert_eq!(fallout("DialogMenu").flags, MenuFlags::MENU_MODE | MenuFlags::CLAIMS_INPUT);
  ```
  `rg obscript_object_menu_mode_system byroredux` finds `menu_stack.rs` only. The unit test `menu_mode_replaces_game_mode_while_a_menu_mode_menu_is_open` calls both systems by hand, so it cannot see the gap.
- **Impact**: any scripted reference with a `Begin MenuMode` block (written for dialogue, `MenuMode 1009`, or "any menu") is dead during conversations and every other non-pausing menu-mode menu. Quest `MenuMode` blocks run in the same situation, so the two script kinds now disagree. `ui.md:886-890` documents the object path as live.
- **Related**: GUI-EVT-006; `/audit-ui` owns the registry and pause policy.
- **Suggested Fix**: register `obscript_object_menu_mode_system` in `register_obscript_objects_systems` right after `obscript_object_system`, as `obscript_quest_menu_mode_system` is, and add a schedule-level assertion (the `boot::SOURCES` style used by `scheduler_access_tests.rs`) that both menu-mode systems are registered.

### SCR-D6-02: Sneak banks experience when nobody is nearby
- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript (skill-use banking gates, `docs/engine/skill-use.md`)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/skill_use_emit.rs:176-183` (`stance_of`) and `:209-212` (`undetected`)
- **Status**: NEW
- **Description**: UESP *Oblivion:Sneak* (local dump `uesp-wiki/Oblivion/Sneak.wiki` line 20): "You gain 0.75 experience points every second (real time) spent moving in sneak mode without being detected, **but only if there is a person or creature nearby**. No experience is gained when sneaking with nobody nearby". The emitter reads "undetected" as `PlayerDetection.level < Noticed` and takes a missing resource as undetected too. `DetectionLevel` defaults to `Lost`, so with no detector in range the stance fires and Sneak banks. `skill-use.md` §3 records that the executable's own gate (a flag at `player+0x5a8`) was "not traced" and that the engine substitutes the detection level. The documented "someone nearby" half of the rule is not in the PRD (OSL-USE-005), `skill-use.md` or the code.
- **Evidence**:
  ```rust
  let undetected = world
      .try_resource::<crate::detection::PlayerDetection>()
      .is_none_or(|detection| detection.level < DetectionLevel::Noticed);
  ...
  } else if sneaking && undetected { Some(Stance::Sneak) }
  ```
  `a_held_key_banks_no_stance_use_while_the_body_cannot_move` and the stance tests never put the player in an empty cell with sneak held.
- **Impact**: Sneak can be trained anywhere, empty rooms included, which UESP names as the thing that does not work; that is the main Sneak power-levelling exploit the rule exists to stop. It also feeds the level-up tallies (Agility).
- **Suggested Fix**: gate the Sneak stance on a documented "someone could detect the player" condition from the detection module (an actor within its detection range). If the engine cannot answer it, name the gap in `skill-use.md` §3 under "Not fed, and why" and decline the stance rather than guess.

### SCR-D6-03: The unknown-operator log-once list is unbounded and scanned linearly per token
- **Severity**: MEDIUM (the shared table says unbounded allocation on mod bytes is HIGH; downgraded because the list can hold at most the distinct tokens already inside the loaded SCDA, so the amplification is CPU, not memory)
- **Dimension**: Legacy ObScript
- **Untrusted-Input**: Yes (SCDA expression bytes)
- **Location**: `crates/scripting/src/obscript_vm.rs:1444-1449` (reached from `eval_expr_value`, `:1021-1033`)
- **Status**: NEW — introduced by b485627e3 (SCR-2026-11-15-01 fix)
- **Description**: the fix for the missing `%` made the unknown-operator arm log "once per token" through a process-wide `static REPORTED: Mutex<Vec<String>>`. The token is whatever space-separated byte run the expression holds (`from_utf8(..).unwrap_or("")`, then `parse::<f32>` falls through to `apply_operator`). An expression is a `u16`-framed payload (up to 65 535 bytes), so one statement can carry about 20 000 distinct garbage tokens. Each unseen token pushes onto the `Vec` and logs a `warn!`; each repeat does `reported.iter().any(..)` over the whole list under the mutex. The `Vec` is never cleared and never capped.
- **Evidence**:
  ```rust
  static REPORTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
  if let Ok(mut reported) = REPORTED.lock()
      && !reported.iter().any(|seen| seen == token) {
          reported.push(token.to_string());
          log::warn!("ObScript expression uses an operator the VM does not know: {token:?} (read as 0)");
      }
  ```
- **Impact**: a hostile or corrupt plugin ships one object-script `GameMode` block with a ~64 KB expression of distinct tokens. After the first run the list holds ~20k entries, and every later frame's run costs on the order of 20k × 10k string compares (about 10^8, tens to hundreds of ms) under a global lock, plus a warn flood on the first run. The same list grows across every script the session loads. The old arm was silent and O(1), so this is a regression in robustness on mod-supplied input, not a correctness change.
- **Suggested Fix**: cap the list (for example 32 entries, then log "further unknown operators suppressed" once) or key a `HashSet` with a length cap on the token text; better, move the report to the per-script compile/attach step (`ObScriptProgram::new`) so it is once per script, not per evaluation.

### SCR-D1-02: Doc and skill text still call `Game.ShowRaceMenu` an auto-accepting placeholder

- **Severity**: LOW
- **Dimension**: Recognizer Chain (placeholder accounting)
- **Untrusted-Input**: No
- **Location**: `docs/engine/ui.md:899-902`; `crates/scripting/src/translate/effects/model.rs:1133-1134`; `crates/scripting/src/obscript_more.rs:105-106`; `.claude/commands/audit-scripting/SKILL.md` Dim 1 "Placeholders are coverage with a hole"
- **Status**: NEW (comment and doc rot)
- **Description**: `bcfe6b5dd` made `Effect::ShowRaceMenu` a real effect. The placeholder set is now `ShowAppearanceMenu`, `RequestSave`, `SetHudCartMode` and `AddAchievement`. The pin `only_the_counter_only_primitives_are_placeholders` agrees with `is_placeholder` (verified below). Four places still describe the old state.
- **Evidence**:
  - `ui.md:900-902`: "`menu RaceSex Menu (S4, accepts the current appearance) from ShowRaceMenu`). `Game.ShowRaceMenu`'s auto-accept placeholder is that fallback."
  - `model.rs:1133`: "a `ShowRaceMenu` inside an `If` is still a stub claimed as coverage (#5975)"
  - `obscript_more.rs:105`: "What the effect already did in the menu's place (`Game.ShowRaceMenu` accepts the current appearance)"
  - SKILL.md Dim 1: "`Effect::is_placeholder` (`ShowRaceMenu`, `RequestSave`, `SetHudCartMode`, `AddAchievement` …)".
- **Impact**: A reader of the coverage harness or the skill believes a ShowRaceMenu inside an `If` is reported as a stub. It is not. The live registry report now only names `ShowAppearanceMenu`.
- **Related**: #4328, #5975
- **Suggested Fix**: Edit the four texts. In ui.md, use the appearance report (`menu LooksMenu (S4, no appearance editor until the slider milestone (GUI-NG-005))`) as the S4-with-fallback example.

### SCR-D2-03: A Papyrus message box the UI queue refuses leaves its ticket waiting forever; the run parks until a load or 64 later tickets
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No (mod scripts can spam boxes, #6012's premise)
- **Location**: `byroredux/src/notifications.rs:48-62` (`route_script_messages`), `crates/scripting/src/messages.rs:159-172` (`issue`), `:190-197` (`take_papyrus_answer`)
- **Status**: NEW (the drop-at-16 bound is #6012, CLOSED; this is its consequence for the new ticketed box)
- **Description**: `route_script_messages` refuses a box when 16 are already waiting ("script message box dropped") and never calls anything for the dropped message. For `MessageAnswer::Papyrus(ticket)` the ticket was already issued into `PapyrusMessageAnswers::waiting`, so `take_papyrus_answer` returns `None` forever and `fragment_continuation_system` keeps the run in `still_pending`. Only `issue()`'s eviction at 64 outstanding tickets or `forget_boxes` on a live load ever releases it. The skill names "a ticket nothing ever answers" as the finding.
- **Evidence**:
  ```rust
  // notifications.rs:51-59
  if boxes.0.len() >= MAX_PENDING_BOXES { log::warn!("script message box dropped, ...") } else { boxes.0.push_back(message); }
  // messages.rs:195-197
  (!answers.waiting.contains(&ticket)).then_some(0)   // None while the ticket is "waiting"
  ```
- **Impact**: A quest run (e.g. a polled `OnUpdate` handler that `Show()`s, or any flood of boxes) that hits the cap hangs silently; the continuation also occupies a `FragmentExecutionQueue` slot and is re-polled every frame.
- **Related**: #6012, SCR-D2-02.
- **Suggested Fix**: In the drop branch, if `message.answer` is `Papyrus(ticket)`, call `record_papyrus_answer(world, ticket, 0)` (the box's default) so the run resumes with button 0 instead of hanging.

### SCR-D2-04: The Message.Show() value path takes six resources inline under the quest pair; the deferral rule and the apply_effect lock doc no longer describe it
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/vm/interp.rs:1192-1224`, `crates/scripting/src/messages.rs:525-565` (`show_message_awaiting` / `queue_message` / `alias_name` 463-496), `crates/scripting/src/story_manager.rs:783-786`
- **Status**: NEW
- **Description**: The lowered/statement `Message.Show` defers onto `DeferredFragmentEffects::messages` and `show_message` runs after the quest guards drop. The value form calls `show_message_awaiting` straight from the interpreter, i.e. under `QuestStageState`/`QuestObjectiveState` WRITE: `MessageCatalog` (read ×2), `PapyrusMessageAnswers` (write), `Globals` (read, `{Global=}` tags), `SceneQuestAliasRegistry`, `LocationAliasBindings`, `SceneActorBindings` (reads, `{Alias=}` tags) and `PendingScriptMessages` (write). 10f7c16fc already hit one cycle from this (`QuestStageState → MessageCatalog → SceneQuestAliasRegistry → QuestStageState`) and fixed it only by making the catalog a leaf. The direct edge `QuestStageState(W) → SceneQuestAliasRegistry(R)` for an alias-tag message still opposes `story_manager_dispatch_system`, which holds the registry (`aliases`) across its `try_resource::<QuestStageState>()` read. No unit test shows a box with an alias tag under the quest pair, so `BYRO_LOCK_ORDER_CHECK` cannot see it.
- **Evidence**:
  ```rust
  // story_manager.rs:783-786
  let aliases = world.try_resource::<crate::scene::SceneQuestAliasRegistry>();
  let stages = world.try_resource::<QuestStageState>()...
  // interp.rs:1219  (under the pair)
  match crate::messages::show_message_awaiting(self.world, form, &numbers) {
  ```
- **Impact**: Not a runtime deadlock today — every holder of the pair and the story manager are `add_exclusive` — but it breaks the "touches a resource outside the quest pair defers" rule and rests the safety solely on exclusive scheduling; the apply_effect doc inventory does not name these types (the VM-native disclaimer covers it only in prose).
- **Related**: 10f7c16fc, #6667.
- **Suggested Fix**: Issue the ticket inline (one resource) and push a `PendingMessage::ShowAwaiting(form, args, ticket)` flushed with the other deferred messages after the guards drop; add an alias-tag Show test under `BYRO_LOCK_ORDER_CHECK`.

### SCR-D2-05: `Message.Show()` resume paths: an unwound `AWAITS_MESSAGE` stop is uncaught in the resumed-`While` path, and two `Show()` calls in one statement exchange answers
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/vm/interp.rs:378-381` and `:457-461` (`resume_statement` `While` arm), `:394-396` (the only catch), `:88` / `:131` (`answer` slot)
- **Status**: NEW
- **Description**: (a) The `AWAITS_MESSAGE` stop is converted to a suspension only inside the statement loop of `exec_block`. After a `Utility.Wait` inside a `While` body whose *condition* contains `Msg.Show()`, resumption goes `resume_statement` → `Flow::Normal => self.exec_while(...)`; the condition's `Show` stops with `AWAITS_MESSAGE` through `?` and no catch, so `run_fragment` logs "stopped: a Message.Show() awaiting its answer" and the run ends (box raised, answer discarded). (b) `VmResume.answer` is one slot and `run_fragment` resets it to `None` on every suspend. In `If A.Show() == 0 && B.Show() == 1` the replay after A's answer raises B and suspends; after B's answer the replay's *first* `Show` (A) takes B's answer, so A reads B's button and B raises again.
- **Evidence**:
  ```rust
  // interp.rs:459 — resume path, no AWAITS_MESSAGE catch
  Flow::Normal => self.exec_while(frame, &condition.node, body),
  // interp.rs:1211
  if let Some(button) = self.answer.take() { return Ok(Some(button)); }
  ```
- **Impact**: Rare script shapes; a dead run after an unanswerable box, or a wrong value (not a guess-free decline) for the multi-`Show` statement. Neither is rejected by the checker.
- **Related**: SCR-D2-02.
- **Suggested Fix**: Catch `AWAITS_MESSAGE` in the resume arms too (or restart a resumed `While` through the statement loop); have the checker decline a statement containing more than one value-`Show`.

### SCR-D2-06: apply_effect's lock inventory omits the `Sneaking` marker and the `sneaking_enabled` pair that the M65 effect/guard take under the quest pair
- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/effects/player_control.rs:35-43` (`Effect::StartPlayerSneaking` → `crate::stance::toggle_sneaking`), `fragment/effects/guards.rs:88-98` (`FragmentGuard::ActorSneaking` → `stance::is_sneaking`), `crates/scripting/src/stance.rs:40-79`, doc at `fragment/effects.rs:78-155`, scan at `fragment/tests.rs:4728-4788`
- **Status**: NEW
- **Description**: `toggle_sneaking` write-locks `Sneaking` and `sneaking_enabled` reads `PlayerControlState` and `InputLayers`; `is_sneaking` read-locks `Sneaking`. All run under the paired guards. `Sneaking` appears nowhere in the apply_effect doc block. The #3949 scan reads `fragment::SOURCES`, `magic.rs`, `equipment.rs` and `created_references.rs` (the `IN_CREATED_REFERENCES` precedent of #6926 shows the mechanism for adding a call-out module); `stance.rs` is not among them, so the test stays green. The same commit range did keep `SightQuery` current in the doc, so this is an isolated omission.
- **Evidence**: `grep -n Sneaking crates/scripting/src/fragment/effects.rs` returns no line in the doc block; `player_control.rs:39` `crate::stance::toggle_sneaking(world, player)`.
- **Impact**: The running lock inventory (the re-derivation baseline for any ABBA analysis) is incomplete; `Sneaking` ↔ `Awareness`/detection ordering was not reviewed against it.
- **Related**: #6910 (M65 Phase 1), #6926.
- **Suggested Fix**: Name `Sneaking` (write/read), `PlayerControlState`/`InputLayers` (read) under the "via `crate::stance`" bullet and add `stance.rs` to the scan inputs.

### SCR-D3-02: `RecurringUpdate` burst-fires after a long stall, against its documented "missed fires drop" contract
- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/recurring_update.rs:139-176`, test `recurring_update/tests.rs:71`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The module doc says missed fires drop and "burst-fire on resume … is uniformly worse". The re-arm is `seconds_until_next += interval_secs`. After a stall of N intervals the counter is still negative, so it fires again on every following frame until it is positive. A `dt` of 5.0 on a 0.5 s subscription fires on that tick, and then on about 9 consecutive subsequent ticks. The test `dt_overshoot_fires_only_once_per_tick` checks one tick only, so it cannot see this.
- **Evidence**: `recurring_update.rs:163-164`:
  ```rust
  // (the missed-fire contract — see module doc).
  ru.seconds_until_next += ru.interval_secs;
  ```
  The arithmetic is 0.5 - 5.0 = -4.5, then +0.5 = -4.0. The next tick at dt 0.016 gives -4.016 <= 0, which fires and re-arms to -3.5, and so on. `app_events.rs:1573-1608` passes the unclamped `wall_dt`.
- **Impact**: Demo-only today. Nothing in production inserts `RecurringUpdate` (only `dlc2_ttr4a.rs:210`; the #5842 comment in `byroredux/src/boot/schedule/update/player_animation/recurring_update.rs` says no recognizer emits it). It would be real the day a recognizer registers a handler.
- **Related**: none
- **Suggested Fix**: After a fire, clamp the counter to a single interval (`seconds_until_next = interval_secs - overshoot.rem_euclid(interval_secs)`, or reset). Extend the test to tick twice.

### SCR-D3-03: Two new `HitEvent` producers resolve `weapon_ignores_resistance` while holding the HitEvent write guard
- **Severity**: LOW
- **Dimension**: Event Runtime (lock discipline)
- **Location**: `byroredux/src/combat.rs:310-347`, `byroredux/src/systems/combat_ai.rs:266-291`
- **Status**: NEW (introduced by fba412ebb)
- **Untrusted-Input**: No
- **Description**: In both producers the struct literal passed to `events.insert(..)` calls `crate::combat::weapon_ignores_resistance(world, ..)`, which takes `world.get::<EquippedWeapon>` and `try_resource::<LoadedCellIndex>`. Both run under the live `query_mut::<HitEvent>()` guard (HitEvent write, then EquippedWeapon read, then a resource read). The sibling producer in the same commit window resolves before the guard: `systems/vats.rs:959` ("resolved before the events guard is taken").
- **Evidence**: `combat_ai.rs:291`: `ignores_resistance: crate::combat::weapon_ignores_resistance(world, decision.entity),` inside `for decision in decisions.iter_mut() { … events.insert(..) }` under `if let Some(mut events) = world.query_mut::<HitEvent>()` (line 266). `combat.rs:347` is the same shape inside `events.insert(target, HitEvent { … })`.
- **Impact**: No inverse edge exists today. I grepped every `query_mut::<EquippedWeapon>` and `resource_mut::<LoadedCellIndex>` site: `crafting.rs:410`, `inventory/loot.rs:293,324`, `fragment/effects/inventory.rs:117`, `script_requests.rs:627`, `relocated.rs:1074`. None of them holds HitEvent at the time. It is a latent edge for `BYRO_LOCK_ORDER_CHECK`.
- **Related**: none
- **Suggested Fix**: Hoist the value into a local before taking `events`. `combat_ai` can collect it into `decisions`, as vats does for `sneaks`.

### SCR-D3-04: cleanup.rs's Pattern-B table and summary paragraph still describe the pre-#6150 drain shapes
- **Severity**: LOW (doc rot)
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/cleanup.rs:47-48,57`; the Dim 3 text of `.claude/commands/audit-scripting/SKILL.md` (the `HavokImpulseRequest` sentence)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The `MotionTypeChangeRequest` row says "the one tail-drain — it removes exactly the entities it snapshotted, after an empty-set early return". At HEAD `scripted_motion_type_system` opens with `let asked = super::take_requests::<MotionTypeChangeRequest>(world);` (`systems/cinematic.rs:417`), a head drain, and `take_requests` is used identically by all eight binary consumers (`systems.rs:113`). The `HavokImpulseRequest` row's "snapshots every request and drains exactly those; … a missing HavokImpulseRules drains the snapshot before returning" is likewise now by construction. The behaviour is correct; the table misleads a future author about which drain shape is canonical.
- **Related**: #6150 (the `take_requests` extraction)
- **Suggested Fix**: Rewrite the rows as "head drain via `take_requests` (#6150)". Drop the tail-drain wording.

### SCR-D3-05: `pattern_b_markers_survive_event_cleanup` is a hand-kept list that omits `ReleasePackageHold`
- **Severity**: LOW (test gap)
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/cleanup.rs:286-355`
- **Status**: NEW. #7264 added the drain half for this row, not the survive half.
- **Untrusted-Input**: No
- **Description**: The table has 21 markers and `survives(..)` has 20 calls; `ReleasePackageHold` is missing. If someone adds it to `event_cleanup_system`, the new marker is wiped before `ambient_ai_package_system` runs. `every_pattern_b_table_row_has_a_drain_test` (`request_drain_tests.rs`) matches `drains("X"` only, so it does not enforce the survive half.
- **Suggested Fix**: Add `survives("ReleasePackageHold", byroredux_scripting::package::ReleasePackageHold)`, or extend the table-driven check to require a matching `survives(` call.

### SCR-D4-01: Door runtime parks a script-state row for every unscripted in-cell door that nothing ever consumes, and a scripted door ignores the re-entry default
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/reference_state.rs:499` (`store.scripts.extend(scripts)`), `:505-:547` (`capture_script_states`), `:552` (`restore_script_state`); sole spawn-time consumer `byroredux/src/cell_loader/references/attach.rs:216-220`; test `byroredux/src/systems/door.rs:390-402`
- **Status**: NEW, but overlaps the open #7059. Its acceptance criterion, "a door returns to its default on same-session re-entry", is not met on the real unload path. I treat the leak and the test gap as findings, not as an already-listed gap.
- **Description**: Before door-runtime commit `50abfd008`, a `TwoStateActivator` existed only on a reference with an attached script. That script's attach consumed the parked row through `restore_script_state`. `attach_door` (`byroredux/src/systems/door.rs:193`) now stamps a `TwoStateActivator` on every in-cell door whose model authors `Open` and `Close` sequences, with or without a script. The rest of the path is unchanged:
  - `capture` (called from `unload_cell` and `evict_actor`) parks `ScriptState { two_state: Some(..) }` for every victim that has a `FormIdComponent`. The `states.retain(|(_, s)| !s.is_empty())` filter keeps the row.
  - `restore_script_state` is called only inside `if attached {` in `attach_script_for_refr`. An unscripted door never attaches, so its row is never removed. `store.scripts.remove` at `reference_state.rs:558` is the only removal in the tree.
  - The `scripts` map is serialized (`SavedPersistentReferenceStates.scripts`, `reference_state.rs:208` and `:247-249`), so every visited unscripted door leaves a permanent saved row.
  - A door that does attach a script, such as an ObScript `SCRI` or a recognized or VM Papyrus script, gets the parked `is_open` restored over the `Open by Default` seed. That is the #6071 behaviour. It contradicts the door runtime promise and the Phase 1 acceptance criterion of #7059. So re-entry default behaviour differs between scripted and unscripted doors.
- **Evidence**:
  ```rust
  // attach.rs:216-220 — the only spawn-time consumer, gated on a script having attached
  if attached {
      // #6071 — the state an earlier visit left, over the authored defaults ...
      crate::cell_loader::reference_state::restore_script_state(world, entity);
  ```
  ```rust
  // door.rs:394-398 — the re-entry test never runs capture / restore_script_state
  world.query_mut::<TwoStateActivator>().unwrap().get_mut(first).unwrap().is_open = !default_open;
  world.despawn(first);
  let second = placed_door(&mut world, default_open);
  ```
  The `a_door_returns_to_its_default_state_on_reentry` test does a bare `despawn`. It bypasses `reference_state::capture` and the script attach, so it passes only because it does not exercise the parking path.
- **Impact**: Unscripted doors give a bounded but permanent growth in the saved `PersistentReferenceStates.scripts`. It is one small row per distinct door ever evicted, bounded by the number of doors, which is about 1.7k sequence placements in Oblivion. Scripted doors are not reset on re-entry while unscripted ones are, so one design promise gives two behaviours. For a scripted door the original behaviour is unsourced (#7062 lists the default-state question), so I rate this LOW and not MEDIUM.
- **Related**: #6071 (closed, the parking design), #7226 (closed, a door parked mid-swing, which can only happen on the scripted path), #7059, #7062.
- **Suggested Fix**: Drop the `two_state` capture for a door that carries a `Door` component, or have `capture_script_states` skip an entity whose `TwoStateActivator` has no script attached. Alternatively consume the row for it at `attach_door` time. Then make the re-entry test go through `capture` and the real attach, so it pins the decision either way.

### SCR-D4-02: A recognized `default2StateActivator` script overwrites the `Open by Default` state `attach_door` just seeded
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Untrusted-Input**: Yes. The recognizer reads `isOpen` and `doOnce` from modded VMAD or `.pex` property values, but this is no new decode path.
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:917` (`attach_door`, primary-synth block) versus `:980` (`attach_script_for_refr`); `crates/scripting/src/translate/recognizers/two_state_activator.rs:44`
- **Status**: NEW (hedge: overlaps the #7059 acceptance criterion that `SetOpenState`, Papyrus `SetOpen` and an activation drive one transition, so a maintainer may fold it in)
- **Description**: `attach_door` seeds `TwoStateActivator { is_open: default_open, .. }` from the REFR's `ONAM`. About 60 lines later the same function attaches scripts. A recognized `default2StateActivator` runs `world.insert(entity, state)`, and `state.is_open` comes from the script's `isOpen` property, defaulting to `false`. The insert replaces the whole component. A door that is `Open by Default` and carries that script therefore spawns closed. The `doc` comment says the seed and the recognizer coexist ("the Papyrus recogniser keeps filling it"), but nothing orders them. The overwrite also replaces the seed's `is_open` before `door_motion_system` poses the door, so the pose follows the wrong state. I could not find a vanilla door that combines both. That is unverified, and the severity reflects it.
- **Evidence**:
  ```rust
  // synth_child.rs:915-918
  if let Some(door) = stat.door.filter(|_| placed_ref.teleport.is_none()) {
      crate::systems::door::attach_door(world, placement_root, door, record_index.game, placed_ref.open_by_default);
  }
  ...
  // synth_child.rs:980 -> attach_vmad_scripts -> (recognized.spawn)(world, entity)
  // two_state_activator.rs:44
  world.insert(entity, state);   // is_open = script prop, default false
  ```
- **Impact**: The `Open by Default` placement contract (DRT-REC-004, DRT-STATE-001) breaks for scripted doors of that one archetype. The door's pose follows the script's default instead.
- **Related**: #7059, #7060 (Skyrim, where `default2StateActivator` is the common Papyrus door/lever script).
- **Suggested Fix**: Run `attach_door` after the script attach and merge it (set `is_open` only if the script did not author `isOpen`), or have the recognizer preserve an existing seeded state.

### SCR-D4-03: The skill points at the wrong source pin, and the XCT stub spawn has no pin of its own
- **Severity**: LOW
- **Dimension**: Attach & Reference Identity (comment and guard rot)
- **Untrusted-Input**: No
- **Location**: `.claude/commands/audit-scripting/SKILL.md` Dim 4 ("the source pin in `cell_loader/references/source_pin_tests.rs` rejects the bare spelling"); `byroredux/src/cell_loader/relocated.rs:890`
- **Status**: NEW
- **Description**: `references/source_pin_tests.rs` holds three tests (`npc_spawn_jobs_are_resumable_and_wall_clock_timed`, `actor_spawn_branch_attaches_vmad_scripts`, `scol_expansion_is_cached_across_a_budget_yield`). None greps for the bare `spawn_logical_quest_reference(` spelling. The real pins are:
  - `scene/scene_runtime.rs:704-724`
  - `cell_loader/exterior/logical_stub_source_pin_tests.rs`
  - `references/synth_child_tests.rs:472` and `:522` (the helper behaviour test is `logical_quest_reference_spawns_identity_and_attaches_scripts` at `:538`)

  XCT's `reconcile_logical_identities` (`relocated.rs:890`) calls `spawn_logical_quest_reference_with_scripts` correctly today but has no pin. A future edit of that loop to the bare spawner would not fail any test.
- **Evidence**: `rg "spawn_logical_quest_reference_with_scripts" byroredux/src/cell_loader/relocated.rs` gives only the call at `:890`. There is no `production_text` scan of that file.
- **Impact**: This is the recurrence the #4112/#4331/#5650 pins exist to stop, and one of the four callers has none.
- **Related**: #4112, #4331, #5650.
- **Suggested Fix**: Add a `production_text` pin for `relocated.rs`. Correct the skill sentence to name the three real pins.

### SCR-D5-02: #7115's `FollowUpTopics` cache has no test; a missing resource makes every follow-up silently never fire

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic (dialogue)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/npc_dialogue.rs:738-763` (`FollowUpTopics`, `follow_up_topic`) and `:771-826` (`npc_dialogue_follow_up`)
- **Status**: NEW (test gap; the fix itself checks out, see Verified)
- **Description**: The cache's own invariants are the Weak keying (`Weak::ptr_eq`, rebuild on index swap) and agreement with the uncached `values().find_map` scan. Neither is pinned. `rg 'follow_up|FollowUp' byroredux/src` finds only the system, the row declaration, the init and the registry allow-list entry. The `tests` module at `:1197` has no case for `npc_dialogue_follow_up` or `follow_up_topic`. The only exercise is the live Doc Mitchell route (smoke dt1). `follow_up_topic` does `world.try_resource::<FollowUpTopics>()?`, so a world without the resource (a harness that does not go through `build_world`, which does `init_resource` at `boot/world.rs:466`) answers `None` and the loop `continue`s past every follow-up with no log.
- **Evidence**: `let cache = world.try_resource::<FollowUpTopics>()?;` (`npc_dialogue.rs:750`)
- **Impact**: A future change to the keying (for example a clone of the index) or a regression in the `entry().or_insert` order would silently stop the follow-ups (FNV's opening Pip-Boy and gear hand-out). Nothing but the live smoke would notice.
- **Related**: #7115, #7114
- **Suggested Fix**: Add two unit tests in `npc_dialogue.rs`: (1) two indexes swapped through `LoadedCellIndex` rebuild the map, and a follow-up INFO moved between DIALs resolves to the new owner; (2) `follow_up_topic` equals the old scan for every INFO of a small multi-DIAL index. Optionally log once when the resource is missing.

### SCR-D6-04: `ObScriptLedger::forms` is a write-only sink behind eleven "kept" commands
- **Severity**: LOW
- **Dimension**: Legacy ObScript (ledger honesty, #6190)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_ledger.rs:64-67,187-193` (the map and `form()`); writers `obscript_more/state.rs:325-334` (`cmd_111c`: ids 0x111c `SetActorFullName`, 0x10b1 `SetCombatStyle`, 0x1171, 0x11b7), `tail.rs:227,464,510,543,728,740,796`, `actions.rs:252,263,274`
- **Status**: NEW (the closed #6190 covered five other fields; `forms` was not among them)
- **Description**: the ledger module doc says a command "whose value nothing in the engine would read back is not stored here: it declines". `set_form` has 11 production call sites (SetActorFullName/SetCombatStyle/SetTalkingActivatorActor/SetMerchantContainer, PipBoyRadio Tune, SetDisposition, MatchFaceGeometry, SetNoRumors and friends, SetCellOwnership, SetCellFullName, ForceActiveQuest, SetCellImageSpace, SetLastExtDoorActivated, SetNPCRadio). `form()` has no production caller anywhere: `rg '\.form\(' crates byroredux` outside tests finds only the accessor itself, and the field is private. The commands return `done(..)`, so `ObScriptQuestTimers`/the effect log count them as run, and the map is serialised into saves. `docs/engine/scripting.md:2690` lists `SetCombatStyle` and `SetTalkingActivatorActor` under "What stays declined", but the code stores and reports done.
- **Evidence**:
  ```rust
  // state.rs:325
  fn cmd_111c(...) -> Action { match caller(world, ctx) { Some((subject, _)) => {
      let form = form_arg(args, 0).unwrap_or(0);
      ledger_mut(world, |l| l.set_form(cmd, subject, form));
      done(format!("{cmd:#06x} {subject:08X} {form:08X}")) ...
  ```
  The `values` map has the same shape for many ids: readers exist for only eight keys (0x1212, 0x125c, 0x10d3, 0x1151, 0x116c, 0x118b, 0x10e7, 0x1129); for example 0x114e, 0x10bb, 0x10d8, 0x1156, 0x1186 and 0x124b are stored by `store(..)` and appear nowhere outside `obscript_more/` (`EnableLinkedPathPoints` is documented as such; the others are not).
- **Impact**: no gameplay effect, which is the point: a script's `SetCombatStyle` is reported as run and saved while nothing changes, so coverage metrics overstate and the doc table is wrong. Same class as #6190 / SCR-D5-2026-09-14-04.
- **Suggested Fix**: either decline each of these commands (`Action::Declined`, drop the `forms` map and its save column, a `FORMAT_MAJOR` question for `/audit-save`) or wire the consumer; at minimum correct the `scripting.md` table and list the stored-but-unread commands there.

### SCR-D6-05: Two skill-use rule claims rest only on a disassembly
- **Severity**: LOW
- **Dimension**: Legacy ObScript (owner rule: no rule sourced from the executable)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/oblivion_skill_use.rs:236-265` with `obscript_more/state.rs:34-39` (`AdvancePCSkill` keeps "the executable's banked − cost, clamped at 0"); `crates/core/src/character/skill_use.rs:230-232` (`ARMOR_HIT_ATTEMPTS = 7`, "`0x5e5a00` loops seven times") with `byroredux/src/skill_use_emit.rs:77-98`
- **Status**: NEW (a pending owner question per the 2026-11-20 ruling already covers the player-spellcasting numbers; these two are in Dim 6 code and not listed there)
- **Description**:
  - *`AdvancePCSkill` surplus.* CS *ModPCSkill* documents two behaviours: unpatched `usage -= old limit` and Beta Patch 1.1 "setting your uses value to 0.0"; PRD Q5 chose the patched one for `ModPCSkill`. CS *AdvancePCSkill* says only "as if the player earned the skill by use or training. This function is subject to the same issue as ModPCSkill". The engine gives `AdvancePCSkill` a third behaviour (`(banked - cost).max(0.0)`), and its comment names the executable as the source. Neither the unpatched nor the patched page behaviour matches it.
  - *Armor re-roll count.* CS *iArmorDamageXXChance* documents the five weights (defaults 10/25/15/10/10, "percentage of damage absorbed"); UESP *Armor* says "the exact mechanism for determining which armor pieces you are wearing take how much damage is unclear" and UESP *Light Armor* says only that a hit on covered body earns 1.5. The "roll, then re-roll up to seven times on an empty slot" mechanism and the shield-as-remainder come from the `0x5e5a00` decode (`skill-use.md` §3 "decoded this phase"), dated before the ruling.
- **Evidence**: `banked = if keep_surplus { (banked - cost).max(0.0) } else { 0.0 };` and `pub const ARMOR_HIT_ATTEMPTS: usize = 7;`.
- **Impact**: low and bounded (a script-only command; a Light/Heavy Armor split for mixed outfits), but the behaviour is presented as a rule rather than a declared gap.
- **Suggested Fix**: follow the ruling: for `AdvancePCSkill`, either adopt the Q5 behaviour (0.0) because CS ties it to `ModPCSkill`, or record the surplus rule as an owner decision; for the armor roll, state in `skill-use.md` that only the weights are documented and that the attempt count and shield remainder are an engine choice, or re-source them.

### SCR-D7-01: `papyrus_provider_system`'s access row omits `NativeActivation`, which it reads through `papyrus_activations`
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Untrusted-Input**: No
- **Location**: `byroredux/src/boot/schedule/late/script_hosts.rs:42-58` (row) vs `crates/scripting/src/papyrus_provider/execute.rs:173-174` (acquisition); scan root `byroredux/src/boot/schedule/mod.rs:735-736,1142-1146`
- **Status**: NEW (no matching issue; closed #3951/#4574 cover the same class, for other types)
- **Description**: The body calls `crate::papyrus_activations(world)`. That function (`crates/scripting/src/events.rs:63-73`) takes `query::<ActivateEvent>()` and `query::<NativeActivation>()`. The sibling row `legacy_obscript_load_order_system` declares it with a #6218 comment. The provider row declares `ActivateEvent` but not `NativeActivation`. `papyrus_provider_system_declares_everything_it_acquires` passes anyway, because its only scan root is `execute.rs` and `events.rs` is not a listed hop (the scan follows callees only within a listed file).
- **Evidence**:
  ```rust
  // execute.rs:173-174
  let activated: BTreeMap<EntityId, EntityId> =
      crate::papyrus_activations(world).into_iter().collect();
  // script_hosts.rs: papyrus_provider row lists .reads::<ActivateEvent>() ... no .reads::<NativeActivation>()
  // script_hosts.rs:30 (legacy row only): .reads::<byroredux_scripting::NativeActivation>()
  ```
- **Impact**: None today. The system is an exclusive, so the declaration does not affect scheduling. It is the comparison baseline if the system is ever promoted to a parallel lane, and an under-declaration makes that promotion look safe (#3473/#3951 rationale).
- **Related**: #6218 (`NativeActivation`), #3951.
- **Suggested Fix**: Add `.reads::<byroredux_scripting::NativeActivation>()` to the row. Add `(EVENTS_SRC, "papyrus_activations")` to the `papyrus_provider_system` scan list so the next hop is not silently missed.

### SCR-D7-02: the provider once-only `OnInit` flag is stamped for every `OnInitEvent` entity, including entities with no provider program
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Untrusted-Input**: No (growth is bounded by the number of scripted refs a plugin places)
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:27-55,155,220`
- **Status**: NEW (the #6017 fix is intact; this is a scope slip in it)
- **Description**: `initialized` is every entity carrying `OnInitEvent`. `attach_script_for_refr` (`byroredux/src/cell_loader/references/attach.rs:230`) gives `OnInitEvent` to every identity-only scripted ref, whichever script kind attached (VM or ObScript included). `first_initializations` runs on that whole set before the `PapyrusProviderProgram` query is taken. It writes `::init::papyrus_provider = 1.0` into the saved `QuestScriptVariables` under the ref's FormID for each one. For a ref with no provider program the flag is never read, so it is dead state in every save.
- **Evidence**:
  ```rust
  let initialized = world.query::<OnInitEvent>() ... .collect::<BTreeSet<_>>()   // :155
  let initialized = first_initializations(world, initialized);                     // :220
  // in first_initializations:
  variables.set(owner, key, 1.0);                                                   // :51
  ```
- **Impact**: Save-size growth, one row per identity-only scripted ref (logical actor stubs, `placement_disabled` bodies, markers) that does not use the provider seam. No functional misbehaviour was found: the VM's own `::init::<script>` flags use different keys.
- **Related**: #6017, #5997.
- **Suggested Fix**: Intersect `initialized` with the entities that hold a `PapyrusProviderProgram` (a cheap `programs.get(e).is_some()` filter, done before stamping), or stamp only inside the `enqueue(OnInit)` branch.

### SCR-D7-03: the `feature-matrix.md` extender row still says "unaudited" with 2026-09-10 LOC figures
- **Severity**: LOW
- **Dimension**: Provider/Extender Layer (doc-rot)
- **Untrusted-Input**: No
- **Location**: `docs/feature-matrix.md:215`
- **Status**: NEW (#3953 closed the "silent row" gap and added this row; it has not been refreshed since)
- **Description**: The row says "~23.9k LOC across `crates/sdk/src/` (14 245), `papyrus_provider/` (6 377) and `compatibility.rs` + `obscript*.rs` (3 294), measured 2026-09-10. No dedicated audit pass has run over it". Dim 7 has been a dedicated pass since the 2026-09-14 skill revision, and AUDIT_SCRIPTING_2026-10-11/-10-17 recorded Dim 7 findings. The current live counts are `crates/sdk/src` 14 701, `papyrus_provider/` 7 634 (8 743 with `compatibility.rs`), and `obscript*.rs` has since grown (the 3 294 figure is not comparable).
- **Evidence**: `docs/feature-matrix.md:215` (quoted above); `find crates/scripting/src/papyrus_provider -name '*.rs' | xargs wc -l` gives 7 634.
- **Impact**: The status floor understates audit coverage and the LOC are stale. Doc rot only.
- **Related**: #3953 (closed).
- **Suggested Fix**: Re-measure the LOC and replace "No dedicated audit pass has run over it" with a pointer to `/audit-scripting` Dim 7. Keep the "not verified against real mod content" caveat.

## Future-phase readiness

- The door runtime (#7059, open) has two attach-path seams to settle before it closes:
  - eviction-row restore against the "returns to default on re-entry" criterion (SCR-D4-01);
  - a recognized `default2StateActivator` overwriting the `Open by Default` seed (SCR-D4-02).
- Before M48 widens `Message.Show()` use beyond the current fragments, its latent path needs to be closed: SCR-D2-01, -02, -03 and -05.
- Skill text to sync (`/audit-sync`):
  - Dim 1 and the docs still call `ShowRaceMenu` a placeholder (SCR-D1-02).
  - Dim 4 cites the wrong source pin (SCR-D4-03).
  - The `ReleasePackageHold` "not yet run by the drain-pin test" sentence is stale since #7264 / #7273.
  - `sources_completeness` is a module, not a test fn.
  - The deferred queue count is 73 fields, not ~50.
