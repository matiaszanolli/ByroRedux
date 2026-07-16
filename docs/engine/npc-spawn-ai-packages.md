# NPC Spawn → AI Package Execution

Fourth in the cross-cutting series alongside [Pipeline Overview](pipeline-overview.md),
[Exterior Grid Streaming](exterior-grid-streaming.md), and
[Save/Load Round-Trip](save-load-roundtrip.md). This one traces an NPC_
record from cell-load spawn through AI package selection to an actor
actually running behavior — and it's the most incomplete of the four.
Say that up front rather than let the prose imply otherwise: of the
~17-value FO3/FNV package-procedure enum, **exactly two procedures
execute anything at runtime.** This doc documents what's real, not
what's planned.

> **Currency note.** Verified against the tree as of 2026-07-15. One
> stale comment found and fixed: `boot.rs`'s inline comment gating
> `BYRO_SANDBOX_SIT` (and its runtime log message) still describes the
> M42.0 float bug — "actors will float above seats until the sit-enter
> transition lands" — but that transition landed in M42.1
> (`c5dcad97`, 2026-07-12); `systems/sandbox.rs`'s own module doc
> already describes the fix correctly. Also fixed: ROADMAP.md's Known
> Issues list still carried `PACK records have stubs only — no
> evaluator (#446, M42)` as an open item, contradicting its own M42 row
> three sections above, which documents #446 as closed.
>
> **Update, same day (M42.3).** Wander (procedure type 5) now has a
> runtime too — §6 below. This is the engine's first NPC locomotion of
> any kind (Sandbox never needed one; it teleports onto a seat), and the
> pattern it establishes is the seam future procedures (Follow, Travel,
> Patrol, Guard, …) build on.

## 1. Spawn trigger: NPC_ vs. static

`cell_loader::references::load_references` (`byroredux/src/cell_loader/references/mod.rs:92`)
checks each REFR against the NPC index **before** the statics index —
deliberately: NPC_ records are *also* indexed as statics (they carry a
MODL body-mesh path), so if the statics check ran first every actor
would render as a single unskinned static mesh instead of dispatching
to actor spawn. A pre-fix regression test name preserves the symptom:
"61 statics hits, 0 NPCs spawned" for a 31-NPC test cell.

Dispatch branches on FaceGen strategy: `game.has_runtime_facegen_recipe()`
→ `npc_spawn::spawn_npc_entity` (`byroredux/src/npc_spawn.rs:671`, FO3/FNV
— morphs computed at load time); `game.uses_prebaked_facegen()` →
`npc_spawn::spawn_prebaked_npc_entity` (`npc_spawn.rs:1549`, Skyrim+ —
resolves a pre-baked FaceGen NIF by plugin+FormID).

## 2. What spawn produces

T-pose humanoid + skeleton + body/hand meshes + head mesh + FaceGen
morphs (via `byroredux_facegen::apply_morphs`, kf-era games only) —
confirmed current against README's "State" section, no drift found
there. Components stamped on the placement root: `Name`,
`FactionRanks`, `ActorValues` (via CHARAL's `derive_npc_actor_values` —
see [CHARAL](charal.md)), `CharacterLevel`/`Background`/`Perks`,
`Inventory` + `EquipmentSlots`, `AnimationPlayer` (idle clip, kf-era
only) — and, when the actor's packages include a Sandbox- or
Wander-type entry, `SandboxBehavior` or `WanderBehavior` respectively
(§4).

## 3. PACK record parsing

`crates/plugin/src/esm/records/mod.rs:587` dispatches `b"PACK"` groups
to `parse_pack` (`crates/plugin/src/esm/records/misc/ai.rs:178`),
populating `EsmIndex.packages: HashMap<u32, PackRecord>`. `PackRecord`
(`ai.rs:20`) decodes three sub-records: `PKDT` (flags +
`procedure_type`, a **single byte** — a pre-#446 bug had read this as a
polluted `u32`), `PSDT` (`PackSchedule { start_hour, duration_hours }`),
and `PLDT` (`PackLocation { location_type, target, radius }` — only 3
of the location-type variants carry a resolvable FormID). `PTDT`/`PTD2`
(target data) is **not parsed at all**. `NpcRecord.ai_packages: Vec<u32>`
(`crates/plugin/src/esm/records/actor.rs:190-191`, from `PKID`
sub-records) holds the NPC's package list in priority order.

