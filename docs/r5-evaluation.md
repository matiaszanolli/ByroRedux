# R5 — Papyrus quest prototype evaluation

Status: **prototype landed**, verdict below.
Source: [`docs/r5/source/defaultRumbleOnActivate.psc`](r5/source/defaultRumbleOnActivate.psc)
Translation: [`crates/scripting/src/papyrus_demo/mod.rs`](../crates/scripting/src/papyrus_demo/mod.rs)
Tests: [`crates/scripting/src/papyrus_demo/tests.rs`](../crates/scripting/src/papyrus_demo/tests.rs)
ROADMAP gate cleared: Tier 3 / R5 — sequenced first per the 2026-05-03 priority review.

## Verdict

**Go ECS-native.** Continue with the M47.0 / M47.2 plan as roadmapped. No
fallback to a Papyrus stack-VM-as-ECS-system needed for the patterns this
candidate exercises. Concrete data points below.

The bet was: collapse Papyrus's State + latent-wait + cross-subsystem-call
trio into plain ECS components + dt-driven systems with no VM, no fiber, no
suspendable script frames. Verdict from the hand-translation:

- The **complete** `defaultRumbleOnActivate.psc` (50 LOC raw, 31 LOC
  non-comment, ships attached to hundreds of vanilla Skyrim references)
  translates to **135 LOC of Rust** (production code, excluding doc
  comments and blanks) — components + two systems + one resource. The
  doc-comment surface is heavy because every Papyrus construct is
  annotated with its translation rationale; the actual machinery is
  small.
- Every R5-flagged semantic gap has a clean ECS encoding (table below).
- The translation runs against real ECS storages with 9 end-to-end tests
  passing covering all three states + the wait continuation + edge cases.
- No fundamental construct in the script required a runtime interpreter,
  a stack-walking continuation, or a per-script heap allocation.

## What was translated

`defaultRumbleOnActivate.psc` was selected as the R5 candidate from a
14,026-script Skyrim corpus survey (`Skyrim - Misc.bsa` → Champollion v1.3.2
→ all `.pex` decompiled). The selection criteria from the R5 spec required
all three of:

1. Latent `Utility.Wait()` call.
2. Multiple `State` blocks (state-keyed event dispatch).
3. Cross-script / cross-subsystem callback.

`defaultRumbleOnActivate.psc` is **also** maximally representative — it's a
default reusable script Bethesda authored to be attached at edit-time to
every vibrating activator in the game (rumble plates, shrines, ritual
buttons). The script attaches to objects via the ESM's `VMAD` subrecord
with per-REFR property overrides; the pattern recurs widely.

## Semantic translation table

| Papyrus construct | ECS encoding | Where it lives |
|---|---|---|
| `ScriptName X Extends ObjectReference` | A `Component` struct attached to the in-world reference's entity | `RumbleOnActivate` struct |
| `Float Property X = 0.25 Auto` | Public field on the component, defaulted via `Default::default` | `RumbleOnActivate::default` |
| `Auto State active / State busy / State inactive` | Rust enum with one variant per state | `RumbleState` |
| `Self.GotoState("busy")` | `rumble.state = RumbleState::Busy { … }` | `rumble_on_activate_system` |
| `Event OnActivate(actronaut)` per state | Single system that `match`es on `state`; empty-body Papyrus states become no-op match arms | `rumble_on_activate_system` |
| `Utility.wait(duration)` | A `wait_remaining_secs: f32` field inside the `Busy` enum variant | `RumbleState::Busy` |
| post-wait branch (`If repeatable / Else`) | Conditional state transition inside the tick system when the counter hits zero | `rumble_tick_system` |
| `Game.shakeCamera(None, …)` | Marker component (`CameraShakeCommand`) emitted on the player entity; resolved via a `PlayerEntity` resource | `CameraShakeCommand` + `PlayerEntity` |
| `Game.shakeController(…)` | Same shape — `ControllerRumbleCommand` marker | `ControllerRumbleCommand` |

## The structural difference: event handler splits in two

The single load-bearing observation: a Papyrus event handler with a
latent `Utility.wait()` in the middle **splits into two ECS systems** —
the part before the wait (the "immediate" effect: state transition +
side-effects) lives in the event-driven system, and the part after the
wait (the post-wait branching) lives in a dt-driven tick system. The
two communicate via the script component's state field.

