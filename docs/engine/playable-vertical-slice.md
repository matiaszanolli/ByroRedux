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

#### Live-environment recheck (2026-09-16)

The absence of `/dev/dri` inside the coding sandbox is **not** evidence that
this host cannot run the engine. An approved outside-sandbox `vulkaninfo
--summary` identified the RTX 4070 Ti (NVIDIA 580.178.04), working display
access, and the validation layer. After rebuilding both release binaries from
the current worktree (HEAD `2e2f40b23` plus local changes), the existing P1
gate ran under `xvfb-run -a`, with isolated debug port 19876.

FNV remains **FAIL**: startup, Character mode, grounding, the door prompt,
and the 120-frame forward input countdown all completed, but walking did not
leave the door's interaction volume. The initial bench eye position was
`(536.001, 3575.700, -472.000)`; after the hold the body was
`(547.02, 3523.70, -507.53)`, grounded, with door entity 792 still selected
at distance zero. Failure artifacts are retained locally at
`/tmp/byro-p1-traversal.ozRSGf`. This run does not prove the historical
fall-through-floor arm fixed or still present: it stopped before that arm.

The fixture camera was not applied. P1 passes `--camera-pos=...` and
`--camera-forward=...`, while `parse_string_arg` intentionally accepts only
space-separated values. Its warning is hidden by this gate's log filter.
Independently, `plan_character_spawn` prefers the first eligible door and a
64-BU nudge toward the aggregate static-collider AABB centre over the camera
column. Thus correcting argument spelling alone does not establish valid
character placement. The next movement fix must verify actual capsule
placement and clearance, then rerun the unchanged walk/transition assertions;
these observations are not a reason to weaken the gate.

The Skyrim SE control also **FAILS**, later in the route: interior walk-away
and return, the outbound XTEL transition, and both exterior boundary crossings
`(6,-2) -> (6,-3) -> (6,-2)` passed. All intermediate return-road waypoints
completed, but the final reverse-door approach exhausted twenty 30-frame
backward holds at `z=7356.99`, short of the required `z>=7670`, while remaining
grounded. Artifacts: `/tmp/byro-p1-traversal.GpvgyI`. This supersedes the older
green P1 result for the current worktree; the cause of that final obstruction
is not yet isolated (collision versus fixture trajectory/overshoot or input
drift). In particular, an intermediate blocked sample still had yaw/pitch
`0/0`, but the final retained status had `69.8/8.7` despite `run_hold` reseeding
look before each segment; the virtual display does not by itself prove input
isolation. Both
smoke processes exited with status 1 and cleaned up their engine processes;
neither was abandoned on an observation timeout. No panic/VUID/error-pattern
matches appeared in their retained logs, but these were release runs, not the
required validation-enabled debug run or 30-minute soak.

The follow-up input audit found and fixed a separate focus-lifecycle defect:
`WindowEvent::Focused(false)` now releases held keys/buttons, debug input,
and cursor capture before any native or Scaleform menu can consume the event.
Raw device mouse motion now requires both actual window focus and gameplay
capture, and closing a menu cannot recapture an unfocused window. Refocusing
alone does not recapture the cursor; a gameplay click does. Tests cover the
focus/capture combinations, idempotent release, held mouse-button cleanup,
and unchanged sensitivity, inversion, and pitch clamping. All **2,193 engine
tests passed** with lock-order checking (34 ignored). This fixes the Alt-Tab
input contract; it has not yet been verified with live OS focus changes and
does not establish the cause or resolution of either traversal failure.

The FNV collision follow-up reproduced a concrete startup defect: after its
inward probe failed, the door fallback accepted floor at `y=3455.7` directly
under the door pivot `(536,3460,-472)`. A downward cast with penetration
continuation could find that floor despite the final capsule intersecting the
door panel. Floor support alone was not a valid spawn certificate.

Door-local floor probes now also check final capsule clearance with the same
solid-world query filters, including kinematic architecture and excluding
sensors/live actor bones and the caller's excluded player body. If both normal
columns fail, startup tries eight directions at 64 and 128 BU, retaining the
collision-ready exterior-cell boundary. The wide door-column fallback also
checks clearance. Tests reproduce the occupied-column floor hit, reject it,
find a clear nearby candidate, retain self-exclusion/filter behavior, and
reject candidates outside the foreground cell. **2,195 engine tests** passed
with lock-order checking (34 ignored); **170 physics tests** passed.

The rebuilt live FNV probe selected `(472.29,3523.70,-465.90)` instead of the
door pivot. After the same 120-frame forward hold, the capsule reached
`(444.83,3539.20,-595.77)`, grounded, and `interaction.status` reported
`target=none prompt=none`. Startup/bench logs are retained at
`/tmp/byro-spawn-check.HFXOUI`; the pre-fix comparison is at
`/tmp/byro-spawn-diagnosis.8CZV3E`. This verifies escape from the original
blocked spawn, not the complete cross-cell route or a general safe-spawn proof.

The unchanged full FNV P1 gate subsequently passed walk-away, grounded return,
prompt recovery, and bound activation. Its selected first door (entity 792,
source collider form `00108BD6`) actually leads to WastelandNV `(-17,1)` at
Z-up `(-67452,4900,8352)`, and that transition completed. The fixture instead
expects exit `0010618E` to `(-17,0)`. This exposes the previously noted ignored
fixture pose/first-door selection mismatch after the movement obstruction was
removed. Do not change the expected grid to paper over it: the intended route
must first select its authored door, then pass its original boundary/return
assertions. This run's artifacts are `/tmp/byro-p1-traversal.0RNNYp`.