## 4. Package selection: narrower than "priority stack" suggests

`active_package` (`ai.rs`, private) picks the **first** package in
priority order whose `PSDT` schedule covers a given hour (no `PSDT` =
always eligible) **and whose `CTDA` conditions pass** — that's the
selection logic as of M42.2. Package conditions *are* now evaluated
(they were not before 2026-07-15): `parse_pack` captures the flat CTDA
list onto `PackRecord.conditions`, and the selector takes a
caller-supplied `condition_met` predicate that the spawn site fills
with the M47.1 evaluator (`byroredux_scripting::condition::evaluate`).
The predicate lives at the caller because `scripting` depends on
`plugin`, not the reverse — the plugin crate carries the conditions but
can't reach the evaluator. **Fail-open on unimplemented functions:**
the M47.1 catalog covers ~15 of Bethesda's ~300 condition functions, so
if any condition in a package's list references an out-of-catalog
function, `package_conditions_pass` (`npc_spawn.rs`) treats the whole
list as passing rather than let an unevaluable `Func == 1` silently
resolve to `0.0 == 1` (false) and drop a package the engine can't
reason about. Only lists whose every function is implemented gate for
real — honoring the common `GetIsID` / `GetActorValue` /
`GetFactionRank` / `GetStage` cases without regressing the rest.

