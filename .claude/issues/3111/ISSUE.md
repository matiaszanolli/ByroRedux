# Issue #3111: player_controller_system reads WindField undeclared while its parallel Stage::Early sibling weather_system writes it every tick

- **Finding ID**: `ECS-2026-08-20-01`
- **Severity**: HIGH
- **Labels**: `high,ecs,bug`
- **Source report**: `docs/audits/AUDIT_ECS_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3111

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3111 --json state`.

---

**Severity**: HIGH
**Dimension**: 5b — Scheduler Access Declarations
**Source**: `docs/audits/AUDIT_ECS_2026-08-20.md` (`ECS-2026-08-20-01`), independently corroborated by `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md` (`CONC-2026-08-20-01`, also HIGH). Filed once, from ECS.

**Location**
- `byroredux/src/boot.rs:691-724` — the `player_controller_system` `Access` chain (the gap)
- `byroredux/src/systems/character.rs:911-916` — the undeclared read
- `crates/physics/src/water.rs:323-327` — where the read actually lands
- `byroredux/src/systems/weather.rs:690-692` + `byroredux/src/boot.rs:726-740` — the concurrent writer

## Description

The WATAL wave work added a `WindField` read to the character controller's water path:
`player_controller_system` → `character_controller_system` → `player_water_state` →
`byroredux_physics::weather_wave_adjustment(world, time)` → `world.try_resource::<WindField>()`.

`player_controller_system` is registered with `add_to_with_access(Stage::Early, …)` — i.e. it is in the
**parallel batch** — and its `Access` declaration lists `TotalTime` (added in the same delta) but **not**
`WindField`. `weather_system` is the second member of that same parallel batch and takes a
`try_resource_mut::<WindField>()` write every tick.

`Stage::Early`'s parallel batch is exactly three systems (`boot.rs:692`, `:727`, `:743`):
`player_controller_system`, `weather_system`, `timer_tick_system`. `scheduler.rs:502-503` dispatches the
batch through `par_iter_mut().for_each`.

The session-70 commit that added the water sampling *did* extend the declaration — `TotalTime`,
`ActorVitals`, `WaterPlane`, `WaterVolume`, `WaterFlow`, `ActorValues`, `Dead` were all added in this
delta. `WindField` was missed because it is not named anywhere in `character.rs` — it is reached one
call frame down, inside the physics crate.

## Evidence

`byroredux/src/systems/character.rs:912-916` — unconditional on the `PlayerMode::Character` path
(`player_water_state` is called at `:224`, before any early-return that could skip it):

```rust
let wave_height = world
    .try_resource::<TotalTime>()
    .map(|time| {
        let (weather_scroll, wind_wave_scale) =
            byroredux_physics::weather_wave_adjustment(world, time.0);
```

`crates/physics/src/water.rs:323-327` — the read reached from the controller:

```rust
pub fn weather_wave_adjustment(world: &World, time_secs: f32) -> ([f32; 2], f32) {
    let wind = world
        .try_resource::<WindField>()      // <- the undeclared read
        .map(|field| *field)
        .unwrap_or_default();
```

`byroredux/src/systems/weather.rs:690-691` — the writer, unconditional, same parallel batch:

```rust
if let Some(mut wind) = world.try_resource_mut::<WindField>() {
    *wind = WindField::from_weather_byte(weather_wind_speed, wind.direction);
}
```

`grep -c WindField` over `boot.rs:691-724` (the whole `player_controller_system` `Access::new()` chain)
returns **0**. The chain declares 12 resources and 10 components; `WindField` is not among them.

### Trigger conditions (from the concurrency-side analysis)

Any frame in which (a) the player controller is the active `PlayerMode` branch, (b) the player capsule's
XZ column intersects a `WaterVolume` — i.e. standing in or wading through water — and (c) rayon schedules
`weather_system` and `player_controller_system` onto different workers, which is the normal case for a
3-system parallel batch on a 16-core machine. The write is only *interesting* on frames where the WTHR
wind byte changes (weather transition, worldspace entry), but the unsynchronised read/write pair exists
on every such frame regardless.