```text
Papyrus (one handler, suspends at the wait):
  OnActivate                                  ┐
    emit shake commands                       │
    GoToState("busy")                         │  rumble_on_activate_system (event-driven)
    Utility.wait(duration)   ◀── suspends     ┤
    if repeatable: GoToState("active")        │
    else: GoToState("inactive")               │  rumble_tick_system (dt-driven)
                                              ┘

ECS (split at the would-be suspension point):
  rumble_on_activate_system(world):           ◀── runs on ActivateEvent
    if state == Active:
      emit shake commands
      state = Busy { wait_remaining_secs = duration }

  rumble_tick_system(world, dt):              ◀── runs every frame
    for each Busy { wait_remaining_secs }:
      wait_remaining_secs -= dt
      if wait_remaining_secs <= 0:
        state = if repeatable { Active } else { Inactive }
```

This split is the entire trick. Every Papyrus latent-call follows the
same shape: code-before-call lives in the system that responds to the
event that initiated it; code-after-call lives in whatever system
notices the wait elapsed. The continuation isn't a thing that needs to
"resume" — it's just code that runs when the data says it's time.

That's all the prototype proves, but it's exactly the structural risk R5
was filed to de-risk.

## Other patterns this script covered cleanly

**State debounce.** Papyrus's empty `State busy` / `State inactive`
OnActivate bodies (which swallow re-activations) become no-op match arms
in the dispatch system. The "lock out re-entry while busy" idiom is the
single most common Papyrus pattern; it costs literally nothing in the
ECS shape.

**Property linking.** Papyrus's `Auto Property` (defaulted at script
authoring time, overridable per-REFR via VMAD) maps to `Default`-derived
struct fields with optional VMAD-driven overrides at component-attach
time. No runtime property table needed.

**Compile-time exhaustiveness.** Papyrus's `GoToState("busy")` is a
string lookup — a typo doesn't surface until the matching state would
have fired. The Rust enum makes every transition compiler-checked.

**Per-instance state.** Every reference with the script attached has its
own component instance; no shared state, no global script table. The
`HashMap<EntityId, ScriptState>` that a VM would need is replaced by the
sparse-set storage we already have.

**Cross-subsystem callback model.** `Game.shakeCamera(None, …)` —
Papyrus's call into the engine's input/camera layer — becomes a marker
component on the player entity. The (future) camera-shake system drains
those at end of frame. Per-frame causal ordering is preserved (the
camera knows WHICH script frame emitted which shake), which Papyrus's
queue-based dispatcher does not preserve.

## What this prototype did NOT cover

The R5 spec asked for a candidate exercising latent wait + state + cross-
script call. `defaultRumbleOnActivate.psc` hit all three but only at the
bottom intensity for the third axis — its cross-subsystem call is into a
global engine subsystem (`Game.shakeCamera`), not into a *user-defined
function on another reference's attached script*. The latter is the form
the M47.2 transpiler must also handle, and it has a different shape:

```papyrus
ObjectReference Property OtherDoor Auto
…
(OtherDoor as MyDoorScript).Open()   ; calls user-defined Open() on another reference's script
```