Explicit startup poses now take precedence: in Character mode,
`--camera-pos x,y,z` supplies the eye position and fixes the capsule's XZ
column. A bounded capsule probe near the expected feet finds clear walkable
floor, rather than choosing the first door or raycasting onto the roof.
An unsupported requested column fails the automatic Character-mode gate;
`--player` still explicitly forces the capsule at that requested eye position.
Default startup without a position override retains the door-search ladder.
Three regression tests cover competing doors, an unsupported column, and a
roof above the intended floor; the full engine suite passed **2,198 tests**
with lock-order checking (34 ignored).

P1 now passes camera arguments in the CLI's supported space-separated form.
The live FNV rerun passes interior walking/return and reaches the intended
WastelandNV `(-17,0)` exit. It then fails the outbound exterior leg: twenty
30-frame forward holds leave the grounded player at
`(-67762.86,8451.60,-3593.49)`, yaw/pitch `0/0`, short of `z<=-4150`.
Artifacts: `/tmp/byro-p1-traversal.wC6ts4`. The fixture's straight exterior
line was derived from grid arithmetic, not a measured walk around authored
obstacles; collision and route geometry now need inspection at this position.
The wrapper exited 5 after its own temporary-directory cleanup warning;
the smoke reported the traversal failure, and port 19876 was verified free.

`scripts/check-playable-smoke-contracts.sh` now pins the supported P1 argument
spelling and passes. Its stale Battleaxe assertion was also updated to the
fixture's already-existing War Axe leaf after rerunning the installed Skyrim
master's `probe_combat_fixture`: `000236A5` (Greatsword, 17 damage) and
`0002C672` (War Axe, 9 damage) are emitted; the selected reference `000383F7`
still has base `000E9895`, Health 50, and the Greatsword. No gameplay gate or
destination assertion was relaxed. Skyrim traversal, full FNV traversal, the
debug-validation run, and the 30-minute soak remain unclosed.

The subsequent FNV road-route recheck **passes the full P1 gate**. Live
inspection confirmed the arithmetic northward leg walked into the saloon
and neighboring buildings. The fixture now backs into the road, walks west
across `(-17,0) -> (-18,0)`, returns across the same boundary, and approaches
the same authored saloon entrance. No collision bypass or movement teleport
was added. The fresh automated run reached outbound `x=-69908.54`, inbound
`x=-67907.29`, entrance alignment `x=-67747.62`, and final approach
`z=-3457.17`; grounded checks, both bound door activations, correct interior
return, exactly two activations, and two streaming crossings all passed.
The command was `BYRO_DEBUG_PORT=19876 BYROREDUX_SMOKE_TIMEOUT=90 xvfb-run -a
bash docs/smoke-tests/p1-character-traversal.sh fnv`, exiting 0 with 3,149
source-cell entities. The smoke-contract checker also passed. This supersedes
the FNV P1 failures above, but is a release traversal check, not evidence of
debug-validation, long-soak, quest/dialogue, or all-game playability closure.

The immediate Skyrim SE control rerun also **passed the full P1 gate** with
the unchanged Skyrim route: both streaming crossings, final reverse-door
approach (`z=7797.21`), bound return activation, and grounded interior return.
The same command with `skyrim_se` exited 0 and reported 5,857 source-cell
entities. This supersedes the earlier failed Skyrim route observation, but
does not isolate which prior correction resolved it or prove repeatability
under different frame timing. Both P1 results remain release-only evidence.

