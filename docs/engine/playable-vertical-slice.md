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

**Current state (2026-08-16):** character and fly-camera WASD, jump/ascent, and
sprint/boost consumers read `ActionState`; fly-camera Q descend remains a
debug-only physical axis. The action snapshot refreshes once in `Stage::Early`
and is shared with `Stage::Update` interaction, preserving one-frame edges.
Regression tests pin remapped movement actions and focused-UI transfer clearing
world keys/cursor capture into release-only action edges. The native Escape
menu now owns pause/focus/cursor transfer, Shift is the default sprint/boost
binding, Q is fly-camera descend, and settings-backed key rebinding swaps
collisions without losing an action. Mouse sensitivity and invert-Y apply live
and persist with the rest of the universal registry.

The deterministic character gate now passes via
[`p1-character-traversal.sh`](../smoke-tests/p1-character-traversal.sh): the
real capsule walks away from and back to the Bannered Mare threshold, activates
both sides of the XTEL pair, crosses `WhiterunWorld (6,-2) → (6,-3) → (6,-2)`
through live collision/streaming, and returns grounded to the interior. The
smoke's bounded holds resolve through live bindings, its look fixture writes
the normal mouse-look accumulator, and no camera/body teleport participates in
the route. Door transitions now honor an explicit `--radius`, allowing the gate
to retain a radius-1 exterior ring. Gamepad physical sources remain open, so P1
as a whole is not closed yet.

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
  volume/flow; add enter/surface/exit and boundary regressions. **CLOSED
  2026-09-09** — see the W1 block below.
- W2: close default-water, CELL override, shoreline, and LOD coverage/seam gaps.
- W3: tune reflection/refraction/absorption/normals/foam against the frozen
  captures, finishing only WATR fields whose real bytes are verified.
- W4: add underwater audio, breath/drowning, and splash/ripple feedback after the
  traversal and visual gates are stable.

**W0 closed 2026-09-04:** `m-exteriors.sh water` now freezes paired
above-surface and submerged fly-camera poses on the Skyrim `(2,-10)`
`RiverWater` tile and the older-generation FNV Lake Mead `(19,13)` CELL water.
The gate retains both captures plus an unescaped transition log and requires
the intended WATR source, canonical camera-containing volume, Skyrim's nonzero
authored flow, finite pre-tonemap output at both poses, and a non-trivial image
delta across the waterline. The first captures also exposed a real visibility
defect: the water shader built a complete reflected/refracted `surfaceColor`
and then attenuated that result a second time with low authored alpha. Output
coverage now includes the Schlick Fresnel share while preserving authored zero
opacity as fully transparent. Skyrim mesh-bound water also no longer treats
generic `Material` defaults as authored opacity/reflectivity.

**W1 closed 2026-09-09:**
[`docs/smoke-tests/w1-water-traversal.sh`](../smoke-tests/w1-water-traversal.sh)
drives the real `CharacterController` capsule shore → swim → dive → surface →
shore → water-adjacent cell boundary on both frozen profiles, entirely through
`ActionBindings` → `ActionState` → the Rapier KCC (`input.look` writes the same
yaw/pitch accumulator mouse look owns, and here pitch is a *movement* input —
no teleport participates in any route leg). Four reference-anchored controller
corrections came out of it, each cited to OpenMW's movement solver (WATAL §9
Q3) and unit-pinned: full 3D pitched swim movement, swim input taking
precedence over the buoyancy spring (a held dive previously stalled ~23 BU
below the neutral point — "swim down" did not exist), the
"don't swim up into the air" surface clamp, a swimmer never reading grounded,
and a water exit that starts from rest instead of inheriting the spring. The
player also now publishes a canonical `WaterContact` — it is the one body the
dynamic buoyancy pass structurally cannot see — so it appears in
`water.contacts` and in a new `player.status` water line.