The natural ECS translation is: a system that mutates the target entity's
component directly, or emits an event marker the target's system picks up.
Both patterns are already in the existing `crates/scripting` toolbox
(`ActivateEvent` is exactly this — a marker emitted by one entity's
system, consumed by another's). A second R5 follow-up against the
already-stashed `MG07LabyrinthianDoorScript.psc` (at
`docs/r5/source/MG07LabyrinthianDoorScript.psc`) would close this gap —
it has a `myDoor.activate(actronaut, False)` line that is exactly the
cross-reference-method-call pattern, plus a quest-stage condition
(`MG07.getStageDone(10)`) which is the second outstanding form.

Other patterns left untranslated in this prototype:

| Pattern | Status | Closest ECS shape |
|---|---|---|
| ~~`RegisterForUpdate` / `OnUpdate` / `UnregisterForUpdate`~~ | **Closed (RegisterForUpdate half)** — see [§ Follow-up 2](#follow-up-2--registerforupdate--onupdate-via-dlc2ttr4aplayerscriptpsc). `RegisterForAnimationEvent` untranslated, same substrate shape. | Subscription components + per-event-type dispatch system. |
| `SendModEvent` / `SendCustomEvent` | **Closed as a non-pattern** — see [§ Follow-up 3](#follow-up-3--mg07-cross-reference-call--the-vanilla-customevent-non-finding). Vanilla Bethesda content doesn't use these at all (raw-bytes grep across 21 901 .pex confirms). Pub/sub shape would be a marker on an EventBus entity if a SKSE/mod fixture ever required it. | Broadcast event component on a global "EventBus" entity. |
| ~~`SetStage(N)` / `GetStageDone(N)`~~ | **Closed** — see [§ Follow-up 1](#follow-up-1--setstage--getstagedone-via-da10maindoorscriptpsc). | Quest-stage as an ECS resource keyed by quest FormID. M47.0 surface area. |
| `OnPlayerLoadGame` / save-restoration hooks | Untranslated | Sits with M45 save/load. |
| `if foo as MyScript` — Papyrus's runtime type test on a script-typed property | Untranslated | Rust enums / trait objects depending on how the M47.2 transpiler shapes script types. Not in the candidate. |

None of these look harder than what the prototype already showed. The
M47.2 transpiler will produce the ECS shape for each pattern lazily
(per-script-archetype) so the encoding only needs to exist for patterns
the consumed scripts actually use.

## Numbers from the prototype

- **Source**: 50 LOC Papyrus raw (31 non-comment), 5 properties, 3 states,
  1 latent wait, 2 cross-subsystem calls, 1 event handler.
- **Translation**: 364 LOC of `mod.rs` (135 production, the remainder is
  per-construct docs explaining the Papyrus-to-ECS mapping), 297 LOC of
  tests (200 non-comment) covering 9 distinct semantic scenarios.
- **Compile time**: incremental check on the scripting crate at ~0.4 s.
- **Test runtime**: 9 new tests, 0.0 s combined (all pure-data, no
  Vulkan / no fixtures).
- **Workspace test count**: scripting crate went 8 → 17 (9 new, all
  passing), no regressions elsewhere.

## What this changes downstream

- **M47.0 (event hooks runtime)** stays on the original roadmap shape:
  `OnActivate` / `OnHit` / `OnTriggerEnter` / `OnCellLoad` / `OnEquip`
  as marker components emitted by the engine into per-script systems.
  The hook contract is the marker-component pattern — already shipped
  in `crates/scripting/src/events.rs`.
- **M47.2 (full scripting runtime)** can proceed as a per-script
  transpiler emitting Rust components + systems. Papyrus's AST →
  matching component shape is the consumed pattern (this prototype's
  shape, lifted into a per-script-archetype emitter).
- **R5 is closed.** The fallback path ("Papyrus stack-VM as an ECS
  system") is parked — only re-open if a future translation surfaces
  a Papyrus construct genuinely incompatible with the marker + tick +
  state-enum trio. `defaultRumbleOnActivate.psc` was the canonical
  example; if it had failed the bet, basically nothing in Skyrim's
  authored content would have been salvageable.

## Follow-up 1 — SetStage / GetStageDone via DA10MainDoorScript.psc

After the initial verdict landed, the next pattern down the R5
"untranslated" list was the quest-stage state surface (`SetStage` +
`GetStage` + `GetStageDone`). Re-ran the corpus survey for scripts
exercising both a write (`SetStage`) and a read (`GetStage` /
`GetStageDone`) — selected `DA10MainDoorScript.psc` (13 LOC raw, 6
LOC of actual code) as the canonical example:

```papyrus
ScriptName DA10MainDoorScript Extends ReferenceAlias

Event OnActivate(ObjectReference akActionRef)
  If (Self.GetOwningQuest().GetStageDone(37) == 1) && \
     (Self.GetOwningQuest().GetStageDone(40) == 0)
    Self.GetOwningQuest().SetStage(40)
  EndIf
EndEvent
```

Same shape recurs across dozens of Skyrim quest scripts — the
"activate-this-thing-to-advance-the-quest, gated on prior stage
done and current stage not yet done" idiom. Translated into:

### Runtime store

`crates/scripting/src/quest_stages.rs` (~190 LOC, ~120 production):

- `QuestStageState` — an ECS `Resource`, `HashMap<QuestFormId,
  QuestStageData>`. Lazy entries (quests the player never touched
  don't allocate). Per-quest state is `current_stage: u16` +
  `stages_done: HashSet<u16>`. The set-vs-current distinction is
  load-bearing: a quest at `current_stage = 40` still reports
  `GetStageDone(37) == true`, matching Papyrus's runtime exactly.
- `set_stage(quest, stage) -> u16` — returns the previous
  `current_stage` for callers detecting transitions.
- `get_stage(quest) -> u16` — defaults to `0` for untouched
  quests.
- `get_stage_done(quest, stage) -> bool` — set membership check.
- `reset(quest)` — for restart sequences.
- `QuestStageAdvanced` marker component emitted on every
  advance, ready for the M47.0 fragment dispatcher to consume.

### Generic translation target

`crates/scripting/src/papyrus_demo/quest_advance.rs` (~190 LOC,
~120 production):

```rust
pub struct QuestAdvanceOnActivate {
    pub owning_quest: QuestFormId,
    pub require_done: Vec<u16>,
    pub forbid_done: Vec<u16>,
    pub target_stage: u16,
    pub activator_gate: ActivatorGate,
}
```

This is **the** decision point of the follow-up: the translation
went **generic, not specific**. A specific `DA10MainDoor`
component compiled per Skyrim quest-door script would explode the
component-type count by ~1000×; the generic shape carries the
script's constants as data and reuses one dispatch system. The
`da10_main_door(quest_id)` builder produces the DA10-specific
component preset (`require_done: vec![37]`, `forbid_done: vec![40]`,
`target_stage: 40`, `ActivatorGate::Any`) — equivalent semantics, no
new types.

This is also the shape **M47.2's transpiler will emit naturally**.
The transpiler's job for this pattern family is "detect the
shape, extract the constants, populate the component". Per-script
component types are reserved for the long tail where the
generalization stops paying for itself.

### What this proves

- **State mutation translates to resource writes**, trivially. No
  global mutex, no journal allocation, no per-quest entity. The
  resource shape mirrors Papyrus's mental model 1:1 — `Quest.X` →
  `stage_state.X(quest_id)`.
- **Stage history is set-semantics**, not single-current. The DA10
  predicate `GetStageDone(37) && !GetStageDone(40)` requires
  carrying the full done-set, not just the most-recent stage.
  Papyrus's runtime does the same; the Rust shape matches.
- **Cross-quest isolation is free**. Pure hash-map key separation —
  two quests can never alias state. Papyrus achieves the same via
  per-quest VM stack frames; we get it via map keys.
- **The transpilation pattern compresses well**. One generic
  component + one system covers an entire family of Skyrim's
  quest-gated activation scripts. Per-script specialisation is
  reserved for the genuinely-unique long tail.

### Numbers

- **Source**: 13 LOC Papyrus raw (6 of actual code), 1 event, 2
  stage reads, 1 stage write.
- **Translation**: 190 LOC `quest_advance.rs` (~120 production +
  docs), 286 LOC tests (8 distinct semantic scenarios — predicate
  gating ×3, activator gate ×2, no-precondition tail, cross-quest
  isolation, same-frame collision).
- **Runtime store**: 190 LOC `quest_stages.rs` (8 unit tests for
  the resource itself — covering history retention, idempotency,
  backwards-advance, per-quest reset).
- **Workspace test count**: scripting crate went 17 → 33 (16 new,
  all passing).

### Outstanding bits still untranslated

- **`OnStageSet` event handlers**. The marker emission is in
  place ([`QuestStageAdvanced`]); the dispatch loop that runs
  fragment-script systems on advance is M47.0.
- **`SetObjectiveDisplayed` / `SetObjectiveCompleted` /
  `SetObjectiveFailed`**. Parallel objectives state. Same shape
  as stages — drops in next to `QuestStageState` when a journal
  UI consumer exists to read it.
- **`Quest.Start()` / `Quest.Stop()` / `Quest.IsRunning()`**.
  Quest lifecycle (separate from stage state). Trivial — adds a
  `is_running: bool` to `QuestStageData`. Deferred until a
  consumer requires it.

None of these change the verdict.

[`QuestStageAdvanced`]: ../crates/scripting/src/quest_stages.rs

## Follow-up 2 — RegisterForUpdate / OnUpdate via DLC2TTR4aPlayerScript.psc

After the SetStage half closed (§ Follow-up 1), the next pattern up
the original "untranslated" table was `RegisterForUpdate` — the
periodic-timer subscription system. Surveyed the corpus for
scripts hitting `RegisterForUpdate(N)` + `UnregisterForUpdate()` +
an `Event OnUpdate()` body. Selected `DLC2TTR4aPlayerScript.psc`
(23 LOC raw, 13 of actual code, from Dragonborn DLC) as the
canonical example. It hits the full lifecycle in isolation:

```papyrus
ScriptName DLC2TTR4aPlayerScript Extends ReferenceAlias

Quest Property DLC2TTR4a Auto

Event OnInit()
  Self.RegisterForUpdate(5 as Float)
EndEvent

Event OnUpdate()
  If Game.GetPlayer().GetActorValue("Variable05") > 0
    DLC2TTR4a.SetStage(200)
    Self.UnregisterForUpdate()
  EndIf
EndEvent
```

The "register on init, poll periodically, fire once on threshold
cross, self-cancel" idiom is one of the two ways Papyrus does
long-running observation (the other is `RegisterForCustomEvent`,
pub/sub — separate work).

### Reusable substrate: `RecurringUpdate` + `OnUpdateEvent`

The novel piece is `crates/scripting/src/recurring_update.rs` (~155
LOC production + 175 LOC tests):

- **`RecurringUpdate { interval_secs, seconds_until_next }`** — the
  subscription IS the component. Insert to register, remove to
  cancel. The Papyrus runtime's subscription table becomes the
  sparse-set storage; no separate registry.
- **`OnUpdateEvent`** — transient marker emitted by the tick
  system when an interval elapses. Per-script handlers query for
  `(RecurringUpdate, OnUpdateEvent, MyScript)` and run their body.
- **`recurring_update_tick_system(world, dt)`** — drives all
  subscriptions on the same dt. Cumulative-overshoot handling:
  re-arms via `seconds_until_next += interval_secs` so a long
  frame doesn't lose phase. Missed fires drop (Papyrus's
  documented behaviour for stalled scripts).
- **Lifecycle parity with Papyrus**:
    - `RegisterForUpdate(N)` doesn't fire immediately — first
      OnUpdate is N seconds out. Pinned.
    - `UnregisterForUpdate()` is observable in the next tick (no
      late-fire on a removed subscription). Pinned.
    - Handler-internal unsubscribe (the DLC2TTR4a "self-terminate"
      idiom) works without races. Pinned.

The substrate is RECURRENT-USE: every Papyrus script that uses
`RegisterForUpdate` will subscribe via this same component +
events. The DLC2TTR4a translation is the first consumer; future
M47.2 transpiler emissions reuse the same primitives.

### Per-script translation: `Dlc2Ttr4aPlayerScript`

The script itself goes per-script, not generic. Reasoning in the
new module's doc, summary version:

- The constants ("Variable05", `> 0.0`, `SetStage(200)`) don't
  match a recurring catalogue shape — other `RegisterForUpdate`
  scripts poll different stats with different comparisons and
  fire different side-effects.
- A generic component would have ~6 fields covering a
  small-and-varied surface; per-script is structurally cheaper
  given the substrate already abstracts the timing.
- The M47.2 transpiler can emit per-script components like this
  one from the AST trivially. The transpiler's two-track design:
  pattern-match common shapes into a small catalogue
  (`QuestAdvanceOnActivate`, `RumbleOnActivate`) for ~70% of
  scripts; emit per-script fall-throughs for the long tail.

The per-script artifacts: 233 LOC across
`papyrus_demo/dlc2_ttr4a.rs` + a 50-LOC `actor_stats.rs` stand-in
for `GetActorValue` (the full ActorValue system is M47.1; the
stub is enough for the demo).

### What this proves

- **Subscription IS the component.** Papyrus's
  `RegisterForUpdate(N)` doesn't need a parallel registry —
  insertion into the ECS storage replaces both. The lookup-time
  cost is O(1), same as Papyrus's hash-keyed table.
- **Self-cancel during handler is structurally safe.** The two-
  phase collect-then-apply pattern (already used in the rumble +
  SetStage demos) handles "handler removes its own subscription"
  cleanly. The tick system has already finished by the time the
  handler runs; the next tick observes the cancellation.
- **Cross-entity stat reads compose cleanly.** Papyrus's
  `Game.GetPlayer().GetActorValue("Variable05")` becomes a
  resource-resolve plus a component read — three lookups, no
  serialization, no proxy objects, no method-dispatch boxing.
- **Missed-fire policy matches Papyrus by construction.** The
  cumulative-overshoot tick logic emits one fire per tick
  regardless of dt magnitude. Long-frame stalls don't burst-fire.
- **Per-script vs generic emission strategies coexist.** The
  transpiler design has both lanes; the DLC2TTR4a translation is
  the first per-script demonstrator. Same crate, same patterns,
  different emission decision based on whether the pattern fits
  the catalogue.

### Numbers

- **Source**: 23 LOC Papyrus raw, 13 of actual code, 2 properties
  (one Quest), 2 event handlers (`OnInit` + `OnUpdate`), 1
  `RegisterForUpdate`, 1 `UnregisterForUpdate`, 1 cross-entity
  read (`GetActorValue`), 1 `SetStage` write.
- **Reusable substrate**: 155 LOC `recurring_update.rs` (~100
  production + docs), 175 LOC substrate tests (9 distinct
  scenarios pinning every lifecycle edge case).
- **Per-script translation**: 233 LOC `dlc2_ttr4a.rs` (~140
  production + docs), 246 LOC script-translation tests (10
  scenarios covering OnInit idempotency, polling below/above
  threshold, self-cancel, post-cancel quietness, cross-entity
  isolation, missing-stat default).
- **ActorStats stand-in**: 50 LOC, single component with
  `get`/`set` and a `register` helper.
- **Workspace test count**: scripting crate went 33 → 52 (19 new,
  all passing).

### Outstanding bits still untranslated

- **`RegisterForAnimationEvent`** — same substrate shape
  (subscription component + dispatch system) but the events come
  from animation text-keys rather than a dt counter. The
  scripting crate already has `AnimationTextKeyEvents` (from
  M30); wiring it through a per-animation-event subscriber lands
  with M47.0.
- **`RegisterForCustomEvent` / `SendCustomEvent`** — pub/sub
  pattern. Different shape: a global event bus with named events.
  Conceptually a `EventBusSubscription { event_name, target }`
  component plus a broadcast-dispatch system. Untranslated.
- **`OnAnimationEvent(akSource, asEventName)`** event handler —
  consumes the substrate above. Untranslated.
- **`Quest.Start()` / `Quest.Stop()` / `Quest.IsRunning()`** —
  lifecycle separate from stage state. Trivially extends
  `QuestStageState` with a `is_running: bool`; deferred until a
  consumer needs it.
- **The full `Actor.GetActorValue` / ModActorValue surface** — the
  prototype's stub is read-only and string-keyed; the production
  shape (M47.1) plumbs through AVIF records + perk-modifier
  composition.

None change the verdict. The bet holds at three of four pattern
axes (latent wait, state machine, cross-subsystem call, periodic
timer); the fourth (pub/sub via custom events) is the only
remaining structural unknown.

## Follow-up 3 — MG07 cross-reference call + the vanilla-CustomEvent non-finding

Two outcomes from one investigation. First, the corpus survey for
a pub/sub fixture returned an unexpected null result: **vanilla
Bethesda content doesn't use `CustomEvent` or `ModEvent` at all**.
Then the work re-scoped to the original R5 third-axis criterion
(cross-reference method call) — translation landed via
`MG07LabyrinthianDoorScript.psc`.

### The vanilla-CustomEvent non-finding

The R5 spec asked for one quest exercising "a cross-script
callback". After the first three demos closed the latent-wait /
state-machine / SetStage / RegisterForUpdate axes, the obvious
next pattern was the Papyrus `RegisterForCustomEvent` / `SendCustomEvent`
pub/sub mechanism — the runtime feature most often documented as
"this is the way to do cross-script messaging" in modder docs.

Surveyed the corpus:

```
Skyrim SE base + DLC (14 026 .pex files):
  CustomEvent          declarations: 0
  SendCustomEvent      call sites:    0
  RegisterForCustomEvent call sites:  0
  UnregisterForCustomEvent call sites: 0
  SendModEvent         call sites:    0
  RegisterForModEvent  call sites:    0
  UnregisterForModEvent call sites:   0

Fallout 4 base (7 875 .pex from Fallout4 - Misc.ba2):
  (same patterns — zero results across the board)
```

Verified via raw-bytes `grep` against the .pex bytecode directly
(not via Champollion) to rule out decompiler artifacts. **Zero
hits**. Bethesda ships these mechanisms in the runtime stdlib
(`Form.psc` carries the signatures, the .pex linker resolves
them) but **does not use them in any shipped quest script**. The
shipped patterns for cross-script messaging are:

1. **Direct method calls on typed `ObjectReference Property`-held
   targets** — e.g., `myDoor.activate(actronaut, False)` from
   MG07 below. Bethesda's go-to pattern for "this script tells
   that reference to do something."
2. **Quest stage as a global rendezvous** — script A writes
   `Quest.SetStage(N)`, script B polls `Quest.GetStageDone(N)`.
   Documented and validated in [Follow-up 1](#follow-up-1--setstage--getstagedone-via-da10maindoorscriptpsc).
3. **Quest fragment scripts** — engine fires per-stage scripts
   via the QSDT fragment system on stage advance. M47.0 surface.
4. **`StorageUtil` / `JContainers`** — SKSE-extender APIs that
   add real K/V storage. NOT a Bethesda API and outside R5 scope.

So the pub/sub axis isn't validated against shipped content
because shipped content doesn't exercise it. The implication for
M47.2:

- The transpiler's CustomEvent / ModEvent emission can be **stubbed
  with a panic / log-and-skip** for the initial release. No
  vanilla content exercises it; mod content that does is targeting
  an SKSE-dependent runtime anyway.
- The pub/sub ECS shape (a global `EventBus` entity carrying
  named markers + per-subscriber components) is sound on paper
  but unvalidated against real bytecode. The non-finding becomes
  the finding.

The `r5_extract_pex_ba2.rs` BA2-side extractor + the FO4 BSA
walk both stay in the repo — useful for future M47.2 corpus
surveys, not just R5.

### The MG07 cross-reference call demo

The original R5 spec's third axis ("cross-script callback")
turned out to be best validated by the same fixture stashed
alongside the rumble demo months ago:
`MG07LabyrinthianDoorScript.psc` (47 LOC raw, 23 of actual
code, the "Hall of the Vigilant of Stendarr" lockout door in
Labyrinthian — the door the player needs the Saarthal Amulet
keystone to open during the College of Winterhold mid-game
quest). The script ends its successful-activation branch with

```papyrus
myDoor.activate(actronaut, False)
```

— Papyrus's "tell another reference to do something." The
target's `myDoor` property is a typed `ObjectReference` —
resolved at edit-time to a placed REFR in the same cell.

The ECS translation is the load-bearing finding of this
follow-up:

> **A Papyrus cross-reference method call lowers to a marker
> component insert on the target entity's storage.** The same
> `ActivateEvent` the engine emits when the player presses E on
> a door is what one script emits when it tells another door to
> activate. The cross-script boundary collapses to the same
> surface as player input. No proxy object, no vtable, no
> message-dispatch boxing.

Concretely:

```rust
// Papyrus: myDoor.activate(actronaut, False)
// ECS:
events.insert(door.my_door, ActivateEvent { activator: player });
```

That's it. The target's own OnActivate-handling systems pick the
event up next frame — they don't know (and don't need to know)
whether the activation came from the player, an NPC, or another
script.

### What MG07 incidentally re-covers

- `OnLoad` lifecycle event (the script's first-frame setup
  hook). Maps to an `Uninitialized → Waiting` transition system
  that runs once per component. Cell-streaming makes this
  re-fire on re-load — matches Papyrus.
- `Self.GotoState("waiting")` / `Self.GotoState("inactive")`
  state-machine transitions. Same Rust enum shape as the rumble
  demo.
- `Utility.wait(delayAfterInsert)` latent wait. Same
  `wait_remaining_secs: f32` inside-state-variant shape.
- `Quest.GetStageDone(10)` predicate. Reuses the
  `QuestStageState` resource from Follow-up 1.
- Persistent script-instance state (`Bool beenOpened`). Just a
  field on the script component.

### What's deliberately stubbed

The script touches more engine surface than the prototype can
wire up in one pass; minimal stubs cover the bits:

| Papyrus call | ECS stub |
|---|---|
| `Game.GetPlayer().GetItemCount(MG07Keystone)` | `KeystoneInventory` boolean on player |
| `Game.GetPlayer().RemoveItem(MG07Keystone, …)` | flip the boolean off in-line |
| `Self.blockActivation(True)` | `activation_blocked: bool` field, enforced explicitly in the OnActivate system |
| `Self.disable(False)` | `disabled: bool` field, observed post-wait |
| `Self.playAnimationAndWait("Insert", "Done")` | collapsed into `Utility.wait(delayAfterInsert)`'s counter — both are latent waits at the script-semantic level, the animation duration is engine surface not in scope for R5 |
| `dunLabyrinthianDenialMSG.show(...)` | `UiMessageCommand { message_form_id }` marker on player |

None of the stubs change the load-bearing observation about
cross-reference activation — that translates fully with the
already-built infrastructure.

### The faithful-to-source bug pin

The Papyrus source has a typo: at the equivalent of line 28 it
writes `beenOpened == False` (comparison) where the author plainly
meant `beenOpened = False` (assignment). The Papyrus compiler
accepts both — the comparison-form compiles as a discarded
expression, a silent no-op. Vanilla Skyrim ships this bug; the
post-open lockout behaviour relies on `Self.disable()` (which
DOES fire) rather than the `beenOpened` flag (which never flips).

The translation **preserves** the bug. R5's contract is faithful
translation; if M47.2 silently "fixes" typos it would diverge
from shipped behaviour for the (possibly intentional, possibly
accidental) cases where the bug carries the load. Pinned by the
`beenopened_is_never_flipped_due_to_source_typo` test.

### Numbers

- **Source**: 47 LOC raw, 23 of actual code, 5 properties, 1
  persistent `Bool`, 2 explicit named states (+ implicit default),
  1 `OnLoad` handler, 1 `OnActivate` handler, 2 latent waits
  (collapsed), 1 cross-reference method call, 1 UI message,
  multiple Self-state mutations.
- **Translation**: 380 LOC `mg07_door.rs` (~200 production + docs),
  315 LOC tests (12 distinct semantic scenarios — OnLoad branches
  ×3, BlockActivation enforcement, denial branches ×3, success
  path ×2, mid-wait + post-wait gates ×2, faithful-bug pin).
- **Workspace test count**: scripting crate went 52 → 64 (12
  new, all passing).

### R5 closing tally

All four pattern axes from the original spec are now validated:

| Axis | Fixture | Status |
|---|---|---|
| Latent wait | `defaultRumbleOnActivate.psc` | Closed (verdict) |
| State machine | `defaultRumbleOnActivate.psc` | Closed (verdict) |
| Cross-subsystem call | `defaultRumbleOnActivate.psc` (Game.shake*) + `MG07` (UI message) | Closed |
| Cross-reference method call | `MG07LabyrinthianDoorScript.psc` (`myDoor.activate(...)`) | Closed (this follow-up) |
| SetStage / GetStageDone | `DA10MainDoorScript.psc` | Closed (Follow-up 1) |
| RegisterForUpdate / OnUpdate | `DLC2TTR4aPlayerScript.psc` | Closed (Follow-up 2) |
| Custom-event pub/sub | (no vanilla fixture exists) | Non-pattern — Bethesda doesn't use it |

**Verdict unchanged: go ECS-native.** Three follow-ups in, the
bet has held against every pattern Bethesda actually uses in
shipped content. The one structural unknown (pub/sub) turned out
to be a non-pattern. M47.0 and M47.2 proceed as roadmapped, with
the substrate `crates/scripting` has accumulated:

- `ActivateEvent` / `HitEvent` / `TimerExpired` / `AnimationTextKeyEvents` (M30 baseline)
- `ScriptTimer` (one-shot)
- `RecurringUpdate` + `OnUpdateEvent` (periodic — Follow-up 2)
- `QuestStageState` + `QuestStageAdvanced` (Follow-up 1)
- `CameraShakeCommand` / `ControllerRumbleCommand` (cross-subsystem boundaries)
- `UiMessageCommand` (Follow-up 3 — Message dispatch)
- `event_cleanup_system` (canonical end-of-frame marker sweep)
- `PlayerEntity` resource (the `Game.GetPlayer()` resolver)

— covers the entire script-system substrate Bethesda quest
content needs. Adding new marker types for future-discovered
patterns is the same shape every time (Component impl + register
+ optionally drain in cleanup).

## Replay this evaluation

```bash
# Run all the translation tests (initial demo + the SetStage
# follow-up + the quest_stages resource).
cargo test -p byroredux-scripting

# Inspect the sources side-by-side with their translations:
cat docs/r5/source/defaultRumbleOnActivate.psc      # rumble demo
cat crates/scripting/src/papyrus_demo/mod.rs

cat docs/r5/source/DA10MainDoorScript.psc           # SetStage demo
cat crates/scripting/src/papyrus_demo/quest_advance.rs
cat crates/scripting/src/quest_stages.rs

cat docs/r5/source/DLC2TTR4aPlayerScript.psc        # RegisterForUpdate demo
cat crates/scripting/src/papyrus_demo/dlc2_ttr4a.rs
cat crates/scripting/src/recurring_update.rs

cat docs/r5/source/MG07LabyrinthianDoorScript.psc   # cross-reference call demo
cat crates/scripting/src/papyrus_demo/mg07_door.rs

# Decompile additional candidates from the Skyrim BSA (requires wine +
# Champollion v1.3.2 at ~/.tools/Champollion.exe; the BSA extraction
# tool was a one-shot scratch — write a fresh examples/r5_extract_pex.rs
# if you need it again).
WINEDEBUG=-all wine ~/.tools/Champollion.exe -r -t -p Z:\\tmp\\psc Z:\\tmp\\pex
```
