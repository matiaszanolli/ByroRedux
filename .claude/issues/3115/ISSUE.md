# Issue #3115: the two SplashEvent producers disagree with each other, and with the component's own doc, about which entity hosts the marker

- **Finding ID**: `ECS-2026-08-20-06`
- **Severity**: LOW
- **Labels**: `low,ecs,bug`
- **Source report**: `docs/audits/AUDIT_ECS_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3115

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3115 --json state`.

---

**Severity**: LOW
**Dimension**: 7 — Component Lifecycles
**Source**: `docs/audits/AUDIT_ECS_2026-08-20.md` (`ECS-2026-08-20-06`)

**Location**
- `crates/scripting/src/events.rs:45-56` — the documented contract
- `byroredux/src/systems/water.rs:275-292` — producer A (`submersion_system`), honours it
- `byroredux/src/systems/water.rs:419-432` — producer B (`make_water_interaction_system`), does not

## Description

`SplashEvent`'s doc is unambiguous:

> Fired when an actor enters or re-enters a water surface. The event lands **on the water-plane entity**
> so audio, gameplay and presentation systems can consume the same source interaction without inventing
> a second queue.

`submersion_system` honours that — it inserts on the `WaterVolume`/plane entity. But
`make_water_interaction_system`, in the same file, inserts `SplashEvent` on the **body** entity while
inserting its sibling `RippleEvent` on `surface_entity`. So within one function the two markers use
different host classes, and one of them contradicts the documented contract.

## Evidence

`byroredux/src/systems/water.rs:422-436` — `entries`' first tuple element is the body, its second is
`surface_entity`; the splash loop binds the first and discards the second:

```rust
if let Some(mut q) = world.query_mut::<SplashEvent>() {
    for &(entity, _, position, intensity, entering, _) in &entries {
        if entering {
            q.insert(
                entity,
                SplashEvent { actor: entity, intensity, position: [position.x, position.y, position.z] },
            );
```

The ripple loop directly above (`:410-421`) keys on `surface_entity`:

```rust
for (&surface_entity, &(entity, position, intensity)) in &ripple_by_surface {
    q.insert(surface_entity, RippleEvent { actor: entity, intensity, position: [...] });
}
```

Two of `entries`' six tuple fields (`surface_entity` and the trailing `submerged_fraction < 0.98` flag)
are never read at all.

## Impact

The only consumer today, `water_audio_system` (`byroredux/src/systems/audio.rs:244-256`), reads the event
payload's own `position` and ignores the host entity, so nothing is visibly wrong. The cost is future:
the contract exists so a scripting/quest consumer can attach to a water surface the way
`OnTriggerEnterEvent` attaches to a volume, and that consumer would silently see half the splashes.

Also, because `insert` overwrites, a `RippleEvent` written by `submersion_system` in PostUpdate is
clobbered by `make_water_interaction_system` in Late whenever a dynamic body shares the plane — the
camera's disturbance is dropped without trace.

## Related

None — both producers are new in this delta. (Not a duplicate of #2887 / #2888, which are WATAL
depth-measurement and overlapping-plane-selection defects, unrelated to event host entity.)

## Suggested Fix

Insert `SplashEvent` on `surface_entity` with `actor: entity` (matching `RippleEvent`'s existing shape
and the doc), drop the two dead tuple fields, and apply the same strongest-wins dedup the ripple path
already uses so two bodies entering one plane in the same frame do not silently collapse to whichever
iterated last.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `RippleEvent`'s two producers, and the
      `submersion_system` PostUpdate / `make_water_interaction_system` Late clobber ordering
- [ ] **TESTS**: A regression test pins this specific fix — assert the host entity of a `SplashEvent`
      produced by each of the two producers is the same class of entity