Measured on the closing runs: FNV Lake Mead enters at the beach, dives past
depth 100 (fully submerged, camera underwater), holds the waterline through
240 frames of hard upward swim (`y=2577.28` against a 2600 surface), exits
grounded at `vertical_velocity=0.00`, keeps support swimming across the
(20,13)→(19,13) edge, and logs exactly one camera waterline enter/exit pair —
no strobe. Skyrim's authored White River at Tamriel (4,-11) runs the same
route; its water is ~96 BU deep bed-to-surface, so the fixture declares
`W1_HEAD_SUBMERSION=0` and gates the descent on passing the passive float
depth instead. Fixture geometry, not the engine, decides which side of the
exit the boundary crossing falls on, so that too is a declared fixture field.

The wider shoreline/LOD perceptual capture set (W2/W3) remains open.

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

**Fixture frozen 2026-08-10; grounded placement corrected 2026-08-17:**
[`p2-combat-fixture.md`](p2-combat-fixture.md) pins direct NPC reference
`000383F7` in `BleakFallsBarrow01`, a level-1 Draugr with explicit creature /
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

**Partial 2026-08-15:** the native HUD now has a configurable crosshair and its
interaction prompt reflects the live Activate binding. Escape opens a modal
pause surface with Continue, categorized Settings, and orderly Quit; simulation
stops while it is open, while rendering and menu interaction continue. Settings
cover FOV, HUD visibility, UI scale, upscaler, mouse look, and the keyboard
actions currently consumed by gameplay, with validated TOML persistence and
stale-entry recovery. Tab now opens a native inventory backed by the player's
canonical `Inventory` / `EquipmentSlots`: it resolves authored item metadata,
filters by category, shows stack/value/weight/equipped state, and mutates armor equip slots. Fallout
4/76 `Mods`, component-bearing `Junk`, and ordinary `Misc` remain distinct.
At that checkpoint, container/corpse transfer, weapon equip, visible player-mesh
attachment, bars, notifications, and objective text remained P3 closure work.

**Container interaction progress (2026-09-16):** nonempty placed CONT references
now expose `Take all` through the normal ray-selected Activate binding (E by
default). A scheduled consumer transfers their canonical inventory into the
player's inventory, before script consumers and without removing ActivateEvent.
Locked targets, non-player activators, and inventories whose base is not CONT
are rejected. Transfers append intact stack rows, preserving instance handles,
counts, and the player's existing equipment indices; a missing player inventory
leaves the source untouched. Reprocessing the emptied source adds no duplicates.
The input-path regression covers selection, E activation, transfer, and removal
of the empty-container prompt; focused transfer tests cover the safety gates.
This is a basic take-all action, not a container browser: selective transfer,
pickup handling, theft/ownership rules, item-added/removed script events,
open/close presentation, and live save/reload validation remain open. No Vulkan
device was available for the live smoke.
Validation: inventory tests 14 passed / 1 ignored; interaction tests 20 passed
with `BYRO_LOCK_ORDER_CHECK=1`; boot schedule tests 11 passed.

**Corpse-loot progress (2026-09-16):** the same activation consumer accepts actors
with the canonical `Dead` marker, excluding the player and living NPCs. Physical
ragdoll targeting resolves `ActorColliderOwner` back to the actor inventory even
when the body has fallen away from its placement bound. Collider-backed corpses
no longer expose a second target at that old bound; intervening walls still block
looting. Taking equipment clears the source's slots/weapon and emits one unequip
event per inventory index (not one per occupied biped slot). Death reconciliation
also clears a respawned weapon when a save overlays an empty corpse inventory.
Regression coverage drives lethal damage through combat, then E activation and
transfer; it also covers obstruction, equipment events, and post-overlay logical
equipment reconciliation. Visible armor meshes are still spawn-time attachments:
stripping/rebuilding corpse visuals and a full live save/reload smoke remain open.