**Not a deadlock.** The concurrency pass checked for a cycle and there is none: `weather_system` takes
the `WindField` write guard standalone, holding nothing else, while `player_controller_system` holds
`WaterPlane`/`WaterVolume`/`WaterFlow` storage reads plus a `TotalTime` resource read when it asks for
`WindField`. No edge runs the other way.

## Impact

Two failure modes, one structural and one observable.

**Structural (the reason this is HIGH):** the M27 access model's core promise —
"`known_conflict_count() == 0` ⇒ no two parallel same-stage systems touch the same component or resource
with a write" — is false at HEAD, and the guard built to enforce it (`boot.rs:1449-1454`,
`debug_assert_eq!(report_snapshot.known_conflict_count(), 0, …)`) cannot see the violation.
`AccessConflict`'s resource rules (`crates/core/src/ecs/access.rs:198-213`) classify read-vs-write on the
same resource as `ConflictKind::ReadWrite`, so declaring the read honestly would make the boot guard
**fail** — the fix is a schedule change, not a one-line declaration. Until then `sys.accesses` reports 0
conflicts from an incomplete declaration, and every future system added to `Stage::Early` is analysed
against a false premise. Dimension 3 of the concurrency audit uses that same promise to argue
cross-thread ABBA is structurally unreachable among parallel systems.

**Observable:** the player's water surface height is computed from wind that may be either this frame's
or last frame's, non-deterministically per frame and per machine, on the exact frames a weather
transition is in flight. Magnitude is one wave-amplitude step (`wind_wave_scale` spans 1.0–1.5), so it is
small and transient — but it feeds `swimlevel_reached`, which is a *boolean* state transition
(walk <-> swim). At the swim threshold a sub-frame wind difference is enough to flip it, and a flip that
alternates frame-to-frame is a visible controller-state strobe. Blast radius is one system, but it is the
player's.

## Related

- #1787 / CONC-D4-01 (`ContactConfig` undeclared on `physics_sync_system` — same shape, no parallel
  writer, CLOSED)
- #2676 / CONC-D3-NEW-02 (`camera_follow_system` reads `PlayerMode` undeclared — same shape, no parallel
  writer, CLOSED; fix verified intact this run, `boot.rs:1276-1295` carries the declaration and its
  rationale comment)
- #2389, #1602, #1394 (the boot guards)

Distinct from all of these: they had **no concurrent writer in the same stage**. This one does — the
first instance of this defect class with a live parallel writer.

Sibling declaration gaps found in the same sweep, filed separately (no live writer, hence lower
severity): `physics_sync_system` / `make_animation_system` / `make_billboard_system`.

## Suggested Fix

Adding `.reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()` to the
`player_controller_system` `Access` chain is correct but **will make the debug assertion at
`boot.rs:1449` fire** — that is the true state of the batch. Resolving it needs a scheduling decision:

- **(a)** Move `weather_system` to `add_exclusive_with_access(Stage::Early, …)`. It writes seven
  resources and reads none that a parallel sibling writes, so exclusivity costs nothing — the same
  treatment `audio_system` / `spin_system` got in M27 Phase 3. Smaller change, matches precedent.
- **(b)** Hoist the WTHR wind update into its own earlier stage so the write is complete before any
  reader runs.

Then add the declaration and confirm `known_conflict_count()` is still 0.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the three sibling declaration gaps
      (`physics_sync_system`, `make_animation_system`, `make_billboard_system`) and any other
      `add_to_with_access` chain whose body reaches a resource through a cross-crate call frame
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — ideally one that asserts
      `known_conflict_count() == 0` *after* the honest declaration is added, so the guard can no longer
      be kept green by an omission
