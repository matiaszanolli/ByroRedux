# Scripting Architecture: From Papyrus VM to ECS-Native

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
owns all execution. No VM, no stacks, no separate threads.

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

### Expression Grammar

Standard operator precedence:
```
|| → && → comparison → +/- → */% → unary(-,!) → cast(as) → dot(.) → array([]) → atoms
```

No bitwise operators. Cast (`as`) is runtime downcasting. Dot chaining
(`MyFunc().Prop[0]`) is the fluent API modders expect.

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

## The 136 Events — Categorized by ECS Pattern

Every Papyrus event maps to one of three ECS mechanisms. No event queue, no VM
dispatch table, no registered callback lists.

### Mechanism 1: Marker Components (interaction events)

A transient component is added to the entity by an engine system. Script systems
process it and remove it. Covers ~50 events.

| Papyrus Event | Marker Component | Added By |
|---|---|---|
| `OnActivate(akActionRef)` | `ActivateEvent { activator: EntityId }` | Activation system |
| `OnHit(akAggressor, akWeapon, akProjectile)` | `HitEvent { aggressor, weapon, projectile }` | Combat system |
| `OnTriggerEnter(akRef)` / `OnTriggerLeave` | `TriggerEvent { entity, entered: bool }` | Physics system |
| `OnEquipped` / `OnUnequipped` | `EquipEvent { item: FormId, equipped: bool }` | Inventory system |
| `OnItemAdded` / `OnItemRemoved` | `InventoryEvent { item, count, source }` | Container system |
| `OnContainerChanged` | `ContainerChangeEvent { ... }` | Container system |
| `OnGrab` / `OnRelease` | `GrabEvent { grabbed: bool }` | Interaction system |
| `OnWorkshopObjectPlaced` / etc. | `WorkshopEvent { kind: WorkshopEventKind }` | Workshop system |

The marker component pattern is zero-cost for entities that don't participate —
no registration needed. If an entity doesn't have a script system watching for
`HitEvent`, the component is never added in the first place (the combat system
checks for the presence of a `ScriptInstance` component before adding markers).

### Mechanism 2: State Field Watches (state change events)

Systems compare current vs previous frame state on existing components. When a
field transitions, the associated logic fires. Covers ~40 events.

| Papyrus Event | Watched Component | Field Transition |
|---|---|---|
| `OnDeath` / `OnDying` | `ActorState` | `health > 0` → `health <= 0` |
| `OnCombatStateChanged` | `ActorCombat` | `state` field changes |
| `OnLoad` / `OnUnload` | `CellPresence` | `loaded: false` → `true` |
| `OnOpen` / `OnClose` | `DoorState` | `open` field changes |
| `OnLockStateChanged` | `LockState` | `locked` field changes |
| `OnPowerOn` / `OnPowerOff` | `PowerState` | `powered` field changes |
| `OnPackageStart` / `OnPackageEnd` | `ActorPackages` | `active_package` changes |
| `OnStageSet` (Quest) | `QuestState` | `stage` field changes |
| `OnLocationChange` | `ActorLocation` | `current_location` changes |

Implementation: each watchable component stores a `prev_` shadow field or the
system keeps a parallel `PreviousState<T>` component. The diff system runs
early in the frame, before script systems.

### Mechanism 3: Timer/Condition Components (scheduled events)

Components with countdown or condition data, checked by systems each tick.
Covers ~15 events.

| Papyrus Event | Component | System Behavior |
|---|---|---|
| `OnTimer(aiTimerID)` | `ScriptTimer { id, remaining, game_time }` | Tick down, fire when zero |
| `OnTimerGameTime` | `ScriptTimer` (game_time variant) | Tick by game hours |
| `OnDistanceLessThan` / `GreaterThan` | `DistanceWatch { target, threshold }` | Check distance each frame |
| `OnGainLOS` / `OnLostLOS` | `LOSWatch { target, had_los }` | Raycast check |
| `OnTranslationComplete` | `TranslationMove { target_pos, speed }` | Movement system, fire on arrival |

### Remote Events (cross-entity listening)

Papyrus (Fallout 4+): `RegisterForRemoteEvent(akSender, "OnDeath")` — script
on entity A receives events from entity B.

ECS: entity A has a `RemoteWatch { target: EntityId, event: EventKind }`
component. The event system queries both the source entity (for the marker
component) and all watchers (for `RemoteWatch` pointing at the source). No
registration list — the query is the registration.

### Custom Events (user-defined broadcast)

Papyrus: `CustomEvent MyEvent` + `SendCustomEvent("MyEvent", args)`.

ECS: `CustomEvent { name: FixedString, sender: EntityId, args: Vec<ScriptValue> }`
marker component added to interested entities. Any system watching for that
event name processes it. The `SendCustomEvent` equivalent just adds the marker
component to all entities that have registered interest (via a
`CustomEventWatch { name: FixedString, source: EntityId }` component).

---

## ECS Mapping: Papyrus Concepts → ByroRedux

### Properties → Component Fields