**Loot save-overlay validation (2026-09-16):** a headless regression now captures
both pre-loot and post-loot worlds, encodes/decodes the real save format, restores
resources, remaps stable FormIDs onto different entity IDs, applies the production
mutable-column overlay, and runs corpse reconciliation. It verifies empty source
inventories, surviving instance-pool handles, preserved player equipment indices,
and no duplication on another activation. All 64 binary save tests pass with
`BYRO_LOCK_ORDER_CHECK=1`. This validates the same-cell persistence machinery,
not a Vulkan-driven load or cross-cell continuity. The existing stream snapshot
stores actor position/package/seat state but not inventory/death state; retaining
looted state across ordinary cell eviction and later revisits was still required
at that checkpoint.

**Eviction persistence progress (2026-09-16):** a separate
`PersistentReferenceStates` resource now captures reference inventories,
equipment, actor values, and death state before cell teardown. Container attach
and NPC placement-identity stamping consume those rows on respawn. Keys include
the plugin identity, and item instances are stored as owned payloads rather than
soon-to-be-freed arena handles. The resource survives worldspace-cache drains
and is registered in saves, so unloaded references are not discarded by saving
in another cell. Save reload bypasses the outgoing session's store during
teardown/respawn, then installs the saved store alongside resident component
deltas. Headless tests cover repeated eviction, empty containers/dead actors,
handle reuse, plugin-key separation, save encoding, and reload isolation.
The new registry entry changes the save-schema fingerprint: older saves are
rejected rather than silently dropping unloaded reference state. Authored cell
reset/restock timing, visual corpse stripping, and a live cross-cell/save/reload
smoke remain open. The initial store captures every unloaded reference carrying
inventory/death state; baseline-difference compaction is not implemented yet.
Validation: the cell-loader suite passed 516 tests (8 ignored), and all 64
save tests passed, with `BYRO_LOCK_ORDER_CHECK=1` enabled for both suites.

**Placed-actor restoration fix (2026-09-16):** spawn-boundary tests now distinguish
two ACHR placements sharing the same NPC base: only the looted placement restores
empty/dead state. They also exposed an older movement-state wiring bug: NPC jobs
requested streaming snapshots by base NPC FormID, but eviction captured them by
placed reference FormID. Both runtime and pre-baked actor paths now defer that
restore until placement stamping, restoring the correct actor's position and
Travel completion without affecting its sibling. The new travel regression
failed before the fix and passes afterward. Save reload also suppresses and
invalidates the outgoing transient streaming cache, preventing its state from
being mistaken for state in the loaded save.

**Cross-game loot progress (2026-09-16):** container loading now resolves
CNTO leveled-list references into terminal item stacks, preserves nested LVLO
counts, and rejects cyclic, zero-count, and 100%-chance-none branches. It uses
the live player's level when available, otherwise level 1. Selection remains
deterministic (highest eligible entry, or all eligible Use All entries);
random rolls, partial chance-none probabilities, and encounter-zone levels
are still open. This is progress toward playability across all games, not
closure of container interaction or the playable slice.

`cargo run -p byroredux-plugin --example probe_container_loot -- <ESM>...`
checks parsed container lists at levels 1, 10, and 50 without Vulkan. The
installed base masters produced these results; stack counts sum the three
levels, and missing item metadata includes specialized non-equipment records:

| Game | Container list references | Resolved stacks | Leaves outside item catalog |
|---|---:|---:|---:|
| Oblivion | 2718 | 7947 | 540 |
| Fallout 3 | 1018 | 8121 | 0 |
| Fallout New Vegas | 4826 | 37059 | 0 |
| Skyrim SE | 2409 | 8922 | 207 |
| Fallout 4 | 1162 | 9020 | 0 |
| Fallout 76 | 1004 | 63502 | 1068 |
| Starfield | 737 | 21725 | 1677 |