The FNV gate passed again after adding original LSCR artwork/tips around
door transitions. It now also asserts that each loading screen presents
before scene teardown and dismisses after a ready destination frame; both
ordered lifecycles, the original walking route, two streaming crossings,
and grounded interior return passed (exit 0). Visual captures and remaining
cross-game/startup/save-loading work are tracked in
[UI loading-screen integration](ui.md#original-game-loading-screens-during-scene-transitions).

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

**Live recheck (2026-09-16):** the current Skyrim gate passed the blocked
swing and seven 8-damage hits against the 50-Health Draugr, but initially
crashed during process-restart restore in Rapier's multi-SAP broad phase
(`proxy.aabb.maxs ≈ 2.68e11`; retained save/logs:
`/tmp/byro-p2-melee-core.7n1lJf`). Corpse reconciliation can activate a ragdoll
before the fresh bones have their first physics sync. Activation removed
kinematic `RigidBodyData` only from bones already carrying `RapierHandles`,
so the next sync created overlapping follower bodies alongside the dynamic
ragdoll. Activation now removes every ragdolled bone's follower recipe,
including not-yet-registered bones. The regression failed before the change;
17 ragdoll tests and all 2,207 engine tests then passed (34 ignored).

The retained crash save subsequently ran 120 benchmark frames and exited 0.
The full Skyrim P2 rerun also passed (exit 0), including the original combat,
floor, ragdoll and loadout gates and a new explicit check that the same killed
FormID remains `GetDead = 1` after restart. The gate now waits for the save
drain before comparing restored state, so unchanged default inventory cannot
masquerade as a successful load. This does not close animation/audio polish,
live loot mutation, corpse-pose fidelity, debug validation, or P2 as a whole.

The FNV recheck remains **FAIL**, now at a different point than the historical
bystander hit: the blocked swing and 19 damaging hits reached the intended
GSTrudy reference (`entity 1136`, Health 240 -> 88), with floor support intact.
The twentieth damaging swing was acknowledged by `input.press attack` after
`combat.approach` reported `physics_synced=true`, but `combat.status` stayed
at `attacks=20 hits=20 kills=0 cooldown=0.000 blocking=false` (including the
initial blocked hit) until the 90-second gate timeout. The engine remained
responsive and the player grounded. Artifacts:
`/tmp/byro-p2-melee-core.8tjgKv`. No forced hit or retry was substituted;
the input/action-consumption failure still needs diagnosis. The later
read-only attempt to inspect player death state arrived after cleanup and
connected to nothing, so it supplies no evidence about the cause.

Two diagnostic FNV reruns then **passed the full P2 gate** (exit 0): one
blocked hit, 30 damaging hits at 8 damage, GSTrudy death/ragdoll, save/process
restart, restored loadout, and `GetDead = 1` on the same placed FormID. Artifacts
are `/tmp/byro-p2-melee-core.8b2Tgs` and `/tmp/byro-p2-melee-core.GyvZhv`.
The second uses the new exact `cooldown_ready` status instead of treating
three-decimal `cooldown=0.000` as proof of readiness. Two regression tests
pin sub-millisecond cooldown and missing-state behavior; all 2,209 engine
tests passed (34 ignored). Queued/refreshed/cancelled key-pulse and rejected
combat-edge debug logs now distinguish the failure boundaries, and smoke
failure cleanup captures bounded read-only state before terminating.
Neither run reproduced the missing edge, so its original cause remains
**unproven**; the readiness correction and green reruns are not evidence that
the intermittent input failure is conclusively fixed.

Follow-up corpse inspection exposes a separate **blocking physics failure**
that P2's death/loadout assertions do not cover. Saved local placements were
applied before fresh global transforms reached the bones; a regression
initially seeded a restored body's physics pose at the origin instead of its
saved placement. Restore reconciliation now propagates the hierarchy before
activation. It also removes actor-owned and skeleton-owned animation players
and stacks, with matching scheduler declarations and regression assertions.
The 23 combat tests pass, but these are not proof of live corpse stability.
The retained FNV save from `/tmp/byro-p2-melee-core.GyvZhv/saves` still fails:
an isolated release restore on 2026-09-17 UTC panicked in Rapier multi-SAP
with body bounds around 1e11. Logs are retained in
`/tmp/byro-corpse-placement.WzTUOw/restore-panic.stderr`. Earlier inspection found
million-unit bone coordinates despite correct actor-root placement. The
animation cleanup therefore does **not** resolve the articulation instability;
corpse pose fidelity and stable post-restore physics remain unverified.

Isolation on the same retained FNV save narrows this to ragdoll contact
simulation, independently of the saloon scene and ECS update loop. Rebuilding
the exact activation spec in a fresh `PhysicsWorld` with zero gravity and no
scene colliders survived 600 fixed 1/60-second ticks, although maximum bone
displacement grew to 412 units by tick 540 (not a stability pass). Replaying
with normal gravity and only one static cuboid floor, top at Y=3456, instead
jumped to **69,650,440 units of displacement at tick 75**. No other actors,
animation sampling, save overlay, or per-frame ECS writeback ran in this
isolated replay. This rules out needing saloon mesh contacts or ongoing
animation as the trigger, but does not yet distinguish imported joint data,
mass/inertia, or multibody solver behavior. Temporary activation instrumentation
was removed; its source and the two logs remain under
`/tmp/byro-corpse-placement.WzTUOw/` as `flat-floor-probe.rs`,
`isolation-zero-gravity.stderr`, and `isolation-flat-floor.stderr`.

The next isolated comparison kept the full authored articulation but raised
solver iterations from 4 to 16: all 600 ticks remained bounded, with maximum
displacement settling near 120 units. Diagnostic alternatives (removing joint
limits, replacing authored masses with equal masses, or replacing shapes with
balls) also avoided divergence, but were not adopted because they discard
authored behavior. `build_ragdoll` now requests 12 **additional body-local
solver iterations**, applying the extra work to interacting ragdoll islands
without raising the world's default budget or altering masses/limits/shapes.
A unit test pins that scope and preservation contract; all 171 physics tests
and 2,210 engine tests passed (34 engine tests ignored).

Two fresh release FNV restores with this change kept the inspected pelvis and
thigh near the saved corpse: pelvis approximately `(-328.3, 3469.0, 172.1)`,
thigh `(-324.3, 3467.3, 177.6)`, `GetDead = 1`. Repeated inspection in the first
run remained bounded; the viewed `solver-budget-corpse.png` shows a fallen
corpse. Logs, diagnostic variant source, and screenshot are retained beside
the earlier artifacts (`isolation-variants.stderr`, `variant-probe.rs`,
`solver-budget-first.stderr`, `solver-budget-repeat.stderr`). Temporary probes
were removed from runtime code. This resolves the observed FNV divergence in
these runs, not exact saved bone-pose restoration, long-soak stability,
all-game ragdoll behavior, or performance under many simultaneous corpses.
The retained Skyrim SE crash save also completed a 120-frame release replay
with this change (exit 0, one reconciled dead actor, restored player pose).
That is a crash-regression check, not a visual or bone-coordinate stability
check for Skyrim; the benchmark reported about 51 ms in systems per frame,
without a matched pre-change run to attribute the cost.

P2 now checks the missing property directly: the read-only
`ragdoll.status <actor_id>` command resolves the actor's skeleton and samples
every Rapier body, reporting completeness, finite poses/velocities, maximum
distance from actor placement, and maximum linear speed. Six unit tests cover
normal placement-relative measurement, missing/empty bodies, million-unit
finite displacement, non-finite placement, and bad arguments. The smoke gate
requires twenty consecutive complete/finite samples over at least ten seconds,
all within each humanoid fixture's 512-unit bound; it never retries an invalid
sample into success. Both FNV and Skyrim SE **passed the strengthened full
combat/save/process-restart gate**. FNV artifacts are retained at
`/tmp/byro-p2-melee-core.i8yLRN`: 18/18 live bodies throughout, maximum measured
distance 81.906 BU initially, settling near 58.6 BU. Skyrim's passing run used
normal automatic artifact cleanup. All 2,216 engine tests passed (34 ignored),
as did shell syntax and smoke-contract checks. This is a bounded short-window
physics regression gate, not proof of exact saved poses, sleeping, collision
fidelity, or long-term stability.

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

Armor attachment groundwork now preserves actor ownership, inventory row,
resolved FormID, intrinsic race-skin classification, and import-time hidden
partition mask on each successfully loaded armor root in both runtime and
prebaked NPC spawn paths. Multiple ARMA roots can share one inventory row;
ordinary body/head/skeleton attachments are not marked as equipment. This is
spawn-derived metadata, not serialized entity IDs. It does **not** remove looted
meshes yet: covered body meshes may never have been loaded, and suppressed skin
partitions require rebuilding. Reconciliation must also run after save overlays,
not only in response to unequip events.
Validation: 75 NPC-spawn tests passed (7 ignored), including attachment ownership
and skin/gear classification; the full engine suite passed 2,217 tests
(34 ignored). No live visual-removal claim is made by this metadata change.

Race skin now resolves as an intrinsic body layer without a synthetic inventory
stack or equipment occupant. Actual armor still suppresses its covered regions,
including partial coverage and fully displaced skins; zero-mask creature bodies
remain renderable. Armor-root ownership uses no inventory index for intrinsic
skin. The spawn-to-corpse-loot regression checks that only actual inventory gear
transfers, no skin unequip event is emitted, and real equipment indices stay
valid. An explicitly authored CNTO copy of the same FormID remains an ordinary
item, so this is not a global skin-FormID blacklist. Existing saves containing
old synthetic skin stacks are not rewritten: they cannot be distinguished from
explicit inventory copies by FormID alone. Visible body restoration remains open.
Validation: all 75 NPC-spawn tests pass with lock-order checking, all 2,217 engine
tests pass (34 ignored), and all four `on_real_skyrim_data` tests pass with
`BYROREDUX_REQUIRE_GAME_DATA=1` (outfits, multi-addon body coverage, zero-mask
creature skins, and helmet FaceGen masks). These are resolution/logic checks,
not a rendered corpse-equipment-removal smoke.

**Real-ragdoll loot targeting fix (2026-09-17 UTC):** a live FNV check found
that the above physical-targeting test used a stand-in collider with
`RapierHandles`. Actual ragdoll activation removes that row and stores dynamic
body handles in `Ragdoll`; the interaction ray therefore hit a corpse but
could not resolve its owner. Candidate suppression and both ray-hit/occlusion
lookups now include active ragdoll bodies. The lethal-combat loot regression
now activates a real ragdoll and asserts removal of its old handles; it failed
before the fix. The wall test also uses real activation, preventing the old
placement bound from bypassing an obstruction. All 24 interaction tests pass
with `BYRO_LOCK_ORDER_CHECK=1`, and all 2,216 engine tests pass (34 ignored).

Live FNV validation used a copy of the retained P2 save under
`/tmp/byro-loot-restore.X3g1dd/`, leaving the original untouched. From the saved
character position, `input.look 18.82 -42.86` selected GSTrudy's fallen body
at 127.09 BU and displayed `[E] Take all`. `input.press activate` transferred
her four rows (one base 979403, three base 584962; count 1 each): player rows
and item count rose from 2 to 6, her inventory became empty, and her equipment
slots cleared. Slot 8 validated and a fresh process loaded it successfully:
the same four rows remained in the player inventory alongside the original
two, the corpse stayed empty/dead, and another Activate press added nothing.
All 18 physical bodies remained present/finite. This verifies same-cell FNV
loot/save/process-restart behavior, not Skyrim loot, cross-cell continuity,
theft semantics, or visible removal of spawn-time armor meshes.

**FNV unloaded-reference live validation (2026-09-17 UTC):** starting from
the looted slot 8 above, a setup-only `cam.pos 45 3576 741` plus
`input.look -172.36 0` placed the grounded character at the verified saloon
door. Both exits/entries used the normal `[E] Open` target and
`input.press activate`, not direct cell-load commands. On the first
saloon -> exterior -> saloon round trip, GSTrudy respawned as entity 15598
but remained empty, unequipped, and `GetDead = 1`; all 18 physical bodies
were present/finite, maximum distance 65.581 BU. Player inventory stayed
at six rows/items and two occupied equipment indices.

Slot 7 was written **outside while the saloon was unloaded**. A fresh process
loaded that exterior save (nine streamed cells, 2,738 deltas applied across
2,634 matched FormIDs, zero resident dead actors), then returned through the
normal door. `mesh.info` confirmed the new entity 12451 was still placed
reference `0x104C6D`. It restored empty inventory/equipment, death state,
and 18 finite ragdoll bodies (maximum distance 53.020 BU); the player retained
all six items. This checks the persisted unloaded-reference store rather than
only resident save columns. Both return arrivals were grounded. Save copies,
runner, and logs are retained in `/tmp/byro-loot-restore.X3g1dd/` (slots 7/8,
`loot-cell-roundtrip.stderr`, `engine.stderr`). No implementation change was
needed for this persistence path. These are manual debug-input-driven FNV
checks, not yet an automated P3 gate or cross-game/reset/restock proof.

**Skyrim SE corpse-loot live validation (2026-09-17 UTC):** copied the
retained P2 corpse save into `/tmp/byro-skyrim-loot.Q4p1Kf/saves`, confirming
the target by placed reference `0x0383F7` rather than the duplicated NPC name.
The restored target (entity 36761) had four inventory rows and weapon index 3;
its 18-body ragdoll was finite and at rest. Normal forward input moved the
grounded player closer, then `input.look 84 -38` selected the fallen body at
164.99 BU with `[E] Take all`. Activate transferred all four rows (base IDs
93923, 244868, 244866, 145061; one each): player rows 23 -> 27, total items
207 -> 211. The player's six occupied equipment indices and equipped weapon
`0x00013790` at inventory index 5 remained unchanged. Corpse inventory and
weapon slot cleared and its prompt disappeared.

Slot 8 validated, and a fresh process restored all four transferred rows,
empty corpse inventory/equipment, and `GetDead = 1`. All 18 bodies were
present/finite with zero reported linear speed; another Activate press did
not duplicate items. Runner, copied pre-loot save, post-loot save, and logs
remain in `/tmp/byro-skyrim-loot.Q4p1Kf/`. No Skyrim-specific code change was
needed. Together with the FNV checks this verifies same-cell take-all/save/
process-restart behavior in both reference titles; Skyrim cell eviction,
visible equipment removal, selective looting, and other games remain open.

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

**Key-based unlocking (2026-09-16):** normal Activate input now checks the
authored lock's key FormID against positive-count player inventory stacks.
Looking at a keyed lock does not mutate it; pressing Activate removes the lock
and records an unlocked override in the existing saved `ReferenceLockState`
ledger. Keys are not consumed. The same path supports doors and take-all
containers, while wrong keys, zero-count stacks, missing keys, and keyless locks
remain blocked. Physical-input tests cover the door ledger update and both
locked/unlocked container transfers. Lockpicking remains open.

**Gameplay feedback (2026-09-16):** key unlocks and successful take-all transfers
now feed the native four-second HUD message used by save/load feedback. The
shared `PlayerNotifications` queue retains at most eight pending messages;
same-frame unlock and loot messages display together rather than hiding the
second action in the debug console. Loot feedback reports the total item count,
and an empty/repeated transfer emits nothing. Input-path regressions cover both
keyed and unlocked containers, message order, and held-key non-repetition. This
uses the existing HUD renderer, independent of debug-panel visibility; a live
visual smoke remains pending.

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

#### Skyrim scroll inventory metadata (2026-09-16)

SCRL now enters the item catalog and world-model index as a distinct Scroll
category, including the native inventory and SDK/WIT metadata projection.
The parser retains names, value, weight, and remapped effect identities using
[xEdit's Skyrim SCRL definition](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas).
It does not interpret SPIT casting parameters or EFIT effect magnitudes yet;
scrolls are not registered as learned spells, equipped as weapons, or consumed.
The native inventory explicitly reports casting as unavailable.

The installed Skyrim SE master probe at levels 1, 10, and 50 still expands
8,922 stack occurrences from 2,409 leveled container seeds. Missing item
metadata fell from 207 to **54**, recovering all 153 scroll occurrences;
the remaining missing leaves are LIGH. This measures catalog coverage, not
randomized loot fidelity or scroll casting. Plugin tests pass **982**, with
27 ignored; native inventory tests pass **16**, with one real-data test ignored.
New tests cover full dispatch, model retention, load-order remapping, truncated
payloads, and the native/SDK category. All **70** mod-runtime tests pass,
including Scroll's WIT metadata conversion. Live gameplay verification was not
performed in that run. The missing `/dev/dri` observation was sandbox-local;
the live-environment recheck below establishes GPU access outside the sandbox.

#### Carryable light inventory metadata (2026-09-16)

LIGH dispatch now retains world-model/light data and adds item metadata only
when the authored Can Be Carried flag is set. A plugin override clearing the
flag removes earlier inventory metadata without deleting the world light.
Names, duration, and available value/weight are retained; native inventory and
SDK/WIT expose a distinct Light category. Equipping, held-light emission,
burn-time consumption, and loose-world pickup are not implemented by this
change; native inventory labels equipping unavailable.

Economic offsets follow xEdit's [Oblivion](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES4.pas),
[FNV](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFNV.pas),
[Skyrim](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas),
[FO4](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO4.pas), and
[FO76](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO76.pas)
definitions, rather than guessing from payload length. Starfield uses its
[DAT2 layout](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsSF1.pas);
that block has no economic fields, so those remain zero rather than interpreting
photometric data as value/weight. Optional absent economic fields also stay zero.

The installed-master container probes recovered **54** Skyrim and **375**
Oblivion light-stack occurrences. All **8,922** measured Skyrim stack occurrences
now have catalog metadata. Oblivion has **165** missing entries out of **7,947**,
all APPA. These figures cover deterministic leveled container expansion at
levels 1, 10, and 50, not all inventories or full gameplay compatibility.
Plugin tests pass **986** (27 ignored); native inventory tests pass **17**
(one ignored). Tests cover per-game layouts, carryability, truncated data,
world-light preservation, plugin override/remapping, and native metadata.
All **71** mod-runtime tests pass, including Light's WIT category conversion.
Live equipped-light behavior remains unverified and unimplemented.

#### Alchemy apparatus inventory metadata (2026-09-16)

APPA now enters the item catalog alongside its existing apparatus and world-model
indexes. The Oblivion parser decodes its packed 13-byte DATA (type, value,
weight, floating quality); Skyrim's distinct DATA and integer QUAL tier are
decoded separately, following xEdit's
[TES4](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES4.pas) and
[TES5](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas)
definitions. Other game families retain the existing name/model index without
guessing an apparatus payload layout. Truncated fields remain absent/default.
Native inventory and SDK/WIT now expose the Apparatus category; crafting is
explicitly unavailable, and apparatus is not treated as wearable equipment.

The installed Oblivion master probe recovered the remaining **165** apparatus
stack occurrences: all **7,947** measured leveled-container stack occurrences
now have catalog entries. Skyrim remains at **8,922**, also with zero missing
entries. This is catalog coverage at levels 1, 10, and 50, not proof that all
item data or actions are correct. Alchemy effects, crafting, and live gameplay
validation remain separate work. The plugin suite passes **989** tests with
27 ignored, including packed-layout, truncation, remapping, world-model, and
legacy-index preservation regressions.
Native inventory tests pass **18** (one ignored), and all **72** mod-runtime
tests pass, including Apparatus's WIT category conversion.

#### Native restorative consumables (2026-09-16)

The native inventory now offers **Use** for supported immediate Skyrim and
Fallout 3 restoratives. ALCH EFID/EFIT chains retain remapped effect identities and authored
magnitudes; the per-game MGEF reader identifies beneficial vital-value modifiers
using [xEdit's TES5 layouts](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas).
Resolution uses the existing canonical actor-value lookup, including Skyrim's
`AVHealth` / `AVMagicka` / `AVStamina` spelling. Every effect must resolve before
an item becomes usable; unsupported mixed potions cannot be partly applied.

Use validates the player, selected stack identity/count, equipment state, live
instance handle (when present), and all affected actor values before changing
anything. It restores damage through
`ActorValues::restore`, decrements one item, keeps zero-count slots to preserve
equipment indices, releases the last instance payload, and posts HUD feedback.
The changes live in the existing persisted ActorValues and Inventory components;
the effect catalog is rebuilt from plugin records, not serialized.

The installed Skyrim SE master yields **84** supported immediate restorative
ingestibles (90 structurally immediate chains, six with unsupported effects).
A real-master headless test runs `RestoreHealth01` (`0003EADD`) through the native
action: Health **40 → 65**, potion count **1 → 0**. Synthetic tests cover repeated
use, no overhealing, stale selection, dead players, missing effect targets,
equipment-index stability, last-instance cleanup, and rejection of poison,
scripts, conditions, timed/area effects, and malformed payloads. A headless egui
pointer test clicks the actual Use button and checks its identity-bearing action.
Verification: **993** plugin tests, **21** native inventory tests (lock-order
checking enabled), **12** debug-UI tests, and **64** existing save-I/O tests
passed. The real-master healing test was run explicitly and passed separately.

Consumable persistence now has synthetic and installed-master disk round-trip
tests. They save before use, after one use, and after depletion, then load those
slots backward, forward, and repeatedly into the same reconstructed world using
the production resource restoration, FormID remapping, and mutable-delta helpers.
Assertions cover health and stack counts together, unchanged equipment indices,
colliding item-instance handles, final-instance cleanup, no replayed use feedback,
and further consumption after loading. **66** save-I/O tests passed with lock-order
checking enabled, including the explicitly enabled installed-Skyrim test. This
is headless disk/overlay verification; it does not execute the Vulkan cell-reload
or full pending-load orchestration path.

A subsequent preflight regression reproduced consumption through a freed
instance handle. Consumption now rejects missing pools and freed handles before
changing health, count, or feedback; tests cover both final-item and multi-item
stacks and verify that rejection leaves arena slot reuse intact. The full engine
binary suite passed with lock-order checking: **2,184 passed, 29 ignored**.

The FO3/FNV path now decodes 20-byte integer-magnitude EFITs, self delivery,
72-byte MGEF DATA, and game-local Health/ActionPoints AV indices according to
[xEdit's FO3](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO3.pas)
and [FNV layouts](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFNV.pas).
ENIT's three padding bytes are ignored (shipping records contain `CD CD CD`),
not interpreted as Skyrim poison flags. Counter-effect padding is also ignored.
Skill/attribute-scaled MGEFs remain unsupported; EFIT's redundant AV cache does
not override the MGEF target. Synthetic tests cover these distinctions and
reject truncated/cross-game payloads, non-self delivery, addiction, timed/area
effects, scripts, and conditions.

Installed-master probes identify **five** immediate FO3 restoratives (apple,
carrot, pear, potato, purified water), from seven immediate ALCH chains; FNV has
**zero** supported restoratives from one immediate chain. Both games' Stimpaks
contain CTDA branches, and FNV additionally contains timed healing: neither is
flattened or enabled by this change. The FO3 `WaterPurified` (`000151A3`) test
uses the native action, real disk save/load commands, and live-overlay helpers:
Health **40 → 60 → 80**, stack **2 → 1 → 0**, with backward/forward/repeated loads.
All three consumable disk/overlay tests (synthetic, Skyrim, FO3) passed with
lock-order checking; the plugin suite passed **994 tests, 27 ignored**.

The next ingestion step retains `Aid::authored_effects` separately from the
executable immediate subset. Each effect keeps its duration/area, FO3/FNV
delivery and cached AV, remapped condition FormIDs/global comparands, OR ordering,
string parameters, and original condition flag bytes. Malformed or orphaned
conditions invalidate the chain rather than being dropped. Parsing this metadata
does **not** enable conditional consumption. Installed-master tests verify FO3
Stimpak's two perk-gated branches (30/36) and FNV's four branches (5/6 over six
seconds and 30/36 immediately). The installed FO3 `RestoreHealthStimpak` MGEF also
uses archetype **34**, not the supported value-modifier archetype **0**; condition
evaluation alone is therefore insufficient to enable it. The plugin suite now
passes **996 tests, 28 ignored**, with the installed FO3/FNV chain test explicitly
enabled and passing separately.

Conditional immediate restoration is now connected to native consumption for
the checked `HasPerk` subset (Skyrim function 448; FO3/FNV 449). A separate plan
compiler checks every branch before execution: self/target context, literal
comparands, recognized flags/comparators, supported instantaneous MGEFs, and a
script/poison/addiction-free item header. Unknown functions are not delegated to
the general evaluator's zero fallback, which would make `unknown == 0` succeed.
Known condition lists use the existing OR/AND evaluator at the time of each use;
the caster and target both refer to the consuming player. A native test verifies
that gaining a perk selects the other branch without reinstalling the catalog,
and that all-false conditions change neither health nor inventory. Unsupported
branches cannot be hidden behind a false condition. The plugin suite passed
**997 tests**, and native inventory tests passed **23 tests** with lock-order
checking. This does not implement Stimpak archetype 34 or timed healing.

The installed FO3 master supplies one newly executable conditional item:
`BloodPack` (`00034051`), one unconditional health point plus 19 gated on perk
`00003131`. An explicit real-master native test verifies **40 → 41** without the
perk, then **41 → 61** after granting it in the same world/catalog, consuming one
item per use. Its no-perk path also passes the repeated disk/load-overlay test.
FO3 now exposes five unconditional restoratives plus this conditional one;
Skyrim remains at 84 unconditional/zero conditional, FNV at zero/zero. The full
engine binary suite passed **2,185 tests** before the additional ignored
real-BloodPack test, and both real-FO3 tests passed explicitly with lock-order
checking. No live Vulkan smoke was performed.

Perk ownership now participates in save/load rather than being treated as
spawn-only metadata. `Perks` and nested `PerkRank` serialize stable perk FormIDs
and ranks. Its opt-in replacing column restores populated/empty rows and clears
a saved absence on FormID-matched entities, including the persistent player;
unmatched entities remain untouched. Other component registrations stay additive.
An empty replacing column is retained in the snapshot, whereas an entirely
missing column is not interpreted as a removal. Typed decoding precedes any
replacement, and the overlay policy participates in the registry fingerprint.
**Compatibility:** adding this column changes the save schema fingerprint;
older snapshots are rejected, not migrated or silently default-filled.

A regression first reproduced loss of a saved rank. It now passes populated,
empty, and absent ownership loads backward/forward into the same live world.
The real BloodPack disk/overlay test checks both the 1-point and 20-point paths,
deliberately installing the opposite outgoing perk state before every load.
Verification: **55 save-library tests** and **68 save-I/O tests**, including
explicit installed-master tests, passed; save-I/O ran with lock-order checking.
This remains a headless overlay check, not the Vulkan cell-reload smoke.

The full engine regression suite subsequently passed **2,186 tests** with 31
ignored and lock-order checking enabled. Its first run took 415 seconds with
the last footstep test waiting alongside an ALSA/PipeWire thread. Footstep,
water-event, and reverb logic fixtures now use explicit `AudioWorld::headless()`
instead of opening the host audio device; the next full run took 5.90 seconds.
Production `new()`/`Default` still initialize audio normally. A new audio test
checks that explicit headless playback retains neither queued sounds nor their
shared references. This verifies logic without claiming audible playback or a
diagnosis of the unsymbolized backend wait.

Constant-rate, non-recover restorative Value Modifiers now run over their
authored duration. The inventory catalog uses one checked plan for immediate,
conditional, and timed branches. Use consumes one stack entry; timed branches
restore no health upfront, then tick in simulation seconds with the last frame
clamped to the remaining duration. Zero/invalid deltas do not advance effects,
overheal is discarded, and dead/zero-health actors cannot be revived by ticking.
`TimedRestorations` persists source/AV FormIDs, rate, and remaining time with
authoritative-absence replacement. The new registry column changes the schema
fingerprint again: earlier saves are rejected, not migrated.

Installed New Vegas data now exposes **two** base-restoration plans:
`NVBitterDrink` (`001613BD`, 2 health/second for 18 seconds) and `BloodPack`
(`00034051`, 1 immediate health plus 4/second for 5 seconds with Hematophage).
Real-master disk tests verify midpoint save/load, exact remaining restoration
after an oversized frame, repeated before-use/midpoint loads, inventory counts,
and Blood Pack's no-perk 1-point path. Skyrim remains at 84 immediate plans and
FO3 at five unconditional plus one conditional plan; neither adds a timed item
under the current capability check. Verification: **2,188 engine tests** passed
(32 ignored), **70 save-I/O tests** passed including installed masters with
lock-order checking, and **998 plugin unit tests**, one integration test, and
two doctests passed.

The timed decoder distinguishes FO3/FNV No Duration (bit 7) from Skyrim's bit 9;
it excludes no-death-dispel effects, and Skyrim keyword-dispel, no-recast, and
nonzero taper-duration effects. Layouts are checked against xEdit's
[FNV definitions](https://raw.githubusercontent.com/TES5Edit/TES5Edit/dev-4.1.6/Core/wbDefinitionsFNV.pas)
and [TES5 definitions](https://raw.githubusercontent.com/TES5Edit/TES5Edit/dev-4.1.6/Core/wbDefinitionsTES5.pas).
Rates above are authored base rates, not a claim of Survival/Medicine/perk
scaling parity. Each consumed timed instance currently runs independently;
game/equip-category-specific stacking and replacement remain to be verified
and implemented. These are headless native-action/overlay checks, not a live
Vulkan consumption or door-transition smoke.

Stimpak (`00015169`) now has a native consumption path in both FO3 and FNV.
The FO3/FNV Value and Parts archetype expands into Health plus the seven
canonical body-condition AVIFs. Those actor values are seeded at 100 for
populated FO3/FNV actors and share the existing saved damage/restoration layers;
no duplicate limb-health store was introduced. Missing mappings or runtime
values reject consumption before any inventory or health mutation. The mapping
and archetype follow the GECK [Stats List](https://geckwiki.com/index.php/Stats_List)
and [Base Effect](https://geckwiki.com/index.php/Base_Effect) descriptions.

ENIT's Medicine flag is now retained. Medicine-flagged restorative plans use
the live composed Medicine skill, clamped to its 0–100 range, and authored
`fMagicMedicineSkillBase`/`fMagicMedicineSkillMult` (documented defaults 1/2).
The magnitude multiplier is `base + multiplier * Medicine / 100`, per
[Ingestible Settings](https://geckwiki.com/index.php?title=Ingestible_Settings).
Timed doses snapshot their resolved rate at use time. This also scales FNV's
Bitter Drink, which its master flags as Medicine; the earlier 2/second figure
is its unscaled base, not its effect at a nonzero Medicine skill.

FNV's authored `IsHardcore` (586) conditions now select the saved
`HardcoreMode` resource. `hardcore on|off` changes that flag through the normal
console, and `hardcore` reports it. **This does not yet implement Hardcore
hunger, thirst, sleep, ammo weight, or companion-death rules.** Normal Stimpaks
restore health and limbs immediately; the authored Hardcore branch restores
health alone over six seconds. Fast Metabolism selects the authored 36 vs 30
immediate points, or 6 vs 5 points/second, before Medicine scaling. At Medicine
50, real-master tests verify 60/72 health and limb restoration in normal mode,
and the same health total over time with no limb healing in Hardcore mode.
Save tests cover both perk branches, mode restoration over the opposite live
flag, inventory/pool state, limb damage, and midpoint timed effects. Adding the
mode resource changes the schema fingerprint; prior snapshots are rejected.

Verification: **2,190 engine tests** passed (34 ignored), **71 save-I/O tests**
passed with installed masters and lock-order checking, the two explicit
real-Stimpak tests passed, and **999 plugin unit tests** plus **473 scripting
unit tests** passed. FNV now has three eligible restoration plans (Stimpak, Blood
Pack, Bitter Drink). This is still headless gameplay/overlay evidence; targeted
limb selection, combat limb damage/cripple penalties, Survival scaling,
ingestible stacking parity, and live Vulkan verification remain open.

This is not a general magic system: the remaining games' consumption, other timed effects,
poison application, addiction, scripted effects, other condition functions, linked abilities,
image-space effects, other perk/magnitude modifiers, audio/VFX, and item-use
script event delivery remain unimplemented. Unsupported items stay unavailable
without losing a stack. Live Vulkan gameplay and the consumption-specific full
save/reload smoke remain pending.

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

**Process-restart baseline (2026-09-16):**
[`p5-save-restart.sh`](../smoke-tests/p5-save-restart.sh) passed on FNV and
Skyrim SE using isolated saves and debug port 19876. Each run requires actual
bound-input displacement from startup, writes a validated save, terminates
the engine process, launches a fresh process with `--load 1`, checks the
restored grounded body against the saved pose, and saves again. FNV's original
loading cover presents before restore and dismisses afterward; Skyrim uses
the existing no-artwork fallback. The embedded NIF animation-player hierarchy
fix removed the previously observed save-validation blocker without weakening
validation. Logs and snapshots are retained in `/tmp/byro-p5-save-restart.gPuzHI`
(FNV) and `/tmp/byro-p5-save-restart.4M7liQ` (Skyrim SE).
FNV passed again with the final ordered loading-lifecycle assertion in
`/tmp/byro-p5-save-restart.RXMPkN`; shell syntax and missing-data/SKIP contract
checks also passed for both fixtures.

This closes only the pose/process-restart baseline, **not P5**: the native
F5/F9 event path, inventory/equipment/quest/world-state assertions, graceful
quit, debug Vulkan validation, and the 30-minute soak still need live gates.

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