More significant: **selection runs exactly once, at spawn time**
(`npc_spawn.rs:1433-1479`), against whatever `GameTimeRes.hour` happens
to be at that instant. There is no per-frame or per-hour
re-evaluation — an NPC picked for a 20:00-22:00 sandbox slot keeps that
`SandboxBehavior` tag forever, regardless of in-game time passing. Of
the FO3/FNV procedure enum's values, only `PROCEDURE_SANDBOX = 12`
(`ai.rs`) and `PROCEDURE_WANDER = 5` (`ai.rs`, M42.3) have a name and a
consumer; the other ~15
(Find/Follow/Escort/Eat/Sleep/Travel/Accompany/UseItemAt/Ambush/
FleeNotCombat/CastMagic/Patrol/Guard/Dialogue/UseWeapon) are captured
as a raw integer and dispatched nowhere. `active_package_is_sandbox`/
`active_sandbox_location` and `active_package_is_wander`/
`active_wander_location` are independent mirror pairs — since an NPC's
active package is always a single winning `PackRecord`
(`active_package`'s `find`), the two checks are naturally mutually
exclusive per actor with no extra guard logic needed.

## 5. Sandbox seating

`active_package_is_sandbox`/`active_sandbox_location` (`ai.rs:147,159`)
feed `npc_spawn.rs`, which inserts `SandboxBehavior { search_radius }`
(`crates/core/src/ecs/components/sandbox.rs`) using the active
package's authored `PLDT.radius` when present. At runtime,
`sandbox_seat_system` (`byroredux/src/systems/sandbox.rs`) — **opt-in
only**, registered when `BYRO_SANDBOX_SIT` is set (`boot.rs`) —
finds the nearest unreserved `Furniture` sit marker within radius,
snaps the placement-root `Transform` onto it, and swaps
`AnimationPlayer` onto a sit-**enter** clip.
**Every** sit marker on a furniture is its own reservable seat, keyed
`(furniture entity, marker index)` (M42.2 seat-polish) — a multi-seat
piece (counter / bench / multi-chair table authored as one FURN with
several `BSFurnitureMarker` positions) seats one actor per marker; before
this the reservation was keyed by furniture entity alone and a six-stool
counter seated exactly one NPC. The seat pose comes from the sit-enter
clip (`sandbox_sit_enter_kf_path`, `npc_spawn.rs` — FNV/FO3 only,
`chairskirt_leftenter.kf`), parked at its **final frame**
(`local_time = duration`, `playing = false`).

That last detail is the M42.1 fix worth calling out explicitly: the
generic `dynamicidle_*` sit **loop** clips carry no `Bip01`/`Pelvis`/
`NonAccum` channel — they fold the limbs but never lower the body, so
an actor floated ~90 units above the seat (M42.0). FNV's sit-**enter**
transition clips do drive the accum root down onto the seat, and their
final frame is a complete grounded pose — so M42.1 holds that frame
instead of switching to the loop, and no walk-up/transition handling is
needed. Full detail and v0-scope caveats (nearest-chair-only, no
scoring/scheduling/wander/ownership, one enter clip for all sit
markers) live in `systems/sandbox.rs`'s module doc.

Search *center* is always the actor's own spawn position, not a
resolved `PLDT` target reference — a same-day investigation
(2026-07-14, see `SandboxBehavior`'s doc comment and `npc_spawn.rs`)
found only ~12% of vanilla FNV `NearReference`-type packages resolve to
anything spawnable, so FormID-based center resolution isn't planned as
a near-term follow-up.

## 6. Wander locomotion (M42.3)

The first non-Sandbox procedure runtime, and the first NPC locomotion
of any kind in the engine — Sandbox never needed one because it
teleports an actor onto a seat rather than walking there. Wired at
`npc_spawn.rs` exactly like Sandbox: `active_package_is_wander` +
`active_wander_location` gate a `WanderBehavior` insert
(`crates/core/src/ecs/components/wander.rs`), reusing the same
`game_hour`/`condition_met` closure Sandbox's block builds (both
package checks now run *before* either component insert, since
interleaving a read through `condition_met` — which closes over
`world` — after a `world.insert` is a genuine borrow-checker conflict,
not just a style preference).

`wander_system` (`byroredux/src/systems/wander.rs`, opt-in via
`BYRO_WANDER=1`, same gating convention as `BYRO_SANDBOX_SIT`) drives a
`WanderState` (home / target / phase / pick_count) per actor: walk
straight-line toward `target` via `Vec3::move_towards` (no pathing —
open ground only), ground-snap Y each tick via
`PhysicsWorld::cast_ray_down` (the same downward-raycast mechanism
`scene.rs` uses for camera placement), turn to face the direction of
travel via `Quat::slerp`, and on arrival pause for a few seconds before
picking a new point. Target points and pause durations are **not**
`rand`-crate randomness — both are derived from a SplitMix64-style
avalanche hash seeded on `(WanderBehavior::form_id,
WanderState::pick_count)`, the same no-RNG-dependency, save/reload-
stable convention `npc_spawn.rs::idle_desync` uses for per-actor idle
phase/speed desync.

v0 scope, mirroring Sandbox's own documented approximations: no
animation-clip swap (no verified walk `.kf` path exists anywhere in
this codebase — verifying one against a real archive is deferred, not
guessed at); no target-reference resolution (center is always the
actor's own position, same call Sandbox made and for the same reason);
no per-frame package re-evaluation (same limitation as Sandbox).

## What's not covered / honest state

- **Two procedures of ~17 execute.** Sandbox and Wander (M42.3). No
  Follow/Escort/Guard/Patrol/Travel/etc. runtime exists anywhere in the
  engine.
- **No general AI tick.** `byroredux/src/systems/` has no `ai.rs` /
  `behavior.rs` / `npc.rs`; `sandbox_seat_system` and `wander_system`
  are the only AI-adjacent per-frame systems, and neither is part of
  the default scheduler — they require `BYRO_SANDBOX_SIT=1` /
  `BYRO_WANDER=1` respectively.
- **Selection is spawn-time-only.** No package re-evaluation as game
  time advances — `CTDA` conditions *are* now evaluated (M42.2), but
  only once, against the game hour and world state at spawn. `PTDT`/
  `PTD2` target data still isn't parsed.
- **Sit-enter clip coverage is FNV/FO3-only** — `None` for Oblivion
  (deferred), and for Skyrim+/FO4+/FO76/Starfield, whose actors animate
  through Havok `.hkx`, not `.kf`, so this whole mechanism doesn't apply
  there yet.

This is the M42 *bootstrap*, by its own module docs' framing — "v0 is
sit in the nearest free chair, once" for Sandbox, "v0 is walk to a
random point, pause, repeat" for Wander. Anyone building on this
pipeline should treat package selection and procedure dispatch as a
proof of concept, not a general AI system.