No output retained a known LVLI ID or a zero-count stack. Fallout 76 originally
yielded only 105 stacks: its newer four-byte LVLO references were discarded by
the legacy twelve-byte decoder. Game-specific LVLO + LVLV/LVIV scalar decoding
reduced empty parsed lists from 9988 to 701 out of 10784. The decoder also reads
the list-level LVCV chance-none scalar. Layouts were checked against
[xEdit's FO76 definitions](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO76.pas).
Global/curve overrides, per-entry chance-none and conditions remain unsupported;
the higher yield proves restored data flow, not authored loot parity.

Soul gems now populate the shared item catalog as well as their typed soul
table. This restores inventory names, value, weight, icon/model and script
metadata, and prevents NPC inventory expansion from dropping SLGM leaves.
The two real-master probes recovered all 381 Oblivion and 216 Skyrim soul-gem
stack occurrences. Enchanting interactions are not implied by this change.

Oblivion CLOT records now enter the wearable item catalog with their authored
value/weight, low-16-bit biped slots, and male/female worn models. Clothing uses
the existing equipment contract with zero armor rating and durability. The
real-master probe recovered all 1176 clothing stack occurrences, and a parser
integration test verifies FormID/script remapping and both gender mesh paths.
General clothing flags and enchanted clothing effects remain follow-up work;
this does not establish visual equip correctness without a live run.

New Vegas CCRD, CMNY, and IMOD now populate the item catalog and world-model
index alongside their existing specialized tables. Cards/money retain authored
value with zero weight; IMOD retains value/weight and uses the native Mods
category. Their layouts follow [xEdit's FNV definitions](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFNV.pas).
This recovered the remaining 164 occurrences (CCRD 45, CMNY 27, IMOD 92), so
every resolved leaf in the measured New Vegas container probe now has item
metadata. Mod installation and Caravan rules/UI are still separate work.
The full plugin suite passed 980 tests, with 27 ignored; the new integration
test covers plugin remapping, value/weight, models, typed records, and inventory
expansion.

The probe now reports missing catalog leaves by record signature: Oblivion's
540 comprise APPA (165) and LIGH (375). Skyrim's 207 comprise LIGH
(54) and untracked IDs (153). Fallout 76's 1068 and Starfield's
1677 are untracked IDs. These remain concrete follow-up work for usable loot.
The raw-header follow-up identifies Skyrim's untracked `000E0CD5` as SCRL;
Fallout 76's largest missing leaf is CNCY `0000000F` (576 stacks), followed
by LGDI records. Starfield's five most frequent untracked leaves are LGDI.
The probe now prints raw signatures for the five most frequent untracked IDs
so unsupported dispatch can be distinguished from references absent on disk.
Resolver tests (47 equipment tests including three new
loot regressions) and all five container attachment tests passed. Live
interaction/save-reload validation remains pending: this run had no Vulkan
device.

After the FO76 decoder change, the full plugin library suite passed with
977 tests passing and 27 data-dependent tests ignored. The decoder tests pin
companion-field scoping, malformed-entry isolation, finite/bounded scalar
conversion, FormID remapping, and rejection of four-byte LVLO on other games.

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

1. ~~Run W0 on Skyrim `(2,-10)`, freeze the waterline/shore/underwater/LOD
   poses~~ — closed 2026-09-04.
2. ~~Implement W1 character swimming and current response through the canonical
   action + water-contact boundaries; close enter → swim → surface → exit
   first.~~ — closed 2026-09-09.
3. Use the W0/W1 captures to choose the first W2/W3 defect by evidence
   (coverage/seam before local shading polish), then re-run the same fixture.
4. Add P1 gamepad physical sources and resume P2 combat readiness after the
   water gate, carrying the now-passing door-return boundary route through the
   eventual 30-minute soak.
5. W4's underwater audio / breath feedback now has its traversal prerequisite:
   the breath reserve and drowning damage are already live and observable on
   `player.status`, so the remaining work there is presentation.