Papyrus:
```papyrus
Quest Property MyQuest Auto
Int Property StageToSet Auto
Bool Property IsEnabled Auto Const
Actor Property TargetActor Auto Mandatory
```

ECS:
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
ByroRedux, the plugin manifest declares per-instance overrides as component
field values in the record's component bundle. The `Record::spawn()` method
inserts them into the world.

### Events → System Scheduling

Papyrus:
```papyrus
Event OnActivate(ObjectReference akActionRef)
    if akActionRef == Game.GetPlayer()
        MyQuest.SetStage(StageToSet)
    endif
EndEvent
```

ECS:
```rust
fn activator_on_activate(world: &World, _dt: f32) {
    // Query entities that have both our script component and an activation event
    let (scripts, mut events) = world
        .query_2_mut::<MyActivatorScript, ActivateEvent>()
        .unwrap_or_return();

    for (entity, event) in events.iter() {
        if let Some(script) = scripts.get(entity) {
            if event.activator == player_entity(world) {
                set_quest_stage(world, script.my_quest, script.stage_to_set);
            }
        }
        events.remove(entity); // consume the event
    }
}
```

No VM dispatch. The system runs in the ECS scheduler like any other system.
The `ActivateEvent` marker component is the event — when it's present, the
system processes it. When it's absent, the system is effectively a no-op
(empty query).

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

ECS:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PullChainState { AtRest, Busy }

pub struct PullChainScript {
    pub state: PullChainState,
    pub animation: FixedString,
}

// System checks state before processing
fn pull_chain_system(world: &World, _dt: f32) {
    let (scripts, events) = ...;
    for (entity, _event) in events.iter() {
        if let Some(script) = scripts.get_mut(entity) {
            match script.state {
                PullChainState::AtRest => {
                    script.state = PullChainState::Busy;
                    play_animation(world, entity, script.animation);
                    // Animation completion system will set state back to AtRest
                }
                PullChainState::Busy => {} // ignore
            }
        }
    }
}
```

The state is visible data. Any system can read it. No hidden VM state machine.

### Fragments → Triggered Systems

Papyrus fragments are inline script snippets that run at specific moments:
quest stage set, topic info played, package started, scene phase begun.

ECS: systems that watch for the corresponding state changes. A quest stage
fragment becomes a system that queries `QuestState` and fires when the stage
field matches.

```rust
fn quest_stage_0010(world: &World, _dt: f32) {
    let quests = world.query::<QuestState>().unwrap_or_return();
    for (entity, quest) in quests.iter() {
        if quest.form_id == MY_QUEST_ID && quest.stage == 10 && !quest.stage_processed[10] {
            // Fragment logic here
            quest.stage_processed[10] = true;
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
fn read_bounties(world: &World, _dt: f32) {
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
| Replaced `OnUpdate` with per-script timers | Global per-frame callback too expensive | `ScriptTimer` component, ticked by system |
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
eventually corrupting the save.

### ECS Save Solution

Only component data is serialized. There are no stacks, no suspended frames,
no registration lists.

- **Properties** → component fields → serialized as regular data
- **Const properties** → skipped (value comes from the plugin, not the save)
- **Timers** → `ScriptTimer` component → serialized as a remaining-time float
- **States** → enum field on component → serialized as an integer
- **Event registrations** → `RemoteWatch`/`CustomEventWatch` components →
  serialized as entity + event references

Removing a mod removes its components from all entities. No orphaned state.
Adding a mod adds new components to entities that match its records. No
compatibility issues.

---

## Legacy Script Compatibility

For loading existing game content, ByroRedux needs to understand Papyrus
scripts from ESM/ESP files. Two approaches, not mutually exclusive:

### Approach 1: Record-Level Mapping (Phase D)

The NIF parser and record loader extract the *effect* of scripts (what
properties are set, what stages they respond to) and create equivalent
components. This doesn't execute Papyrus — it interprets the configured state.

### Approach 2: Papyrus Transpiler (stretch goal)

Parse `.psc` source files and transpile them to Rust component definitions +
system functions. The grammar is simple enough (standard operator precedence,
no complex control flow) that a transpiler is feasible. This would allow
running legacy scripts from mods without manual porting.

---

## References

- [Papyrus Category](https://falloutck.uesp.net/wiki/Category:Papyrus)
- [Papyrus Introduction](https://falloutck.uesp.net/wiki/Papyrus_Introduction)
- [Events Reference](https://falloutck.uesp.net/wiki/Events_Reference)
- [Function Reference](https://falloutck.uesp.net/wiki/Function_Reference)
- [Expression Reference](https://falloutck.uesp.net/wiki/Expression_Reference)
- [Differences from Skyrim to Fallout 4](https://falloutck.uesp.net/wiki/Differences_from_Skyrim_to_Fallout_4)
- Internal: `docs/engine/lighting-from-cells.md` (cell-based lighting, probe seeding)
- Memory: `papyrus_reference.md`, `papyrus_events_catalog.md`, `scripting_as_ecs.md`
