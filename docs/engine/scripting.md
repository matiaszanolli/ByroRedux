# Scripting Architecture: From Papyrus VM to ECS-Native

> **Status (verified 2026-08-25).** The ECS-native scripting model is now a
> production subsystem rather than only the R5 design proof. M47.0/M47.1 and
> the `.psc` parser remain shipped; the runtime now also decodes `.pex`, lowers
> and dispatches QUST and SCEN fragments, resolves quest aliases and VMAD
> properties, persists fragment continuations, globals, quest progress and
> reference-enable state, and applies a growing conservative `Effect` catalog.
> The live code lives in [`crates/scripting/`](../../crates/scripting/) and
> [`crates/papyrus/`](../../crates/papyrus/). Sections marked **DESIGN** are
> still forward-looking; sections marked **SHIPPED** describe code in the tree
> today. See [Current Implementation](#current-implementation-shipped) for the
> ground-truth map of what exists.

## Three Generations of Bethesda Scripting

### ObScript (Morrowind → Fallout: New Vegas)

The original scripting system ran synchronously in the game loop:

- **GameMode block** executed every frame — any timed behavior required manual
  `GetSecondsPassed()` tracking and state variables
- No user-defined functions, no loops, no states — just sequential IF/ELSE
- `set variable to 25` syntax, `Begin OnTriggerEnter Player` events
- Single-threaded: script execution directly affected framerate
- All variables public, no encapsulation

ObScript was simple but limited. Complex behavior required dozens of tracking
variables and deeply nested conditionals. The per-frame execution model meant
heavy scripts visibly dropped FPS.

### Papyrus (Skyrim → Fallout 4)

Papyrus introduced modern language features but moved scripts into a VM:

- User-defined functions, While loops, states, latent functions (Wait, MoveTo)
- Properties with editor configuration and optional get/set validation
- Multithreaded VM — scripts time-sliced, don't directly affect framerate
- 136 events (Fallout 4), type-safe compilation, script inheritance

But the VM architecture created new problems: *"Telling a script to wait for
0.5 seconds really means 0.5 seconds plus the time it takes for the script to
get its number called."* Under load, this queue delay grows to seconds.
Multiple instances of the same event can run simultaneously (e.g., two
OnTriggerEnter events overlapping), requiring States or control variables.

### ByroRedux (this engine)

Takes the best of both: **ObScript's deterministic synchronous execution** with
**Papyrus's modern features**, implemented as ECS components and systems. No VM,
no event queue, no separate threading model. Scripts run in the ECS scheduler
like any other system — deterministic timing, zero overhead for idle scripts,
and component data is the only state that exists.

This is no longer purely a design claim. The **R5 prototype** hand-translated
`defaultRumbleOnActivate.psc` — a real 50-LOC Skyrim script attached to
hundreds of vanilla references (pressure plates, ritual buttons, shrines) —
into plain ECS components + dt-driven systems, and validated that all three
hard cases (latent `Utility.Wait()`, multi-state dispatch, cross-subsystem
call) translate cleanly with no VM, no fibre, no suspendable script frame. The
load-bearing finding: **a Papyrus event handler with a latent wait splits into
two systems** — code-before-`Wait` runs on the event, code-after-`Wait` runs on
whichever frame the dt counter hits zero. That is the entire pattern. The
verdict was *go ECS-native*; full evaluation at
[`docs/r5-evaluation.md`](../r5-evaluation.md), reference fixtures at
[`docs/r5/source/`](../r5/source/).

| | ObScript | Papyrus | ByroRedux |
|---|---|---|---|
| Execution | Synchronous, per-frame | Async VM, time-sliced | Synchronous ECS scheduler |
| Framerate impact | Direct | None (but script lag) | None (queries skip empty) |
| Functions | No | Yes | Yes (Rust functions) |
| Loops | No | While | Rust loops |
| States | No | GoToState | Enum component field |
| Save safety | Variables only | Stacks + properties (fragile) | Components only (robust) |
| Timing | Deterministic | Non-deterministic (queue) | Deterministic |
| Mod conflicts | Variable collision | Script-level override | Per-component, auto-merge |

---

## Current Implementation (SHIPPED)

The scripting crate ([`crates/scripting/`](../../crates/scripting/)) is the
ECS-native event runtime. It registers its storages via `byroredux_scripting::register(&mut world)`
([`crates/scripting/src/lib.rs`](../../crates/scripting/src/lib.rs)), called
once during engine init. Test counts change quickly; the authoritative
workspace total is maintained in [ROADMAP.md](../../ROADMAP.md#project-stats).

### Module map

| Module | File | Role |
|---|---|---|
| Event markers | [`events.rs`](../../crates/scripting/src/events.rs) | Transient marker components (the event types) |
| Timers | [`timer.rs`](../../crates/scripting/src/timer.rs) | `ScriptTimer` + `timer_tick_system` |
| Recurring updates | [`recurring_update.rs`](../../crates/scripting/src/recurring_update.rs) | `RecurringUpdate` + `OnUpdateEvent` + `recurring_update_tick_system` |
| Cleanup | [`cleanup.rs`](../../crates/scripting/src/cleanup.rs) | `event_cleanup_system` — end-of-frame marker sweep |
| Quest stages | [`quest_stages.rs`](../../crates/scripting/src/quest_stages.rs) | `QuestStageState` resource + `QuestStageAdvanced` event |
| Conditions | [`condition.rs`](../../crates/scripting/src/condition.rs) | `ConditionFunction` + OR-precedence `evaluate` (M47.1) |
| Script registry | [`registry.rs`](../../crates/scripting/src/registry.rs) | `ScriptRegistry` — editor_id → spawn-fn map (M47.0) |
| Fragment runtime | [`fragment.rs`](../../crates/scripting/src/fragment.rs) | QUST/SCEN fragment population, cascade dispatch, deferred effects, enable state |
| Fragment lowering | [`translate/`](../../crates/scripting/src/translate/) | Conservative `.pex`/AST recognizers and canonical `Effect` lowering |
| Globals | [`globals.rs`](../../crates/scripting/src/globals.rs) | Mutable GLOB values consumed by conditions and `SetValue` effects |
| Triggers and packages | [`trigger.rs`](../../crates/scripting/src/trigger.rs), [`package.rs`](../../crates/scripting/src/package.rs) | Trigger-enter detection and runtime package re-evaluation |
| Perk entry points | [`perk_entry_points.rs`](../../crates/scripting/src/perk_entry_points.rs) | `PerkEntryTable` + `compose_entry_point` — composes an actor's ranked perks into a calculation hook (M47.2) |
| Locations | [`location.rs`](../../crates/scripting/src/location.rs) | `LocationTable` (LCTN hierarchy) + current / editor location + `LocationAliasBindings` (M47.2) |
| Scenes | [`scene/`](../../crates/scripting/src/scene/) | SCEN population, phase playback, actor binding and phase fragments |
| Papyrus demos | [`papyrus_demo/`](../../crates/scripting/src/papyrus_demo/) | Hand-translated R5 scripts (the "go ECS-native" proof) |

### The frame lifecycle (M47.0)

The scheduler in [`byroredux/src/main.rs`](../../byroredux/src/main.rs) wires the
scripting systems in this order:

1. **Tick systems** (Update stage): `timer_tick_system`,
   `recurring_update_tick_system` count down their components and emit
   `TimerExpired` / `OnUpdateEvent` markers. (Both halves are dormant in
   production — see [Timers and recurring
   updates](#timers-and-recurring-updates); the M47.2 VM's update
   registrations tick in `script_update_system` instead.)
2. **Handler systems** (exclusive, Update stage): the `papyrus_demo` dispatchers
   — `rumble_on_activate_system`, `quest_advance_system`,
   `dlc2_ttr4a_on_init_system`, `dlc2_ttr4a_on_update_system`,
   `mg07_on_load_system`, `mg07_on_activate_system` — plus the `rumble_tick_system`
   and `mg07_tick_system` continuations. They read event markers + script-state
   components and produce effects.
3. **Cleanup** (`event_cleanup_system`, exclusive, Late stage, registered last):
   drains every transient marker so each event is visible for exactly one frame.

The core lifecycle is the ECS replacement for the Papyrus event queue: **a marker
component appears → handler systems process it the same frame → cleanup removes
it at end of frame.** No enqueue, no dispatch table, no scheduling latency.

### Shipped event markers

`events.rs` defines the live marker catalog. Note: these are the **actual
shipped names and fields** — they differ from the speculative table further down
in the design narrative.

| Marker | Fields | Replaces | Emit site |
|---|---|---|---|
| `ActivateEvent` | `activator: EntityId` | `OnActivate` | shared interaction path used by gameplay input and diagnostics |
| `HitEvent` | `aggressor`, `source`, `projectile: EntityId` + damage/attack flags | `OnHit` | combat runtime |
| `EquipmentEventBatch` | `Vec<EquipmentChange>` (`item_form_id`, `equipped`), keyed by the wearer entity | actor `OnObjectEquipped` / `OnObjectUnequipped` | **live** — [`emit_equipment_changes`](../../crates/scripting/src/equipment.rs) appends to the wearer's one-frame batch (`get_mut`-then-`extend`); callers are [`byroredux/src/inventory/`](../../byroredux/src/inventory.rs) and the `EquipItem` fragment effect |
| `TimerExpired` | `timer_id: u32` | `OnTimer` | `timer_tick_system` — **dormant** (#5608): nothing starts a `ScriptTimer`; FO4 `OnTimer` ships via the update-registration registry |
| `AnimationTextKeyEvents` | `Vec<AnimationTextKeyEvent { label: FixedString, time: f32 }>` | KF text keys | `byroredux::systems::animation` (live — fires on every clip) |
| `OnUpdateEvent` | (unit) | `OnUpdate` | `recurring_update_tick_system` |
| `OnInitEvent` | (unit) | `OnInit` for translated provider handlers | `attach_papyrus_provider_program`; drained after one frame |
| `OnCellLoadEvent` | (unit) | `OnLoad` / `OnCellLoad` | **live** — cell loader's `attach_script_for_refr` ([`byroredux/src/cell_loader/references/attach.rs`](../../byroredux/src/cell_loader/references/attach.rs)) |
| `OnTriggerEnterEvent` | triggerer set | `OnTriggerEnter` (Skyrim+) / `OnTrigger` (FO3/FNV) | `trigger_detection_system` over live trigger volumes |
| `QuestStageAdvanced` | `quest: QuestFormId`, `previous_stage`, `new_stage: u16` | quest stage-advance fragments | consumed by `quest_fragment_dispatch_system`, including bounded cascades |

The marker pattern is zero-cost for entities that don't participate — no
registration list. `AnimationTextKeyEvents` is the most exercised marker today:
the animation system fires it on every clip whose `.kf` carries text keys (`hit`,
`sound: wpn_swing`, `FootLeft`, …). The `label` is an interned
`FixedString` (#231 / SI-04) — resolve via the world's `StringPool`.

### Timers and recurring updates

- **`ScriptTimer { id, remaining }`** — one-shot countdown. `timer_tick_system`
  decrements `remaining` by `dt`; at ≤ 0 it removes the timer and inserts a
  `TimerExpired { timer_id: id }` marker on the same entity. **Dormant**
  (#5608): nothing in production starts a `ScriptTimer` or reads the marker —
  the shipped Papyrus timer paths (`RegisterForSingleUpdate`, FO4
  `StartTimer`/`OnTimer` real- and game-time) ride the update-registration
  registry below, and `Utility.Wait` rides the fragment continuation queue.
  The pair stays registered and round-trip-tested as the one-shot shape beside
  `RecurringUpdate`.
- **`RecurringUpdate { interval_secs, seconds_until_next }`** — periodic
  subscription, the substrate for Papyrus's `RegisterForUpdate(N)` /
  `UnregisterForUpdate()` / `Event OnUpdate()` triad. Construct via
  `RecurringUpdate::every(interval_secs)` (first fire is `N` seconds out, matching
  Papyrus). `recurring_update_tick_system` emits `OnUpdateEvent` when the counter
  crosses zero and **accumulates** `interval_secs` rather than resetting, so a
  long-frame `dt` still fires only once — the same "missed fire drops" contract
  the Papyrus runtime documents. Cancellation is simply removing the component
  (including from inside an `OnUpdate` handler — the fire-once-then-cancel idiom).

### Quest stage runtime

`QuestStageState` is a **resource** (global game state, not bound to a cell or
actor), keyed by `QuestFormId(u32)`. It exposes the Papyrus surface:

- `set_stage(quest, stage) -> prev` — Papyrus `SetStage`. Advances
  `current_stage` and records `stage` in a `stages_done: HashSet<u16>`.
- `get_stage(quest) -> u16` — Papyrus `GetStage` (default 0 for untouched quests).
- `get_stage_done(quest, stage) -> bool` — Papyrus `GetStageDone(N)`. Returns
  true for any stage the quest *ever* passed through, even after advancing past
  it. The DA10 translation surfaced that `GetStageDone(37) && !GetStageDone(40)`
  needs per-stage history, not a single `current_stage` — Bethesda's runtime
  carries the same shape.
- `reset(quest)` — Papyrus `Quest.Reset()`.

Entries are lazy: a quest only gets a map entry on first mutation, keeping the
resource proportional to "quests touched" rather than "every quest in every
plugin". Stage-fragment dispatch and objective state are now live through
`QuestStageFragments`, `QuestObjectiveState`, and
`quest_fragment_dispatch_system`; the dispatcher queues cascaded stage changes
and caps them so cyclic authored content cannot loop forever in one frame.

### Condition evaluator (M47.1)

The universal predicate system (CTDA conditions) is split across two crates:

- **Parse side** — `byroredux_plugin::esm::records::condition`
  ([`crates/plugin/src/esm/records/condition.rs`](../../crates/plugin/src/esm/records/condition.rs))
  parses CTDA sub-records (28-byte FO3/FNV + 32-byte Skyrim+) into
  `Condition { function_index, comparator, comparand, param_1, param_2, run_on,
  reference_form_id, extra_data_id, or_next }`. Supporting types: `ComparisonOp`,
  `RunOn`, `ConditionValue` (`Literal(f32)` | `Global(form_id)`).
- **Eval side** — `byroredux_scripting::condition`
  ([`crates/scripting/src/condition.rs`](../../crates/scripting/src/condition.rs))
  interprets function indices against ECS state and combines per-condition
  booleans with the **OR-precedence rule**.

**The OR-precedence quirk** (load-bearing, with regression tests): the default
operator between conditions is AND. Setting `or_next` combines a condition with
the *next* via OR, and **consecutive ORs form a block that binds tighter than
the surrounding AND chain.** So `A AND B OR C AND D` evaluates as
`A AND (B OR C) AND D`, *not* the standard `(A AND B) OR (C AND D)`. This is the
opposite of standard boolean precedence; Bethesda designers exploit it via the
distributive law. `evaluate(&list, world, &ctx)` walks the list grouping OR
blocks, short-circuits AND failures, and returns `true` for empty lists
("no conditions = always fires").

**Function catalog.** Bethesda ships hundreds of condition functions across
the lineage. The initial seven-function M47.1 catalog has grown to **19 modeled
variants** (plus `Unknown`): actor value/distance/death, quest stage/history,
cell/faction/level/equipment, XP-to-next-level, scene-action completion,
loaded-3D, class/race/base-ID, perks, FNV reputation and VM script variables.
Their per-game indices and exact ECS fallbacks are documented beside the
authoritative `ConditionFunction` enum in
[`condition.rs`](../../crates/scripting/src/condition.rs); keeping a second
index table here previously let the numbers drift.

Unknown indices fall to `ConditionFunction::Unknown(u32)` and evaluate to 0.0 —
Bethesda's "unknown function → safe-default" contract — trace-logged for catalog
tracking. `ConditionContext` resolves the abstract Run-On targets (`Subject` /
`Target` / `CombatTarget` / `LinkedReference`) to concrete `EntityId`s; missing
slots fail the predicate (Bethesda's "missing reference → false"). The first
real consumer is `quest_advance` (the DA10 demo), migrated in M47.1 Phase 4 from
bespoke `require_done`/`forbid_done` fields to a generic `ConditionList`.

### Perk entry points (M47.2)

A `PERK` entry-point entry modifies a hardcoded calculation (weapon damage,
experience, carry weight, …). `install_perk_entry_points` folds every PERK's
value-math entries into the `PerkEntryTable` resource at load-order install
(FO3/FNV and Skyrim; other games get an empty table), and a calculation asks
`compose_entry_point(world, owner, hook, base, tab_subjects)` for the value the
owner's perks make of `base`. The first consumer is `combat::attack_damage_against`
(FNV hook 0; Skyrim hooks 35 then 0) on both the player and NPC strike paths.

The second is New Vegas **Damage Threshold**. `DamageThresholdRules` installs
when the load order authors the `AVDamageThreshold` actor value (the GECK
*Stats List*: "the actor's base Damage Threshold level") and
`fMinDamMultiplier` — New Vegas; Fallout 3 authors neither. A hit's DT is the
target's DT actor value plus its equipped armors' `DNAM` DT. The target's
hook-56 perks compose it ("modifies the DT of the player" when hit), then the
attacker's hook-58 perks ("modifies the DT of the enemy"). The hit deals
`max(damage − DT, damage · fMinDamMultiplier)`, the GECK's floor on what DT
negates. Both hooks' tabs 1 and 2 are weapons, so weapon-conditioned entries
(Stonewall, Piercing Strike) wait on weapon entities as above. DR is not
applied: its order against DT is not documented in the references here. The
FO3/FNV ARMO `DNAM` decode was corrected to xEdit's layout on the way. It had
read the DR field as DT and the DT float as DR (FNV: `AR ×100, 2 unused, DT,
flags`; FO3: `AR, flags`).

The third is Fallout 3 / New Vegas **kill experience** (`ExperienceRules`,
from the GECK *Experience Settings* page). A kill pays the player the
`iXPRewardKillNPC*` / `iXPRewardKillOpponent*` of the difficulty band the
victim's level falls in (band tops `iXPLevelKill*`; above the last band,
"very hard"). It pays only if the player dealt at least
`iXPDeathRewardHealthThreshold` percent of the victim's health. The award is
composed by hook 9 (`Adjust Experience Points` — Swift Learner) and rolls
levels by the ruleset's `LevelingModel`, up to its cap. The masters author no
`iXPRewardKillNPC*`, so those take the engine defaults 0 / 10 / 20 / 30 / 50
(the GECK page, confirmed in `Fallout3.exe`'s static setting table).
`CharacterLevel` is saved since (v46), which retired #2947's
`validate_progression_state` guard.

Each level the player reaches (`grant_experience`, shared by kill experience
and the `levelup xp` console command) banks the ruleset's
`LevelReward::SkillPoints` in the saved `LevelUpPoints`. Fallout 3 grants
`10 + INT` skill points and a perk every level. New Vegas grants `10 + INT/2`
with the half point carried, and a perk every other level (Fallout wiki
*Skill point*). Both masters author the same `iLevelUpSkillPointsBase=11` and
`iLevelUpSkillPointsInterval=1`; those values do not distinguish FO3 from FNV
or derive either runtime formula. The per-game `LevelingModel` rules supply the
awards above. Points are composed by hook 10 (`Adjust
Gained Skill Points`: `Educated`, +3 in FO3 and +2 in FNV). With no level-up
menu yet, the `levelup` console command spends them: a skill rises one per
point, never past 100, and a perk point adds a perk rank (requirements are
not checked). Live (FNV, INT 5): 600 XP reached level 3 and banked 25 points
and one perk. FO4's level grants one SPECIAL-or-perk point, banked the same way.
`Game.RewardPlayerXP(n[, abDirect])` (a literal or a global) runs through the
same roll. A non-direct grant takes FO4's XP modifier `fXPModBase +
fXPModMult · INT` (Fallout4.esm: 1.0 + 0.03·INT, the 3%-per-point the Fallout
wiki measured). `Game.GivePlayerCaps` / `RemovePlayerCaps` add or remove
`Game.GetCaps()` (form 0xF). `SetObjectiveSkipped(id)` lowers to its
`Quest.psc` body, which hides the objective unless it is completed.

The fourth is **encumbrance** (`byroredux/src/encumbrance.rs`). The player's
carried weight is the inventory's item weights times counts (the inventory
menu's total). It is written to the `InventoryWeight` actor value every frame,
so conditions and scripts read it. Capacity is `CarryWeight` composed by Get
Max Carry Weight (FO3/FNV hook 22, Skyrim hook 10; neither master authors an
entry, so this is for mods). The GECK *Stats List*: "If InventoryWeight >
CarryWeight, the actor is over-encumbered". On FO3/FNV, movement speed is
then multiplied by `fMoveEncumEffect` (0.4, the GECK and Fallout3.exe
default). The holstered-weapon variant is not split out, since the player has
no holstered state yet. On Skyrim the player is "limited to walking" (UESP
*Carry Weight*), so sprint is refused. Oblivion's hard-coded encumbrance value
and FO4 / Starfield's effects are not modelled. Live (FNV Goodsprings): 21
chainsaws put the player at 420 of 200, and they moved slower afterwards.

The record layout and the composition rules are pinned to xEdit's
`wbDefinitionsFNV` / `wbDefinitionsTES5` and checked against the vanilla masters
(`cargo run --release -p byroredux-plugin --example perk_entry_census -- <ESM>`,
zero decode mismatches on FalloutNV / Fallout3 / Skyrim):

- **`PRKE`/DATA** is `[entry point, function, condition-tab count]`; **`EPFT`** is
  the *parameter type* (not the math function) and selects the **`EPFD`** shape
  (float, two floats, AV + float, leveled item, Skyrim spell / lstring). The
  actor value inside an AV-scaled `EPFD` is stored as a *float* index
  (`0x43080000` = 136), rounded by xEdit. The pre-M47.2 decoder let `EPFT`
  overwrite the function and indexed `EPFD` by the wrong table, so every vanilla
  `Float` entry decoded to nothing.
- **Rank.** `PRKE` ranks are zero-based and an entry is authored per rank it
  applies at: `DemolitionExpert` is ×1.2 / ×1.4 / ×1.6 at ranks 0 / 1 / 2, each a
  *total*. An actor holding (one-based) rank `N` gets only the `rank == N - 1`
  entries.
- **Functions.** 1 Set, 2 Add, 3 Multiply, 4 Add Range, 5 Add AV Mult, 6
  Absolute, 7 Negative Absolute, and on Skyrim 12 Set to / 13 Multiply / 14
  Multiply 1 + AV Mult. On FNV hooks 53 / 56 / 58 function 1 reads "Subtract
  Value" in the GECK and subtracts (`WeaponHandling`, `PiercingStrike`).
- **Condition tabs** (`PRKC`): tab 0 is the perk owner, the rest are subjects the
  calculation supplies. A conditioned tab with no supplied subject, or a
  condition the evaluator does not model, keeps the entry from firing — the
  engine cannot say the weapon is a one-hander, and guessing would give every
  weapon the bonus. Equipped weapons are not entities today, so weapon-tab
  entries (e.g. `DemolitionExpert`) are inert until they are.
- **Priority** composes ascending (ties: the actor's perk order, then the
  record's entry order). The two CKs disagree on which number "takes
  precedence"; no vanilla FO3/FNV entry has a non-zero priority, and on Skyrim
  the only visible case is a `Set Value` at a high number (`EagleEye30`), which
  only makes sense applied last.

### Quest script variables and the dialogue speaker (M47.2)

Fragments write quest-script *auto properties* (`kmyQuest.Flag = 1`,
`(Self.GetOwningQuest() as MS11QuestScript).GetInHjerimTovaPointer = true`),
and dialogue, package and scene conditions read them back through
`GetVMQuestVariable` (CTDA 629, Skyrim / FO4 — 3,319 conditions in Skyrim.esm,
774 in Fallout4.esm). `Effect::SetQuestVariable` records a literal write in
the saved `QuestScriptVariables` resource under the backing `::Name_var`
spelling the conditions use (`ctda_variable_census` example); the quest
`VMAD`'s authored Int / Float / Bool values live in the unsaved
`QuestScriptVariableDefaults` (re-derived every boot, so a save load cannot
wipe them). A computed value, a compound operator or a bare `Self` receiver
(a topic-info fragment's own script) declines.

Topic-info (`TIF_…`) fragments receive their speaker through the
`akSpeakerRef` parameter. `lower_info_fragment` binds it (and the
`akSpeaker = akSpeakerRef as Actor` local) to `ActorRef::Speaker`, which the
dialogue caller resolves to the NPC saying the line
(`apply_spoken_info_fragment(.., Some(npc))`). A speaker fragment with a
latent tail (a wait, a 3D-loaded poll, a provider barrier) declines: the
speaker is a live entity a saved continuation could not name.

A call into the fragment quest's own script (`kmyQuest.Startup()`) inlines
the callee's body when it takes no arguments. In a quest fragment the
fragment's VMAD is the quest's, so the effects splice in directly. From an
INFO or scene fragment (2026-10-13) the callee's properties live on the
quest's VMAD, not the fragment's. Its effects are wrapped in
`Effect::QuestScope`, which runs them against the quest and its own VMAD
(`QuestScriptInstances`, every quest's VMAD, snapshotted before the quest
guards). A scope never holds a latent effect. Inside an inlined body, writes
to the script's own variables lower too: a literal, `X = X ± c`, and
`X = Y` (`Effect::CopyQuestVariable`). FO4's radio rotation
(`kmyQuest.UpdateRadio()`, 437 INFO fragments) is the main beneficiary. A single INFO or scene statement that reads the quest script's own
properties (`akSpeaker.AddToFaction((GetOwningQuest() as X).Faction)`,
`kmyQuest.CW.SetStage(2)`) is re-lowered against the quest's VMAD and scoped
the same way. The fragment's own properties are forbidden in that re-lowering,
so a statement mixing the two VMADs declines. In a quest fragment
`kmyQuest.Prop` is an ordinary property receiver (the library supplies its
type, and a quest-typed one never becomes an object).

Relationships (2026-10-13): `RELA` records parse into
`EsmIndex.relationships`. The wire rank Lover (0) … Archnemesis (8) becomes the
CK's +4 … −4, and Skyrim.esm authors 642 of them. `AuthoredRelationships`
keys them by the unordered pair of NPC bases, since the CK stores
relationship data on the base, "NOT … for Templated Actors". The saved
`ScriptedRelationships` holds `SetRelationshipRank` changes. Condition
`GetRelationshipRank` (403) and a `GetRelationshipRank(other)` fragment guard
read a script change first, then the record, else 0, the scale's neutral
middle. The player's base is `0x7`.

Run-time numbers (2026-10-13): when no specific guard fits a comparison, the
two sides lower as `ValueExpr`s into `FragmentGuard::Compare`. A value is:
- a literal;
- a numeric reading (stage, global, quest variable, item count, distance,
  relationship rank);
- `GetLevel()`;
- `Utility.RandomInt` (literal bounds, drawn from the seeded quest RNG);
- a fragment local;
- `+ - * /` over those.
A variable of a script attached to a reference, written through a cast to
its script type (`(Alias_Miraak.GetReference() as Script).fireMiraakKill =
True`), lowers to `Effect::SetReferenceVariable`. That lands in the loaded
reference's `ScriptVariables`, which `GetVMScriptVariable` reads, under the
`::Name_var` spelling; a reference script the Papyrus VM runs keeps its variables in the
saved quest-variable store under the reference's FormID, and `GetVMScriptVariable` falls
back to that (#5985). The library must know the type, it must be no quest,
and it must declare the variable. An INFO / scene quest-variable write
through the quest script's own properties is scoped to the quest.
A local assigned a number lowers to `Effect::SetLocal`, kept for the run
(unread ones are pruned). A fragment holding locals never suspends, because a
resumed continuation would not carry them, so one that would declines.

Fragment `If`s lower when their condition is built from `FragmentGuard`s
with `&&`, `||` (`FragmentGuard::Any`) and `!`. Negation is pushed to the
atoms by De Morgan. The atoms: `GetStageDone` / FO4 `IsStageDone`,
`IsRunning`, `IsCompleted`, `GetStage` compared with a literal,
`IsObjectiveDisplayed` / `IsObjectiveCompleted`, a global's `GetValue()` /
`GetValueInt()` / `.Value`, a quest-script variable, an actor's `IsDead`,
`HasPerk` and `GetItemCount`, a scene's `IsPlaying`,
`<actor>.GetCurrentLocation().HasKeyword(kw)` (the location runtime), and —
2026-10-13 — `IsDisabled` / `IsEnabled`, `IsInFaction`, `Is3DLoaded` and
`GetDistance(target)` against a literal. `IsDisabled` falls back to the
authored state by the cell loader's own rule (`AuthoredDisabledReferences`),
and a reference with no live entity reads as not loaded and infinitely far.
Each
compares with a literal on either side. A guard whose quest, global or actor
does not resolve declines the whole conditional at runtime; a quest-variable
guard sees a write made earlier in the same fragment. An actor's own
keywords are not modelled (`NPC_` keywords are not parsed).

`AddToFaction` / `RemoveFromFaction` / `SetFactionRank` (CK semantics: join
at rank 0 unless already a member; leave; set, joining if needed) lower to
`Effect::ChangeFaction`. `FactionRanks` is re-derived from the record at every
spawn and, for the player, on every load, so the edit is also recorded in the
saved `ScriptedFactionChanges` ledger by reference FormID (PlayerRef `0x14`)
and replayed by `reapply_scripted_faction_changes` from `stamp_quest_reference`
and the load path.

Quest-stage fragments inline calls into their quest's own script
(`kmyQuest.StartUp()`, `(Self.GetOwningQuest() as X).Reset()`): when the
receiver is the fragment's own quest and the callee takes no arguments, the
callee's decompiled body (fetched through a `ScriptLibrary` — the app's
`ProviderScriptLibrary` over `--scripts-bsa`) is lowered in its script's scope
and spliced in, transitively through `Self.Other()` calls, cycle-guarded and
four frames deep at most. Only quest fragments inline: their `VMAD` is the one
that carries the quest script's properties. A callee that does not fully lower
declines its caller, so this gains as the effect table grows (+52 fragments
today; most callees are long, bespoke bodies).

A fragment's `Game.GetPlayer().MoveTo(<marker>)` lowers to
`Effect::MovePlayerTo`: the runtime records the destination reference's FormID
in `PlayerMoveRequest`, and the app's `step_player_move_request` moves the
player in place when the reference is resident (camera, then the body grounded
under it) or queues the door-transition machinery to the reference's cell at
its placement. `<ref>.MoveTo(target, x, y, z, matchRotation)` lowers to
`Effect::MoveTo` carrying the literal offset (Gamebryo's Z-up world axes, converted
to Y-up on apply) and the rotation flag (CK *MoveTo - ObjectReference*; the default
matches the target's rotation); a variable offset, Starfield's `abRotateOffset` and
a player teleport with an offset still decline. `AddItem` / `RemoveItem` counts may read a global
(`ItemCount::Global`, truncated toward zero when applied); `G.Mod(x)` and
`G.SetValue(S.GetValue() ± c)` lower to `Effect::SetGlobalRelative`, and so
do assignments to a global's `.Value` (directly or through the quest
script's property, `kmyQuest.SomeGlobal.Value`); a quest-variable counter
`kmyQuest.X = kmyQuest.X ± c` lowers to `Effect::ModQuestVariable`;
FO4's alias `TryToSetActorValue` joins `SetValue` as `Effect::SetActorValue`,
and Skyrim's `SetAV` / `SetActorValue("Name", v)` lower to
`Effect::SetActorValueNamed`. The name resolves through `ActorValueNames`,
the AVIF editor IDs with their `AV` prefix stripped (`AVVariable06`); a
legacy editor ID such as Illusion's `AVMysticism` does not resolve, and that
write skips. `DamageValue` / `RestoreValue` (Skyrim `DamageActorValue` / `DamageAV`, `RestoreActorValue` /
`RestoreAV`) lower to `Effect::DamageActorValue`: the amount (a literal or a run-time number) grows
the actor value's damage layer, or takes up to that much of it back (CK *DamageValue* /
*RestoreValue - ObjectReference*: "Applies damage to"; "Restores (or heals) damage done to").
Skyrim's spellings discard the amount's sign (CK *DamageActorValue* / *RestoreActorValue - Actor*:
"Negative numbers will be converted to positive"), so those lower the magnitude, `ValueExpr::Abs`
(#5973); the Fallout 4 / Starfield pages are silent on sign, so `DamageValue` / `RestoreValue`
keep the amount as written — unverified.
`ModValue` is not modelled: the CK says only "Modifies the specified Actor Value", without
which modifier layer. The reads beyond the current value are `ValueExpr::ActorValueReading`:
`GetBaseValue(av)` / `GetBaseActorValue("Name")` is the base with the
modifiers and damage dropped (a derived stat takes its formula value), and
`GetValuePercentage(av)` / `GetActorValuePercentage("Name")` is the current value
over the undamaged ceiling, 0 to 1 (the CK: "as a percentage of its maximum
value"); both compose through `condition::actor_value_view`, the one read the
`GetActorValue` condition also uses. `AddPerk` / `RemovePerk` edit the actor's `Perks` (parked with the rest of an
evicted actor's state); and the FO4 / Starfield spellings `EnableNoWait`,
`DisableNoWait`, `TryToEnable`, `TryToDisable` and `Unlock` reuse the existing
enable and lock effects.

`BlockActivation([abBlocked])` (FO4 adds a prompt flag) records the
reference in the saved `ReferenceActivationBlocks` by FormID. The CK says a
blocked reference "will still send OnActivate events", so the
`ActivateEvent` still fires for scripts. The engine's default handlers skip
the reference: the door transition, pickup and container looting, and the
activation conversation.

`<marker>.AddToMap([abFastTravel])` records the marker in the saved
`MapMarkers`. `IsMapMarkerVisible` / `CanFastTravelToMarker` guards read it
together with the marker's authored REFR `FNAM` flags (`AuthoredMapMarkers`;
the map-marker block is now decoded onto `PlacedRef::map_marker`). Skyrim.esm
has 397 markers: 20 start visible, 11 of them travelable.

`<alias>.ForceRefTo(akRef)` and `<alias>.Clear()` lower to
`Effect::ForceAlias`. The runtime resolves the alias property to its quest and
alias index (another quest's alias included) and records the override in the
saved `AliasOverrides`, keyed by reference FormID (`0x14` for the player),
once the quest-state guards are released. The alias refresh applies an
override at the alias's place in the fill order, so dependent aliases see it.
`Clear` also empties a location alias, and a stopped quest's overrides
drop.

### Script-raised story events (M47.2)

`Keyword.SendStoryEvent(akLoc, akRef1, akRef2, aiValue1, aiValue2)` lowers to
`Effect::SendStoryEvent`. Once the quest-state guards drop, the runtime
queues an `SCPT` event (`PendingScriptStoryEvents`) carrying the keyword
(`K1`), the location (a property or a location alias's binding), the two
references and the two integers (`V1` / `V2`). The Story Manager dispatcher
drains the queue with the engine markers, so several events raised in one
frame all dispatch.

`SCPT` nodes route on `GetEventData` (fn 576, 174 Skyrim SM uses), now in the
condition catalog with xEdit's packing: the event function in `param_1`'s low
16 bits, the member in the high 16, and the compared form in `param_2`. It
covers `GetIsID` on `K1` / `L1` / `L2`, `HasKeyword` on a location member
(through the location runtime) and `GetValue` on `V1` / `V2`; `IsInList` and
`GetItemValue` read 0.

Selection also honours the CK *Quest Alias Tab* rule that a non-optional
alias which fails to fill "will cause the quest to fail to start". A
`FromEvent` alias whose event slot is empty makes its quest ineligible, and
the walk continues, so the unconditioned `MarriageNode` no longer takes every
script event. Live in Dragonsreach: the College radiant starter's four boot
events start `MGR01` / `MGR02` / `MGR11` / `MGR20` through the four teacher
nodes, and `MQ104` stage 110 starts `MQGreybeardCall` through
`MQGreybeardCallNode`.

### Locations and location aliases (M47.2)

`LCTN` records parse into `EsmIndex.locations` (parent `PNAM`, keywords, the
population / unique-actor tables that give persistent references their editor
location, and the `ACSR`/`LCSR` location-ref-type table). `install_location_table`
flattens them into the `LocationTable` resource with each cell's `XLCN`.

- **Current location** of a cell-loaded entity: its `CellRoot`'s `CellFormId` →
  that cell's `XLCN`. The player is not cell-owned, so
  `story_change_location_system` publishes the player's cell location as
  `PlayerLocation` (only when the load order authors locations).
- **Editor location**: the `LCTN` table that lists the reference (unique
  actors carry their own location field), else its placed cell's.

The CK condition pages define the location family over that hierarchy — "is
(or has a parent location of)": `GetInCurrentLoc` (359), `GetInCurrentLocAlias`
(360), `LocationHasKeyword` (562, the location itself, not its parents),
`LocationHasRefType` (563), `GetIsEditorLocation` (565), `GetIsAliasRef` (566),
`GetIsEditorLocAlias` (567), `LocAliasIsLocation` (605) and
`HasSameEditorLocAsRefAlias` (181). FO3/FNV/Oblivion author none of these
indices; 180 and 610 are left out because Oblivion and FNV author them as
other functions.

Location aliases (`ALLS`) fill in list order with the reference aliases, in
`refresh_scene_actor_bindings`, by the CK *Quest Alias Tab*'s four mechanisms:
Specific Location (`ALFL`), Reference Alias Location (`ALFA` + optional
keyword-selected parent), External Alias Location, and Find Matching Location
(the event's `L1`/`L2` slot with "From Event", otherwise a random location
passing the alias conditions run on the candidate location). Reuse within a
quest and other quests' "Reserves Location" aliases are honoured. A binding
holds until its quest stops and is saved (`LocationAliasBindings`), so a random
pick survives a reload.

**Cleared locations.** The CK *Cell Reset* page: "Locations are cleared when
all of their bosses (creatures tagged with the 'Boss' LocRefType) are dead [or]
a script calls `SetCleared()`." `LocationTable` indexes each location's
`Boss`-typed references (the `LCRT` whose editor ID is `Boss`, `0x130F7` in
Skyrim.esm; 374 locations have bosses, 159 of them several).
`reconcile_dead_actor`, the death path every kill, start-dead actor and save
load funnels through, calls `note_actor_death`. That records the dead boss by
reference FormID in the saved `LocationClearState` and clears the location
once its last boss is dead. `<location>.SetCleared([abCleared])` (a location
property or a location alias's `GetLocation()`) sets it directly, and
`IsCleared()` guards read it. Conditions `GetLocationCleared` (250) and
`GetLocationAliasCleared` (598) read it too. A Find Matching location alias
never picks a cleared location unless flagged "Allow Cleared", and a flagged
one still prefers an uncleared match (CK *Quest Alias Tab*).

Not modelled: an unloaded reference's death for `LocationHasRefType` (it sees
scripted disables only). Against `Skyrim.esm` with no references loaded, all
162 Specific aliases, 3 of 4 External aliases and 28 of 136 Find Matching
aliases bind. The rest of the Find Matching aliases condition on a reference
alias. The 294 From Event aliases wait for their events (223 are
script-raised `SCPT` events).

### Essential and Protected actors, bleedout (M47.2)

The FO4 CK *Actor* page: "Essential actors cannot be killed. When they reach 0
health, they go into a special 'bleedout' state and recover over time.
Protected actors are treated as essential to all damage except that delivered
by the player." Essential is `ACBS` bit `0x2` in every game. Protected is bit
`0x800` from Skyrim on; FO3/FNV use that bit for "No Blood Spray". Both are
resolved through the template's "Use Base Data" (`0x0080`) and stamped as
`ActorProtection` at spawn. An alias's Essential / Protected flag applies while
its quest runs (CK *Quest Alias Tab*). The `essential_census` example counts
269 essential and 81 protected bases in Skyrim.esm, 139 / 253 in Fallout4.esm,
102 in Fallout3.esm, 28 in FalloutNV.esm and 259 in Oblivion.esm.

Every live death producer (combat hits, SDK actor-value writes, water
damage) asks `spare_from_death` before marking an actor `Dead`. A spared
actor gets a saved `Bleedout` and counts as unconscious for every #5017
consumer (no AI, no combat, no conversation). It gets back up after
`fEssentialDeathTime` with `fEssentialHealthPercentReGain` of its health (the
GECK page: "the time … an NPC marked essential will remain unconscious before
getting back up"). Skyrim.esm and Fallout4.esm author a 20 s death time. The
other values are the executables' defaults: 10 s on Oblivion and FO3/FNV, 30 s
on Skyrim and FO4, and 0.3 everywhere. Starfield has no such setting, so
its downed essentials stay down until a script `Resurrect`s them — a gap, not
the game's rule (#6259): it authors per-race RACE `DAT2` and per-NPC `ONA2`
bleedout times (captured on `RaceRecord::bleedout` /
`NpcRecord::bleedout_override`), but how they combine is unsourced, so the
engine warns once instead of inventing a recovery. The player is never spared.
Conditions `IsEssential` (354), `IsProtected` (476) and `IsBleedingOut` (580)
read the state, and fragments lower `IsEssential()` / `IsBleedingOut()`
guards. `<actor>.GetActorBase().SetEssential([b])` / `SetProtected([b])` (or
an ActorBase property's) record a per-base override in the saved
`ActorBaseProtectionOverrides`. Setting one flag clears the other (CK
*SetEssential / SetProtected - ActorBase*), and the override wins over the
base's stamp for every actor of that base. `<actor>.Kill([akKiller])` queues
`PendingScriptKills`, which `scripted_kill_system` drains through the same
path: an essential victim goes into bleedout (the CK *Kill* page: "Essential
actors will be made unconscious"), and Health is untouched ("this function
doesn't kill by modifying the health"). Otherwise the victim gets `Dead`, the
KILL story event with the script's killer, and the shared teardown. A protected
alias's victim dies only when the player is passed as the killer, which is the
CK's own documented workaround. Live (Skyrim, Riverwood's Sleeping Giant Inn): Delphine, essential,
went down at −4 health instead of dying and got back up 20 s later at 30
health. Orgnar, not essential, died to the same blows.

Not modelled: `SetNoBleedoutRecovery` / `GetNoBleedoutRecovery` (694), and
attackers keep striking an actor in bleedout (harmlessly) until their own
combat ends.

### The Papyrus VM (M47.2)

`crates/scripting/src/fragment/vm.rs` runs a quest fragment the effect
lowering declines. The lowering can't express values it only learns at run
time, such as an object local assigned from a call
(`ObjectReference x = Alias.GetRef()`), a quest-script function that returns
a value, or a loop. The VM interprets those over the decompiled AST.

The VM owns control flow and values: locals, numeric script variables (the
`QuestScriptVariables` store), `If`/`While`/`Return`, arithmetic, comparisons,
string concatenation and arrays. Every native still runs through the effect
runtime:
- **Native statements.** A statement such as `x.Enable()` is re-written with
  its evaluated receiver and arguments bound to synthetic names. A form
  becomes a property of a synthetic VMAD, a quest or the player is bound
  through `NativeBindings`, and a number stays a literal. The result is
  classified by the lowering's own primitive table and applied by
  `apply_effects`.
- **Native reads.** Reads (`IsDisabled`, `GetStage`, `GetValue`, …) are
  classified the same way, as a `ValueExpr` or as guards, and evaluated.
- **Object reads.** The VM reads these itself: alias references,
  `GetOwningQuest`, `GetPlayer`, `GetLinkedRef`, base objects, location-alias
  and current locations, and `GetDialogueTarget`.
- **Quest-script calls.** A call into a quest's script runs here against that
  quest's VMAD. A member of another quest reads by its declared type when the
  quest's script is known (`kmyQuest.SomeAlias`, an `ActorValue Property`),
  from the script attached to that quest; only an untyped quest's member is
  taken for a number.
- **Other references' scripts.** A call through a cast or a typed variable
  (`(Trap as TrapBase).Fire()`) reaches the script the VM runs on that
  reference. The check requires the declared script, not an engine class, to
  define it. The call runs as that reference's: its `Self`, its properties
  and its variables. Reading `t.Hits` and writing `t.Armed = False` work the
  same way, and a write lands under the reference.
- **Script variables.** A numeric variable lives in `QuestScriptVariables`.
  An object or string variable lives in the saved `QuestScriptObjects`, so it
  holds from one fragment to the next. A variable or property nothing has
  written reads the literal the script declares (`Int Property Goal = 3`).
- **States.** These follow the CK's *States (Papyrus)*. A call runs the
  version in the current state first, searching down the `extends` chain,
  and falls back to the empty state's version. An instance starts in its
  `Auto State`. The compiler-generated `GotoState` / `GetState` keep the
  state in `::State`, which is stored per script. Unhandled `OnBeginState` /
  `OnEndState` calls are no-ops, and scripts without the generated pair get a
  native one. The check covers every state's version of a call.
- **Global values.** `Global.Value = n` (and `+=` and the like) runs as the
  `Global.SetValue(n)` native it stands for.
- **Engine constants.** An engine class's `AutoReadOnly` property
  (`ObjectReference.Motion_Keyframed`) reads its literal. The compiled form
  keeps it in a getter that returns it. The value is the same whether it is
  read on any reference or unqualified in a script that inherits it. Passed
  to a native, the constant goes by name rather than as a number, because the
  number differs per game (Skyrim's Keyframed is 4) and the primitives decline
  a bare motion-type literal they can't place (#5633).
- **Global functions.** `ScriptName.Fn()` runs another script's global
  function, which has no `Self`. `Game.GetFormFromFile(aiFormID,
  asFilename)` resolves a plugin-local id through the load order
  (`LoadOrderIdentity::form_from_file`), ignoring the id's load-order bits
  as the CK page says.
- **Structs and array methods (Fallout 4).** A struct is a shared value
  with named members; `new StructName` creates one from its declaration.
  The decompiler now emits the `.pex` struct tables, and the parser and AST
  have a distinct `Expr::NewStruct`. VMAD struct (type 7) and
  array-of-struct (17) properties decode by xEdit's layout. Arrays run the
  CK's *Arrays (Papyrus)* methods: `Find` / `RFind`, `FindStruct` /
  `RFindStruct`, `Add` (a struct added several times is the same struct),
  `Insert`, `Remove`, `RemoveLast` and `Clear`. The check follows declared
  types through elements and members. An array or struct a script variable
  holds is one shared value within a run, starting from what a script last
  wrote or else the VMAD's, and the run writes it back whole when it ends
  (`ScriptObject::Array` / `Struct` in the saved `QuestScriptObjects`). A later
  run therefore sees the elements an earlier one added. An array local across
  a `Utility.Wait` still can't be saved.
- **Reference collections (Fallout 4, Starfield).** `RefCollectionAlias`
  natives `GetCount`, `GetAt`, `GetArray`, `GetActorArray`, `Find`, `AddRef`,
  `AddArray`, `AddRefCollection`, `RemoveRef`, `RemoveAll`, and `EnableAll` /
  `DisableAll` (the CK: `EnableNoWait` / `DisableNoWait` on every member) work on a collection alias the quest leaves to
  its scripts. The members are in the saved `AliasCollections`, by FormID. A
  collection the quest fills by conditions or a fill type has no fill runtime
  (`m47-3-quest-alias-design.md`), so the VM refuses it rather than read it as
  empty. A reference is in a collection once: a second `AddRef` of it changes
  nothing.
- **Location ref types.** `HasLocRefType(refType)` reads the reference's XLRT
  tags, and `ReferenceAlias.TryToMoveTo(ref)` is `MoveTo` on whatever the alias
  holds, a no-op when it holds nothing.
- **Math and time.** `Math.Min/Max/Abs/Floor/Ceiling/Sqrt/Pow/Sin` compute in
  the VM (`Sin` takes degrees, per the CK page). `Utility.GetCurrentGameTime()`
  is the host's absolute game clock in days, from `ScriptUpdates`, which
  `script_update_system` refreshes each frame.
- **Followers.** `Actor.SetPlayerTeammate(abTeammate, abCanDoFavor)`,
  `SetDoingFavor`, and the `IsPlayerTeammate()` / `IsDoingFavor()` reads use
  the saved `FollowerState`, keyed by the actor's FormID. Effects: what the CK
  says a teammate does (sneaks and draws with the player, wears its best
  armor, its crimes count as the player's, takes commands) belongs to the
  sneak, equipment and crime systems that will read this state. None of them
  reads it yet.
- **Forms and deletion.** `Game.GetForm(id)` is the form of that ID, or None
  when the ID's plugin isn't loaded. `Delete()` flags a created reference
  for deletion like `DeleteWhenAble()`; an authored one can't be deleted.
- **Achievements.** `Game.AddAchievement(id)` records the award in the saved
  `Achievements` set (a second award changes nothing). The platform profile
  is the host's.
- **Integers.** `Int` arithmetic and comparison are exact 32-bit and never go
  through an `f32`, which can't hold every form id.
- **Dialogue.** `IsInDialogueWithPlayer()` ("Is this actor or talking activator currently talking
  to the player?") is true for the NPC whose response surface is open. `DialogueSurfaceState`
  moved from the binary into the scripting crate for it (one source, re-exported where the
  dialogue systems use it).
- **Furniture and quest transitions.** `IsFurnitureInUse([abIgnoreReserved])` ("if any marker on
  the furniture is in use", or reserved when reservations are not ignored) is true while an actor is
  `Seated` on the furniture or, unless ignored, while the sandbox AI holds a seat in
  `SeatReservations` (moved into core for it); the player cannot use furniture here, so nothing
  else occupies it. `Quest.IsStarting()` / `IsStopping()` ("in the process of starting up" / "shutting
  down") are false: a quest starts and stops in one step, so it is never between the two states.
- **Speechcraft stats and combat stand-down.** `Game.QueryStat` also answers "Persuasions",
  "Bribes" and "Intimidations": the CK lists them as tracked stats, but Skyrim's own
  `FavorDialogueScript` is what raises them (`Game.IncrementStat(..., 1)`) and the engine has
  no speechcraft screen, so the scripts' increments are the whole of them. `StopCombatAlarm()`
  ("Stop all combat and alarms against this actor") removes the combat state of every actor
  whose target is the receiver (the player included); the CK's alarms are the crime system's,
  which the engine does not have, and the ambient hostility system may start a fresh fight if
  they still see the target.
- **Crime faction.** `GetCrimeFaction()` ("the faction this actor reports it's crimes to") is a
  script's `SetCrimeFaction` for that reference, else the base `NPC_`'s `CRIF` (parsed into
  `NpcRecord.crime_faction_form_id`, installed as the derived `CrimeFactions`). `SetCrimeFaction(
  akFaction)` records the override in the saved `CrimeFactionOverrides` (`None` clears it) and,
  as the CK says, adds the actor to the faction if it is not in it; a run sees its own set. An
  actor with neither has no crime faction: the CK does not say how the engine would pick one
  from its other factions.
- **DropObject.** `DropObject(akObject, aiCount = 1)` ("Drops the specified object from this
  object's inventory", returning "the object that was dropped") takes up to `aiCount` of the item out of
  the owner's inventory and creates one reference per item at the owner, returning the last; the CK
  stacks a count above one into a single reference, which created references have no count for. An
  item the inventory does not hold drops nothing and reads None.
- **Location loaded.** `Location.IsLoaded()` ("if this location is loaded or not") is true while a
  resident cell is one of the location's own (`location_loaded`); a loaded child location does not
  load its parent, and a form that is not a location stops the run.
- **Guards (Starfield).** The decompiler lifts `LockGuards` / `UnlockGuards` / `TryLockGuards` to
  `Guard.lock(...)` / `.unlock(...)` / `.trylock(...)`, whose arguments name guards rather than
  values (they used to fail as unknown names, 217 Starfield fragments). A guard serialises threads
  that interleave only when a run suspends; here runs interleave at a wait or a budget slice, and a
  guard held across either is not modelled, so every operation succeeds at once and the try-lock
  reads True.
- **IsInLocation.** `ObjectReference.IsInLocation(akLocation)` follows the CK page's own script:
  false without a current location, else the location asked about or one of its descendants
  (`LocationTable::is_within` over `current_location`; the player's is the published one).
- **Reference flags and the dialogue camera.** `IgnoreFriendlyHits([abIgnore])` ("won't turn hostile
  after several hits from a friendly opponent") and `SetCanDoCommand([abCanDoCommand])` record their
  flag (default True) in the saved `ReferenceFlags`, state the friendly-hit and command systems will
  read. `Game.StopDialogueCamera` ("Stops all dialogue cameras") is a no-op: the engine never
  starts one, so there is nothing to stop (`StartDialogueCameraOrCenterOnTarget` stays declined).
- **Actor base traits.** `ActorBase.GetSex()` ("-1: None, 0: Male, 1: Female") is the base `NPC_`'s `ACBS`
  female bit and `GetVoiceType()` ("the VoiceType for this actor or talking activator") its `VTCK`, from
  the derived `ActorBaseTraits`. A form that is no actor base stops `GetSex`; an actor without a voice
  type reads None. (A leveled actor's spawned sex needs `GetLeveledActorBase`, as the CK notes.)
- **Owners and crime gold.** `SetActorOwner(akActorBase)` / `SetFactionOwner(akFaction)` replace the
  reference's one explicit-owner slot (a `None` clears it) in the saved `OwnerOverrides`, which
  `GetFactionOwner` / `GetActorOwner` read before the placed `XOWN`. A faction's crime gold
  (`SetCrimeGold` non-violent, `SetCrimeGoldViolent`, `ModCrimeGold(amount, violent)`; reads
  `GetCrimeGold` = both kinds, `GetCrimeGoldViolent`, `GetCrimeGoldNonViolent`) is the saved
  `FactionCrimeGold`; the engine's own crime pass (`charge_crime_factions`, `byroredux/src/crime.rs`) charges the same ledger on Skyrim/FO4, so a script's writes and a witnessed crime's fine compose. A modification
  below zero floors at zero, which the CK leaves open. Paying it off (`PlayerPayCrimeGold`) stays declined.
- **Alias scripts through a cast.** A function of an alias script reached through a cast of the
  alias property (`(Alias_X as MyScript).Fn()`, where `MyScript` extends `ReferenceAlias`) is checked
  with an alias as `Self`, as it runs, so its `Self.GetActorRef()` and the like read instead of
  failing as natives of a reference (43 more Skyrim fragments).
- **Chain counts.** `CountLinkedRefChain(keyword, max)` follows the CK page's own script: from the
  reference each step takes the keyword's link and counts it (a missing link too, so A to B to C
  counts 3) unless the link is the reference itself, and stops once the count passes the cap
  (default 100; the page's code returns up to `max + 1`, the prose says `max`). `Quest.IsStopped()`
  is the negation of `IsRunning()` (a quest here starts and stops in one step, so `IsStarting` and
  `IsStopping`, which the CK defines as in-progress states, stay declined). Fallout 4's
  `Game.GetCaps()` is `Caps001`, form `0x0F` (checked against `Fallout4.esm`); other games stop the
  run.
- **Linked reference lists.** `GetLinkedRefChain(keyword, max)` returns the chain after the
  reference ("will stop if it finds a loop", or at `iMaxExpectedLinkedRefs`, default 100), and
  `GetLinkedRefChildren(keyword)` the loaded references directly linked to it (CK pages of the
  same names); both read the effective link of `linked_ref` (a script's over the authored one).
- **Linked references.** `GetNthLinkedRef(n[, keyword])` follows the link under
  the keyword (default none) n times; 0 is the object itself (CK *GetNthLinkedRef*).
- **Waits.** `Utility.Wait(seconds)` suspends the run. This works in the
  fragment and in any function it calls as a statement. The suspended state
  is an `Effect::Interpret` carrying a `VmResume`. It holds one `VmFrame` per
  active function (outermost first), and each frame records its script,
  function, quest, locals, and a path of `VmStep`s (statement indices, plus
  the `If` branch or `While` body taken) to the wait or the call it waits in.
  The fragment continuation queue holds that effect for the wait, as it does
  for a lowered wait. On resume, the innermost callee continues first, and
  each caller then continues after its call statement. A function called for
  its value may not wait: the check rejects it, and the run stops if it does.

`check_function` decides at populate time, against typed placeholders, whether
every statement and call of the fragment and of the functions it reaches runs
on the VM. It passes literal arguments as written. Only fragments that pass
are installed as `Effect::Interpret`. A run that still meets something it
can't do, such as a native that declines its actual values, stops and logs
it.

Quest, scene, topic-info and package fragments all fall back to the VM. The scene and
INFO walks collect the scripts it needs and install them beside the quest
walk's. A topic-info fragment's `akSpeakerRef` binds to the speaker of the
line, as the lowering binds it.

Package (`PF_`) fragments (`crates/scripting/src/fragment/package.rs`) run
on package transitions. The PACK `VMAD` fragment section decodes per xEdit's
`wbScriptFragmentsPack` (flags: 1 Begin, 2 End, 4 Change). A fragment installs
only when the VM's check accepts it, and it runs with `akActor` bound to the
actor running the package. The AI reports each actor's package changes: the
first-tick confirmation of the winner (spawn only stamps the behaviour: the
winner is chosen before the quest-alias overlays and a save's restored clock
exist, #5864), a re-selection, a combat / unconscious suspension and the
actor's death (`→ None`, #5870). The old package's Change fragment then runs,
followed by the new one's Begin. The
CK's *OnEnd* / *OnChange - Package* pages define End as a normal end and
Change as ended or preempted. This engine re-selects by conditions and
schedules, so every switch is a preemption (Change + Begin). The one
procedure that completes on its own is Travel: when `travel_system` marks the
actor `Traveled` it reports `report_package_end(actor, package)` for the
actor's active package (once; the actor keeps the package until re-selection),
and `package_fragment_system` runs that package's End fragment. Sandbox,
Wander and the other procedures never finish here, so their End fragments
don't run. **Not reachable on shipped content yet (#5869):** `TravelBehavior`
installs only from the flat `procedure_type == 6` arm, which only FO3 / FNV /
Oblivion packages reach, and PACK `VMAD` exists only on Skyrim+, whose packages
resolve through the procedure tree (Sandbox and Patrol leaves only,
[`packal.md`](packal.md)). Today only the synthetic VM test drives this End;
resolving the Travel leaf on tree packages is `packal.md` scope.

With the VM, 84.1% of the Skyrim corpus runs (Fallout 4: 79.9%, Starfield
78.5%) (lowered + interpreted; `fragment_coverage`, `VM_REASONS=1` lists why
the check rejects the rest). The front-end feeding both paths — `.pex` parse
+ decompile over the vanilla script archives — is at 100.0% on all four
games (Skyrim 14 026, FO4 7 875, FO76 7 233, Starfield 4 740 scripts; the
two residual control-flow failures, FO4 `StimBoxScript` and FO76
`Debug.TraceLog`, closed by the boolean pass's shared-rejoin guard), and
`pex_corpus_smoke`'s shape gate now counts FO4+ struct items, so it is a
real per-game shape check rather than a Skyrim-calibrated one (it used to
report 44/424/115 phantom mismatches on FO4/FO76/Starfield). FO76's client
archive carries almost no quest fragments (54 behavioral — the quest logic
is server-side), so its fragment number is not meaningful.

Not modelled yet:
- other actor natives;
- alias update registrations;
- package End fragments for procedures other than Travel.

Per-game decline heads behind the remaining fragment gap
(`VM_REASONS=1`, 2026-10-13 census — the worklist order is this table;
`VM_REASON_SCRIPTS=<text>` names the fragments behind a reason). Lowered +
interpreted: Skyrim 88.1%, Fallout 4 83.6%, Starfield 81.8%.
- **Skyrim**: `DropObject` (133, inside the `CarryActorScript` carry packages),
  `ModFavorPoints` (99, defined in `Actor.pex`; now run), `Say` (66), `Game.ShowTrainingMenu` (57,
  no UI), `IsLoaded`, `UnregisterForUpdate`, `GetActorRef`, `IsInLocation`.
- **Fallout 4**: `GetWorkshopResourceObjects` (152, the workshop API),
  `GetLinkedRefArray` (99), `Game.GetCommonProperties` (67), `SetCanDoCommand`,
  `Game.StopDialogueCamera`, `IgnoreFriendlyHits`.
- **Starfield**: `Game.AddToAvailableCrew` (126), `SetMarkerVisibleOnStarMap`,
  `DisableWithGravJump`, `VisualEffect.Play`, and the `IsTrue` calls on condition forms
  that use a function the engine does not model. The ones with no CK page are not
  guessed.

### The frame budget (M47.2, #5763)

Every interpreter system is an exclusive `Update` system, so what the VM
spends in a frame is serial main-thread time. The 100 000-statement cap bounds
one run, not a frame. `fragment/budget.rs` adds a shared per-frame allowance,
`VmFrameBudget` (default 10 000 statements, about 1.4 ms at the census rate of
0.14 µs a statement), renewed by `event_cleanup_system` at the end of every
frame. A run also pays `RUN_START_COST` (16 statements) when it starts, for
the fixed work around it. The unit is a statement count, not a clock reading:
it is deterministic, so the tests assert on it.

When the allowance is gone:

- **A run that has not started is deferred.** It is queued unchanged on the
  `FragmentExecutionQueue` with no delay and starts on a later frame. This is
  one check in `apply_effects` (every `Effect::Interpret`), plus an earlier one
  in the reference-event and update systems so a burst does not pay the
  scheduler snapshot per delivery just to be deferred. A due `OnUpdate` the
  allowance cannot start is not queued but left due: a periodic registration
  re-arms when taken, so queueing would pile runs up behind a budget the demand
  exceeds. It is delivered once, on the first frame with allowance left. A
  registered hit (`RegisterForHitEvent`) the allowance cannot start is queued like any
  other unstarted run (#5987): the hit is a one-shot marker drained at frame end and
  the registration is one-shot, so skipping it would lose both.
- **A run in progress yields.** After the statement that used the last of the
  allowance, the run suspends between two statements, exactly as a zero-second
  `Utility.Wait` would: the same `VmResume` frames, resumed innermost-first
  from the next statement. The resume carries the statements already used
  (`VmResume.carried_steps`), so the per-run cap bounds the whole logical run.
  A real wait carries nothing, so a polling loop with a `Wait` is unchanged.

**A handler that never finishes** is neither starved nor immortal. It runs in
slices, in queue order behind anything raised earlier, and is stopped with a
`step limit` warning at the same 100 000 cumulative statements the cap always
allowed: one `While True` handler takes about 11 frames, five of them about
51, none over the allowance. The work it displaces runs a few frames later.

What a slice cannot split: a statement inside a function called for its value
(a suspended frame cannot hand a value back) and any run whose frames hold an
array or struct local (the saved continuation stores scalars only, the limit
`Utility.Wait` already has). Those finish in the frame they started, capped at
100 000 statements, and are charged to the allowance so the work queued behind
them defers (#5764).

**`ReferenceScriptIndex`** answers "is this reference loaded, and which
scripts does it carry" in one hash lookup. `script_update_system` and the
VM's cross-reference calls used to scan every attached script (and every
FormID-tagged entity) for it, each frame. The map is rebuilt once, at the first
lookup after an attach; the FormID is read then, so a reference whose
`FormIdComponent` arrives after its scripts is found. An unloaded reference is
not tracked: `lookup` re-checks the entity it finds, so a despawned or reused
one reads as not loaded, and a reload attaches again. An idle
`script_update_system` is now the clock advance plus one pass over the
registrations (0.07 µs from 0 to 20 000 attached scripts; it was 0.3 µs a
script).

Measured with `cargo test -p byroredux-scripting --release --lib -- --ignored
--nocapture vm_cost` (`docs/audits/BENCH_vm-cost_after-5763.txt`):

| Case | Before | After |
|---|---|---|
| Idle `script_update_system`, 5 000 / 20 000 attached | 1 579 / 6 874 µs | 0.07 / 0.07 µs |
| One runaway handler, first frame | 13.8 ms | 1.4 ms (killed after ~11 frames) |
| Five runaway handlers, first frame | 69 ms | 1.4 ms (~51 frames) |
| `OnCellLoad` to 5 000 references, first frame | 15.4 ms | 6.9 ms (17 frames to drain) |
| 5 000 references polling at 1 s, worst frame | 13.1 ms | 2.5 ms |

The save format went to v72 for `carried_steps`.

### Reference scripts on the VM (M47.2)

A script attached to a placed reference — through the reference's own VMAD or
its base record's — that no recognizer models now runs on the Papyrus VM
(`crates/scripting/src/fragment/reference.rs`). The cell loader attaches it when
`check_reference_script` accepts every handler the script has of the delivered
events, in every state and down its `extends` chain. The check result is cached
per script name for the session (`VmReferenceScriptChecks`). The
`PapyrusReferenceScripts` component then names the script and its properties.

The script's `Self` is the reference. Its variables and its state live in the
saved script-variable stores under the reference's FormID, so they outlive the
cell unload that despawns the entity. The component itself is re-derived at the
next attach.

`reference_script_event_system` turns engine markers into Papyrus events,
following the CK pages *OnInit*, *OnCellAttach*, *OnLoad - ObjectReference* and
*OnCellLoad - ObjectReference*:
- a cell load delivers `OnInit` the first time the reference loads (a saved
  per-script flag), then `OnCellAttach` ("reliably called before OnCellLoad";
  attach and load are one moment here), `OnLoad` and `OnCellLoad`;
- `ActivateEvent` delivers `OnActivate(akActionRef)`;
- a trigger crossing delivers `OnTriggerEnter(akActionRef)`;
- a `HitEvent` delivers `OnHit(akAggressor, akSource, akProjectile,
  abPowerAttack, abSneakAttack, abBashAttack, abHitBlocked)`. The source is the
  aggressor's equipped weapon, or `None` unarmed (no weapon form is invented),
  and projectiles are not modelled;
- a death delivers `OnDeath(akKiller)` from `OnDeathEvent`. Every live death
  producer raises that marker beside the Story Manager's `KILL` event, and
  this system drains it (Pattern B), so a Late-stage script kill is delivered
  the next frame.

Each event runs as an `Effect::Interpret` that carries its arguments, through
the fragment runtime. Its natives, waits and stage advances therefore behave as
a quest fragment's do. An event the script handles only in other states does
nothing in the current one. The system runs ahead of quest-fragment dispatch, so
a stage a reference script sets runs its fragment the same frame.

With the VM, 20.1% of Skyrim's 19,412 scripted references run a script, up from
7.5% with recognizers alone (`refr_script_coverage`; `VM_REASONS=1` lists why
the VM declines the rest, and `VM_REASON_FAMILIES=<text>` which scripts share a
reason). Bleak Falls Barrow attaches 7, among them MS13's Arvel and spider
triggers and the encounter enablers.

Calls never resolve into the engine classes. `Form`, `ObjectReference`,
`Quest`, `Actor` and the rest declare `Native` functions, and the script
library serves them too, but their API is the native bridge's, including
their scripted wrappers (`Quest.SetStage` → `SetCurrentStageID`). An event
only those classes define (ObjectReference's empty `OnLoad`) is not one the
script handles.

Not modelled yet:
- other events (`OnTrapHit`, `OnCombatStateChanged`, `OnOpen`, …;
  `OnUpdate` arrives through update registrations);
- `OnInit` holding back the script's other events while it waits;
- the variable reset a cell reset brings;
- calls into a reference whose script a recognizer models (no VM instance to
  call), or one that is not loaded.

### Update registrations (M47.2)

`crates/scripting/src/fragment/updates.rs` implements the CK's *RegisterForUpdate -
Form* family for scripts the VM runs. The older `RecurringUpdate` component
above is dormant: no recognizer emits it, and its only writer is the
unspawned `dlc2_ttr4a` demo.
- `RegisterForUpdate(s)` is periodic, and `RegisterForSingleUpdate(s)` fires once.
  Each replaces the script's earlier registration of its own kind; the two kinds
  don't interfere.
- `UnregisterForUpdate()` cancels both.
- The `GameTime` twins count game hours.
- A quest's registrations end when the quest stops.

Fallout 4's timers ride the same registry, following the FO4 CK's *StartTimer
- ScriptObject* family:
- `StartTimer(afInterval, aiTimerID = 0)` and `StartTimerGameTime` are
  one-shot, and deliver `OnTimer(aiTimerID)` / `OnTimerGameTime(aiTimerID)`;
- a timer ID is unique per script, real-time and game-time IDs don't
  conflict, and restarting a ticking ID resets it;
- `CancelTimer` / `CancelTimerGameTime(aiTimerID = 0)` stop one;
- a game-time interval under 0.033 hours is rounded up to it.

A registration on another quest (`kmyQuest.RegisterForUpdate(5.0)`) registers
every script attached to that quest that handles the update. The saved
`ScriptUpdates` holds the registrations and both clocks:
- real time counts each frame's `dt`, so menu mode pauses it;
- game time reads the host's absolute game clock (`GameTimeRes`, day × 24 +
  hour), so sleeping and waiting deliver the update they pass. A game-time
  registration made before the host first reports its clock is armed from that
  first report.

`script_update_system` delivers `OnUpdate` / `OnUpdateGameTime` to the
registering script, through the fragment runtime. A reference's update waits
while the reference is unloaded.

A handler is delivered only when it runs whole on the VM. The check covers it
when it checks the registering code, which installs the scripts it needs, and
the delivery checks it again against the installed scripts (cached per
script). A registration whose handler can't run doesn't fail the registering
code, so its other effects still run; the handler is skipped entirely, never
run partway.

### Distance events (M47.2)

`crates/scripting/src/fragment/distance.rs` implements the FO4 CK's
*RegisterForDistanceLessThanEvent - ScriptObject* family:
- the registering script receives a **single** `OnDistanceLessThan(akObj1,
  akObj2, afDistance)` once two objects are closer than the distance (sent
  immediately if they already are), or `OnDistanceGreaterThan` when they are
  farther;
- an object is a reference or a `ReferenceAlias`, which resolves to its
  reference each check, so an alias filled later counts from then on;
- the pair is interchangeable. Registering it again in either order only
  updates the distance, and the event keeps the order it was first registered
  in. A script holds one less-than and one greater-than registration per pair;
- the receiver need not be either object, and `kmyQuest.Register…` registers
  every script attached to that quest that handles the event;
- `UnregisterForDistanceEvents(a, b)` removes both kinds for the pair, and a
  stopped quest drops its registrations. A reference's registration waits
  while it is unloaded.

The saved `DistanceWatches` holds the registrations. `distance_event_system`
checks them every frame, and the distance is the one the `GetDistance` guard
reads, between world positions. A reference with no placement is infinitely
far, so it triggers a greater-than event and never a less-than one. A
registration the frame's interpreter allowance cannot start stays registered.

### Custom and remote events (M47.2)

`crates/scripting/src/fragment/events.rs` implements the FO4 CK's
*RegisterForCustomEvent* / *SendCustomEvent* / *RegisterForRemoteEvent* family:
- a script registers for a named event of a sender; the event goes only to that
  script;
- `SendCustomEvent(name, args)` "returns immediately": each registered
  receiver's `(akSender, akArgs)` handler is queued on the continuation queue
  and runs on its own, after the sender's run, with the `Var[]` it sent
  (`VmStored::Array`);
- the compiler mangles the names. A custom event is sent as
  `scripttype_Event` and handled by `Event ScriptType_Event`, and a remote one
  by `Event Type_OnEvent`. The registration finds its handler by those names.
- a **remote** registration relays what the sender *receives*. Only the events
  the engine raises on a reference are accepted (`REMOTE_EVENTS`:
  `OnActivate`, `OnDeath`, `OnHit`, `OnTriggerEnter`), so the check refuses
  one that could never fire. `reference_script_event_system` relays them
  from the engine's markers.

The saved `EventRegistrations` holds them. A quest that stops drops its
registrations (checked each frame by `reference_script_event_system`), as the
CK says quests and aliases do.

### Skill advances (M47.2)

`Game.AdvanceSkill(asSkillName, afMagnitude)` (Skyrim) banks usage XP against
the player's skill, from the skill AVIF's `AVSK` tuple and UESP *Skyrim:
Leveling*:
- the XP is `magnitude × Skill Use Mult + Skill Use Offset`;
- raising a skill from level L costs `Skill Improve Mult × L^1.95 + Skill
  Improve Offset`, up to the cap of 100;
- the banked XP is the saved `SkillProgress`, and a level reached rewrites the
  skill's base value;
- each rank reached feeds character XP (`LevelingModel::xp_from_skill_rank`)
  through the shared level roll, in `scripted_skill_system`.

The CK says the amount is "automatically modified by any active effects such as
a Guardian Stone"; those modifiers are not modelled. `IncrementSkill` is not
lowered yet.

### Faction membership (M47.2)

`AddToFaction`, `RemoveFromFaction`, `SetFactionRank` and `RemoveFromAllFactions`
(CK: "Remove this actor from all factions they are a member of") edit the live
`FactionRanks` and the saved `ScriptedFactionChanges` ledger. `RemoveAll` names
no faction: the apply step records every faction the actor was in as removed,
so the spawn re-derivation replays the departure.

### Alias scripts (M47.2)

A quest's `VMAD` carries, after its fragment section, the scripts attached to its
aliases (xEdit `wbVMADFragmentedQUST`, Skyrim and Fallout 4; the Fallout 4 fragment
section embeds the fragment script's own flags and properties first). The plugin
parses them into `QustRecord.alias_scripts` (`parse_quest_alias_scripts_with_remap`;
`examples/qust_alias_script_census.rs` proves the layout: the section ends on the last
byte of every fragment-bearing QUST VMAD in Skyrim, Fallout 4 and Starfield).
Vanilla attaches 2,529 (Skyrim), 2,894 (Fallout 4) and 3,859 (Starfield) alias scripts.

CK *ReferenceAlias Script*: "ReferenceAliases receive events from the ObjectReference
they are pointing at"; *OnAliasInit*: "Event received when this alias has been filled
(or not filled) as a result of the quest starting up. Other aliases in the quest will
have also been filled by this point, and the quest is about to start its startup
stage"; *OnAliasShutdown*: "Event received when the quest that owns this alias has
shut down".

- **Install.** `install_quest_alias_scripts` keeps the authored scripts per quest
  (`QuestAliasScripts`, derived). The host (`check_running_quest_alias_scripts`)
  decompiles and checks a quest's alias scripts, a few quests a frame, once the
  quest runs (`check_alias_script`: the reference-script check with `Self` a
  `ReferenceAlias`, over `DELIVERED_ALIAS_EVENTS`); a script all of whose handlers
  run whole becomes an `AliasScript`.
- **Identity.** An alias script instance's variables, state, update/distance/event
  registrations live in the same saved stores as a reference script's, under an owner
  id in `0xFC00_0000..`: `AliasScriptOwners` allocates one per (quest, alias) on first
  use and is saved with the stores it keys. `Self` is the alias (`Value::Alias`) for
  such an owner; `Self.GetOwningQuest()` is the quest.
- **Lifecycle** (`alias_script_event_system`). A quest that runs and has no pending
  alias fill delivers `OnInit`, then `OnAliasInit`, once (`started` is saved); a quest
  that stops delivers `OnAliasShutdown`, and its alias scripts' registrations end.
- **Events.** `reference_script_event_system` delivers the events a reference
  receives (`OnCellAttach`/`OnLoad`/`OnCellLoad`, `OnActivate`, `OnTriggerEnter`,
  `OnHit`, `OnDeath`) to the alias scripts of every alias it fills, beside its own
  reference scripts. Lock order: the alias standing is read before the
  `PapyrusReferenceScripts` guard is taken (the VM holds the quest guard when it reaches
  for a reference's scripts).

Coverage (`examples/alias_script_coverage.rs`): the VM runs 44 % of Skyrim's alias
script instances; Fallout 4 12.5 % and Starfield 7 % wait on `RegisterForHitEvent`
registrations (their `OnHit` is registration-based, not delivered automatically).

### Hit events (M47.2)

Fallout 4 and Starfield deliver `OnHit(akTarget, akAggressor, akSource,
akProjectile, abPowerAttack, abSneakAttack, abBashAttack, abHitBlocked,
apMaterial)` only to a script that registered for it with
`RegisterForHitEvent(akTarget, akAggressorFilter, akSourceFilter,
akProjectileFilter, aiPowerFilter, aiSneakFilter, aiBashFilter, aiBlockFilter,
abMatch)` (CK *RegisterForHitEvent - ScriptObject*): "a single OnHit event
matching the filters", removed after the first matching hit; the target is a
reference or an alias; the power/sneak/bash/block filters are `<0` either, `0`
must not be, `>0` must be; `abMatch` false inverts. `HitRegistrations` (saved)
holds them; `hit_event_system` (after `reference_script_event_system`, while the
`HitEvent` markers stand) delivers each once, with all nine arguments (the
source is the aggressor's equipped weapon; no projectile or material). The
aggressor filter is `None`, a reference, an alias or a faction (an actor in it);
the source and projectile filters must be `None`; other filters stop the run
rather than match on a guess. A quest's or alias's registrations end when its
quest stops (`prune_stopped_quests`).

Skyrim's seven-parameter `OnHit(akAggressor, …)` is automatic. The check tells
them apart by the handler's parameter count: the nine-parameter `OnHit` is left
out of the automatic events (delivering seven arguments to it would mis-bind
every one) and checked where the script registers.

The delivery systems (updates, distance, custom/remote and hit events) share
`ScriptOwners` (`fragment/owners.rs`): an owner's standing, kind and properties.

### Parent calls, locations and editor placement (M47.2)

- `Parent.Fn()` runs the version of `Fn` in the script the running function is defined
  in extends (in the instance's current state): `Frame.defined_in` is set when a
  function starts, and the check walks the same chain. `Parent.OnInit()` and the like
  past the scripts to an engine class has no body to run, so the call does nothing
  (events only).
- `Owner.Member.Fn()`: a receiver written as a member of another script (`(GetOwningQuest()
  as REScript).REParent.KillWithForce(…)`) is resolved through the script type the member
  is declared as in the owner's script.
- *IsChild - Location*: "true if the other location is a child of this one. If they are
  the same location, the function returns false" (`LocationTable::is_within`).
  `ReferenceAlias.TryToGetCurrentLocation()` (Starfield) is the current location of the
  reference filling the alias, None while empty.
- A quest's `OnStageSet(auiStageID, auiItemID)` is a remote event (`REMOTE_EVENTS`):
  raised for every stage set, fragment or not, with the log entry index 0.
- `MoveToMyEditorLocation()` (CK: "Moves this reference to its original editor location
  and orientation") restores the `EditorLocation` the cell loader stamps when it spawns
  a placed reference.
- `KillSilent` is `Kill` "without a kill event" (CK *KillSilent - Actor*): the Story
  Manager's KILL is not raised; the victim's own `OnDeath` still is.

### Actor lifecycle events (M47.2)

Reference and alias scripts receive these beside `OnActivate` / `OnHit` / `OnDeath`:
- `OnDying(akKiller)` (CK: the actor "is starting to die") with the death, ahead of
  `OnDeath`;
- `OnEnterBleedout()` when an actor goes into bleedout (`ActorWatch` diffs the
  `Bleedout` components between frames);
- `OnCombatStateChanged(akTarget, aeCombatState)` from the same diff over
  `AiCombatState`: the new target and 1, or None and 0 on leaving combat; state 2
  (searching) has no source here;
- `OnLocationChange(akOldLoc, akNewLoc)` for the player only
  (`location::PlayerLocation`; the first sight is a baseline);
- `OnCellDetach()` then `OnUnload()` for the references of a cell about to unload
  (`raise_unload_events`, called from `unload_cell_inner` before the despawn, queued
  on `PendingUnloadDeliveries`, delivered with the next frame's events).

`Actor.IsInCombat()` is a guard (`FragmentGuard::ActorInCombat`, `is_in_combat`): the
actor has a combat target, or, for the player, some actor is fighting it. The condition
function `GetPlayerTeammate` (index 453; CK: "1 if an actor is currently set as a player
teammate via SetPlayerTeammate()") reads `FollowerState`, which the vanilla follower
packages condition on; `GetPlayerTeammateCount` is undocumented and stays unmodelled.

### Location keyword data (M47.2)

CK *SetKeywordData - Location* / *GetKeywordData - Location*: a script attaches a float
to a keyword on a location. The saved `LocationKeywordData` keeps it by (location,
keyword); a read sees a write made earlier in the same run. The condition functions
*GetKeywordDataForLocation* (606), *GetKeywordDataForAlias* (608, the owning quest's
location alias) and *GetKeywordDataForCurrentLocation* (651, the subject's location) read
the same store — "returns this value, if any. It otherwise returns 0".

### Weather and form lists (M47.2)

- `Weather.GetCurrentWeather()` (CK: "the current weather used by the sky system") is the
  `WTHR` the host published as `CurrentWeather` when it installed the sky
  (`apply_environment`); `<weather>.GetClassification()` is the record's classification as
  the CK numbers it: -1 none, 0 Pleasant, 1 Cloudy, 2 Rainy, 3 Snow
  (`WeatherClassifications`, first set bit). The weather does not change at run time
  here, so a script sees the worldspace's weather.
- `FormList.GetSize()` / `GetAt(i)` / `HasForm(f)` / `Find(f)` read the `FLST` record's
  forms (`FormLists`; null entries dropped); `AddForm` is not modelled. Also on the VM:
  `GetTriggerObjectCount()` (CK: "the number of objects currently inside this trigger
  volume", from the occupancy pass) and `Actor.GetRace()` (the record's race,
  `Background.race_form_id`), and `GetFactionOwner()` / `GetActorOwner()` (CK: "returns
  only the explicit owner of an object reference and None when an explicit owner is not
  found"; the cell loader stamps the REFR's `XOWN` as `ExplicitOwner`, `OwnerKinds` says
  faction or actor base). These two sets are
  what let Skyrim's ambient critter spawners (`CritterSpawn`, 3,300 references) run: they
  gate on the game hour, the weather and the player's distance, then `PlaceAtMe` a critter.

- `EnableLinkChain(keyword)` / `DisableLinkChain(keyword)` (CK: the function is script code
  over `GetLinkedRef`) follow the keyword's links from the reference, enabling (disabling)
  each, ending at a loop; `GetEditorLocation()` is the reference's editor location
  (`location::editor_location`).

- `Utility.RandomFloat(min, max)` (CK: "between the minimum and maximum values
  (inclusive)", defaults 0.0 and 1.0) draws from the shared `StoryManagerRng`;
  `GetScale()` reads the reference's `Transform.scale`.
- `SetNoFavorAllowed(abNoFavor = True)` (CK: whether the player can ask a follower to
  operate the object) and `SetActorCause(akActor)` (who is responsible for what the object
  does; None clears) are saved per-reference flags (`ReferenceFlags`) for the follower-favor
  and trap-damage systems to read; neither has a consumer yet.

`Message.ResetHelpMessage(asEvent)`, `UnshowAsHelpMessage()` and `Message.ClearHelpMessages()` are
no-op statements: a help message shows once here (no shown-count to reset, nothing standing to
hide — one still on screen is not dismissed).

`<location alias>.ForceLocationTo(akLocation)` records `AliasOverride::ForcedLocation`: the alias
refresh seats the location in the alias's place in the fill order, as `ForceRefTo` does for a
reference alias, until the quest stops.

`RefCollectionAlias.EvaluateAll()` (CK: "Calls EvaluatePackage on all the actors in the collection")
is the collection native that asks every member for a package re-evaluation.

### Input enable layers (M47.2)

Fallout 4 / Starfield replace `Game.Disable/EnablePlayerControls` with
`InputEnableLayer` (CK *InputEnableLayer Script*): `InputEnableLayer.Create()` makes a
layer, `DisablePlayerControls(…)` / `EnablePlayerControls(…)` (eleven Bools, the CK's
defaults) and `Enable<Control>(abEnable = True)` set what it holds disabled,
`Is<Control>Enabled()` reads it, `Reset()` clears it and `Delete()` removes it ("any
disables it may have held are cleared"). A control is enabled while no layer disables it.
The layers are saved (`InputLayers`); the VM holds a layer as its handle (an Int), and a call
is accepted only on a receiver *declared* an `InputEnableLayer`. After each call the layers
fold into `PlayerControlState`'s eight domains; VATS, favorites, running, jumping, sprinting,
the Z key and fast travel are tracked and answer `Is…Enabled` but nothing reads them yet.

### Sound.Play (M47.2)

CK *Play - Sound*: "Plays the sound from the location of the specified object reference"
(it returns a playback instance id; a source with no 3D plays nothing). The plugin now
parses Skyrim+ `SNDR` sound descriptors into `index.sounds` beside the `SOUN` records
(`parse_sndr`: the repeated `ANAM` files, the `LNAM` looping byte; layout checked against
`Skyrim.esm`). In the VM, `<Sound>.Play(akSource)` as a statement — the check requires the
receiver to be *declared* a `Sound`, since `VisualEffect.Play` shares the name — records a
`PendingSoundPlays` request; `scripted_sound_system` (Update, after the hit events)
resolves the descriptor, takes the next of its files in turn, extracts it from the sound
archives and plays it one-shot from the source's `GlobalTransform` through the shared
`SoundCache`. A looping descriptor is not played (a one-shot cannot loop). Not modelled:
the instance id, `PlayAndWait`, `SetInstanceVolume`/`StopInstance`.

### Raw SetMotionType numbers (M47.2)

`SetMotionType(aiMotionType, abAllowActivate)` takes the game's own number (CK *SetMotionType
- ObjectReference*, Skyrim: Dynamic 1, SphereInertia 2, BoxInertia 3, Keyframed 4, Fixed 5,
ThinBoxInertia 6, Character 7). The lowering is game-blind, so a raw literal used to decline;
the VM now reads the load order's game from `ScriptGame` (set by `install_actor_value_names`)
and passes Skyrim's 1, 4, 5 and 7 as the `Motion_*` constant they name, into the same
`MotionTypeChangeRequest` bridge `scripted_motion_type_system` drains. The inertia variants and
every other game's numbers stop the run instead of being guessed (the Fallout 4 page lists only
the constant names). The traps (`physicstrap`, `soundtrap`, `tripwire`, `magictrap`) now pass
this native; `CalculateEncounterLevel` is modelled (below) and `PlayAnimation` drives the
behavior graph (see *Animation graphs of placed objects*), so `PhysicsTrap` (451 placed
references) and `TrapBear` (182) now run: live in Irkngthand04, `PhysicsTrap.OnCellAttach` runs
`ResolveLeveledDamage` on all of them with no stop, and in HonningbrewMeadery01 the four bear
traps attach `BearTrap01.hkx`, run `OnLoad`, and answer `OnActivate` with `Reset01` / `Trigger01`
accepted by the graph. `Weapon.Fire`, the trap-hit events and `ObjectReference.Activate` are modelled
(below, and *ObjectReference.Activate* after *DamageObject*).

### CalculateEncounterLevel (M47.2)

CK *CalculateEncounterLevel - ObjectReference*: "the reference's encounter level, based on the
player's level, the area's level, and the passed-in difficulty" (0 Easy, 1 Medium, 2 Hard,
3 Very Hard, 4 None, the default). The page gives no arithmetic, but two other CK pages do
(`encounter_level.rs` cites them):

- **The area's level** (*Encounter Zone*): the zone's `Min Level` lifts a lower player level to
  it, unless `Match PC Below Minimum Level` is set, when the zone matches the player's level;
  `Max Level` (0 = none) caps a higher one. A reference's zone is its own `XEZN`
  (`PlacedRef.encounter_zone_form`, stamped as `ExplicitEncounterZone`), else its cell's
  (`CellData.encounter_zone_form`, found through `CellRoot` → `CellFormId`); in neither it stands
  at the player's level.
- **The difficulty** (*LeveledCharacter*, "Leveled Character References"): Easy / Medium / Hard /
  Very Hard scale the resolved level by the `fLeveledActorMultEasy` / `Medium` / `Hard` /
  `VeryHard` game settings (0.33, 0.67, 1.0, 1.25 in `Skyrim.esm`; the same four exist in
  `Fallout4.esm`), "without exceeding" it. None is the resolved level itself.

Engine policy where the CK is silent: the scaled level rounds down and never drops below 1; a
load order missing a multiplier setting leaves that difficulty unsupported (the run stops)
rather than guess one. This also fixes the `ECZN` parse: Skyrim / FO4 / Starfield `DATA` is
`owner, location, rank, min level, flags, max level` (xEdit `wbDefinitions*`), where the old
FO3/FNV-shaped read took the location's bytes for rank, min level and flags.

Live in Irkngthand04 (`script.var 321 Damage`, the console twin of a VM variable read): the falling-rock
trap `000C27C8` (a `PhysicsTrap`, `TrapLevel` defaulting to 1 = Medium) resolved `Damage = 13.0`. By hand
from the data (`examples/refr_script_props.rs`): its cell's zone `IrkngthandZone` has Min Level 12 (and no
Match-PC flag), so the player's level 1 is lifted to 12, Medium scales it by 0.67 to 8.04, floored 8, and
`ResolveLeveledDamage`'s `damageLevel > LvlThreshold1 (6) && <= LvlThreshold2 (10)` picks `LvlDamage2 = 13`.

### GetBribeAmount and GetGoldAmount (M47.2)

CK *GetBribeAmount - Actor* only says "the amount of gold required to bribe this actor". The
formula is the community's, from UESP *Skyrim:Speech* ("Bribe Formula"; its second line has a
stray closing parenthesis, and `bribe.rs` takes the only balanced reading):

```text
morality = Morality(npc) * fBribeMoralityMult + 1
v        = NpcLevel * fBribeNPCLevelMult + PlayerLevel
           - ((PlayerSpeechcraft - NpcSpeechcraft) - 100) * fBribeSpeechcraftMult
amount   = v ^ fBribeCostCurve * (fBribeScale * morality)
```

`Skyrim.esm` authors `fBribeSpeechcraftMult` (0.1), `fBribeScale` (0.5) and `fBribeCostCurve`
(1.4); `fBribeMoralityMult` (1.0) and `fBribeNPCLevelMult` (0.1) are not authored there, so the
engine's own defaults apply, which were read from the static setting table of `SkyrimSE.exe`
(each default sits before a pointer to its name string; the same table gives 1.0 / 1.5 / 0.03
for the three the master overrides, so the slots are the right ones). The result truncates to
the `int` Papyrus returns, and a negative base clamps to 0 (engine policy; the source is
silent). `GetGoldAmount` ("the amount of gold on this actor's person") is the count of Skyrim's
`Gold001` (`0x0F`, checked against the record) in the actor's inventory.

Live in Whiterun's Bannered Mare (`script.qcall FavorDialogueScript Bribe 1`, the console twin of a
dialogue fragment calling into its quest script): `Bribe` compared `pTarget.GetBribeAmount()` with the
player's gold and removed 15 gold (5110 to 5095) for Saadia. By hand from the live inputs (Saadia level 4,
player level 1, neither listing a Speechcraft value, Morality 0): v = 4 x 0.1 + 1 - (0 - 100) x 0.1 = 11.4,
11.4^1.4 = 30.2, x 0.5 x (0 x 1 + 1) = 15.1, truncated to 15. `bribe.info <entity>` reads those inputs from the live actors (Saadia: npc level 4, Speechcraft 0, Morality 0; player level 1, Speechcraft 0; amount 15), so they are read, not assumed. Skyrim's skills are now populated (`derive_skyrim_skills`, the UESP *CLAS* formula: 15 + the race's skill bonus + the class's weighted share of 8 points per level above the first, with the complete-sets-then-leftovers assignment order; the player is a level-1 Nord, 15 + 5 Speech); a non-auto-calc NPC keeps its `DNAM` values as written, since how its race bonus and its skill offsets combine with them is undocumented. `bribe.info` then reads Saadia (level 4) Speechcraft 22 against the player's 20 and 15 gold, Hulda 27 and 16 gold. Not modelled: the page's changed assignment once a skill reaches 100 (capped instead). All 18 skill names are checked against `Skyrim.esm` (`vanilla_skyrim_skill_names_all_resolve_and_a_vanilla_npc_derives_them`): that check found the Illusion skill's `AVIF` is `AVMysticism` (the pre-release name; `AVIllusion` does not exist), which the first version had wrong.

### ApplyHavokImpulse (M47.2)

CK *ApplyHavokImpulse*: the direction is normalized and the magnitude is "how hard"; no unit.
The unit is the game's own, read from `SkyrimSE.exe` (its `.text` is readable): the native
registered under the `"ApplyHavokImpulse"` string normalizes the direction and queues it with
the unmodified magnitude; when the command runs, each rigid body of the reference's collision
gets `direction × magnitude × fGameplayImpulseScale × mult(layer) × 0.0142875` through
`bhkRigidBody::ApplyLinearImpulse` (Havok metres; `0.0142875` is a constant in the executable,
the inverse of the `69.99125` Havok scale). `mult` is a game setting picked by the body's
Havok collision layer: Biped (8, 32, 33) `fGameplayImpulseMultBiped`, Props (10) `…Prop`,
Trap (14) `…Trap`, Debris Large (20) `…DebrisLarge`, every other layer `…Clutter`; the native
leaves the helper's mass-normalisation flag clear, so mass does not enter the factor. Defaults
from the executable's setting table: scale 150, Trap 0.15, Biped 0.2, Prop 0.1, Debris Large
1.0, Clutter 1.0 (`Skyrim.esm` authors Biped, Prop, Debris Large and Clutter with the same
values). In the engine's Bethesda-unit physics world the two conversions cancel
(`0.0142875 × 69.99125 ≈ 1`), so a request is applied as
`direction × magnitude × scale × mult` as a Rapier impulse.

Live in Irkngthand04 (`script.call 321 fireTrap` runs `PhysicsTrap.fireTrap`, whose `Done` state sets
the body dynamic and applies `ApplyHavokImpulse(0, 0, -1, 15)`): a placed reference carries no body
itself, so the requests act on `reference_body_entities` (its own, those beneath it, and the bare collision
entities whose `PhysicsSourceForm` names its REFR); before that, `SetMotionType` and the impulse found
"no live body" on the script's reference. With it the falling rock's body logged an impulse of
`(0, -337.5, 0)` (15 x `fGameplayImpulseScale` 150 x the Trap layer's 0.15), fell 359 units from
y -6107.7 to -6467.1 and came to rest.

`RigidBodyData.havok_layer` carries the authored layer (the low byte of the NIF's packed
`HavokFilter`; `serde(skip)`, re-derived at import, kept across a live load like
`part_surface_materials`). The VM queues a `HavokImpulseRequest` on the reference (calls in one
frame add: the impulse is linear) and `scripted_havok_impulse_system` applies it after
`scripted_motion_type_system` made the body dynamic. A zero-length vector or a reference
with no physics body is the CK's documented failure: no impulse. Skyrim only; other games'
natives were not read.

### ConditionForm.IsTrue (M47.2)

Starfield `ConditionForm.IsTrue(akRefObject, akTargetObject)` evaluates a `CNDF` record (the
plugin now parses it: `EsmIndex.condition_forms`). The argument pairing is read from the corpus
(`examples/cndf_census.rs`): of the 2 535 conditions in Starfield.esm's 899 forms, 2 334 run
on the Subject and 7 on the Target, which is how every condition list reads its two references.
The list is evaluated only when every condition is modeled end to end (`is_modeled`) and none
reads state the interpreter holds locked (quest stages, alias bindings); any other form
stops the run rather than answering with the fail-open pass. 338 of 899 forms qualify.

Live in a Starfield Cydonia boot (`cndf <form> <entity>` is the console twin; `cndf list` names the answerable
forms: 259 with the interpreter's view of that load): the first 40 evaluated against the player gave 34 false and
6 true, so the evaluation discriminates. `0003BF4B` is a single `GetGlobalValue(273872) < Global 273868` (true);
`00030171` is `GetActorValue(196973) == 0 OR GetActorValue(2602891) == 1` (true: the first reads 0).

### Animation graphs of placed objects (M47.2)

`PlayAnimation`, `PlayAnimationAndWait` and `RegisterForAnimationEvent` send events to a
reference's Havok behavior graph ("sends the named event to the object's animation graph", CK
*PlayAnimation*). Skyrim drives its levers, pull chains, pillars, portcullises, pressure plates
and wall traps that way; the engine now runs those graphs.

**The data.** A NIF's `BSBehaviorGraphExtraData` names a *project* packfile
(`meshes\Dungeons\Nordic\Levers\RuinsLever01\NorLever01.hkx`). The project lists a *character*;
the character names its *skeleton*, its *animations* and a *behavior* packfile; the behavior is a
state machine whose transitions fire on named events and whose states each play one clip
(`crates/hkx/src/behavior.rs`, Havok 2010.2, 64-bit — Skyrim SE only; the Havok SDK on disk has
no Behavior headers, so every field offset was derived from the data and checked: the decoder
reads 420 of 423 vanilla graphs, `examples/behavior_census.rs`, and the three that do not carry no
string table; `examples/probe_behavior.rs` prints a graph). What is decoded: the event names, the
root state machine (states, their transitions, the wildcard transitions), each state's clip
(animation, playback mode, speed, start time, held fraction) and the clip's trigger events.

**The runtime** (`animation_graph.rs`, `asset_provider/object_graph.rs`). The cell loader stamps a
`BehaviorGraphRef` on the placement root; `populate_object_graphs` (run after interior loads, the
exterior bootstrap and every streaming step) resolves the project from the archives, registers the
state clips in the animation registry (a once-playing and a looping variant per animation) and
attaches an `ObjectAnimationGraph` in the machine's start state, dropping the NIF's own embedded
controller player, which would override the graph. An event takes the state's own transition on it
(else a wildcard one; `DISABLED` transitions never fire), enters the new state, and starts its clip
on the object's `AnimationPlayer`: single play holds the last pose, looping loops, a user-controlled
clip is held at its authored fraction. The clip's trigger events are scheduled in real time
(`(clip time − start time) / speed`, end-relative ones from the clip's length) and raised by
`object_graph_step_system`, so a lever that finishes `PullDown` raises `Pulled` and settles in the
"Pulled" state with no script involved. A blender keeps its blend weights (weighted from the graph's variables
each frame), and a transition with a blending effect cross-fades the old
pose out as the new one fades in: a state's generator may also be a *Gamebryo sequence* (a `NiControllerSequence`
of the object's own NIF, by name), a *nested state machine*, or a *blender* over such children, and
the decoder flattens those into `State::content` (`examples/graph_dump.rs` prints a decoded graph;
169 of the 420 vanilla graphs carry a clip in a root state and 251 do not, most of them actor and
creature graphs). The runtime keeps one run per machine: an event goes to the root machine first
and, when its current state has no transition on it, to the nested machines of that state; a
machine changing state drops the trigger events its old state scheduled and schedules the new
state's, and a looping clip's triggers come round again each loop. The *motion leaf* is what plays on
the object's `AnimationPlayer`: a Gamebryo sequence the NIF actually carries if the configuration
has one, else the first clip; a change of machine state that does not touch the motion leaf does not
restart it, and one that changes which leaf it is switches the player. The swinging blade
(`meshes\traps\swingingblade\behaviors\behavior00.hkx`) is this shape: its one root state holds a
blender over two nested machines, `Behavior01` swinging the `Trigger` clip (`State00` held,
`State01` looping, `State02` played once on `Single`, raising `Apex`, `Reset` and a swing sound) and
`Behavior02` naming `playanim01` / `playanim02`, which the blade's NIF does not carry. Live in Bleak
Falls Barrow, stepping on trigger `00086018` relays the activation to the three blades, their
`PlayAnimation('Single')` is accepted by the nested machine, and the `Blade01` node sweeps from
+54 degrees through -54 and back. The blender's weights, transition blends and effects, and
transition conditions and variables are all evaluated (layered blenders blend by their
bone-weight masks); a blender that mixes two moving clips shows the blend.

**The natives.** `PlayAnimation` returns whether the graph took the event. `PlayAnimationAndWait`
is a latent statement: the schedule is deterministic, so the seconds until the named event is
raised (following the machine through the transitions the clip events take) are computed up front
and the run suspends for them; the CK's "if the event you send it doesn't exist, the function will
never return" becomes an immediate return, and a reference with no graph (or an event the graph
never raises) also returns at once, the CK's false. Used as a value it is refused (a latent call
cannot sit inside an expression). `WaitForAnimationEvent(event)` is the same wait without the send
(CK: "waits for the specified animation event from this object to happen before returning"; false
when the graph unloaded first, here: no graph). `TrapBear` uses both: `OnTriggerEnter` plays
`Trigger01` and waits for `Trans01`. `RegisterForAnimationEvent(sender, event)` registers the
calling script (a call on another form registers nothing) and answers true when the sender has a
graph, false otherwise (the CK: "the animation graph may not be loaded quite yet"); the event name
is never checked ("the game will allow you to register ANY animation event"). The events a graph
raises are the ones an accepted `PlayAnimation` sent and the ones a clip's triggers fire as it plays
(a trigger raises its event whether or not a transition answers it, as an animation notify does);
`animation_event_system` (`fragment/anim_events.rs`) drains them each frame and queues
the animation-event handler `(akSource, asEventName)` for every registration for that reference and event, as a
separate run like a custom event's receivers. Registrations are a saved `AnimationEventRegistrations`
(FORMAT 101) and end when the owning quest stops. When the sender's graph unloads with its cell the
registrations for it are dropped and each script is sent the unregistered event with the sender and
the event name (CK: "automatically unregistered by the game due to the target animation graph
unloading", not for a manual unregister). A `SoundPlay.<name>` event
a graph raises plays the sound descriptor with that editor id from the object, through the same
queue as `Sound.Play`: 120 distinct `SoundPlay.*` names occur in the vanilla Skyrim graphs, 77 of
them descriptors of Skyrim.esm (the rest are in the DLC masters), and the lever and pull-chain
graphs play live. The node-attached `SoundPlayAt.<name>.<node>` form is not played. Actors'
graphs run too (FO4 actors carry the same `ObjectAnimationGraph` component
the object system drains, driven from their CLKT and skeleton).
`UnregisterForAnimationEvent` removes the registration;
`SetAnimationVariable*` write the graph variables the transitions read.

**Animated nodes.** The cell loader flattens a placed NIF to one entity per mesh, but the graph's
skeleton bones are NiNodes (`Lever01`). A NIF with a behavior graph keeps its node tree
(`cell_loader/graph_nodes.rs`): a hierarchical import records the nodes, the node entities are
spawned under the placement root with the bone names, and each mesh hangs from its node at the
transform the hierarchical import gives it (the meshes' world poses are unchanged, so a prop at rest
renders as before). A NIF whose hierarchical and flat imports do not line up keeps the flat spawn.

**Saves.** The running graph is not saved: a loaded prop starts in the machine's start state, which
is the game's own behaviour too — the props' `OnLoad` scripts replay `PlayAnimation` from their
saved script state (`NorLever01Script.SetDefaultState`).

Verified live on Bleak Falls Barrow: activating the Nordic lever runs `FullPush` (state Pulled →
PulledToPushed), the `Lever01` node turns from −44.96° to +44.96° over ~0.8 s, the clip's
`FullPushedUp` trigger settles the machine in "Pushed", and the next activation's `FullPull` is
accepted from there. 43 animated props attach in that cell.

### Speech outcomes, anim archetype, scanned references (M47.2, FORMAT 98)

Three small saved states, each keyed by reference FormID like the other per-reference script
state:

- **`SetBribed` / `IsBribed`, `SetIntimidated` / `IsIntimidated`** (`speechcraft.rs`,
  `SpeechcraftOutcomes`): CK "flags this actor as bribed / intimidated by the player" and "has
  been bribed / intimidated by the player", the argument defaulting to True. The pages state
  no decay, so a flag lasts until a script clears it. An `Is…` in the same run sees an earlier
  `Set…`. The dialogue condition functions `IsBribedbyPlayer` (402) and `IsIntimidatedbyPlayer`
  (116) are not catalogued yet and stay fail-open.
- **`Actor.ChangeAnimArchetype(keyword)`** (Fallout 4; `animation_graph.rs`,
  `AnimArchetypeOverrides`, consumed by `anim_subgraph.rs`): CK "adds the specified AnimArchetype
  keyword to this actor"; none "removes any existing AnimArchetype keyword (sets actor back to
  'Neutral')"; true on success. The archetype is an *actor keyword*, and the CK *Subgraphs* page
  says what reads it: a RACE's "Subgraph Data" entry lists the actor keywords that "need to exist
  on the Actor for this sub graph data to get picked", and "the game will always choose the
  subgraph data that matches the maximum number of actor keywords". Two consumers follow.
  `keywords::has_keyword` gives an actor with an override exactly that archetype keyword among the
  `AnimArchetype…` ones (the `KYWD`s so named: Fallout4.esm has ten, with the FormIDs the CK page
  lists) and hides its authored one, so `HasKeyword` guards and the `HasKeyword` condition see the
  change. `anim_subgraph::select_subgraph(world, actor, role, target)` picks the entry of a role
  (third person) whose actor keywords the actor all has — the special `Female` keyword is ACBS
  *Female* XOR *Swap Gender Anims* — taking the most keywords, the first authored on a tie (the CK
  calls that undetermined). A race's entries are its own, its `SRAC` template's and those of races
  `SADD`-additive onto it (`RaceRecord::subgraph_data`, xEdit `wbRACE`). Vanilla check
  (`vanilla_fo4_archetype_npcs_select_their_entries`): 710 of the 726 archetype-authoring `NPC_`s
  select an MT entry, and all 710 name their own archetype. The actor's graph itself runs (variables, blends and
  events of `MTBehavior` are evaluated; see the actor-graph paragraphs below). What is **not** built is the pose
  modifiers (look-at, twist, foot IK, ragdoll, jiggle) and the modifiers whose formula the files do not give
  (`crates/scripting/src/animation_modifiers.rs`), and the archetype is visible on screen through the running graph. Fallout 4's HKX files are Havok 2014 packfiles
  (file version 11, a predicate area before the section table; `byroredux-hkx` reads them: all 328
  behavior graphs decode with `LAYOUT_2014`, 13,989 of 13,990 clips and all 121 skeletons; the one
  clip left is a bare `hkaAnimationContainer`. 856 of those clips are `hkaLosslessCompressedAnimation`
  (`decode_animation` reads them; 589 are additive deltas, which the engine has no layering for and
  reports as such), a class the Havok SDK does not have, read from the layout measured on those
  files: see `LosslessLayout` in `crates/hkx/src/animation.rs`). A graph's start
  is `BehaviorGraph::start_clip`: follow each machine's start state down and into the sub-behaviors a
  state refers to, to the first clip. The humanoid `MTBehavior.hkx` enters `MTIdles`, refers to
  `DialogueBehavior.hkx` and starts in `Animations\PoseA_Idle1.hkt`. Which file that name means is the
  subgraph entry's job (CK *Subgraphs*): it names the graph and an ordered list of animation folders,
  and the clip is the first folder holding the file, so a nervous woman idles in
  `...\MT\Nervous\Female\PoseA_Idle1.hkx` and a neutral man in `...\MT\Neutral\PoseA_Idle1.hkx`.
  `populate_archetype_idles` resolves every third-person `MT` entry once per world (50 of the 556
  entries of Fallout4.esm have an idle that resolves: the humanoid ones) into `ArchetypeIdleClips`;
  `archetype_idle_system` gives each actor with a skeleton and no animation the clip of the entry it
  selects, and when `AnimArchetypeOverrides` changes it re-selects, so `ChangeAnimArchetype` swaps the
  idle (`an_actor_idles_in_its_archetype_and_change_anim_archetype_swaps_it`). An actor something else
  animates (a seat, a walk, a played idle), the player and the dead are left alone. Live in
  Goodneighbor's Third Rail: Magnolia, the drifters and the ghoul patrons each carry an `AnimationPlayer` on a
  distinct archetype clip with a running clock (the earlier code gave Fallout 4 no idle:
  `humanoid_default_idle_kf_path` is `None` for it). `anim.archetype <entity> <keyword>` (the console twin of the
  script call) swapped Magnolia's idle live through Friendly\Female, Scared\Female, Confident\Female and
  Neutral\Female (a registration bug the unit test could not see, the marker component missing from the
  engine world, is pinned by `the_engine_world_registers_the_marker_and_the_pass_state`). The graph itself now runs on the actor
  (`asset_provider/actor_graph.rs`): `inline_references` nests each sub-behavior (`DialogueBehavior.hkx` under
  `MTBehavior`'s `MTIdles`) as a machine of the state that refers to it, renumbering its events into the
  parent's table by name, and an `ObjectAnimationGraph` runs it with `animated_root` the actor's skeleton;
  its ~250 clips are resolved on demand (`LazyClips`: a state whose clip is not loaded asks for it and the next
  `populate_actor_graphs` pass, fed by `step_actor_graphs` each frame with the streaming provider or, in an
  interior, a retained one, decodes it from the actor's folders and restarts the graph). So `PlayAnimation`
  reaches actors: live in Goodneighbor's Third Rail, `anim.event 2453 dyn_Flavor` (Magnolia) was accepted,
  `posea_idleflavor2` resolved from her Friendly\Female folders and played its 8.3 s, and the graph's
  clip-end trigger returned her to her idle; an event the graph does not know is rejected. The graph's
  variables, blends, transition fades, conditions, locomotion inputs and root motion are described under
  "Behavior graph depth" below.
- **`SetScanned`, `IsScanned`, `AddToMapScanned`** (Starfield; `scanning.rs`,
  `ScannedReferences`): no CK page, but the game's own `ObjectReference.pex` defines
  `AddToMapScanned(b)` as `Self.SetScanned(true)` then `Self.AddToMap(b)`, and xEdit's condition
  table calls `IsScanned` "check if a object has been scanned". The `AddToMap` half is the
  existing map-marker state. The scanner itself has no player-side runtime, so only scripts
  set the flag.

### Destructible objects (M47.2, FORMAT 102)

`ObjectReference.DamageObject`, `GetCurrentDestructionStage`, `ClearDestruction`, `SetDestroyed`,
`IsDestroyed` and the `OnDestructionStageChanged(aiOldStage, aiCurrentStage)` event
(`destruction.rs`). The data is a base record's destructible block, which the plugin now keeps on
the `StaticObject` (xEdit `wbDEST`: `DEST` health, `DSTD` stages with health %, index, model damage
stage and flags; 615 base objects in Skyrim.esm). The rules are the CK's *Destructible Objects*
tutorial and the function pages: a stage is entered when the object's remaining health falls to its
percentage, the stage number scripts see is the *model damage stage*, an undamaged object is -1,
Cap Damage stops a hit at the capped stage ("if you've got 20 health on your object and you cap with
a health percentage of 80%, the first hit only brings health down to 80%"), Ignore External Dmg
makes the stage ignore `DamageObject`, Destroy sets the destroyed flag and Disable disables the
reference; skipped stages are why the event carries both numbers, and `ClearDestruction` raises
none. Each reference's damage and destroyed flag is a saved `DestructionStates` by FormID. The
visible side (replacement models, explosions, debris, a stage's self damage per second) is not
modelled, and a base with a health of 0 or less takes any damage straight to 0% (the CK says
nothing about it). Census: `refr_script_coverage` on Skyrim 61.1% to 62.6% of scripted references
(`MineOreScript` now stops on `Activate`, not `DamageObject`; see the next section).

### Fallout 4 and Starfield quest stage fragments (M47.2)

None of the 920 scripted Fallout 4 quests or 1,349 Starfield ones bound a stage fragment: the QUST `VMAD` fragment
decoder demanded Skyrim's version byte 2, and a version-6 `VMAD` writes 3 there and puts the fragment script's own
status byte and property table before the fragments. `parse_quest_fragments` and the alias decoder now share one
walker (`quest_tail`): Fallout 4 binds 7,150 fragments on 885 quests, Starfield 10,202 on 1,012 (Skyrim's 845
quests are unchanged; `version_6_quest_vmads_bind_their_stage_fragments`). Of Starfield's FC05 quest's 35 fragments
28 lower. Binding them was not enough: `ScriptProvider::resolve_pex` looked a script up as
`scripts\Fragments:Quests:QF_...pex`, leaving its `:` namespace colons in the archive key, so every
namespaced script (all 1,012 Starfield quest fragment scripts, and any `Namespace:Script` attachment) was "not in
archive"; each colon is a folder now (`a_namespaced_script_name_maps_to_its_archive_folders`). With both,
a Cydonia boot logs `populated 8206 quest-stage fragments from 1012 scripted quests`, and
`quest.setstage 0x1F0267 1600` runs `QF_FC05_001F0267.Fragment_Stage_1600_Item_00` through the VM. That stage
(`Alias_MarcoStarshipMapMarker.GetRef().AddToMapScanned(true)` beside `SetMarkerVisibleOnStarMap(true)`) needed
`SetMarkerVisibleOnStarMap`, a native with no body in the game's `ObjectReference.pex`, now a per-marker
flag in `MapMarkers` (its name is the whole grounding; nothing reads it, there is no star map; `FORMAT_MAJOR` 104).
It first stopped at `getref on None` for two reasons: the QF script's alias properties (`Alias_*`) live in the version-6 fragment section, which the walker read and dropped (they are now attached to the quest's `script_instance` as the fragment script, `parse_quest_fragment_script_with_remap`), and the alias was unfilled. With the properties the fragment runs without a stop. The alias (a location-ref fill into the not-yet-loaded `MarcoStarshipExterior`) cannot fill from a Cydonia boot, so `quest.force 0x1F0267 26 0x9B674` (`ForceRefTo` from the console; the third argument is an entity id, `.`, or a loaded placed reference's FormID `0x…`) binds it to a loaded alias candidate, and `quest.setstage 0x1F0267 1600` then takes `0009B674` from `scanned false, map-visible false, fast-travel false, star-map-visible false` to all four `true` (`scan.info`): the vanilla fragment's `AddToMapScanned(true)` and `SetMarkerVisibleOnStarMap(true)` both ran live. Two console bugs found on the way: `quest.force` did not mark the scene bindings dirty, so the override was never applied, and it accepted a mesh child that only carries the FormId, which an alias cannot bind (it now says so). Starfield's profile has no scripts archive (the path is a literal one; a bare archive name fails to open and the VM then has no scripts):
Starfield's profile has no scripts archive:
`--scripts-bsa "Starfield - Misc.ba2"` is what lets its VM run.

### Behavior graph depth (M47.2)

What the graph runtime (`crates/scripting/src/animation_graph.rs`, `animation_modifiers.rs`,
`crates/hkx/src/behavior.rs`) evaluates beyond events and clips, checked on Fallout 4's `MTBehavior`
(Magnolia, Goodneighbor) and the 328 Fallout 4 graphs (`examples/blend_dump.rs`, `class_census*.rs`).

**Sources.** The Havok Behavior SDK is not on disk (the 2013 SDK here has the animation-level blend job and
class patches, no `hkb*` source), so the semantics come from the files and from the engine itself: Fallout4.exe
carries Havok's description strings and validation messages for the behavior classes (`strings Fallout4.exe`;
its class tables are built in code, so member *offsets* come from the vanilla graphs' objects, in the member
order the strings give). Examples below quote them.

- **Variables and bindings.** `hkbBehaviorGraphData`: `m_variableInfos` (6-byte elements), the initial values in
  `hkbVariableValueSet`'s word array, the names in the string data; `Get/SetAnimationVariable*` and `anim.var`.
  `hkbVariableBindingSet` (at the bindable's `+0x10`, 40-byte bindings: member path, variable `+0x1C`, array
  offset `+0x14`) binds a variable to a clip's `playbackSpeed`, a blender's `blendParameter`, a transition
  effect's `duration`, a blender child's `weight`, a machine's `startStateId`, a selector's
  `selectedGeneratorIndex`, an array element of a bone-weight array. A sub-behavior's variables, events and
  modifier ids are renumbered into its parent's tables when it is inlined.
- **The generator tree.** A state's generator is a tree (`Generator`: clip, Gamebryo sequence, blender, selector,
  nested machine, sub-behavior reference, modifier wrapper), and the pose is the active clips of the whole tree,
  not one motion leaf: the vanilla `MTRoot` is a layered blender whose children are whole nested machines.
  A machine starts in the state its `startStateId` variable names (`iSyncIdleLocomotion`, `iIsInSneak`, ...: 80
  bindings), a selector picks its child by `iIsPlayer` and the like, and a change of the clips that is not an
  event (a selector's variable moving) restarts the pose.
- **Parametric blenders.** Flags `0x10` parametric, `0x20` cyclic, `0x01` sync. A parametric blender's child weights
  are positions on one axis (walk 80 / run 370 for `Speed`; eight headings at 0, 0.125 ... round a cyclic
  `Direction`); the parameter falls between two children, which share the pose; a cyclic blender wraps. The
  top blender of the 8-way graph is fed by its `BSCyclicBlendTransitionGenerator` wrapper ("The blend parameter
  to pass into the cyclic blend generator. DO NOT SET A VARIABLE ON THE REFERENCED BLENDER GENERATOR").
- **Layered blenders and bone masks.** Any other blender mixes its children by their weights ("Non parametric
  blend children should have weight between 0.0 and 1.0"), bone by bone through each child's
  `hkbBoneWeightArray` ("A weight for each bone. If the list is too short, missing bones are assumed to have
  weight 1.") and normalised where the layers meet - the same combine the SDK's `hkaSampleBlendJob` documents for
  per-bone weights. `AnimationLayer::bone_weights` + `AnimationStack::bone_index` carry the mask; the vanilla
  upper-body / lower-body pair is complementary (0.33 / 0.66 / 1 gradients over the spine), so each bone comes
  from the layer that owns it. A mask element can follow a variable (`boneWeights` bindings on the array). A
  *boolean* bound to a child's own `boneWeights` (`bEquipOk`, `bAllowHeadTracking`, `bAnimationDriven`) is read
  as a switch for that child: the camera machine of `MTRoot` is bound to `bAnimationDriven`, and without the
  switch every NPC would carry a 50% camera pose. That is an inference (the engine's strings do not describe
  the binding); integers and reals bound there (`iSyncIdleLocomotion`, `Speed`, `Direction`: 10 children) are
  left unapplied.
- **Sync points.** A synced blender's clips that carry `SyncLeft` and `SyncRight` annotations (1,357 of the
  humanoid clips, left first or right first) share one phase: 0 at each clip's left plant, 1 at its right, 2
  wraps; each clip is put at the time that maps to through its own points and the phase runs at the weighted
  mean of the clips' rates. A walk that plants its left foot at 0.33 s and a run that plants its right foot first
  therefore stay foot for foot. Clips without the pair (idles, gestures) play at their own speed.
- **Transition fades and conditions.** The effect's duration (`+0xA8`; the names `blend_0.25s` agree with the
  floats) or its bound variable fades the old pose out as the new one fades in; the condition at `+0x28`
  (C-like text over variables, `crates/hkx/src/expression.rs`) gates the transition. A settled single clip goes
  back to an `AnimationPlayer`. The engine world registers `AnimationStack` (an unregistered storage dropped the
  pose silently - found live).
- **Modifiers** (`animation_modifiers.rs`; Havok 2014 layouts only). The ones that act on the graph itself run
  while their generator is on the active path: `BSAssignVariablesModifier` ("A float variable to set on activate"),
  `BSModifyOnceModifier`, `hkbEventsFromRangeModifier` (vanilla: `SpeedSmoothed` <= 150 -> `walkStart`, <= 600 ->
  `runStart`), `hkbEvaluateExpressionModifier` ("Expression of the form 'variablename = expression' or
  'eventname = boolean expression'", four event modes), `BSEventOnDeactivateModifier`,
  `BSEventEveryNEventsModifier`, `hkbEventDrivenModifier` ("Event used to activate the wrapped modifier"),
  `hkbModifierList`, `hkbTimerModifier` / `BSTimerModifier`, `BSRandomAlarmModifier` (a deterministic draw), `BSGetTimeStepModifier` ("The current time step.": its bound `fTimeStep` takes `dt`). Their
  events go through the same path as a clip trigger's; the graph moves itself (a modifier raising `walkStart`
  takes the idle's transition, tested). Not run: the modifiers that pose the skeleton (look-at, twist, foot IK,
  ragdoll, jiggle), those whose formula the files do not give (`BSSpeedSamplerModifier`,
  `BSInterpValueModifier`, `hkbDampingModifier`: the strings name their members, not what they compute) and `BSIsActiveModifier` (its booleans are for the engine's own code).
- **Locomotion inputs.** `graph_locomotion_system` (PostUpdate, after the movers, for graph actors the Skyrim
  walk-clip system does not drive) feeds `Speed`/`SpeedSmoothed`, `Direction` and `moveStart`/`moveStop` from the
  actor's displacement (hysteresis windows from the walk-clip system). A graph actor faces -Z: Havok's forward is
  +Y in Z-up, which converts to -Z, and a live Magnolia with an unrotated root has her toe 8.4 units beyond her
  foot along -Z (the skeleton's reference pose agrees for FO4 and Skyrim). `actor.walk <entity> <speed> <seconds>
  [turns]` is a stand-in mover. Live: a 100 u/s walk set `Speed` to 99.9997, took `moveStart`, blended the
  synced walk-lean clips with the camera child switched off, and `moveStop` faded back. The Walk -> Jog -> Run
  states are chosen by events the *engine* sends (`Jog`, `Run`), not by the graph; they are not sent. The mover
  systems face an NPC `+Z` toward its target, the wrong end of an FO4 rig; no FO4 NPC walks by itself yet.
- **Root motion.** Fallout 4 authors locomotion in place and keeps the travel in
  `hkaDefaultAnimatedReferenceFrame` (duration `+0x40`, `hkVector4` samples `+0x48`; 113.9 units over 1.233 s for
  `WalkForward`, along Z-up +Y). The decoder reads it, the clip converter lays it on the root bone and binds the
  root as the accumulation root, a stack's `RootMotionDelta` is the weighted mix of its layers' deltas, and
  `graph_root_motion_system` carries an actor whose graph sets `bGraphDriven` / `bAnimationDriven` by it (live:
  249 units in 3 s on the walk blend). Yaw in the extracted motion is not applied.

### ObjectReference.Activate (M47.2)

CK *Activate - ObjectReference*: `Activate(akActivator, abDefaultProcessingOnly = false)`; with the
flag set "no OnActivate event will be sent to any scripts and the object will ignore the blocked
flag". The engine's activation is one `ActivateEvent` its own consumers (door, pickup, container)
and the Papyrus `OnActivate` delivery all read, so the flag rides it as a one-frame `NativeActivation`
marker: `reference_script_event_system` and the remote-event relay skip `OnActivate` for it, and
`default_activation_blocked` answers false. Queued apart in `PendingFragmentActivations` (`.1`) and
flushed with the plain ones at the head of the next frame. Two things made this the largest single
step of the census: the VM binds the player it holds as a player local, which `prim_activate` did
not recognise (every `Activate(akActionRef, ...)` in an `OnActivate` declined, not only the
flagged ones). Skyrim scripted references that run a script: 66.8% to 75.4% (`MineOreScript` 1,317
and `MineOreFurnitureScript` 348 references now run). Not modelled: the call's Bool result (whether
default processing succeeded) and what default processing does for a furniture, which the CK does
not say; the existing consumers decide. `FORMAT_MAJOR` 103 (`Effect::Activate.native_only`).

### ModFavorPoints and MakePlayerFriend (M47.2)

`Actor.ModFavorPoints(int)` is called by about a hundred vanilla Skyrim fragments (the Thieves
Guild, Mages College and Dark Brotherhood quest rewards, e.g. `akSpeaker.ModFavorPoints(50)`). It has
no CK page and SkyrimSE.exe does not define it (a case-insensitive search of the binary for
`modfavor` finds nothing) - because it is not a native: the game's own `Skyrim - Misc.bsa`
`Actor.pex` defines it in Papyrus, `If iFavorPoints > 0: Self.MakePlayerFriend()`, and
`ModFavorPointsWithGlobal(g)` as `ModFavorPoints(g.GetValueInt())`. `MakePlayerFriend()` has a CK
page with its body: for an actor whose base `IsUnique()`, if `GetRelationshipRank(player) == 0`,
`SetRelationshipRank(player, 1)`. `ModFavorPoints`, `ModFavorPointsWithGlobal` and `MakePlayerFriend` now run as written: a positive amount makes a *unique*
actor (ACBS flag `0x20`, xEdit `wbDefinitionsTES5`) whose rank with the player is 0 a rank-1 friend
(recorded in `ScriptedRelationships`, which `GetRelationshipRank` reads); zero, a negative amount, a
common actor or an actor already ranked changes nothing
(`mod_favor_points_befriends_a_unique_actor_with_a_positive_amount`). The earlier note that the
call stayed declined looked for a native and never opened the script.

### Weapon.Fire and projectiles (M47.2)

CK *Fire - Weapon*: "fires the weapon from the specified object reference, using the specified
ammo" (None: the reference's current ammo). The native records a `ProjectileLaunch`; the host's
`projectile_launch_system` (`systems/projectiles.rs`) resolves weapon, ammo and projectile and
`projectile_flight_system` flies it. The records: `AMMO.DATA` names the `PROJ`; `PROJ.DATA` (xEdit
`wbPROJ`, parsed into `ProjRecord`: flags/type word, gravity, speed, range, explosion, impact force,
and on Skyrim+ collision radius and lifetime) gives the speed in game units per second, the range in
game units, and a lifetime in seconds where 0 means live out the range ("arrows are set to 0.00 to
live forever"). A projectile flies straight along the source reference's forward from just past
it, sweeping each step with a physics corridor; a hit on a live actor queues the `HitEvent` the
damage pipeline already consumes (so health, death, bleedout, `OnHit` and the story events follow),
worth the weapon's damage plus the ammo's (Skyrim: a bow and its arrow add; `TrapDartWeapon01` is 1,
its `TrapDart` ammo 0). Projectiles are plain data in `ActiveProjectiles`, never saved.

Refused or not modelled, said plainly: a projectile with a non-zero gravity does not launch (the CK
describes the field only as "how much gravity has an effect on the projectile", and the game's own
arrow gravity settings do not give a general unit; `TrapDart` has 0, arrows 0.35, ballista bolts 0.8),
the projectile model, impact force on bodies, and the explosion a record names. The corridor width
(at least 4 units) is an engine approximation like the melee sweep's, not game data. Census:
`DartTrap` (177 Skyrim references) now passes the VM; its firing is started by a trigger's
`Activate` (`OnActivate` reads the activating `TrapTriggerBase`'s `Type`), so a live test needs a
trigger crossing, and the flight and hit are covered by tests against a real physics world.

### Activate parents (M47.2)

CK *Reference*, Activate Parents: a reference's activate parents "will 'pass on' an activation event
to this reference when they are activated"; each has a *Delay* ("the number of seconds to wait
after the parent's activation"); "for scripting purposes, this reference will be activated by the
reference which acted as the activate parent, not the originally activating NPC"; and *Parent
Activate Only* means "this reference can ONLY be activated by an Activate Parent - direct activation
(e.g. by the player) will not be possible. Scripts can always activate the object". The CK *Traps
and Prefabs* tutorial: a pull chain opens its portcullis, a tripwire springs its mace trap.

The data is the REFRs' `XAPR` (reference + delay) and `XAPD` (parent only) sub-records, parsed into
`PlacedRef::{activate_parents, parent_activate_only}`; `ActivateParentTable` keeps them by form id
over the whole load order (Skyrim.esm: 4388 parent-to-child edges), so a relay does not depend on
which cell holds the parent. `activate_parent_relay_system` reads the frame's `ActivateEvent`
markers and queues an activation of every loaded child, at once or once its delay is up, with the
parent as the activator, through `PendingFragmentActivations` (so every consumer sees it exactly
once, a frame later). A child that is not loaded when its turn comes is skipped, and a relay that
would close a cycle is not made. The player's direct activation of a Parent-Activate-Only reference
is refused in `emit_activate_event`. Before this nothing passed an activation on, so no lever or
trigger could open anything.

A reference's VMAD names only the script properties it edits, so a script it lists inherits the
rest from its base record's copy (`attach::dedup_vmad_scripts` merges them; Skyrim.esm's `DartTrap`
ports list the script with no properties and the weapons and sound sit on the base). Live in
Blindcliff Cave: pulling the Nordic pull bar `00021F4D`, which is a dart port's activate parent,
runs `DartTrap.OnActivate`, whose loop launches 21 `Weapon.Fire` darts from the chain of linked
ports (`TrapDartWeapon01`, 1 damage, 1300 u/s). Two things stopped a dart from reaching anything,
both found live: the port's muzzle is its NIF's `ProjectileNode` (16 units ahead of the model
origin, `ProjectileMuzzle`), and a dart starting at the origin sat inside the wall the port is
mounted in; and the sweep's 4-unit corridor (the width melee needs against bone boxes) overlapped
that wall, so solid geometry is now swept with a thin ray (the projectile's own radius) and actors
with the corridor, ignoring the launcher's own colliders. With both, standing the player in front of
a port and firing the trap landed two darts on the player: `HealthAV` damage 2.0 of 100 (the
`HitEvent`s reach `combat_damage_system` now that it runs after the projectile systems). Live in Bleak Falls Barrow: stepping on trigger `00086018` (a `TrapTriggerBase`, whose script
`Activate`s itself) passes the activation to the three `TrapBladeSwinging01` references that name it
as their parent (`0003AC37`, `0003AC35`, `0003AC33`; all Parent Activate Only), whose scripts run.
Their behavior graph then rejects `PlayAnimation('Single')` from its start state (the graph decodes
with no clips), so the blades do not swing yet; and the barrow's four `DartTrap` ports name no
activate parent, so their trigger is something else.

### Trap hits (M47.2)

CK *OnTrapHit* / *OnTrapHitStart* / *OnTrapHitStop* / *ProcessTrapHit* (`systems/trap_hits.rs`).
A *trap* is a body on Havok layer 14 (Skyrim's Trap layer, the one the impulse multiplier table
names) that carries Papyrus scripts; a *live target* is an actor that is not dead. Each tick the
trap's colliders are shape-tested against solid colliders (`PhysicsWorld::bodies_overlapping_body_where`,
which also finds fixed and kinematic neighbours that the narrow phase does not pair), and a new
(trap, actor) overlap raises `OnTrapHitStart` and `OnTrapHit` with the CK's ten arguments `(akTarget,
afXVel, afYVel, afZVel, afXPos, afYPos, afZPos, aeMaterial, abInitialHit, aeMotionType)` through
`raise_reference_event`; the pair ending raises `OnTrapHitStop(akTarget)`. Velocity and position are
the trap body's, converted to the scripts' Z-up axes; the motion type is Skyrim's number (Dynamic 1,
Keyframed 4, Fixed 5). The scripts gate when a contact matters themselves (`TrapHitBase` stays in its
`CannotHit` state until its trap springs). `ProcessTrapHit(akTrap, afDamage, …)` on the target
becomes its `HitEvent`, so the damage pipeline takes it from there; the pushback distance and the
stagger have no model.

Not modelled, said plainly: `OnTrapHit` "is sent every so often as long as a trap object is in
contact" with no stated cadence, so it is sent once, with the start, as the initial hit; the
material number ("opaque … just to be passed to other native functions") is 0. `Game.ShakeCamera`
(strength 0..1 default 0.5, a duration of 0 meaning `fCameraShakeTime`, 1.25 s in SkyrimSE.exe's
table) and `Game.ShakeController` (both motors 0..1) place the player's `CameraShakeCommand` /
`ControllerRumbleCommand` markers for the camera and input subsystems to drain; the source's distance
is not modelled. `Actor.HasSpell` reads the actor's spell list. With these `TrapHitBase` (803 Skyrim
references) and `PhysicsTrapHit` (466) pass the VM. `Sound.Play` is now also accepted for its value
(the instance id, 1 once queued) which `StockadeBarricade01ActivatorScript`, the wall torch sconces
and others use; `StopInstance` and `SetInstanceVolume` stay unsupported, so a script that steers the
instance is still declined. Skyrim VM coverage 64.4% to 66.3%. The trap-contact system has been run
against a real physics world in tests (overlap starts, a standing contact is not new, a dead actor
ends it). Live in Bleak Falls Barrow, putting the player on each of the three swinging blades
(`TrapBladeSwinging01`, refs `0003AC37`, `0003AC35`, `0003AC33`) delivers `TrapHitBase.OnTrapHitStart`
and `OnTrapHit` to that reference; the blade's trap stays in `CannotHit` until it springs, so no damage
lands. A trap body is owned by the nearest scripted entity up its parents, or — for the bare
collision entity an animated prop's hierarchical import leaves — the REFR its `PhysicsSourceForm`
names. A bhk body that is an animated node's own (it stands at the node's pose) is spawned beneath
that node's entity instead of detached, and a body authored `Static` is made `Keyframed`: Havok
authors the bodies of graph-driven props as fixed and lets the animation move them, and the physics
sync follows a keyframed body's `GlobalTransform`, so a swinging blade's body swings with it (live,
its AABB in `phys.census` goes from `[2, 91, 122]` to `[2, 110, 106]` and `[2, 120, 94]` through a
swing) and an opened portcullis stops blocking.

### Calls on None (M47.2)

CK *Papyrus Runtime Errors*: "Cannot call X() on a None object, aborting function call ... since
you cannot call a non-global function on nothing, the call is ignored and the error printed out".
A native called for effect on a `None` object is therefore skipped and the script goes on (the VM
logs it at debug); a call whose value is used has nothing to give and still stops the run.
`Weapon.Fire` with no source reference is ignored the same way (`DartTrap` asks for the Nth link of
a chain and can run past its end).

### CreateDetectionEvent (M47.2)

CK *CreateDetectionEvent - ObjectReference*: "creates a detection event at this object's location"
for an `akOwner` at an `aiSoundLevel` (0 to 100, default 0). The engine has no hearing: an actor
notices a target by sight alone (`faction_hostility`'s detection range and line-of-sight test) and
nothing maps a sound level to a distance, so the call is recorded in order in a capped
`PendingDetectionEvents` for the hearing system that will read it, and nothing reacts to it yet. A
tripwire built on it (`SoundTrap`, 166 Skyrim references, which now passes the VM) plays its sound,
animation and physics nudge and wakes no one. Skyrim VM coverage 63.5% to 64.4%.

### Utility.GetCurrentRealTime (M47.2)

CK: "the number of real-world seconds that have elapsed since the game has launched", with time in
a menu counted. `TotalTime` is the engine's wall-clock since start, so that is the value
(`DartTrap` reads it).

Live in a Starfield Cydonia boot, through `script.exec` (one Papyrus statement through the VM) and `scan.info`:
`Game.GetPlayer().AddItem(Game.GetCredits() as Form, 123)` took the player's inventory from 1 stack / 1 item to
2 stacks / 124 items (the credits stack of 123), and `Game.GetPlayer().AddToMapScanned(true)` left reference
`00000014` scanned, map-visible and a fast-travel target (it was none of the three before).

### Game.GetCredits (M47.2)

Starfield's `Game.GetCredits()` has no CK page, but the corpus fixes its meaning: it is the
currency *form*, passed as the item of `Player.RemoveItem(Game.GetCredits() as Form, cost, …)`
(`DialogueShipServicesScript.RefuelPlayerShip` / `RepairPlayerShip`), the Starfield twin of
`Game.GetCaps()`. The form is `Credits`, `0x0F` of `Starfield.esm` (checked against the
record). Outside Starfield the run stops.

### GetCurrentPackage (M47.2)

CK *GetCurrentPackage - Actor*: the package the actor runs now. The AI already reports
every package change to the package fragments (`report_package_transition`: the spawn
winner, a re-selection, a combat suspension), and that report now also keeps
`ActivePackages` (entity → package, unsaved: the AI reports again when an actor spawns after
a load, and a report of "no package" keeps the row absent — ids are never recycled and an unload retires its rows, #6137). The VM reads
it as a value; an actor running none reads None.

### Perk fragments (M47.2)

A Fallout 4 / Starfield perk's `VMAD` binds `Fragment_Entry_NN(akTargetRef, akActor)` to its
entries, in the same layout as a terminal's (`parse_indexed_fragments`; `PerkRecord.script_fragments`).
The index is the entry's `EPFB` "Perk Entry ID" (`perk_fragment_census` matches all 34 of Fallout 4's
fragments on 31 perks to an entry; 24 hang off the Activate entry point, with a button label and
the Run Immediately / Replace Default flags, and 10 off value entry points). CK *Perk Fragments*:
Activate entries run "every frame conditions are met" when Run Immediately and otherwise "when the
player picks the appropriate option in the popup menu"; value entries run "whenever conditions are
met" with `akTargetRef` empty. The engine has neither the activation popup nor a per-frame entry
evaluation that can run a fragment, so the host reports an entry run (`report_perk_entry`) and
`perk_fragment_system` runs the fragment; the `perk.run <perk> <entry id> [target]` console command
is the only source until those exist. Skyrim's PERK fragment layout differs (a file name, no script
entry) and is not decoded.

### FormList.AddForm / RemoveAddedForm / Revert (M47.2)

CK *FormList Script*: `AddForm` "adds the given form to the form list", `RemoveAddedForm`
removes a form ("only forms added to the list via calls to AddForm can be removed") and `Revert`
"removes all script added forms from the list". The records' own lists stay derived; the forms
scripts added live in the saved `FormListAdditions` (list FormID to added forms, in order), and
`forms_of` is the one read every `GetSize` / `GetAt` / `HasForm` / `Find` uses: the record's forms,
then the added ones, with the edits of the run still in flight applied on top (applied to the
resource when the run's guards drop, so a run sees its own `AddForm`). The CK does not say whether
adding a present form adds it again or whether removal takes one occurrence or all, so an add
always appends and a removal takes every added occurrence. Leveled lists' `AddForm(form, level,
count)` is a different call and stays declined.

### Terminal fragments (M47.2)

A Fallout 4 / Starfield terminal's `VMAD` binds `Fragment_Terminal_NN(akTerminalRef)` to its
menu items. The fragment section is xEdit's `wbVMADFragmentedPERK` ("same fragments format as in
PERK"): an s8 version, the fragment script's own entry, a u16 count, then per fragment a u32
index, an unknown byte, the script name and the function name (`parse_indexed_fragments`; the
`terminal_fragment_census` example checks that all 463 of Fallout 4's bindings, on 205
terminals, carry an index equal to one of the terminal's menu item ids `ITID` and to the function
suffix). `TermRecord.script_fragments` holds them; the populate walk installs those the VM's check
accepts into `TerminalFragments`, sharing the package fragments' `FragmentTable` and the bin
crate's single `populate_record_fragments`.

The engine has no terminal screen, so the selection source is `report_terminal_selection(terminal
reference, base, item id)`; the `terminal.select <reference formid> <item id>` console command is
its only caller until a screen exists. `terminal_fragment_system` (Update) then runs the item's
fragment with `akTerminalRef` bound to the reference. Starfield's own terminal records carry a
different menu layout the plugin does not decode, so only the fragment function suffix ties a
Starfield item to its fragment there; its three base-game bindings are not menu-checked.

### GetFactionReaction (M47.2)

CK *GetFactionReaction - Actor*: "the faction-based reaction between this actor and another",
0 Neutral, 1 Enemy, 2 Ally, 3 Friend. It shares one rule with ambient hostility,
`combat::combined_reaction` (moved out of `faction_hostility.rs` so the two cannot drift): over
every directed pair of the two actors' factions (rank below zero is not membership) a script's
`SetEnemy` / `SetAlly` override wins over the authored `XNAM` relation, then a Friend pair beats an
Ally pair, either beats an Enemy pair, and anything else is Neutral. The original engine's
combination of conflicting memberships is undocumented; this is engine policy. The authored
relations reach the VM through `FactionCatalog`, published beside the resolver by
`install_form_resolver`. An actor that is not loaded stops the run.

### SetLocRefType (M47.2)

CK *SetLocRefType - ObjectReference*: "Sets an in-game created object to be the given loc ref
type for the given location"; a `None` ref type clears one already set. `Effect::SetLocRefType`
records the assignment in the saved `LocRefTypeOverrides` (reference FormID to ref types). The
two consumers read it beside the placement's authored `XLRT` tags: the `HasLocRefType` guard
(`has_loc_ref_type`) and the quest-alias fill by location ref type, whose candidate list gets
the assigned types at every refresh (a change marks the bindings dirty). The CK scopes the type
to a location; both consumers ask about the reference's own types, so the location is not part of
the key, and a `None` clears every type scripts assigned to that reference.

### MusicType.Add / Remove (M47.2)

CK *Add - MusicType*: "Adds this music type to the music stack, causing it to start
playing if it's the highest priority item"; *Remove* takes it off. The engine has no
music player (the music files are xWMA, which it cannot decode), so the stack has no
state anyone could observe and `<MusicType>.Add()` / `.Remove()` run as no-ops. They exist
so the rest of the fragment (the `SetStage` beside a music cue, 66 FO4 fragments) runs.
The receiver must be a property or variable of the running script declared a `MusicType`
(checker and runtime share `declares_music_type`), so a script's own `Add()` function is
not swallowed.

### Script-created references (M47.2)

`ObjectReference.PlaceAtMe(akFormToPlace, aiCount, abForcePersist,
abInitiallyDisabled[, abDeleteWhenAble ...])` ("Makes X new references of the
passed in form ... places them at our reference's location, returning the last
one it created"; `abInitiallyDisabled` "is only applied to the last reference
created") runs in the VM as a value read. Each reference takes a FormID from
`ScriptedIds`, the upper half (`0xFF80_0000`) of the created-object slot, drawn
through a shared atomic handle the run snapshots before the quest-state guards,
so the VM never locks `CreatedReferences`. The records land on
`CreatedReferences.scripted` after the guards drop, and the existing created-
reference path places them (`place_created_references`, at the receiver's live
position when it is loaded, else its authored one) and spawns them whenever
their cell is loaded. A disabled one is disabled in `ReferenceEnableState`;
`abDeleteWhenAble` marks `DeleteWhenAble`.

Fallout 4 and Starfield pass the offsets array, the alias to fill and the
navmesh snap too; the checker lets those through only unset (`None`), so a
`PlaceAtMe` with an offset or an `akAliasToFill` stays declined. `PlaceActorAtMe`
(leveled actors) is not wired. `abForcePersist` has no effect: created
references already outlive their cell (they respawn with it).

### Scripted links (M47.2)

`ObjectReference.SetLinkedRef(apRef, apKeyword = None)` ("Sets the reference
linked to this one under the specified keyword", CK) writes the saved
`LinkedRefOverrides`, keyed by (source FormID, keyword FormID or 0). A script's
link overrides the authored `XLKR` link under the same keyword and a `None`
target clears it. `GetLinkedRef` and the lowered `ObjectRef::Linked` both read
through `crate::linked_ref`, overrides first, then the loaded source's
`SceneAliasCandidate.linked_refs`. The write is queued on
`DeferredFragmentEffects` and applied after the quest guards drop.

Starfield's declaration adds `abPromoteParentRefr` (Starfield `ObjectReference.pex`):
the engine promotes a non-persistent parent reference so the link survives.
Keying the saved link on FormIDs already survives the cell unloading, so the
literal flag is accepted and changes nothing.

### Explicit-start pin (M47.2)

A launch with no named location is a new game, and the Start Game Enabled
quests run as authored. MQ101's start-up fragment (stage 0) moves the player to
the Helgen cart and the Helgen NPCs to their markers. Once `TryToMoveTo`
lowered, that fragment ran whole, and it bounced every `--cell` launch to
Helgen, so Riverwood's NPCs were never found. A launch that names its starting
location (`--cell` / `--grid`) now holds the player there with
`ExplicitStartPin`: `step_player_move_request` drops scripted player moves
until the player first presses a key or mouse button, after which scripted
moves apply as in any play session. A benchmark or a debug-server session never
presses a window key, so its pin stays. Only the player's relocation is held;
the rest of the start-up fragment runs.

### Misc stats (M47.2)

`Game.IncrementStat(asStatName, aiModAmount = 1)` adds to the saved
`MiscStats` (`crates/scripting/src/misc_stats.rs`), and `Game.QueryStat` reads
it. Both lower with literal arguments. The CK's *IncrementStat - Game* names
four stats only scripts maintain: Standing Stones Found, Horses Owned, Houses
Owned and Stores Invested In. Every increment is recorded, but a read resolves
only those four. The engine-tracked stats *QueryStat* lists (Days Passed,
Locations Discovered, …) aren't counted here, so reading one declines rather
than answering 0.

### Messages and text replacement (M47.2)

`MESG` records parse their "Message Box" flag (DNAM bit 0) and their `ITXT`
buttons. Skyrim.esm has 571 messages, and 384 of them are boxes. The
`MessageCatalog` (`crates/scripting/src/messages.rs`) holds them along with the
display names and global editor IDs that text replacement reads.

Fragments lower three calls:
- `Message.Show(n1…n9)`, with literal arguments.
- `Message.ShowAsHelpMessage`, shown as a plain message.

- `Debug.Notification("text")`.
- A help-message helper that forwards its own parameters (Starfield's shape:
  `Help(asEvent, afDuration, asContext)` calling `ShowAsHelpMessage`) passes the check
  with the run-time context treated as the unrestricted `""`; if a menu name arrives at
  run time the lowering declines it and the run stops (the context only restricts the
  message to a menu, which this engine does not model).

*Show - Message* substitutes "the passed-in numbers into the appropriate
spots", meaning the `printf` conversions `%[.N]f`, `%d`/`%i` and `%%`.
*Text Replacement* fills these tags from the owning quest:
- `<Alias=X>` and `<Alias.ShortName=X>`. They read `[...]` when the alias is
  unfilled, and `<Alias=Player>` resolves without an alias.
- `<Global=X>`. Float globals show two decimals.

Any other tag stays exactly as authored, which the CK documents as the
fallback.

The CK *Message* page splits presentation two ways. A message without the box
flag is a notification, so it joins the frame's player feedback. A box is "a
dialog box that pauses the game and waits for a response". It goes to
`MessageBoxQueue`, waits for any open page to close, and then opens the native
`GameMenuPage::MessageBox` page with one button per `ITXT`, or a single "Ok"
when there are none. Answering a box pops it, and the page stays open until
the last box is answered. Escape doesn't dismiss a box: the page reopens the
next frame.

ObScript (M47.3) uses the same pipeline. The VM decodes `Message` (`0x1059`)
and `MessageBox` (`0x1000`) payloads using the layout in UESP *Oblivion
Mod:Mod File Format/SCPT* §SCDA, checked against vanilla `Oblivion.esm`. The
payload is the text as a `[u16 length]` string, the variables in the set/if
variable format, and the buttons. `Message` ends with display seconds instead
of buttons.

The quest host substitutes the variables into the text and queues the
result. A box with no buttons gets the CS wiki's default "Done". The answer
reaches the quest's `GetButtonPressed`, which returns it the first time it is
called after the press and -1 otherwise.

Not modelled:
- `Show`'s return value (the chosen button), since fragments that branch on it
  still decline.
- Run-time `Show` arguments.
- The remaining replacement tags: `<BaseName>`, `<Token…>`, `<Alias.PronounObj…>`
  and similar.

### Ghosts (M47.2)

`Actor.SetGhost([abIsGhost])` sets `ActorControlState.ghost`, which is saved
and parked with the actor's other control flags. Condition 237 `GetIsGhost`
reads the flag. Per the CS / GECK *SetGhost* pages, a ghost:
- "will not enter combat": the ambient faction-hostility pass and
  `StartCombat` both skip it, and setting the flag ends its current fight;
- "cannot be harmed by a weapon or spell in any way": a weapon hit lands for
  0;
- "can inflict damage" and is still hurt by water, so drowning isn't exempt.

Not modelled: spell immunity and "not affected by any new magic effects",
because hostile magic doesn't reach actors yet.

### Reference keywords (M47.2)

`crates/scripting/src/keywords.rs` answers condition 560 `HasKeyword` and the
Papyrus `<reference>.HasKeyword(kw)` guard. It also applies `AddKeyword`,
`RemoveKeyword`, and FO4's `ResetKeyword`.

A reference's keywords come from three sources:
- **Script changes.** These are keyed by the reference's FormID, saved in
  `ScriptedKeywords`, and win over everything else. The FO4 CK *AddKeyword*
  page says an added keyword "'overrides' the base object and alias", and
  `ResetKeyword` undoes it.
- **Alias keywords.** These come from every alias the reference fills
  (`QuestAliasInjectedOverlays`).
- **Base object keywords.** These are the base object's `KWDA`, now parsed
  for every `CommonNamedFields` record. For an actor they include its race's
  keywords: Skyrim.esm authors `ActorTypeDaedra` on 4 races and only 1
  `NPC_`, and the CK says Banish affects "Actors that have the
  'ActorTypeDaedra' Keyword".

The `KeywordTable` covers actors and inventory items. Other base types' keywords
aren't collected yet.

### Barter (M47.2)

`Actor.ShowBarterMenu()` takes no arguments in Skyrim, Fallout 4 and
Starfield. It opens the native barter page with the actor's vendor faction.
The engine side is `byroredux/src/barter.rs`, and the scripting side only
raises `PendingBarterMenu`.

**Vendor data.** A `FACT` record names three things (xEdit TES5/FO4):
- `VENC`, the "Merchant Container" `REFR`;
- `VEND`, the "Vendor Buy/Sell List" `FLST`;
- `VENV`, the vendor values, including the "Not/Sell Buy" flag, which turns
  the list into what the vendor does *not* trade.

Every vanilla `VENC` resolves to a placed `CONT`: 145 of 145 in Skyrim.esm and
81 of 81 in Fallout4.esm. The vendor sells whatever its container holds. It
buys a player item when the item's FormID, or one of its new `KWDA` keywords,
is in the list.

**Prices.** UESP *Skyrim:Speech* §Prices gives:

```text
price factor = fBarterMax - (fBarterMax - fBarterMin) * min(Speech, 100) / 100
buy price    = round(value * buy modifier * price factor)
sell price   = round(value * sell modifier / price factor)
```

The buy and sell modifiers are what the player's perks compose from 1.0 at the
*Mod Buy Prices* (8) and *Mod Sell Prices* (60) entry points. In vanilla, the
`Haggling*` perks apply to the player, and `Allure` and the vendor perks
condition tab 1 on the vendor.

Fallout 4 has the same shape over Charisma / 10, uncapped. The Fallout wiki
gives "3.50 − Charisma × 0.15", which is Fallout4.esm's fBarterMax 3.5 and
fBarterMin 2.0. Prices are clamped: a purchase costs at least `fBarterBuyMax`
(1.2) × value and a sale pays at most `fBarterSellMax` (0.8) × value.
Skyrim's executable carries the matching pair `fBarterBuyMin` 1.05 and
`fBarterSellMax` 0.95.

**Stock.** A merchant chest normally sits in an unloaded holding cell. For the
length of the session, an entity stands in for the chest under the chest's own
FormID. It is seeded from the `CONT` and the chest's parked
`PersistentReferenceStates` row, the same way a cell load seeds a container.
Closing the page parks the stock again, so the next barter, or the chest's own
cell load, sees what was traded. Each trade moves one unit and its gold
through the fragment runtime's `remove_items`, so equipment indices and
`OnItemAdded`/`OnItemRemoved` behave as they do for any other transfer.

The page pauses the game, like other menus. Opening it ends an open
conversation, as the Inventory key does.

Not modelled:
- vendor hours and radius;
- stolen-goods rules;
- restocking;
- Fallout 4 perk prices, because its `PERK` entry layout isn't decoded;
- `GetDialogueTarget().ShowBarterMenu()` (98 fragments), where the speaker's
  side isn't known.

### Script registry (M47.0)

`ScriptRegistry` is a resource mapping SCPT/Papyrus `editor_id` strings to
`ScriptSpawnFn = fn(&mut World, EntityId)` spawners that install ECS state
components on a target entity. The cell loader's per-REFR walk drives it: when a
REFR's base record carries a `SCRI` cross-ref → SCPT → `editor_id`, the loader
looks the editor_id up here and runs the spawner. Keying on **editor_id** (not
FormID) is deliberate — editor IDs are stable across plugin loads, so two SCPT
records sharing an editor_id (e.g. `defaultRumbleOnActivate`) resolve to the
same spawner without per-plugin registration. Re-registering an editor_id
overrides the prior spawner (the intended mod-override path).

Today exactly **one** hand-written spawner is registered: `defaultRumbleOnActivate`
(`register_spawners` in [`papyrus_demo/mod.rs`](../../crates/scripting/src/papyrus_demo/mod.rs)).
The other three R5 demos carry per-instance properties that live in Skyrim+
`VMAD` subrecords the parser doesn't decode yet, so spawning them would attach
inert components — they defer to M47.2's VMAD decode. Unregistered scripts are
the common case (vanilla FO3 ships ~1257 SCPT records; M47.0 hand-translates a
handful). Before that fallback, the loader now conservatively translates
pre-Skyrim handlers containing engine-native load-order assignments and nested
`if`/`elseif`/`else` branches. It prefers preserved `SCTX`, then applies the same
boundary to source-less `SCDA`: exact statement framing, local indices resolved
through `SLSD` metadata, and one complete supported command expression per
assignment or condition. `GameMode`, `OnLoad`, and `OnActivate` execute in the
ECS schedule against the live immutable content catalog; numeric results land
in save-backed `ScriptVariables`. Nesting and `elseif` chain length are capped at 32. Any
unsupported statement, expression tail, event filter, or malformed branch
rejects the whole handler. A registry miss is therefore silent only when
neither path can attach behavior.

**The FO3/FNV quest-script census (2026-10-11).** Quest SCPTs execute on
every pre-Papyrus game, not just Oblivion: `install_quest_scripts`
(`asset_provider/script.rs`) resolves each quest's `script_ref` into the
obscript runtime with no game gate, and the 5 s GameMode tick
(`obscript_quest_tick_system`, registered in `boot/schedule/update/`) runs
whatever it installed. Command ids are shared across the three games (measured
per id and per game in `obscript_quests.rs`'s evidence table, #6015 — the one
exception is `0x1000` `MessageBox`, which no Fallout script compiles to and the
host therefore declines there), so the interpreter needs no per-game table. The 2026-10-11 agent census measured the
live installed base: **322 FNV** and **101 FO3** quest scripts install and
tick ungated, with the wrong-target reference-slot defects of that audit's
SCR-D6-01/02 affecting 3–6 % of vanilla quest scripts — tracked separately
from this execution fact.

**M47.3 phase 2 (2026-10-18) — measured, not guessed.** Command ids were
named by aligning each script's `SCTX` source with its compiled statements
(`obscript_tools names`; 321 of 322 FNV quest scripts align). That showed the
rule the getters follow: an ObScript getter's id is `0x1000` plus its CTDA
function index (`GetDead` 46 = `0x102e`, `GetInCell` 67 = `0x1043`,
`GetFactionRank` 73 = `0x1049`, `GetQuestCompleted` 546 = `0x1222`), the same
in Oblivion, FO3 and FNV. So `ObScriptCommandHost` answers an allow-listed set of
getters through `condition::evaluate_script_getter` rather than carrying a
second copy of each; a reference that is not loaded, a null form argument, or a
function the evaluator cannot read in this game (`GetActorValue` in a *CTDA*, whose
authored argument is an enum code, #5989 — the ObScript `GetAV` takes the same code through
[`obscript_actor_values`](../../crates/scripting/src/obscript_actor_values.rs), below) is a counted decline, never an
invented `0`. The allow-list is deliberate: a catalogued CTDA index is not
proof that the same number is a getter in a legacy game.

Two defects fell out of the same census. The `z` argument tag is a 64-bit
double (UESP *Mod File Format/SCPT* `[FloatFuncParam]`), not a NUL-terminated
string, so any script with a float literal argument (`SetQuestDelay Q 0.1`)
misframed or came back `Malformed`; and the quest tick ignored the `QUST`
record's authored script interval (FO3/FNV `DATA` bytes 4..8: most scripted FNV
quests author 0, but 81 author a delay — 21 of them 0.1 s, 11 of them 0.5 s). The cadence is
now `quest_script_interval(authored, SetQuestDelay override)` with `0` meaning
the INI default (GECK *SetQuestDelay*), and the override is saved state.

The FO3/FNV-only quest commands (`SetObjectiveDisplayed` `0x11a3`,
`SetObjectiveCompleted` `0x11a2`, `GetObjectiveDisplayed` `0x11a5`,
`GetObjectiveCompleted` `0x11a4`, `SetQuestDelay` `0x11dc`, `GetQuestCompleted`
`0x1222`) are gated on the loaded game, follow the GECK pages (displaying an
objective starts a stopped quest; an index the quest does not author is
ignored), and write the same `QuestObjectiveState` the Papyrus fragment path
does. What was still declined was mostly *actions* — about two thirds of the
call sites left — which need a reference-bound effect host rather than more
getters.

**Actions (`obscript_actions.rs`).** `Enable` `0x1021`, `Disable` `0x1022`,
`GetDisabled` `0x1023`, `MoveTo` `0x109e` and `EVP` `0x105e` (the same ids in
Oblivion, FO3 and FNV) act on the *explicit calling reference* the VM resolved,
and lower onto the sink the Papyrus effect of the same name already uses:
`ReferenceEnableState::set_enabled` (the FormID-keyed ledger, so it works on a
reference in an unloaded cell and `reference_enabled` reads it back over the
authored "Initially Disabled" flag), `fragment::apply_move_to_effect` (the
Papyrus `MoveTo` body, extracted so the two share it) and
`package::request_package_evaluation`. Semantics are the GECK/CS pages': the
fade argument is cosmetic; `MoveTo` places the caller at the target plus the
Z-up offset and leaves its rotation alone (neither wiki says the facing
follows); the player's `MoveTo` is queued through `PlayerMoveRequest` on
FO3/FNV only — on Oblivion it also ends the script, which the host cannot do,
and the request carries no offset, so both are declined instead of
approximated. The documented "Enable/Disable has no effect on a reference with
an Enable Parent" rule is not applied (the Papyrus path does not either; the
runtime has no enable-parent ledger). A reference that is not resolved, or a
missing ledger, is a counted decline. `EnablePlayerControls` /
`DisablePlayerControls` were left out on purpose: the GECK flags
(Movement/Pipboy/Fighting/POV/Looking/Rollover/Sneaking) do not map one-to-one
onto `PlayerControlSelection`'s domains, and the mapping is not documented.

**Script packages (`script_packages.rs`).** `AddScriptPackage` `0x1097` /
`RemoveScriptPackage` `0x1098` fill an actor's one `ScriptPackage` slot (saved,
`FORMAT_MAJOR` 110): the GECK says the package "takes precedence over all other
packages until it is done", that a second add replaces the first, and that
"unless the added package has some constraint on it ('must complete' and/or
'must reach location'), it will be removed the next time the actor
reevaluates". `ambient_ai_package_system` reads the slot ahead of the actor's
own stack and skips the schedule and condition scan for it. The re-evaluation
cadence is the one place the docs only hedge ("Packages seem to be re-evaluated
every 10 seconds", *How Are Packages Evaluated*), and the ambient system's
once-per-game-minute pass is not that cadence, so an unconstrained package
lapses once it has been *running* for 10 s on its own clock. A package not yet
installed is never dropped, so the common `AddScriptPackage X` then `EVP` idiom
starts it; `EVP` only ends one that is already running (the GECK says it drops
even a must-complete package). A constrained package (PKDT `0x2` / `0x4`) stays
until removed, replaced, evaluated away, or — Travel only — the actor arrives
(`Traveled`). Completion of the other procedures (Follow, Escort, Dialogue, ...)
is not modelled, so those stay until one of the other exits.

**`PlaySound` and object scripts.** `PlaySound` `0x1026` needs no calling
reference ("plays the specified sound, non-locational"); it queues the sound on
the `Sound.Play` ingress from the player, the listener, which is where a
non-locational sound is heard. The *SystemSound* flag is moot, as only `GameMode`
blocks run.

`obscript_objects.rs` runs the `SCRI` scripts on placed references on the same
host (`ObScriptCommandHost`, now generic over its owner: a quest, or a reference
whose entity it holds, so a command written with no calling reference acts on the
script's own reference and a quest command with no quest declines). The block
numbering was read off the vanilla masters by aligning each script's `Begin`
lines with its compiled blocks (`obscript_tools census`): `GameMode` `0`,
`MenuMode` `1`, `OnActivate` `2`, `OnAdd` `3`, `OnHit` `8`, `OnDeath` `0xa`,
`OnLoad` `0x15`, `OnTrigger` `0x18`, `OnStartCombat` `0x19`, `OnReset` `0x1e`;
`0x1a` is `OnTriggerEnter` on FO3/FNV and `OnTriggerActor` on Oblivion, so the
numbering is not one table across the three. Four blocks run: `GameMode` every
frame for every loaded scripted reference, `OnLoad` on the cell-load marker,
`OnDeath` on the death marker (read, not drained: the Papyrus reference system
owns it) and `OnActivate` (below). Locals live in the reference's saved `ScriptVariables` by name; a save carries
them for resident references, and an evicted reference's set is parked in the
cell loader's per-reference row and restored over the fresh one at attach (#6071). All object scripts of
Oblivion, Fallout 3 and New Vegas walk cleanly and those four blocks run to
`End` under both gate answers (`legacy_object_scripts_walk_and_run_without_
malformed_bytecode`, ignored: needs the masters). `BYRO_NO_OBSCRIPT_OBJECTS`
leaves the table uninstalled.

**`OnActivate`.** The GECK *Activate* page: a reference's `OnActivate` block runs
"instead of the default activation", `Activate` (flag `0` or unwritten) performs
the default one "bypassing any OnActivate Block", and `Activate <actor> 1` runs
the block immediately. The engine already had the gate for this — every native
consumer (doors, containers, pickups, dialogue, the carried-key spend) asks
`default_activation_blocked`, Papyrus's `BlockActivation` — so a reference whose
script has an `OnActivate` block now counts as blocked, the block runs with
`GetActionRef` / `IsActionRef` (`0x10cd` / `0x1069`) answering for the activator
(and null/false anywhere else), and `Activate` queues the native-only activation
(`Activate(…, true)`'s path, delivered next frame with `NativeActivation`, which
the object system does not run the block for again). `Activate <actor> 1` nests
at most four deep, as the GECK runs the target "immediately" and a cycle would
not return.

One deliberate deviation from the original: if an `OnActivate` run **declined a
command** (one the host does not model) or came back malformed, it is unknown
whether the script meant to call `Activate`, and withholding would leave a door
nothing can open, so the default activation goes ahead — what the reference did
before scripts ran. Only a block the host ran in full can withhold it. On the
masters (every gate answered true) 239 of FNV's 524 `OnActivate` blocks, 139 of
FO3's 373 and 189 of Oblivion's 888 reach an `Activate`; the rest are objects that
replace default processing outright, which is the case the withholding is for.
The GECK's bug note (flag `0` on an unscripted object stops it being activated
normally ever again) is not modelled.

**Trigger blocks.** `OnTriggerEnter` (`0x1a`; `OnTriggerActor` in Oblivion, "run
once when an actor triggers"), `OnTriggerLeave` (`0x1b`, Fallout 3 / New Vegas
only: it is `OnTriggerMob` in Oblivion, so it is only read as a leave there) and
`OnTrigger` (`0x18`) run on the volume reference. Enter and leave run once per
reference crossing, with `GetActionRef` that reference (the GECK sets the action
ref on the trigger for both); `OnTrigger` "will be run every frame when something
triggers the scripted object" and its action ref is NOT set, so it runs each
frame while `trigger::trigger_occupants` is non-empty. The leave half is new:
`trigger_detection_system` raises `OnTriggerLeaveEvent` (Pattern A, beside the
enter marker) when the player or a tracked actor goes from inside to outside, and
a despawned actor is not a leave. Only the player and the tracked quest/scene
actors are detected today, so a trigger some other NPC would set off stays quiet,
and a visible scripted activator with a primitive still takes the ordinary spawn
path and has no volume.

A block can be **written for one reference** (`Begin OnTrigger player`), and a
script can have several blocks of one type, each for a different one (FO3's
`Begin OnTriggerEnter player` / `... StickyRef` / `... RedRef`). The `Begin`
payload after the block type and length is the same `[u16 count][args]` blob a
command's arguments use (`01 00 72 01 00`: one argument, reference variable 1), so
blocks are addressed individually (`ObScriptProgram::blocks`, `BlockInfo`) and a
parameterised one runs only for its reference. Checked against the masters: all
187 Oblivion, 349 FO3 and 922 FNV `Begin OnTrigger*` lines agree with the bytes on
whether and where a parameter is written.

### M47.3 phase 3 — the rest of the VM (2026-10-18)

Phase 2 put quests, a handful of actions and the object-script blocks on the one host. Phase 3
closes what the roadmap row listed as open, and in doing so found three grammar gaps that had
made whole classes of vanilla expressions wrong. `obscript_tools names <ESM> all` now marks each
command the host models and prints the share of call-site lines it covers: **96 % of New Vegas's
25 532, 93 % of Fallout 3's 11 892 and 94 % of Oblivion's 21 431** (after the slices below). What is left
is a long tail of single commands and the few subsystems listed at the end.

**Expression grammar (`obscript_tools exprcheck`).** Measured over the three masters (24 275 / 13 021
/ 24 564 expressions), the VM understood `' X'` implicit calls, locals, globals and ASCII, and nothing
else of the binary escapes — which silently mis-evaluated three shapes, every one of them common:

- **A call on a reference** inside an expression is `' r<ref>'` immediately followed by `'X' <cmd u16>
  <len u16> <args>` — no space before the `X`; an implicit-caller call is `' X'`. It was read as the
  reference alone and the rest skipped as text: 3 342 of FNV's expressions (14 %), so `if Actor.GetDead`
  and `if player.GetItemCount x > 3` compared a *form id*. They now reach the host with that caller; a
  caller that is a reference variable nobody assigned is declined, never run on the script's owner.
- **A remote variable** (`Quest.var`, `SomeRef.var`) is `' r<ref>'` followed by `'s'|'f'` and the
  variable's **ordinal** in that reference's script (`20 72 04 00 73 06 00` is `VCG01.bBeganTest`); the
  statement form `set Quest.var to x` is the target `r<ref> <s|f><ordinal>`. Reads were the form id
  (4 170 in FNV); writes were declined (#5630). The host now reads and writes them: a quest's variables
  are its saved locals by slot, a loaded reference's are the saved variables of its object script, found
  through the name of its local at that ordinal. The script that is running is not read this way — its
  variables are mid-run in the VM.
- **`' Z<ref>'`** is a reference literal as a value (`Set rActor to SomeREF`, `GetActionRef ==
  SomeREF`; 1 355 in FNV). Two references compare by form id, exactly: as `f32`s the DLC forms
  (`0x01000004` and `0x01000005` are one float) would compare equal.

The vanilla Oblivion check of `MS23Script` had been passing on the first of these: its first gate is
`( GetStage MS23 == 40 ) && ( Player.GetDistance MS23RolandsHouseMarker <= 300 )`, which the old
evaluator turned true under an all-zero world. It now stays shut at stage 0 and opens, once, for stage 40 with
the player 100 units away (`real_data_tests`). The phase-1 note that MS23 "self-advances to stage 90"
live predates this fix and is withdrawn: its gates are distance-gated and want the player near the markers.

**Reference variables.** A local the script uses as a `ref` is one whose slot has an `SCRV` entry
(`ScriptRef::Variable`). `set linkedRef to getLinkedRef` stores the reference in `VmState::ref_locals`
exactly (an `f32` local rounds a form id past `2^24`, which is every DLC form), the nearest `f32` stays in
`locals`, and `linkedRef.enable` then acts on it. The hosts keep the exact id without changing a saved
shape: the reference's `ScriptVariables` hold it as `<name>#lo` / `<name>#hi` (two 16-bit halves, each exact as
an `f32`), the quest locals as slots `0x4000 + n` / `0x8000 + n`. `OnDeathEnableLinkedRef` runs byte for byte in
`obscript_objects` tests.

**The effect bridge (`obscript_effects.rs`).** Most of what a legacy script *does* the Papyrus effect runtime
already does, with the engine's sinks behind it. An ObScript command is lowered to the `Effect` its Papyrus
namesake builds and applied through `apply_fragment_guard_free`, the resolved references bound as the
properties of a throwaway VMAD (the device the Papyrus VM uses for a native call), so nothing is
implemented twice: `AddItem` / `RemoveItem` (a form list stands for each member) / `EquipItem`, `KillActor`,
`SetEnemy` / `SetAlly`, `StartCombat` / `StopCombat` (with a target, only that fight) /
`StopCombatAlarmOnActor`, `SetUnconscious` / `SetGhost`, `Lock` (the level argument as the GECK and the CS
define it) / `Unlock` / `SetOpenState`, `AddToFaction` / `RemoveFromFaction` / `SetFactionRank` (applies to the
reference; the GECK also writes the base, so actors spawned from it later are not updated), `AddSpell` /
`RemoveSpell`, `AddPerk` / `RemovePerk`, `AddAchievement`, `SetPlayerTeammate`, `SetEssential <base> <flag>`,
`RewardXP`, `ShowMessage <message> [values]` (Fallout's form-based message shares `0x1059` with Oblivion's
text `Message`, and was declined whole until the VM read it as the form and its values),
`Enable` / `DisablePlayerControls` (the GECK's flag definitions give the domain mapping: Movement also takes
world activation, PipBoy the pip-boy and wait menus, POV camera switching; RolloverText has no domain here)
and the actions that already had sinks of their own: `MarkForDelete` (only on a *disabled* reference, as the GECK
says; a created reference is deleted when its cell goes, an authored one loses its script), `SetDestroyed`,
`SetRestrained`, `RemoveAllItems`, `PlaceAtMe` (answers the created reference; an offset is declined, not
dropped), `ResetAI`, `SetOwnership` / `ClearOwnership`, `UnequipItem`, `PlaySound3D`, `AddReputation` (the
points its 1-5 amount names), `StartConversation player <topic>` (the force-greet the dialogue runtime
already has; NPC-to-NPC has no runtime), `MoveToMarkerWithFade`.

**Actor-value commands.** `GetAV`, `GetBaseAV`, `SetAV`, `ModAV`, `ForceAV`, `DamageAV`, `RestoreAV`,
`ResetHealth` and `RewardKarma` name the value by the GECK's *Actor Value Codes* integer (0 `Aggression` .. 76
`DamageThreshold`). `obscript_actor_values` translates a code to the AVIF whose editor ID names it; the table
was checked against the vanilla masters rather than taken from the page — `obscript_tools avcodes <ESM>` pairs
every compiled call's argument with the word in its `SCTX` source, and all of FNV's 120-odd (command, code) pairs agree (`Guns` 41 is
`AVSmallGuns`, `Survival` 44 is `AVThrowing`, `Turbo` 51 is `AVDetectLifeRange`; `SpeedMult`, `ElectricResist`,
`FrostResist`, `EmpResist` and `Variable02`..`Variable10` have no AVIF in the master and decline). Oblivion
has no `AVIF` records, so its actor-value commands stay declined. This is what #5989 asked for on the ObScript
side. The CTDA `GetActorValue` (fn 14) authors the same enum on Oblivion / FO3 / FNV / Skyrim
(only FO4+ authors an AVIF FormID); `condition_actor_value_form` translates it at evaluation — the FO3/FNV
table above, Skyrim's through the UESP *Actor Value Indices* table — and a `param_1` that is itself a loaded
AVIF (an engine-built condition) passes through. Every vanilla INFO `GetActorValue` index resolves on
Skyrim.esm (318 uses) and FalloutNV.esm (1 264; the 5 `Variable10` uses have no AVIF); Oblivion has no
AVIF records and stays declined.

**Getters.** The host answers from the engine's own reads: the effect model's guards and values where it has
one (`GetItemCount`, `IsInCombat`, `GetIsReference`, `GetPlayerTeammate`, `GetAV`), the components the runtime
keeps otherwise (`GetLocked`, `GetLockLevel`, `GetUnconscious`, `GetRestrained`, `GetCombatTarget`,
`GetDestroyed`, `GetHealthPercentage`, `GetInSameCell`), the link and enable-parent tables (`GetLinkedRef`;
`GetParentRef`, the reference in the Enable Parent tab — Oblivion's 345 scripts), `GetIsCurrentPackage` (the
package the AI reported), `HasPerk` / `IsHardcore` through the condition evaluator, and `GetCurrentTime`.
`GetCurrentTime` reads the game clock the update system reports, and the `GameHour` global is now written from
that clock every frame (the CS: "the same value can be accessed by referring to the special global variable
Gamehour") — scripts and conditions that read the global used to see the authored default.

**More blocks.** `obscript_objects.rs` now also runs `OnHit` / `OnHitWith` (a struck actor's script, filtered by
attacker or by the attacker's equipped weapon; the CS: "only works on NPCs and Creatures" — the GECK's New
Vegas note about a weapon's own script is an inventory item's, below), `OnStartCombat` / `OnCombatEnd` (an
actor's combat state against the mark its last transition left), `OnPackageStart` / `OnPackageDone` /
`OnPackageChange` (delivered by `package_fragment_system` from the queues it already drains) and
`SayToDone`. The hit and combat systems are scheduled beside `hit_event_system`, after every `HitEvent`
producer.

**Items in inventories.** An item's script runs for an item that has no reference. `ObScriptItemScripts` finds the
script by the item's base form, and `obscript_item_system` (in `Late`, ahead of the event cleanup) runs `OnAdd`,
`OnDrop`, `OnEquip` and `OnUnequip` from the item and equipment batches the frame's wearers carry, a block
written for a container or actor (`Begin OnAdd player`) only for it. The host knows the item and its holder:
`GetContainer` answers the holder, `RemoveMe` takes the item out of it and **ends the block** (a new
`ObScriptHost::ends_script` hook, as `Return` does — "the lines of script following it will NOT be executed"), and a
command with no caller has nothing to act on. Locals do not outlive a run, and an item's `GameMode` block does not
run in the inventory: an item has no reference to keep either on.

**`Say` / `SayTo`.** The calling actor speaks the first eligible INFO of the topic (`select_first_info`, the pick an
activation greeting makes, the target the player by default). The call answers the line's length in seconds
(CS: "useful for timing followups"): the subtitle estimate, until the conversation voice player resolves a
recording and replaces it with its real duration; when that has elapsed the speaker's `SayToDone` blocks run, each for its
topic or any. Not modelled: the optional speaker form (a creature speaking with an NPC's voice), the voice-position
flag, `SayTo`'s head tracking, and subtitles — an ambient line has no presenter, only the conversation page.

**Placement and presentation commands.** `GetPos` / `SetPos` / `GetStartingPos` (the axis is one bare letter
`X`/`Y`/`Z` in the bytecode, `AXIS_COMMANDS` decodes it to 0/1/2), `GetScale` / `SetScale`, `GetAngle` /
`SetAngle` / `Rotate` / `GetStartingAngle` act on the placement's Z-up Euler triple (`Rx(-rx)·Rz(ry)·Ry(-rz)`, and
its inverse `quat_yup_to_euler_zup`), a reference's heading in degrees `[0, 360)`; `Rotate` turns by the speed over
the frame (`Context::elapsed`); the player's angles and a parented entity decline. `ApplyImageSpaceModifier` /
`RemoveImageSpaceModifier` / `IsImageSpaceActive` (`IMod`/`RIMod`, Fallout only) play, remove and report an `IMAD`
on the cinematic presentation state's modifier runtime. `AutoSave` raises the `RequestSave { auto }` effect
`Game.RequestAutoSave` raises; `GetSitting` answers 3 for an actor with a `Seated` component and 0 otherwise (the
GECK's "getting ready to sit/stand" states 2 and 4 have no runtime here).

**`PlayGroup` / `IsAnimPlaying` (`obscript_anim.rs`).** An object's `NiControllerSequence`s are named for the GECK
*AnimGroups* (FalloutNV.esm's object meshes: 217 `forward`, 156 `specialidle`, 145 `backward`, 136 `idle`,
126 `left`, 107 `right` — `crates/nif/tests/sequence_names_census.rs`). The NIF import now keeps those clips
(`SceneSatellites::object_sequences`, converted once per model into the `AnimationClipRegistry` by
`ObjectSequenceStore`) and the spawn stamps a `NamedSequences` component (name → clip handle) on the placement root.
`PlayGroup <index> <flag>` maps the index through the AnimGroups list (`3` Forward, `4` Backward … `16` TurnRight;
Oblivion's documented depth is the first five) to a name and starts that clip from the first key on the object's
`AnimationPlayer`; every init flag starts at once (the GECK's `0`, "let the current cycle finish", begins a cycle
early). `IsAnimPlaying [group]` is whether that player runs a clip that has not ended (a `Clamp` clip ends at
its duration), optionally for that group. An object with no sequence of the name — an actor's `AttackPower`/`Recoil`
groups come from its animation set, not its mesh — declines.

**Live verification (`docs/smoke-tests/m47-3-obscript.sh`).** The gate boots FNV and checks, through `byro-dbg`
(`obscript.objects`, `quest.effects`, `anim.pose`): object scripts attach (found live: an interior's references
spawn *before* `populate_scene_runtime`, so the first interior load had no object-script table — `ensure_obscript_tables`
now installs it ahead of the spawn), `OnLoad` (`GoodspringsRadioScript`: remote quest variable + `GetDestroyed` +
`SetDestroyed`), `OnTriggerEnter` (`SunnySmilesTriggerSCRIPT`: one run, `PlaySound` queued), `OnDeath` (Sunny:
`SetEnemy` via the bridge + `SetStage`, nothing declined), item `OnAdd` + `RemoveMe` (`inv.add` now emits the item
transfer), and `PlayGroup` (`OnLoadPlayForwardSCRIPT` on Lucky38BasementFloorB2's force-field glows).

**The caster runtime (`magic_effects.rs`).** A spell, an ingestible or an ability puts *effects* on an actor and
they keep running: a *script effect* runs `ScriptEffectStart` once, `ScriptEffectUpdate` every frame it is active and
`ScriptEffectFinish` at its end (Fallout authors the script on the `MGEF`, archetype *Script*, whose "Assoc. Script"
is the `SCPT`; Oblivion on the spell's effect item, `SCIT`); a value-modifier effect changes its actor value once
(instant), once at the start and back at the end (*Recover*) or every second (neither). The effect carries its own
script locals (an actor can have an object script and several effect scripts that reuse a name) and is saved
(`ActiveMagicEffects`, `FORMAT_MAJOR` 111). `Cast <spell> [target]`, `CastImmediateOnSelf`, `Dispel`,
`HasMagicEffect`, `ScriptEffectElapsedSeconds`, the `OnMagicEffectHit` block, and an ability's scripted effects
(`AddSpell`) use it; so does using a chem — the menu's `Consume` now starts a scripted ingestible's effects (23
FNV consumables carry one), and `inv.use` / `magic.cast` / `magic.effects` drive and show it from the console. Live:
using a NukaCola on FNV runs `NukaColaAddCapEffectScript` (`additem caps001 1`) on the player. Not modelled: effect
archetypes beyond script and value modifier (summons, light, paralysis ...), area and touch/target delivery,
resistance.

**Idle, look, shaders, weather.** `PlayIdle <idle>` writes the Papyrus `PlayIdle` request (the compiled argument is
the idle's *editor ID string*, which the VM resolves through `text_argument`); the app installs the IDLE's `.kf` for
the KF-era games (`populate_kf_idle_clips`, resolved when an actor asks) and the existing idle runtime plays it —
a one-shot hands the actor back the animation it was in (`IdleResume`). Live: Sunny Smiles' trigger makes
Cheyenne play `specialidle_aroo.kf`. `PickIdle` restarts the actor's generic idle (the engine's idle choice *is* that
generic idle, the pool `pick_idle_handle` draws from); `Reset3DState` rewinds the object's animation. `Look` /
`StopLook` hold a `HeadLook` on the actor and `head_look_system` turns its head bone toward the target within
±70° / ±40° at a steady rate, as a world-space delta on the animated pose (no bone-axis knowledge needed).
`PlayMagicShaderVisuals` / `PlayMagicEffectVisuals` / `StopMagicShaderVisuals` play an `EFSH`: its Fill layer's
colour and alpha timeline (fade in, full, fade out to the persistent ratio, pulse) glow the reference's materials and
are put back when it ends — the fill *texture*, the edge layer and the particles are not drawn. `TriggerHitShader`
is a screen blur (2 px per unit of strength, capped at 5, fading over a second: the engine's policy, the GECK
gives none). `ForceWeather` / `SetWeather` / `ReleaseWeatherOverride` set a saved `ForcedWeather`; the exterior's
streaming step re-installs the sky (live on FNV: `NVWastelandClear` -> `NVWastelandHazy` and back), and the engine,
which never changes weather by itself, keeps the override flag for when it does. `GetIsCurrentWeather` reads the sky.

**World, crime and bookkeeping.** `IsInInterior` / `GetInWorldspace` read the cell the reference is in
(`cell_kind.rs`: an exterior root carries the worldspace's form id, the player's is published each frame);
`GetOpenState` reads the two-state activator, and `SetOpenState` now gives a door one so the call is not lost;
`OnReset` runs for a cell the player re-entered after `iHoursToRespawnCell` (72 h by default) — the entry times are a
saved `CellVisits`; `OnDestructionStageChange` follows `DamageObject`; `GetDestructionStage`. `AddNote` /
`RemoveNote` / `GetHasNote` keep the player's notes (saved); `GetDeadCount` / `IsKiller` read a saved kill ledger
the one death funnel fills (a resurrection does not lower it); `IgnoreCrime` is honoured by the hostility pass (the
actor does not start combat with the player), `SetAlert` / `IsWeaponOut` keep a drawn-weapon flag, `GetDetected` /
`GetDetectionLevel` ask the same sight test the hostility pass uses (range from `fSneakMaxDistance`, line of sight
through physics; the level is 3 or 0), `SendAssaultAlarm` turns the named actor — or the faction's loaded member
nearest the player — on the player. `Resurrect` revives in place (`revive.rs`: `Dead` and bleedout cleared, health
back, the ragdoll freed and the bones' collision bodies restored, the idle and the AI package stack re-armed; live:
Sunny Smiles killed, brought back, standing, the count kept); the get-up animation is not modelled. `DropMe`,
`RemoveAllTypedItems` (the FOSE type codes), `GetGS` (and `ShowWarning`), `IsSwimming`, `AutoSave`, `GetSitting`
round out the tail. Oblivion's `IsPlayerInJail` (the served-sentence state `jail.rs` keeps, the same one CTDA 171 reads)
and `GetWeaponAnimType` (the `WEAP` `DATA` type grouped into the CS page's 0 hand-to-hand/staff, 1 one-handed,
2 two-handed, 3 bow; `ItemKind::Weapon::oblivion_type`) run since 2026-10-30. `obscript.cmd` runs any of these through the real host from the console, which is how the
smoke gate drives them.

**What stays declined, and what it waits for.** Every one is a counted decline (`obscript_tools live`,
`ObScriptQuestTimers`, `obscript.objects`), never a made-up value:

| Commands | Blocked on |
|----------|-----------|
| `PickIdle`'s idle *tree*, `GetKillingBlowLimb` ... | the IDLE tree's conditions and parents are not parsed (PickIdle restarts the generic idle instead) |
| `EnableLinkedPathPoints` / `DisableLinkedPathPoints` (Oblivion, 71 / 69 scripts) | Oblivion path grids: the engine paths on navmeshes |
| the crime consumers behind `GetPCFame` / `GetPCInfamy` / `ModPCFame` / `GetCrimeGold` / `ModCrimeGold` / `SetPCExpelled` / `SetPCEnemyOfFaction` / `SetFactionReaction` | these commands run: they read and write the saved `ObScriptLedger` (`obscript_ledger.rs`; `obscript_more/state.rs`). The engine raises the bounty for three crimes so far, on Oblivion (`byroredux/src/crime.rs`, `CrimeModel`): a witnessed theft (`item value × fCrimeGoldSteal`, rounded up, minimum 1), an assault on a non-hostile NPC (`iCrimeGoldAttack`, once per victim) and a murder (`iCrimeGoldMurder`, plus infamy). The Steal / Murder / Attack faction flags are set without a witness. Authored guard dialogue can read the bounty (the Oblivion condition dialect in `condition/catalog.rs`, `guards.rs`). `PayFine`, `PayFineThief` and `GoToJail` run (`jail.rs`). A sentence ends by resting in a cell bed. A witnessed crime calls the guards, and a guard who sees a wanted player (bounty at or past `iCrimeGoldAttackMin`) rushes them (`crime.rs`), Skyrim reports each witnessed crime to the witness's crime faction's bounty (`FactionCrimeGold`, fined by the faction's Crime tab), and every game records who saw a crime in `CrimeKnowledge`, which `GetCrime` (CTDA 122) and `GetCrimeKnown` read. The confiscated goods are delivered to the jail's `StolenGoods` chest. Still missing: pickpocket and trespass fines, witness disposition drops, guards that read the bounty (arrest, pay, jail), Expelled, and FO3 / FNV's faction-based crime |
| `GetDisposition` / `ModDisposition`, `AddTopic`, `SetQuestObject`, `SetItemValue` | a per-NPC disposition value, a topic-known gate on the opening menu (#5350), a quest-item flag and a script-overridable base value — each has no reader in the engine, so it declines rather than store a value nothing consults |
| `GetDayOfWeek` | the game calendar's epoch: the weekday of day 0 is in no record, and neither the CS nor the GECK page says which weekday a new game starts on |
| `CloseCurrentOblivionGate` (Oblivion, 16 scripts) | the Oblivion-gate quest machinery: the gate's open state and the Sigil Stone it spawns have no runtime |
| `ModPCSkill`, and every other Oblivion `GetAV` / `SetAV` / `ModAV` | Oblivion has no `AVIF` records, so its hard-coded attributes and skills have no key in `ActorValues` yet: it needs an Oblivion CHARAL ruleset first (`ModPCSkill` also moves the skill-usage counter, CS page *ModPCSkill* §Issues) |
| `EnableFastTravel`, `UnlockChallenge`, `SetReputation` (no GECK page) | no fast-travel or challenge UI exists to honour them |
| `PushActorAway`, `UseWeapon` / `FireWeapon`, `OnGrab` / `OnRelease` / `OnFire` | a knockdown cycle (the ragdoll teardown above is the first half), a scripted attack, and a player grab |
| `SetCombatStyle`, `SetTalkingActivatorActor`, `PlayMusic`, `SetVATSTarget`, `ShowMap` ... | each its own missing state; none is an ObScript-layer gap |

### M47.3 — result scripts (2026-10-22)

The second playable route (FNV, [`playable-vertical-slice.md`](playable-vertical-slice.md))
found that the Fallout-era quests had never moved on their own: their stages and
objectives live in *result scripts*, the compiled statement streams a `QUST` stage
item and a dialogue `INFO` carry inline, and the parser kept only a `has_script`
flag. They are now parsed and run
([`obscript_results.rs`](../../crates/scripting/src/obscript_results.rs)):

- **Parsing.** Each stage item's `SCHR`/`SCDA`/`SCTX`/`SLSD`+`SCVR`/`SCRO`/`SCRV`
  run becomes `QuestStageLogEntry::result_script`, and an INFO's runs before and
  after its `NEXT` marker become `InfoRecord::result_script_begin` / `_end`
  (xEdit `wbEmbeddedScript`), through the same `parse_scpt` a standalone `SCPT`
  takes (`script::embedded_script_at`). FalloutNV.esm: 682 stages carry one.
- **Stage items.** `obscript_stage_result_system` (Update, beside the quest tick)
  polls its own quest-event journal subscriber and runs every item of every stage
  set since its last run whose conditions pass (GECK *Quest Stages Tab*: "each
  stage item whose conditions evaluate to true runs its result script"), then the
  stages those scripts set, bounded at 16 rounds a frame. They are non-reference
  scripts: the quest owns them, no implicit reference. The one deviation from the
  original: `SetStage` there runs the stage's items inside the calling statement;
  here the caller finishes first.
- **Dialogue lines.** The Begin script runs when the line is spoken and the End
  script when it is replaced or the conversation closes — the hooks the Skyrim
  `TIF_` fragments use — with the speaker as the implicit reference (GECK
  *Reference functions*: "dialogue result scripts can use implicit reference
  syntax").
- **Topic links are selectable.** `select_topic_by_form_id` used to accept only a
  topic an alias-bound running quest owns; FO3 / FNV quests bind no aliases, so
  their topics are reached only through the spoken line's `TCLT` links, and a
  topic the open menu offers is now selectable (`dialogue.select <topic>` is the
  console door to the same selection).

What it fixed live, on VCG02 "Back in the Saddle": Sunny Smiles's force-greet
(`SunnyMeetPlayerDialoguePackage`, gated on `VFreeformGoodsprings.bMetSunny == 0`)
re-opened after every boot and every reload, because her greeting's End script
`set VFreeformGoodsprings.bMetSunny to 1` never ran — the #6052 workaround in the
gates was masking it. Her "teach me" line's Begin script now starts the quest,
completes objective 3, displays 5 and sets stage 10, and each later stage's items
display and complete the rest.

Two VM defects running these scripts exposed (both had been setting stages at every
FNV boot, invisibly, since the stage's items never ran):

- **`GetPCMiscStat` / `ModPCMiscStat` take a stat *code*.** The quoted name compiles
  to a `u16` index into xEdit's `wbMiscStatEnum` (`challenges::MISC_STAT_NAMES`;
  CGTutorial's `GetPCMiscStat "Times Addicted"` is `01 00 0a 00`), measured over
  every vanilla FO3 / FNV call by `obscript_tools avcodes`. The VM decoded it as a
  string and the host answered the code as the count, so `> 0` held and the
  addiction tutorial fired at boot.
- **A condition that read a declined value runs no arm.** A declined getter answers
  `0` so the expression can be evaluated, but the `If` chain it gates is now skipped
  whole (`ObScriptHost::declined_calls`), the fail-closed rule the condition
  evaluator already applies (#5993). Before, `player.GetWeaponHealthPerc < 25` with
  no weapon equipped read `0 < 25`. On a bare world (`obscript_tools live`) this
  removed exactly the bogus advances — CGTutorial 210 / 218 on FO3 / FNV, and
  Oblivion's MQ01 at stage 76, MS13, SQ07, MS06 and SE13 mid-quest — and kept the
  boot quests (CG00 95, Charactergen 5, E3 5, MQDragonArmor 20).

Vanilla tutorial boxes are real now: CGTutorial's items `ShowMessage` the
radiation, addiction and other tutorials, and a box pauses the world until it is
answered (`ui.answer` is the console door; gates call `smoke_answer_message_boxes`).

### The R5 demo translations (`papyrus_demo/`)

The proof-of-concept hand-translations that closed R5. Each is a faithful
port of a real shipped script, kept in-tree as living evidence and a test
surface; together they cover the four major Papyrus pattern families:

| Module | Source `.psc` | Pattern exercised |
|---|---|---|
| `papyrus_demo` (root) | `defaultRumbleOnActivate` | latent `Utility.Wait()` + multi-state dispatch + cross-subsystem call |
| `quest_advance` | `DA10MainDoorScript` | stage-gated `SetStage` (uses `ConditionList`) |
| `dlc2_ttr4a` | `DLC2TTR4aPlayerScript` | `RegisterForUpdate` / `OnUpdate` poll-fire-cancel |
| `mg07_door` | `MG07LabyrinthianDoorScript` | stage-gated activation + cross-reference method call after a latent wait |

`RumbleOnActivate` is the canonical example: its `state: RumbleState { Active,
Busy { wait_remaining_secs }, Inactive }` enum replaces Papyrus's
`Auto State active / busy / inactive` trio, and the `Busy` variant's
`wait_remaining_secs` field replaces Papyrus's stack-suspended `Utility.wait()`
frame with plain data. The single Papyrus event body splits into
`rumble_on_activate_system` (the immediate camera-shake + transition into `Busy`)
and `rumble_tick_system` (the post-wait `If repeatable / Else` branch) — exactly
the two-system split that is R5's load-bearing finding. Cross-subsystem calls
(`Game.shakeCamera`, `Game.shakeController`) become `CameraShakeCommand` /
`ControllerRumbleCommand` markers emitted on the `PlayerEntity` resource's
entity; the camera/input subsystems will drain them once they land.

### Papyrus parser (M30 + M30.2)

[`crates/papyrus/`](../../crates/papyrus/) parses `.psc` source into a typed AST.
It does **not** execute anything — the AST feeds the future M47.2 transpiler that
will emit ECS component + system definitions of exactly the shape the
`papyrus_demo` translations hand-built.

- **M30 Phase 1** — logos lexer ([`lexer.rs`](../../crates/papyrus/src/lexer.rs),
  [`token.rs`](../../crates/papyrus/src/token.rs)) + Pratt expression parser
  ([`parser/expr.rs`](../../crates/papyrus/src/parser/expr.rs)).
- **M30.2** (closed 2026-05-23, `ab0eee96`) — the full `.psc` parse:
  - statement parser ([`parser/stmt.rs`](../../crates/papyrus/src/parser/stmt.rs)):
    Return, If/ElseIf/Else/EndIf, While/EndWhile, local `VarDecl` with a
    speculative-type disambiguator, expr-stmt, assignment with compound operators;
  - top-level item parser ([`parser/script.rs`](../../crates/papyrus/src/parser/script.rs)):
    ScriptName + Extends header, Property (short + full, six `PropertyFlags`),
    Function (typed/untyped + four `FunctionFlags` incl. bodyless `Native`), Event,
    Auto State / State, Struct, CustomEvent, Group, Variable, Import;
  - public `parse_script(source) -> Result<(Script, Vec<ParseError>), …>` driver
    with per-item error recovery (`crates/papyrus/src/lib.rs`).

  Load-bearing finding: `Parser::peek()` silently skips newlines, so
  Return-with-value vs without needs `peek_raw()`. All four R5 source scripts
  round-trip end-to-end with zero recovered errors — asserted in
  [`crates/papyrus/tests/r5_round_trip.rs`](../../crates/papyrus/tests/r5_round_trip.rs).
  FO4 extensions (`Const`, `Hidden`, `Mandatory`, `BetaOnly`, `DebugOnly`) land
  as flag tokens decorating existing items. Semantic validation and the
  transpiler proper are M47.2. (#1270 later added a recursion-depth guard to the
  Pratt parser.)

The parser is *not* yet on the cell-loading hot path; M47.0's runtime attaches
hand-written components via `ScriptRegistry`, and the transpiler that would
consume parsed `.psc` automatically is M47.2.

---

## Why Replace Papyrus?

Papyrus is the scripting language used in Skyrim and Fallout 4. It runs inside a
separate virtual machine with its own threading model, execution queue, and save
serialization. This architecture causes several well-known problems:

- **Save corruption** — the VM serializes execution stacks (suspended function
  calls) into save files. Orphaned stacks from removed mods consume RAM
  indefinitely and can corrupt saves beyond recovery.
- **Script lag** — the VM has a per-frame budget for executing scripts. Under
  load (many mods, many NPCs), the queue backs up and scripts run seconds
  behind game state. Players see this as delayed quest updates, broken
  animations, and unresponsive activators.
- **Forced multithreading** — the VM runs on its own thread pool to maintain
  stability despite the single-threaded game engine. This creates race
  conditions, and latent functions (Wait, MoveTo) suspend execution mid-stack,
  creating invisible state that is expensive to serialize and fragile to restore.
- **Memory overhead** — every running script instance allocates a stack frame.
  Const scripts (introduced in Fallout 4) were an explicit attempt to let the
  engine unload scripts to save memory — an admission that the VM's footprint
  is a problem.
- **No direct memory sharing** — scripts access game state through a slow native
  function bridge. Every `GetActorValue()` or `GetPositionX()` crosses the
  VM/engine boundary.

ByroRedux eliminates all of these by making scripts first-class ECS citizens:
script state is component data, script logic is systems, and the ECS scheduler
owns all execution. No VM, no stacks, no separate threads. The R5 prototype
confirmed this isn't theoretical — `RumbleOnActivate`'s `Busy {
wait_remaining_secs }` field *is* the serialized suspended-wait frame, except
it's plain data the save system already knows how to round-trip.

### The Numbers: Bethesda's Own Profiling Data

Bethesda's official [Papyrus performance guide](https://falloutck.uesp.net/wiki/Performance_(Papyrus))
includes profiling data that quantifies the problems:

**Queue push latency dominates execution time:**
```
ObjectReference.GetPositionY:  0.04ms actual execution
                            5,760.60ms waiting in queue
```
That's a **144,000x overhead** — the work is instant, the VM scheduling is
catastrophic. A function that calls `GetPositionX`, `GetPositionY`, and
`GetPositionZ` separately spent 7,233ms total, of which ~7,200ms was queue
waiting. In ECS, reading `transform.translation` is a single field access —
nanoseconds, no queue.

**Object contention serializes access:**
When multiple scripts need data from the same object (e.g., the player), they
queue behind each other. 10 scripts asking for the player's level execute
serially, each waiting for the previous one to finish. In ECS, component reads
are concurrent (`RwLock<T>` allows multiple readers).

**Native function framerate sync:**
Most native function calls sync to the game's framerate — 30 native calls at
30fps takes ~1 second. "Non-delayed" functions bypass this, but most common
operations (position, inventory, actor values) don't qualify. In ECS, component
access is immediate — no frame boundary synchronization.

**Persistence traps:**
Storing an ObjectReference in a script variable makes that object persistent —
it cannot be unloaded to save memory. Function-local variables release on
return, but script-level variables persist indefinitely. Bethesda's advice:
"prefer function variables over script variables" and "set to None when done."
In ECS, components are owned by the world — no persistence surprises.

**Bethesda's own recommended patterns validate ECS:**
- "Use states with empty handlers instead of condition checks" → ECS queries
  skip entities without the relevant component (zero-cost non-participation)
- "Don't re-do work; cache in local variables" → component data is always
  available, no need to call through a native bridge
- "Prefer single point of entry over multiple cross-script calls" → in ECS,
  a system reads all components it needs in one query
- "Prefer events over polling loops" → ECS systems are inherently event-like
  (marker components appear, systems process, markers removed)
- "Break large functions into smaller ones" → each system does one thing

---

## Papyrus Language Model

### What Modders Write

A Papyrus script is a text file (`.psc`) that gets compiled to bytecode (`.pex`).
It declares a type (`extends ObjectReference`, `extends Quest`, etc.) and contains:

- **Properties** — typed variables exposed to the Creation Kit editor for
  per-instance configuration. Can be `Auto` (simple get/set), `Const`
  (editor-set, never changes at runtime), or `Mandatory` (editor warns if empty).
- **Events** — callbacks the game fires when something happens: `OnActivate`,
  `OnDeath`, `OnHit`, `OnTimer`, etc. 136 events in Fallout 4.
- **Functions** — reusable logic. Can be `Global` (no `Self`), `Native`
  (engine-implemented), or custom. Support default parameters and named
  out-of-order arguments.
- **States** — named modes (`Auto State AtRest`, `State Busy`) where different
  versions of the same event handler are active. `GoToState("Busy")` switches
  which handlers respond.
- **Fragments** — inline script snippets on quest stages, topic infos, packages,
  scenes, and terminals. Run at specific narrative moments.

The ByroRedux Papyrus parser (M30 + M30.2) covers all of these top-level item
kinds plus the FO4 flag extensions — see
[Papyrus parser](#papyrus-parser-m30--m302) above.

### Expression Grammar

Standard operator precedence:
```
|| → && → comparison → +/- → */% → unary(-,!) → cast(as) → dot(.) → array([]) → atoms
```

No bitwise operators. Cast (`as`) is runtime downcasting. Dot chaining
(`MyFunc().Prop[0]`) is the fluent API modders expect. This is implemented by the
Pratt parser in [`parser/expr.rs`](../../crates/papyrus/src/parser/expr.rs)
(with a recursion-depth guard, #1270).

### Type System

- Primitives: `Bool`, `Int`, `Float`, `String`
- Object types: `ObjectReference`, `Actor`, `Quest`, `Form`, etc.
- Arrays: `Int[]`, `Actor[]`, dynamically resizable (Fallout 4+)
- `Var` type (Fallout 4+): holds any value, used for custom event arguments
- Structs (Fallout 4+): grouped variables, passed by reference

### Inheritance

Scripts extend base types: `Scriptname MyScript extends ObjectReference`. The
child script inherits all functions and events from the parent, can override
them, and access the parent's version via `Parent.DoStuff()`. The base script
hierarchy mirrors the game object hierarchy (Actor extends ObjectReference
extends Form).

---

## The 136 Events — Mapped to ECS Patterns (DESIGN)

Every Papyrus event maps to one of three ECS mechanisms. No event queue, no VM
dispatch table, no registered callback lists. The shipped catalog so far is in
[Shipped event markers](#shipped-event-markers); the tables below are the
**full design plan** — most of these marker types are not yet implemented, and
the names are illustrative (the shipped markers use the names in the catalog
above, e.g. the activation event ships as `ActivateEvent`).

### Mechanism 1: Marker Components (interaction events)

A transient component is added to the entity by an engine system. Script systems
process it and remove it. Covers ~50 events.

| Papyrus Event | Marker Component (illustrative) | Added By |
|---|---|---|
| `OnActivate(akActionRef)` | `ActivateEvent { activator: EntityId }` ✓ shipped | Activation system / console (M47.0) |
| `OnHit(...)` | `HitEvent { aggressor, source, projectile, … }` ✓ shipped (no emit yet) | Combat system |
| `OnTriggerEnter(akRef)` / `OnTriggerLeave` | `OnTriggerEnterEvent { triggerer }` ✓ shipped (no emit yet) | Physics / Rapier sensor |
| `OnEquipped` / `OnUnequipped` | wearer-owned `EquipmentEventBatch` with stable item FormIDs ✓ shipped | Inventory / equip system and translated `EquipItem` fragments |
| `OnItemAdded` / `OnItemRemoved` | `InventoryEvent { item, count, source }` (planned) | Container system |
| `OnContainerChanged` | `ContainerChangeEvent { ... }` (planned) | Container system |
| `OnGrab` / `OnRelease` | `GrabEvent { grabbed: bool }` (planned) | Interaction system |
| `OnWorkshopObjectPlaced` / etc. | `WorkshopEvent { kind }` (planned) | Workshop system |

The marker component pattern is zero-cost for entities that don't participate —
no registration needed. If an entity doesn't have a script system watching for
`HitEvent`, the marker is never added in the first place.

### Mechanism 2: State Field Watches (state change events)

Systems compare current vs previous frame state on existing components. When a
field transitions, the associated logic fires. Covers ~40 events. **Not yet
implemented** — these are planned component/field shapes.

| Papyrus Event | Watched Component | Field Transition |
|---|---|---|
| `OnDeath` / `OnDying` | `ActorState` | `health > 0` → `health <= 0` |
| `OnCombatStateChanged` | `ActorCombat` | `state` field changes |
| `OnLoad` / `OnUnload` | `CellPresence` | `loaded: false` → `true` |
| `OnOpen` / `OnClose` | `DoorState` | `open` field changes |
| `OnLockStateChanged` | `LockState` | `locked` field changes |
| `OnPowerOn` / `OnPowerOff` | `PowerState` | `powered` field changes |
| `OnPackageStart` / `OnPackageEnd` | `ActorPackages` | `active_package` changes |
| `OnStageSet` (Quest) | `QuestStageState` | `stage` field changes (resource shipped; `QuestStageAdvanced` marker emitted, dispatcher pending) |
| `OnLocationChange` | `ActorLocation` | `current_location` changes |

Implementation plan: each watchable component stores a `prev_` shadow field or
the system keeps a parallel `PreviousState<T>` component; the diff system runs
early in the frame, before script systems. (Note: `OnLoad` at spawn time is
already covered by the shipped `OnCellLoadEvent` marker rather than a state
watch.)

### Mechanism 3: Timer/Condition Components (scheduled events)

Components with countdown or condition data, checked by systems each tick.
Covers ~15 events. The Papyrus timer events ship through the
update-registration registry (§[Update
registrations](#update-registrations-m472)); `RecurringUpdate` is dormant
(only the unspawned `dlc2_ttr4a` demo writes it). The rest are planned.

| Papyrus Event | Component | System Behavior |
|---|---|---|
| `OnTimer(aiTimerID)` | `UpdateRegistration { timer: Some(id) }` ✓ shipped | `script_update_system` delivers `OnTimer` when due; the dormant `ScriptTimer`/`TimerExpired` pair is not on this path (#5608) |
| `OnUpdate` (`RegisterForUpdate`) | `UpdateRegistration` ✓ shipped (the M47.2 VM path); `RecurringUpdate` dormant (only the unspawned `dlc2_ttr4a` demo writes it) | Tick, fire `OnUpdateEvent` / deliver `OnUpdate`, re-arm |
| `OnTimerGameTime` | `UpdateRegistration { clock: GameTime, timer }` ✓ shipped | Game-hours due check in `script_update_system` |
| `OnDistanceLessThan` / `GreaterThan` | `DistanceWatch { target, threshold }` (planned) | Check distance each frame |
| `OnGainLOS` / `OnLostLOS` | `LOSWatch { target, had_los }` (planned) | Raycast check |
| `OnTranslationComplete` | `TranslationMove { target_pos, speed }` (planned) | Movement system, fire on arrival |

### Remote Events (cross-entity listening)

Papyrus (Fallout 4+): `RegisterForRemoteEvent(akSender, "OnDeath")` — script
on entity A receives events from entity B.

ECS plan: entity A has a `RemoteWatch { target: EntityId, event: EventKind }`
component. The event system queries both the source entity (for the marker
component) and all watchers (for `RemoteWatch` pointing at the source). No
registration list — the query is the registration. (Not yet implemented; R5's
notes record that the chosen R5 candidate scripts didn't exercise this, so it
remains design.)

### Custom Events (user-defined broadcast)

Papyrus: `CustomEvent MyEvent` + `SendCustomEvent("MyEvent", args)`.

ECS plan: `CustomEvent { name: FixedString, sender: EntityId, args: Vec<ScriptValue> }`
marker added to interested entities; `SendCustomEvent` adds the marker to every
entity holding a matching `CustomEventWatch { name, source }` component — the
query is the registration. (R5 follow-up 3 recorded a "vanilla CustomEvent
non-finding": the R5 candidate scripts didn't use custom events, so this stays
design until a consumer needs it.)

---

## ECS Mapping: Papyrus Concepts → ByroRedux

The examples below are illustrative of the translation strategy. Where a concrete
shipped translation exists, the corresponding `papyrus_demo` module is named.

### Properties → Component Fields

Papyrus:
```papyrus
Quest Property MyQuest Auto
Int Property StageToSet Auto
Bool Property IsEnabled Auto Const
Actor Property TargetActor Auto Mandatory
```

ECS (illustrative; cf. `RumbleOnActivate`'s real `Float Property` → `f32` field
mapping in [`papyrus_demo/mod.rs`](../../crates/scripting/src/papyrus_demo/mod.rs)):
```rust
pub struct MyActivatorScript {
    pub my_quest: FormId,          // Auto → regular field
    pub stage_to_set: i32,         // Auto → regular field
    pub is_enabled: bool,          // Const → field with default, skip in save serialization
    pub target_actor: Option<FormId>, // Mandatory → Option, validated at load time
}
impl Component for MyActivatorScript {
    type Storage = SparseSetStorage<Self>;
}
```

Editor configuration: the Creation Kit sets property values per-instance. In
ByroRedux the per-instance values live in Skyrim+ `VMAD` subrecords; the spawner
in `ScriptRegistry` installs the component with `.psc` defaults today, and
VMAD-override decode is the M47.2 follow-up (this is exactly why three of the
four R5 demos aren't registered yet).

### Events → System Scheduling

Papyrus:
```papyrus
Event OnActivate(ObjectReference akActionRef)
    if akActionRef == Game.GetPlayer()
        MyQuest.SetStage(StageToSet)
    endif
EndEvent
```

ECS (illustrative; cf. `rumble_on_activate_system` /
`quest_advance_system`):
```rust
fn activator_on_activate(world: &World) {
    let Some(events) = world.query::<ActivateEvent>() else { return };
    let Some(mut scripts) = world.query_mut::<MyActivatorScript>() else { return };

    for (entity, event) in events.iter() {
        if let Some(script) = scripts.get_mut(entity) {
            if event.activator == player_entity(world) {
                set_quest_stage(world, script.my_quest, script.stage_to_set);
            }
        }
        // No remove here — event_cleanup_system sweeps ActivateEvent at end of frame.
    }
}
```

No VM dispatch. The system runs in the ECS scheduler like any other system. The
`ActivateEvent` marker *is* the event — present → process; absent → empty query,
effective no-op. (Note: handler systems do **not** remove the marker themselves;
the shipped contract is that `event_cleanup_system` drains all transient markers
at end of frame, so multiple handlers can observe the same event.)

### States → Enum Fields

Papyrus:
```papyrus
Auto State AtRest
    Event OnActivate(ObjectReference akActionRef)
        GoToState("Busy")
        PlayAnimation("Pull")
        GoToState("AtRest")
    EndEvent
EndState

State Busy
    Event OnActivate(ObjectReference akActionRef)
        ; ignore while busy
    EndEvent
EndState
```

ECS (this is exactly the `RumbleState { Active, Busy { wait_remaining_secs },
Inactive }` shape shipped in `papyrus_demo`):
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PullChainState { AtRest, Busy }

pub struct PullChainScript {
    pub state: PullChainState,
    pub animation: FixedString,
}

fn pull_chain_system(world: &World) {
    let Some(events) = world.query::<ActivateEvent>() else { return };
    let Some(mut scripts) = world.query_mut::<PullChainScript>() else { return };
    for (entity, _event) in events.iter() {
        if let Some(script) = scripts.get_mut(entity) {
            match script.state {
                PullChainState::AtRest => {
                    script.state = PullChainState::Busy;
                    // play_animation(...); a tick/animation-completion system
                    // transitions back to AtRest — the latent-op split.
                }
                PullChainState::Busy => {} // ignore (empty Papyrus body)
            }
        }
    }
}
```

The state is visible data. Any system can read it. No hidden VM state machine.
Papyrus's runtime `GoToState("active")` string lookup (which couldn't catch
typos until the branch fired) becomes a compile-time-exhaustive `match`.

### PEX Extender Compatibility Preflight

Before a compiled Papyrus script is decompiled, `Pex::call_sites()` walks
state functions and full-property accessors in bytecode order. Static,
instance, and parent calls retain their script object, lexical scope,
instruction offset, argument count, and debug source line when present.

The scripting compatibility pass classifies recognized extender-era calls
against the SDK service catalog. Calls with an engine semantic destination
(for example, StorageUtil collections or mod events) report the target service
and required migration; unsafe facilities such as arbitrary JSON file access,
physical-key injection, and raw Scaleform access produce explicit policy
diagnostics. Vanilla and unknown mod calls are left alone, while malformed call
metadata is reported rather than silently omitted.

`analyze_source_compatibility` applies the same catalog directly to a parsed
Papyrus AST. It recursively covers functions, events, states, full properties,
groups, initializers, defaults, nested expressions, and control-flow bodies,
returning byte spans and one-based source lines in source order. This gives
authoring and porting tools the same compatibility policy before a `.pex` is
compiled.

Loose scripts can be checked without launching the engine:

```bash
cargo run -p byroredux-scripting --example extender_preflight -- \
  path/to/source.psc path/to/compiled.pex
```

The tool accepts multiple mixed inputs, prints each attributed finding and an
aggregate summary, exits `1` for input/parse errors, and exits `2` when an
unsupported or malformed call makes the compatibility gate fail. Mapped calls
remain visible but do not fail the command because they have a documented
engine-service migration target.

The engine also records compatibility evidence while it loads ordinary VMAD
scripts, quest fragments, and scene fragments. Each boundary reuses its
existing PEX parse, fingerprints the bytes, and publishes only relevant calls
to a bounded `CompatibilityRegistry`; attaching the same compiled script to
many references therefore counts it once. Operators can run `sdk.compat` in
`byro-dbg` to see deterministic provider/function aggregates, occurrence and
unique-script counts, mapped engine services, malformed-call totals, and
whether the registry hit its budget. This is an observed-script report for the
current engine world, not yet an eager scan of every PEX in every archive.
For exact `StorageUtil` global integer/string read aliases, each aggregate also
prints the target `byro.storage` operation, expected value variant, and
`ObjKey == None` constraint. Writes and other `StorageUtil` families are
reported unsupported rather than receiving a provider-wide compatibility
claim.

### Fragments → Triggered Systems

Papyrus fragments are inline script snippets that run at specific moments:
quest stage set, topic info played, package started, scene phase begun.

ECS: systems that watch for the corresponding state changes. The production
path populates `QuestStageFragments`/`SceneFragments` from compiled `.pex`,
then `quest_fragment_dispatch_system` and the scene runtime react to stage and
phase transitions. Lowering is conservative: an unmodelled statement declines
the whole fragment rather than approximating it; runtime resolution failures
skip the affected effect and leave the remaining chain available.

```rust
fn quest_stage_0010(world: &World) {
    let Some(advances) = world.query::<QuestStageAdvanced>() else { return };
    for (_entity, adv) in advances.iter() {
        if adv.quest == MY_QUEST_ID && adv.new_stage == 10 {
            // Fragment logic here — idempotency is free because the marker
            // only exists on the frame the stage actually advanced.
        }
    }
}
```

### Functions → Rust Functions / System Utilities

| Papyrus | ByroRedux |
|---|---|
| `Global Function` (no Self) | Regular Rust function |
| `Native Function` (engine-implemented) | System function exposed to scripting |
| Custom function on a script | Method on a component or standalone system |
| `Self` variable | Entity ID passed as parameter |
| `Parent.DoStuff()` | Not needed — no inheritance, compose components |
| `DebugOnly` / `BetaOnly` | `#[cfg(debug_assertions)]` on systems |
| Default parameters | Rust default trait or builder pattern |

### Inheritance → Composition

Papyrus: `Scriptname MyActor extends Actor` — inherits all Actor functions and
events, adds custom logic.

ECS: attach `ActorTraits`, `ActorStats`, `ActorAI`, etc. as individual
components, then add `MyActorScript` as an additional component. No inheritance
hierarchy. An entity can have any combination of components. Two mods can add
different script components to the same entity without conflict.

### Inter-Mod Communication → Component Queries

Papyrus (Fallout 4+): special API for calling functions on scripts from other
mods without compile-time dependency.

ECS: query by component type. If the component exists on an entity, interact
with it. If not, the query returns nothing. No special API, no registration,
no dependency declaration. This is free.

```rust
// Mod A adds a "Bounty" component to NPCs
// Mod B wants to read bounties if Mod A is installed
fn read_bounties(world: &World) {
    // If no entity has BountyComponent, this query returns None — zero cost
    if let Some(bounties) = world.query::<BountyComponent>() {
        for (entity, bounty) in bounties.iter() {
            // Mod B can read Mod A's data with no dependency
        }
    }
}
```

---

## What Bethesda Fixed in Fallout 4 (and Why It Validates ECS)

Several changes Bethesda made from Skyrim to Fallout 4 were explicit attempts
to fix problems inherent to the VM architecture. Each one validates our
approach:

| Bethesda's Fix | The Problem | ECS Solution |
|---|---|---|
| Replaced `OnUpdate` with per-script timers | Global per-frame callback too expensive | `RecurringUpdate` component + the saved `ScriptUpdates` registry (`UpdateRegistration` timers), ticked by system (shipped) |
| `OnHit`/`OnMagicEffectApply` require registration | Too many scripts receiving unwanted events | Only entities with marker participate |
| `OnItemAdded`/`Removed` requires filters | Player dumps 100 items → VM floods | Query-based, zero cost for non-participants |
| `Const` properties | Save bloat from storing values that never change | Component field defaults, skip in serialization |
| `Const` scripts | VM memory overhead, "game may unload to save memory" | Stateless systems, zero footprint when idle |
| Remote Events | Cross-entity scripting required scripting both entities | Component referencing other entity + system query |
| Custom Events | No script-defined broadcast mechanism | Marker component with `Vec<ScriptValue>` args |
| Structs | No way to group related variables | Components |
| Namespaces | Script name collisions between mods | Rust module paths |
| Inter-mod communication | No way to interact with other mods' scripts | Query by component type — if present, interact |
| Fragment improvements | Fragment scripts created invisible dependencies | Triggered systems with explicit state queries |

Every one of these is a patch on a VM architecture. In ECS, the underlying
problems don't exist in the first place.

---

## Save Serialization

### Papyrus Save Problem

The Papyrus VM serializes:
1. All script instances and their property values
2. All active stack frames (suspended function calls)
3. All timer registrations
4. All event registrations

Orphaned stacks (from removed mods) persist forever, consuming RAM and
eventually corrupting the save. Bethesda's official position: *"Removing mods
and continuing to use the same save game is not supported by the game."*

The save can happen **between lines or even mid-line**. When scripts change
between saves, the engine must reconcile old serialized execution state with
new code. [Bethesda's official rules](https://falloutck.uesp.net/wiki/Save_File_Notes_(Papyrus))
reveal the complexity:

**Property/variable rules:**
- Auto property: saved value overrides masterfile value (unless const)
- Non-auto property: backing variable saved, won't get masterfile value on new add
- Const property: always uses masterfile value, ignores save — but still
  serializes the old value, and other variables assigned from it keep the old value
- Changing const↔non-const swaps which value wins on load
- `OnInit` runs only once ever — new variables added after first save won't be initialized
- Rename = delete + create (value lost)
- Type change = value discarded

**Function rules (mid-execution changes):**
- Removed function mid-execution: old bytecode loaded from save, finishes
  running, then future calls fail with errors
- Changed function: old serialized version runs to completion, new version
  used for subsequent calls — two versions coexist in one session
- Native→scripted change: **stack thrown out entirely**

**Persistence trap:** putting an ObjectReference in a script variable makes
that object permanently loaded in memory. Function-local variables release
on return, but script-level variables persist indefinitely.

### ECS Save Solution

Only component data is serialized. There are no stacks, no suspended frames,
no registration lists. None of the above complexity exists. (The R5 prototype
made this concrete: `RumbleOnActivate.state == Busy { wait_remaining_secs:
0.13 }` is the entire serialized form of a suspended `Utility.wait()` — a single
`f32`, not a heap stack frame.)

- **Properties** → component fields → serialized as regular data
- **Const properties** → skipped (value comes from the plugin, not the save)
- **Timers** → `RecurringUpdate` components and the `ScriptUpdates`
  registration registry (the dormant `ScriptTimer` aside) → serialized as a
  remaining-time float / a due timestamp
- **States** → enum field on component → serialized as an integer
- **Quest stages** → `QuestStageState` resource → serialized as `current_stage`
  + the `stages_done` set (sorted by FormID before writing)
- **Event registrations** → `RemoteWatch` / `CustomEventWatch` components
  (planned) → serialized as entity + event references

**Mod changes between saves — clean semantics:**
- Adding a component field → field gets default value, no initialization timing issues
- Removing a component field → field ignored on load, no orphaned state
- Changing a field type → migration system handles conversion or resets to default
- No mid-execution saves — systems complete atomically within their frame tick
- No function versioning — systems are compiled Rust code, not serialized bytecode
- No persistence traps — components are world-owned, not reference-counted

Removing a mod removes its components from all entities. No orphaned state.
Adding a mod adds new components to entities that match its records.
**Removing mods and continuing to play is fully supported.**

---

## Legacy Script Compatibility

For loading existing game content, ByroRedux needs to understand Papyrus
scripts from ESM/ESP files. Three approaches, not mutually exclusive — and the
shipped runtime already uses (1):

### Approach 1: Record-Level Mapping (SHIPPED for a handful of scripts)

The record loader extracts the *effect* of scripts (what properties are set,
what stages they respond to) and attaches equivalent ECS components. This
doesn't execute Papyrus — it interprets the configured state. This is exactly
the M47.0 `ScriptRegistry` path: a REFR's `SCRI`→SCPT→`editor_id` is looked up
in the registry and a hand-written spawner installs the component. Today only
`defaultRumbleOnActivate` is mapped; the hand-translated `papyrus_demo` modules
are the worked examples this approach generalises.

### Approach 2: Papyrus Transpiler (M47.2, parser SHIPPED)

Parse `.psc` source files (the M30/M30.2 parser — already shipped) and transpile
them to Rust component definitions + system functions of the same shape the
`papyrus_demo` translations hand-built. The grammar is simple enough that a
transpiler is feasible, and R5 proved the target shape works. The parser feeds
this; the transpiler itself is M47.2.

### Approach 3: PEX Bytecode Interpreter (stretch goal)

Load compiled `.pex` bytecode directly and interpret it within the ECS. This
would support mods that distribute only compiled scripts (no `.psc` source).
Requires understanding the `.pas` assembly-level opcodes. The pipeline is:
`.psc` source → `.pas` assembly → `.pex` bytecode → interpreter → ECS actions.

### API Surface to Support

The complete Papyrus API surface that legacy content uses is documented in
[`docs/legacy/papyrus-api-reference.md`](../legacy/papyrus-api-reference.md).
Key numbers:

- **101 script types** in the inheritance hierarchy
- **ScriptObject root:** registration functions, events, state machine, timers,
  reflection
- **Form base:** native + F4SE functions (keyword checks, name/weight/value)
- **Actor:** ~150 member functions decomposable into ~15 ECS components, ~40 events
- **Utility scripts:** global function containers (Game, Math, Debug, etc.)
- **ObjectMod system:** data-driven property modification with operators,
  value types, weapon properties

### UI Compatibility

Legacy content uses Scaleform GFx (Flash) for all UI menus. 34 named menus
are accessed via the `UI` script object. ByroRedux will use the Ruffle
project (Rust-native Flash emulator) as a compatibility layer for loading
original `.swf` files. See
[`docs/legacy/creation-engine-ui.md`](../legacy/creation-engine-ui.md).

---

## Milestone Status (verified 2026-10-12)

| Milestone | Subject | Status |
|---|---|---|
| **R5** | Papyrus quest prototype | **Closed 2026-05-16** — verdict *go ECS-native*. Hand-translated `defaultRumbleOnActivate.psc` to `papyrus_demo/`. See [`docs/r5-evaluation.md`](../r5-evaluation.md). |
| **M30 Phase 1** | Papyrus lexer + expression parser | Closed — logos lexer + Pratt parser. |
| **M30.2** | Papyrus statement + item parser (full `.psc` parse) | **Closed 2026-05-23** (`ab0eee96`). All four R5 scripts round-trip with zero recovered errors. Unblocks M47.2. |
| **M47.0** | Event-hooks runtime | **Closed 2026-05-23** (6 phases, `6c51af55..03837739`). `ScriptRegistry`, marker catalog, `script.activate` console command, e2e tests. |
| **M47.1** | Condition eval | **Closed 2026-05-23** (`ea9d0cfa`, `0a835e3e`). CTDA parser + OR-precedence evaluator; the initial 7-function catalog has since grown to 19 modeled variants and multiple quest/scene/package consumers. |
| **M47.2** | Full scripting runtime | **Closed 2026-10-12** — `.pex` decompilation, QUST/SCEN/INFO fragment lowering (58.2% of the 43,818 behavioral Skyrim + FO4 + Starfield fragments), VMAD binding, quest aliases, quest-script variables, saved faction membership, quest-script call inlining, perk entry-point composition, deferred waits/cascades. It intentionally does not promise exhaustive arbitrary-Papyrus execution; the follow-ups are listed in [`m47-2-design.md` §M47.2 closeout](m47-2-design.md). |
| **M43** | Quests & dialogue | **Quest/scene core shipped; broader integration open** — lifecycle, stages, objectives, aliases, fragments and scene phases are live. Story Manager search/payloads and full dialogue/UI integration remain open; see ROADMAP. |

Design doc for the event runtime: [`docs/engine/m47-0-design.md`](m47-0-design.md).
Design doc for the full scripting runtime: [`docs/engine/m47-2-design.md`](m47-2-design.md).
Ground-truth milestone state: always confirm against [ROADMAP.md](../../ROADMAP.md).

---

## References

- [Papyrus Category](https://falloutck.uesp.net/wiki/Category:Papyrus)
- [Papyrus Introduction](https://falloutck.uesp.net/wiki/Papyrus_Introduction)
- [Events Reference](https://falloutck.uesp.net/wiki/Events_Reference)
- [Function Reference](https://falloutck.uesp.net/wiki/Function_Reference)
- [Expression Reference](https://falloutck.uesp.net/wiki/Expression_Reference)
- [Differences from Skyrim to Fallout 4](https://falloutck.uesp.net/wiki/Differences_from_Skyrim_to_Fallout_4)
- [Differences from Previous Scripting](https://falloutck.uesp.net/wiki/Differences_from_Previous_Scripting) (ObScript → Papyrus)
- [Script Objects](https://falloutck.uesp.net/wiki/Category:Script_Objects) (101 script types)
- [Script File Structure](https://falloutck.uesp.net/wiki/Script_File_Structure) (`.psc` grammar)
- [Extending Scripts](https://falloutck.uesp.net/wiki/Extending_Scripts_(Papyrus)) (inheritance rules)
- Internal: [`docs/legacy/papyrus-api-reference.md`](../legacy/papyrus-api-reference.md) (full API surface documentation)
- Internal: [`docs/legacy/creation-engine-ui.md`](../legacy/creation-engine-ui.md) (Scaleform UI system, Ruffle strategy)
- Internal: [`docs/engine/lighting-from-cells.md`](lighting-from-cells.md) (cell-based lighting, probe seeding)
- Internal: [`docs/engine/m47-0-design.md`](m47-0-design.md) (event-hooks runtime design)
- Internal: [`docs/r5-evaluation.md`](../r5-evaluation.md) + [`docs/r5/source/`](../r5/source/) (R5 prototype evaluation + reference `.psc` fixtures)
- Code: [`crates/scripting/`](../../crates/scripting/), [`crates/papyrus/`](../../crates/papyrus/)
- Memory: `papyrus_reference.md`, `papyrus_events_catalog.md`, `scripting_as_ecs.md`
