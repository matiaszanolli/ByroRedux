# M27 — Phase 1 Survey

**Goal**: enumerate every parallel-stage system's actual reads/writes/structural-mutations so Phase 2 (migrate to `add_to_with_access`) is a mechanical edit and Phase 3 (resolve real conflicts) has a target list.

**Registration site**: [byroredux/src/main.rs:504-591](../../byroredux/src/main.rs#L504-L591). Counted `parallel-scheduler` is **default-on** in `crates/core/Cargo.toml:7-8`; the binary is running rayon today. The gate from the ROADMAP isn't "flip the feature" — it's "prove conflict-freeness."

## Headcount (current, vs ROADMAP's stale "4 unknown pairs")

- **12 parallel-stage systems** (3 declared today, 9 undeclared).
- **6 exclusive-stage systems** (already serial; not in conflict analysis).
- **Unknown pairs**: 13 (computed below, not 4 — ROADMAP figure is from a much earlier snapshot).

| Stage | Parallel systems | Declared today | Unknown pairs (this stage) |
|-------|------------------|----------------|----------------------------|
| Early | 4 | 1 (fly_camera_system) | C(4,2)=6, all of which are Unknown (3 declared-vs-undeclared + 3 undeclared-vs-undeclared) |
| Update | 2 | 1 (spin_system) | 1 |
| PostUpdate | 1 | 0 | 0 |
| Physics | 1 | 0 | 0 |
| Late | 4 | 1 (log_stats_system) | 6 |
| **Total** | **12** | **3** | **13** |

## Per-system access shape (the 9 undeclared)

Access shape derived by inspecting each function body for `world.query::<T>()` / `world.query_mut::<T>()` / `world.resource::<R>()` / `world.try_resource::<R>()` / `world.resource_mut::<R>()` / `world.try_resource_mut::<R>()`. Test-only registrations (in `#[cfg(test)]` blocks) excluded.

### 1. `character_controller_system` [Stage::Early] — `byroredux/src/systems/character.rs:77`

```rust
.reads_resource::<PlayerEntity>()
.reads_resource::<InputState>()
.reads_resource::<byroredux_physics::PhysicsWorld>()
.reads::<byroredux_physics::CharacterController>()
.writes::<byroredux_physics::CharacterController>()  // CC is read-then-write
.reads::<Transform>()
.writes::<Transform>()                                // Transform same
```
**Structural mutation**: none in body.
**Note**: there's a `*world.resource_mut::<PlayerMode>() = next;` at line 510, but that's in `toggle_player_mode(world: &mut World)` — an `&mut World` helper, not the system. The system only reads `PlayerMode` indirectly via gating (none observed; the gating happens at the caller side).

### 2. `weather_system` [Stage::Early] — `byroredux/src/systems/weather.rs:287`

```rust
.reads_resource::<WeatherDataRes>()
.writes_resource::<WeatherDataRes>()
.reads_resource::<WeatherTransitionRes>()
.writes_resource::<WeatherTransitionRes>()
.writes_resource::<GameTimeRes>()
.reads_resource::<CellLightingRes>()
.writes_resource::<CellLightingRes>()
.writes_resource::<SkyParamsRes>()
.writes_resource::<CloudSimState>()
```
**Structural mutation**: none in body. (`world.insert_resource(...)` calls at lines 1138/1160/1177/1272/1292 are inside `#[cfg(test)]` blocks.)

### 3. `timer_tick_system` [Stage::Early] — `crates/scripting/src/timer.rs:28`

```rust
.writes::<ScriptTimer>()       // remove(entity)
.writes::<TimerExpired>()      // insert(entity, ...)
```
**Structural mutation**: none — `QueryWriteMut::insert(entity, T)` and `::remove(entity)` are storage-local mutations (per-storage RwLock), not World structural mutations. The `world.spawn()` calls in the file are in `#[cfg(test)]` only.

### 4. `animation_system` [Stage::Update] — `byroredux/src/systems/animation.rs:305`

```rust
.reads_resource::<AnimationClipRegistry>()
.writes_resource::<AnimationClipRegistry>()   // both touched on cache-rebuild path
.reads_resource::<SubtreeCache>()
.writes_resource::<SubtreeCache>()
.reads_resource::<NameIndex>()
.writes_resource::<NameIndex>()
.writes_resource::<StringPool>()
.reads::<Name>()
.writes::<Transform>()
.writes::<RootMotionDelta>()
.writes::<AnimatedVisibility>()
.writes::<AnimatedDiffuseColor>()
.writes::<AnimatedEmissiveColor>()
.writes::<AnimatedAlpha>()
.writes::<AnimatedUvTransform>()
.writes::<AnimatedShaderFloat>()
.writes::<AnimatedMorphWeights>()
.writes::<byroredux_core::ecs::LightSource>()
.writes::<AnimationPlayer>()
.writes::<AnimationTextKeyEvents>()
.writes::<AnimationStack>()
```
**Structural mutation**: none in body. (Many helpers, but they all route through queries.)
**Note**: this is the largest access set by far. Likely an over-declaration — many of these are written only on specific code paths (e.g., `AnimatedShaderFloat` only when shader-animated content is present). The declaration must be the union of all paths.

### 5. `make_transform_propagation_system()` [Stage::PostUpdate] — `crates/core/src/ecs/systems.rs:41`

```rust
.reads::<Parent>()
.reads::<Children>()
.reads::<Transform>()
.writes::<GlobalTransform>()
```
**Structural mutation**: reads `world.next_entity_id()` for a generation key (read-only — fine; not a write).
**Note**: this is a closure factory (`impl FnMut`). The blanket impl gives it `access() = None`. Migration requires either (a) wrapping in a struct that impls `System` with `access()` overridden, or (b) using `add_to_with_access` which carries the declaration as scheduler-side metadata. Path (b) is the existing pattern.

### 6. `physics_sync_system` [Stage::Physics] — `crates/physics/src/sync.rs:75`

```rust
.reads_resource::<PhysicsWorld>()
.writes_resource::<PhysicsWorld>()             // pw.step(dt) on line 92
.reads::<CollisionShape>()
.reads::<RigidBodyData>()
.reads::<GlobalTransform>()
.writes::<Transform>()                          // pull_dynamic writes back
.reads::<RapierHandles>()
.writes::<RapierHandles>()                      // register_newcomers writes
```
**Structural mutation**: none in body. (`register_newcomers` uses `world.query_mut::<RapierHandles>().insert(...)` — storage-local.)

### 7. `camera_follow_system` [Stage::Late] — `byroredux/src/systems/character.rs:332`

```rust
.reads_resource::<PlayerEntity>()
.reads_resource::<ActiveCamera>()
.reads_resource::<InputState>()
.reads::<byroredux_physics::CharacterController>()
.reads::<GlobalTransform>()
.writes::<Transform>()
.writes::<GlobalTransform>()
```
**Structural mutation**: none in body.

### 8. `reverb_zone_system` [Stage::Late] — `byroredux/src/systems/audio.rs:40`

```rust
.reads_resource::<CellLightingRes>()
.writes_resource::<byroredux_audio::AudioWorld>()
```
**Structural mutation**: none. Self-contained — just reads cell type, mutates the audio world's reverb send.

### 9. `audio_system` [Stage::Late] — `crates/audio/src/lib.rs:638`

```rust
.writes_resource::<AudioWorld>()
.reads::<AudioListener>()
.reads::<GlobalTransform>()
.reads::<OneShotSound>()
.writes::<OneShotSound>()                       // prune_stopped_sounds removes via query_mut
.reads::<AudioEmitter>()
.writes::<AudioEmitter>()                       // prune sweep removes ended looping sounds
```
**Structural mutation**: none in body. (The `world.spawn()` at end of file is in tests.)

## Predicted conflict graph (after declarations land)

The conflicts the analyzer will surface once Phase 2 ships, computed from the union of declared accesses above + the 3 already-declared systems. **Each row below would surface as a `CONFLICT` in `sys.accesses`** — these are the ones Phase 3 has to resolve, NOT bugs to fix at declaration time.

### Stage::Early (4 systems → 6 pairs)

| Pair | Conflict | Why |
|------|----------|-----|
| `fly_camera_system` ↔ `character_controller_system` | Transform WriteWrite, PhysicsWorld WriteRead, InputState ReadRead (no conflict — both read) | Both write Transform; both touch PhysicsWorld. Today they avoid colliding because each gates on `PlayerMode` — but the analyzer can't see that. |
| `fly_camera_system` ↔ `weather_system` | none | Disjoint access sets |
| `fly_camera_system` ↔ `timer_tick_system` | none | Disjoint |
| `character_controller_system` ↔ `weather_system` | none | Disjoint |
| `character_controller_system` ↔ `timer_tick_system` | none | Disjoint |
| `weather_system` ↔ `timer_tick_system` | none | Disjoint |

**One real conflict** in Early (`fly_camera ↔ character_controller`). The 5 others should report `AccessConflict::None`.

### Stage::Update (2 systems → 1 pair)

| Pair | Conflict | Why |
|------|----------|-----|
| `animation_system` ↔ `spin_system` | Transform WriteWrite | Both write Transform. Real conflict, will serialize via RwLock today. |

### Stage::PostUpdate (1 parallel system → 0 pairs)

`transform_propagation_system` is alone in the parallel batch; the 5 exclusive systems (`footstep_system`, `particle_system`, `billboard_system`, `world_bound_propagation_system`, `submersion_system`) run serially after it. No parallel-pair conflicts.

### Stage::Physics (1 parallel system → 0 pairs)

`physics_sync_system` is alone.

### Stage::Late (4 systems → 6 pairs)

| Pair | Conflict | Why |
|------|----------|-----|
| `camera_follow_system` ↔ `reverb_zone_system` | none | Disjoint |
| `camera_follow_system` ↔ `audio_system` | GlobalTransform ReadWrite is actually ReadRead (no conflict). Transform WriteRead — but audio_system only reads GlobalTransform, not Transform. Actually: `camera_follow` writes Transform + GlobalTransform; `audio_system` reads GlobalTransform → **GlobalTransform WriteRead conflict**. | camera_follow writes camera pose; audio_system reads listener pose. Real ordering dependency — camera must finish before audio listener-sync runs. |
| `camera_follow_system` ↔ `log_stats_system` | none | log_stats only touches resources |
| `reverb_zone_system` ↔ `audio_system` | AudioWorld WriteWrite | Both write AudioWorld. Real conflict — these must serialize. |
| `reverb_zone_system` ↔ `log_stats_system` | none | Disjoint |
| `audio_system` ↔ `log_stats_system` | none | Disjoint |

**Two real conflicts** in Late: `camera_follow ↔ audio_system` (GlobalTransform ordering) and `reverb_zone ↔ audio_system` (AudioWorld WriteWrite). The first one has an existing comment at main.rs:563-567 acknowledging the order — `camera_follow` "MUST run BEFORE `audio_system`" — but the scheduler today picks an arbitrary parallel order. The RwLock makes it correct, but slow; a 4-way Late stage with 2 conflicts will serialize ~half the work.

## Phase 3 candidates (resolutions to consider)

| Conflict | Suggested resolution |
|----------|---------------------|
| `fly_camera ↔ character_controller` on Transform/PhysicsWorld | **Merge** into one Stage::Early system that branches on `PlayerMode`. The two systems are mutually-exclusive-at-runtime by design (line 515-518 comment confirms). One system removes the conflict cleanly. |
| `animation_system ↔ spin_system` on Transform | **Move `spin_system` to a separate sub-stage** OR mark exclusive. `spin_system` is the spinning-cube demo system — low-priority gating. Likely just move to exclusive in PostUpdate (or remove from the binary's default schedule for non-demo builds). |
| `camera_follow ↔ audio_system` on GlobalTransform | **Sequence via stage split**: move `audio_system` to a new `Stage::AudioListener` after `Stage::Late`, or just mark `audio_system` exclusive in Late (it runs after the parallel batch finishes, which is what main.rs:563-567's comment requests). The latter is one-line. |
| `reverb_zone ↔ audio_system` on AudioWorld | Same fix as above — `audio_system` as exclusive sequences it after `reverb_zone` automatically (reverb_zone is parallel, audio_system is exclusive; exclusive runs after parallel batch). |

The first conflict (fly_camera ↔ character_controller) is the biggest architectural decision. Merging the two systems is the cleanest path but requires touching the PlayerMode gating, which lives across two files today.

## Existing declarations have bugs too

Surveying the 3 already-declared systems against their bodies surfaced a missing read:

### `fly_camera_system` (declared at main.rs:508-513) — MISSING `PlayerMode` read

Body at [camera.rs:15-25](../../byroredux/src/systems/camera.rs#L15-L25) reads `PlayerMode` first thing (the early-return gate against character mode):
```rust
let mode = world.try_resource::<PlayerMode>().map(|r| *r).unwrap_or_default();
if mode == PlayerMode::Character { return; }
```
The declaration omits `.reads_resource::<PlayerMode>()`. **This is a latent bug**: if anything ever flips `PlayerMode` from a parallel system in the same stage, the access analyzer would falsely report no conflict. Today no system writes `PlayerMode` from a parallel stage (the only writer is `toggle_player_mode` which takes `&mut World`), so it doesn't surface, but the declaration should be honest. **Phase 2 must fix this in the same pass as the new migrations.**

### `spin_system` (declared at main.rs:524-525) — OK

Body uses `world.query_2_mut::<Spinning, Transform>()`. Declaration is `.reads::<Spinning>().writes::<Transform>()`. The `query_2_mut::<Spinning, Transform>` returns `(QueryRead<Spinning>, QueryWriteMut<Transform>)`, so Spinning is read-only — declaration is accurate.

### `log_stats_system` (declared at main.rs:584-589) — OK

Body uses `world.resource::<TotalTime>()`, `world.resource::<DeltaTime>()`, `world.resource::<DebugStats>()`. All declared as reads. Accurate.

## Phase 2 entry checklist

1. For each of the 9 undeclared parallel systems, replace `scheduler.add_to(stage, fn)` with `scheduler.add_to_with_access(stage, fn, Access::new().<reads/writes...>)`.
2. After each migration, re-run `cargo test` + `cargo check` to confirm no compile breakage.
3. After all 9 land, run the binary briefly (or write a one-shot test that builds the same schedule) and dump `sys.accesses`. Expected: 0 Unknown pairs, ~5 Conflict rows.
4. Phase 3 then triages each Conflict.

## What's NOT in scope for Phase 2

- **Merging fly_camera + character_controller** (architectural — Phase 3).
- **Moving spin_system out of Update** (gameplay — Phase 3).
- **Re-staging audio_system as exclusive** (Phase 3, but trivially mechanical).
- **Removing the parallel-scheduler feature flag** (it's default-on; the feature flag stays as a runtime-disable knob for diagnostics).
- **Migrating exclusive systems to declared access** (exclusive-phase systems aren't paired against each other; declaring their access is nice-to-have for documentation but doesn't change conflict-graph quality).

## Source of truth

This survey was produced by static inspection of [byroredux/src/main.rs:504-591](../../byroredux/src/main.rs#L504-L591) registrations + each system's function body. The actual `sys.accesses` snapshot from a running engine will validate or disprove the predicted conflict graph above; Phase 2 produces that snapshot as a side effect.
