# #3183 — AUD-2026-08-20-D7-01: water_audio_system mixes the position of one RippleEvent with the intensity of another

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3183
- **Labels**: `low,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: Gameplay Audio Wiring
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (`AUD-2026-08-20-D7-01`) — HEAD `bb0b92f2`

## Location

- `byroredux/src/systems/audio.rs` — the `RippleEvent` selection inside `water_audio_system`
- `byroredux/src/systems/audio.rs` — `WaterAudioState.ripple_cooldown` decay and the `= 0.18` re-arm
- `byroredux/src/components.rs` — the `WaterAudioState` resource

## Description

### 1. The ripple's position and its intensity come from different events

The selection takes its **position** from the first `RippleEvent` the query yields, but its
**intensity** from the maximum across *all* live `RippleEvent`s:

```rust
let ripple = world.query::<RippleEvent>().and_then(|events| {
    events.iter().next().map(|(_, event)| {
        (
            Vec3::new(event.position[0], event.position[1], event.position[2]),
            events.iter()
                .map(|(_, candidate)| candidate.intensity)
                .fold(0.0f32, f32::max),
        )
    })
});
```

When more than one `RippleEvent` is live the played sound is a hybrid of two different events: the
loudest disturbance's amplitude, placed at an unrelated surface's coordinates. Storage iteration order
is not a stable, meaningful ordering (both producers insert keyed by *surface* entity —
`byroredux/src/systems/water.rs`, camera path and physics path), so which event donates the position is
effectively arbitrary and can change frame to frame, moving the sound between surfaces.

### 2. One global cooldown throttles every water surface in the world

`WaterAudioState.ripple_cooldown` is a single scalar for the whole world. One ripple anywhere suppresses
ripples on **every other surface** for 180 ms — a player standing at the edge of one pond mutes the
stream running beside it, and vice versa, with no relation to which is closer or louder.

The `0.18` is also an undeclared magic number sitting inline in the system body, unlike
`INTERIOR_REVERB_SEND_DB` / `EXTERIOR_REVERB_SEND_DB` two functions above, which are named `const`s.

## Evidence

Multiple concurrent `RippleEvent`s are ordinary, not hypothetical:

- `submersion_system` inserts one **per water-volume entity** whose `disturbance_rate > 0`
  (`byroredux/src/systems/water.rs`, looping over every `(WaterVolume, ParticleEmitter)` pair), and
  exterior worldspaces spawn one water plane per cell — so a camera standing on a shared cell edge
  produces one event per adjacent plane in the same tick.
- `make_water_interaction_system` adds one more per wet surface.

The splash branch in the same function is handled **correctly** and is the pattern to mirror: it
collects **all** splashes and plays each at its own position with its own intensity.

## Impact

Cosmetic-to-mild audio artefact, confined to multi-surface situations: a ripple can be spatialised at
the wrong pond, and one surface's cooldown silences another's. It does not leak, panic, or accumulate.

Per **#3178** it is currently masked by the fact that the ripple is barely audible at all — which is
exactly why it is worth fixing *before* the unit-conversion fix makes these sounds actually reach the
listener.

## Suggested Fix

- Select one ripple **as a unit** — e.g.
  `events.iter().max_by(|a, b| a.1.intensity.total_cmp(&b.1.intensity))` — so position and intensity
  come from the same event.
- Key the cooldown by surface entity (a small `FxHashMap<EntityId, f32>` on `WaterAudioState`, or at
  minimum reset it only for the surface that actually played).
- Promote `0.18` to a named `const RIPPLE_COOLDOWN_SECS`, matching the reverb constants above it.

## Related

- **#3178** (`AUD-2026-08-20-D2-01`) — the unit bug currently masking this.
- **#3115** (`ECS-2026-08-20-06`, OPEN) — the sibling defect that the two `SplashEvent` producers
  disagree about which entity hosts the marker. Same producer pair, different symptom; the fixes touch
  adjacent code.
- If a per-surface map is added, it is per-frame entity-keyed state — use `FxHashMap` per **#2923**
  (see also **#3137**).

## Completeness Checks

- [ ] **SIBLING**: the splash branch in the same function already does this correctly — mirror it rather
      than inventing a third shape
- [ ] **LOCK_ORDER**: the `RippleEvent` query guard is still dropped before `try_resource_mut::<AudioWorld>()`
      (the established `footstep_system` lock-drop pattern)
- [ ] **TESTS**: a guard with two concurrent `RippleEvent`s on different surfaces, asserting the played
      position and intensity come from the same event, and that a ripple on surface A does not suppress
      surface B
