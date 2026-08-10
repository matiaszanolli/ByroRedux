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

**Closed 2026-08-10:** [`p0-door-interaction.sh`](../smoke-tests/p0-door-interaction.sh)
passes the production Bannered Mare exit route: camera-forward XTEL target →
native `[E] Open` prompt → one bound E-key edge → canonical `ActivateEvent` →
deferred arrival in `WhiterunWorld (6,-2)`. The smoke exposed one real-data
lookup gap: exterior destination doors stored in worldspace persistent CELLs
were absent from `cell_for_refr_form_id`; persistent references now map to
their authored exterior grid, including floor-correct negative coordinates.
The existing `PhysicsSourceForm` collider ownership path passed the fixture's
line-of-sight gate without further correction.

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

**Current state (2026-08-10):** character and fly-camera WASD, jump/ascent, and
sprint/boost consumers read `ActionState`; fly-camera Shift descend remains a
debug-only physical axis. The action snapshot refreshes once in `Stage::Early`
and is shared with `Stage::Update` interaction, preserving one-frame edges.
Regression tests pin remapped movement actions and focused-UI transfer clearing
world keys/cursor capture into release-only action edges. Mouse/gamepad sources
and the deterministic character traversal smoke remain open.

### Water focus — playable traversal + EX-13 visual closure

**Active next push (2026-08-10).** Water temporarily leads the queue by explicit
project direction. “Right” means one coherent surface/volume contract survives
authoring, rendering, physics, player traversal, and cell/LOD boundaries; it does
not mean adding another isolated shader effect.

The reference fixture starts with Skyrim Tamriel grid `(2,-10)`
(`BleakfallsBarrowPath`, the proven water-adjacent streaming repro), then adds one
older-generation profile to catch false Skyrim-only assumptions. Closure gates:

1. `water.dump` proves the intended worldspace default/CELL override, WATR source,
   plane height, volume, material, and flow; `water.contacts` proves the same flow
   reaches dynamic-body physics.
2. A character can enter, swim horizontally and vertically, float/clamp at the
   surface, exit onto land, and cross a water-adjacent cell boundary without
   falling, sticking, or losing input. Camera waterline hysteresis must not strobe.
3. Fixed above-surface, grazing-angle, underwater, shoreline, and full-detail↔LOD
   captures show finite reflection/refraction, readable depth absorption, moving
   normals, bounded foam, no dry ocean tiles, and no visible water seam.
4. Dynamic clutter rises, settles, and drifts downstream without pinning the
   physics world awake. Calm water must still reach the static-scene fast path.
5. One scripted GPU smoke retains screenshots plus `water.dump`,
   `water.contacts`, `tex.missing`, frame/streaming telemetry, and fails on Vulkan
   validation errors or non-finite output. A short manual swim/shoreline checklist
   covers input and perceptual judgments that image-health statistics cannot.

Order within the push:

- W0: freeze camera/player poses and baseline artifacts on the two real-data
  profiles; use the new diagnostics before changing visuals.
- W1: make kinematic character contact/swimming consume the canonical water
  volume/flow; add enter/surface/exit and boundary regressions.
- W2: close default-water, CELL override, shoreline, and LOD coverage/seam gaps.
- W3: tune reflection/refraction/absorption/normals/foam against the frozen
  captures, finishing only WATR fields whose real bytes are verified.
- W4: add underwater audio, breath/drowning, and splash/ripple feedback after the
  traversal and visual gates are stable.

**Bootstrap landed 2026-08-10:** live dynamic-body current drag now consumes
`WaterFlow` in the same pre-step as buoyancy, with bounded velocity matching and
real Rapier coverage. `water.dump` and `water.contacts` are registered and the
cross-game exterior smoke records both and fails if an XCLW no-water sentinel
escapes into live bounds. A real Skyrim `(2,-10)` probe exposed the second
sentinel spelling (`FLT_MAX`) and the missing tri-state at the CELL→WRLD fallback:
the fix preserves absent XCLW as “inherit” while an authored sentinel stays dry.
The rebuilt probe reduced the fixture from 16 water planes to the expected two
(one LOD plane plus the authored `RiverWater` tile), retained flow
`[0.878, 0, 0.479] @ 90`, and resolved every texture. W0's fixed above/underwater
capture set is still open.

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

**Fixture frozen 2026-08-10:**
[`p2-combat-fixture.md`](p2-combat-fixture.md) pins direct NPC reference
`000380B4` in `BleakFallsBarrow01`, a level-1 Draugr with explicit creature /
Draugr factions, a death-item list, and one two-handed weapon family. The
surface trace found the first implementation blockers: Skyrim NPCs currently
receive no `ActorValues`; weapon records stay inventory-only; actor ray hits
end at bone bodies without canonical placement-root ownership; the ragdoll
template lives on the skeleton root; and `HitEvent` has cleanup but no
production producer or damage consumer.

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

1. Run W0 on Skyrim `(2,-10)`, freeze the waterline/shore/underwater/LOD poses,
   and turn the retained diagnostics into hard assertions where the authored
   values are stable.
2. Implement W1 character swimming and current response through the canonical
   action + water-contact boundaries; close enter → swim → surface → exit first.
3. Use those captures to choose the first W2/W3 defect by evidence (coverage/seam
   before local shading polish), then re-run the same fixture.
4. Resume P1 door-return traversal and P2 combat readiness after the water gate,
   carrying the water-adjacent boundary through the eventual 30-minute soak.
