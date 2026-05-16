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
| `RegisterForUpdate` / `RegisterForAnimationEvent` | Untranslated | Subscription components + per-event-type dispatch system. Not in the candidate. |
| `SendModEvent` / `SendCustomEvent` | Untranslated | Broadcast event component on a global "EventBus" entity. Not in the candidate. |
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

# Decompile additional candidates from the Skyrim BSA (requires wine +
# Champollion v1.3.2 at ~/.tools/Champollion.exe; the BSA extraction
# tool was a one-shot scratch — write a fresh examples/r5_extract_pex.rs
# if you need it again).
WINEDEBUG=-all wine ~/.tools/Champollion.exe -r -t -p Z:\\tmp\\psc Z:\\tmp\\pex
```
