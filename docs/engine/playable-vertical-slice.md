# Playable Vertical Slice

**Status:** active execution plan (started 2026-08-09)

This plan defines the shortest route from “loads and renders Bethesda content”
to “can be played as a game.” It is intentionally narrower than full engine
parity: one curated Skyrim SE route must work without debug-console assistance
before compatibility breadth or further renderer polish can take priority.

## Mid-term outcome

The reference slice starts in a Skyrim SE interior, lets the player walk to and
activate a door, transitions to the exterior, completes one small authored
objective involving an NPC or activator and one hostile encounter, changes
inventory/equipment, saves, exits, reloads, and continues from the restored
state.

The slice is “playable” only when all of these gates hold:

1. Normal keyboard/mouse input is sufficient after launch; `byro-dbg` is not
   required to move, interact, fight, navigate UI, or save/load.
2. Character movement, collision, camera, activation, and interior/exterior
   transitions survive a 30-minute session without a soft lock or falling out
   of the world.
3. One authored quest/objective path advances from world actions and presents
   enough dialogue/objective feedback for the player to understand the next
   action.
4. One combat loop supports attack, hit, health, death, and loot. One weapon
   family is sufficient for the gate; breadth follows after the slice closes.
5. Inventory/equipment changes, quest state, world-reference state, player
   pose, and current cell survive save → process exit → reload.
6. The reference path has an automated smoke script plus a written manual
   visual/input checklist. Vulkan validation remains clean in a debug run.

## Execution order

### P0 — Input and world interaction

Goal: the player can discover and activate one canonical target without a
console command.

- Keep physical device state at the platform edge; expose stable gameplay
  actions with held/pressed/released semantics and runtime bindings.
- Select exactly one camera-forward interactable within a bounded reach.
- Emit canonical `ActivateEvent` markers so package, script, and player-driven
  activation share one consumer path.
- Route XTEL doors through the existing deferred cell-transition orchestrator.
- Present a minimal native `[E] Open/Activate` HUD prompt.
- Add occlusion and collider-to-reference resolution once the first real-data
  smoke identifies the required collision ownership mapping.

**Current state (2026-08-09):** keyboard action edges, E-key target selection,
script activation, shared door queueing, and the HUD prompt are implemented and
unit-tested. A Skyrim interior→exterior real-data smoke is the remaining P0
closure gate.

### P1 — Reliable character control

Goal: walking around the reference route is boringly reliable.

- Migrate movement/jump/sprint from raw `KeyCode` checks to the action layer.
- Add mouse-button and gamepad physical sources without changing gameplay
  consumers.
- Pin character spawn, floor recovery, slopes, stairs, door thresholds, and
  cell-transition placement in the reference interior/exterior pair.
- Add pause/input-focus semantics so native UI, Scaleform, and gameplay never
  process the same input event.
- Record a deterministic traversal smoke: spawn → walk route → cross door →
  cross exterior cell boundary → return.

### P2 — Minimal combat and actor response

Goal: one hostile encounter has a complete cause-and-effect loop.

- Add canonical Attack/Block action consumers and weapon timing state.
- Resolve camera/weapon traces to ECS entities and emit the existing `HitEvent`.
- Apply damage through `ActorValues`; drive stagger/death state and disable AI
  participation on death.
- Play one attack/hit/death animation family and spatial sound family.
- Make a dead actor lootable and persist its dead/looted state.

Defer weapon-family breadth, advanced perks, dismemberment, and generalized
behavior-graph parity until the one-family closure gate passes.

### P3 — Inventory and native game UI

Goal: the player can understand and change game state without diagnostics.

- Add native HUD bars, crosshair/target prompt, notifications, and objective
  text as presentation consumers of canonical ECS state.
- Add container/corpse/pickup interaction and an inventory screen.
- Wire equip/unequip through the existing `Inventory`, `EquipmentSlots`, and
  mesh attachment pipeline.
- Add pause/menu input routing and settings-backed key rebinding.
- Preserve Scaleform as a compatibility frontend; native UI is the reference
  slice's reliable path.

### P4 — Authored objective and dialogue loop

Goal: a small piece of shipping content can be followed and completed.

- Choose and freeze one Skyrim quest/objective fixture whose required PEX,
  QUST, SCEN, DIAL/INFO, PACK, and alias shapes are already mostly covered.
- Connect NPC activation to dialogue selection/presentation.
- Surface objective start/update/complete feedback in native UI.
- Add only the recognizers/condition functions/effects found missing by this
  fixture; do not broaden the catalog speculatively.
- Turn the route into a repeatable smoke with observable quest-stage and
  presentation assertions.

### P5 — Persistence and session hardening

Goal: the complete slice survives ordinary play behavior.

- Extend change-form save coverage only for mutable state introduced by
  P0–P4; keep caches, bindings, targeting, GPU handles, and transient events
  explicitly re-derived.
- Test save/reload before and after door transitions, combat, looting,
  equipment changes, and objective completion.
- Run the 30-minute soak with repeated transitions and saves; fail on panic,
  stuck transition, unbounded memory growth, or lost player control.
- Establish a release-build frame-time and memory baseline for the reference
  route. Optimize only measured blockers to the playability gate.

## Work rules

- Capability beats visual polish until P5 closes, unless a rendering defect
  prevents the player from reading or completing the reference route.
- Every player action enters through a canonical action/event boundary; debug
  commands are alternate frontends, never separate implementations.
- Every phase closes with a real-data smoke. Unit tests protect algorithms and
  lifecycle contracts but do not substitute for a playable route.
- Prefer one deep, observable content fixture over broad partial support. Once
  the slice closes, add a second game route to expose false Skyrim-specific
  assumptions.

## Immediate queue

1. Add the P0 Skyrim door-interaction smoke and verify prompt → E →
   `ActivateEvent` → transition on real data.
2. Resolve line-of-sight occlusion/collider ownership found by that smoke.
3. Migrate movement/jump/sprint consumers to `ActionState` and add input-focus
   regression tests.
4. Freeze the P2 hostile/weapon fixture and trace its existing actor, animation,
   equipment, and physics surfaces before implementing combat.

