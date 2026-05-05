# #794 — M41-IDLE: AnimationPlayer attaches but mtidle.kf produces no visible motion

## Resolution (2026-05-05)

**All three suspects in the issue body ruled out via a layered test
suite.** Animation pipeline is healthy end-to-end on real FNV
content; the visible "rigid NPC" symptom is the known Phase 1b.x
body-skinning artifact, not the animation chain.

## Methodology

Three layers of regression test, narrowing from data → system →
real-content composition.

### Layer 1 — Parser diagnostic (rules out suspect 2)

[`crates/nif/tests/mtidle_motion_diagnostic.rs`](../../../crates/nif/tests/mtidle_motion_diagnostic.rs)
opens FNV `Fallout - Meshes.bsa`, extracts
`meshes\characters\_male\locomotion\mtidle.kf`, parses + imports it
through the production path, samples each rotation channel at
seven times across the clip, and asserts at least one inter-sample
delta exceeds 1e-3.

```
mtidle.kf: clip='Idle' duration=4.000s channels=56 freq=1
rotation_keys per channel: empty=0 single=0 multi=56
max inter-sample rotation delta = 0.065366 (channel 'Bip01 R Finger1')
max inter-sample translation delta = 0.389104
```

The B-spline decoder produces visible rotation variation across
all 56 channels (0 empty, 0 single-key, 56 multi-key). Suspect 2
("B-spline rotation sampling produces near-identity quaternions")
is dead.

### Layer 2 — Synthetic system e2e (rules out suspects 1 + 3)

Four `#[cfg(test)]` units in
[`byroredux/src/systems.rs`](../../../byroredux/src/systems.rs)
under `animation_system_e2e_tests`:

| Test | Pins |
|------|------|
| `rotation_channel_writes_bone_transform_through_animation_system` | Apply phase writes to the resolved bone (suspect 3 dead) |
| `animation_player_local_time_advances_per_tick` | `advance_time` actually advances `local_time` (suspect 1 dead) |
| `player_on_separate_entity_still_drives_bone_rotation` | cell_loader's player-on-fresh-entity pattern is functionally equivalent to npc_spawn's player-on-placement_root pattern |
| `npc_spawn_shape_drives_skeleton_bone_not_body_clone` | Scoped subtree map dispatches to skeleton bone, not body's local clone of the same name (key concern from #772 sibling work) |

All four pass without any code change. The lab apply phase is
correct under the npc_spawn shape, including the body-NIF clone
duplicate-name case that #772 worked around.

### Layer 3 — Real-data closure e2e

`rotation_through_animation_system_on_real_mtidle` (same module,
`#[ignore]`d). Loads real FNV `mtidle.kf`, builds a fake skeleton
with one entity per channel name, attaches an `AnimationPlayer` with
`with_root(skel_root)`, ticks `animation_system` four times across
the 4-second clip, and measures rotation delta from initial
`Transform::IDENTITY`:

```
real mtidle: 'Idle' duration=4.00s freq=1 channels=56
max rotation delta after 4 ticks @ 1.00s = 1.493697 on bone 'Bip01 Spine'
```

A 1.49 component-wise quaternion delta is enormous — well beyond
any "subtle idle motion" perception threshold. The animation
pipeline is healthy on real content.

## Where the visible-motion gap actually lives

The user's symptom ("Doc Mitchell stands rigid") cannot be in the
animation chain — the chain demonstrably moves bones by ~1.5
quaternion units. The remaining candidate is the **already-known
Phase 1b.x body-skinning artifact** documented in
[`byroredux/src/npc_spawn.rs:402-431`](../../../byroredux/src/npc_spawn.rs):

> M41.0 Phase 1b.x — body skinning catastrophically misrenders
> interactively (long-spike vertex artifact). The artifact is
> independent of `external_skeleton`, `0 unresolved` bones are
> reported, so the bug is in the runtime entity transform / palette
> composition, not the bone-name resolution.

When body skinning is broken at the per-vertex level, bone rotation
animates correctly *but the visible mesh doesn't follow the bones*.
The user reads this as "no animation" even though the bones rotate.

This means **#794 closes without a code change in the animation
chain**. The visible-content gap moves to the Phase 1b.x bucket
and gets its own issue.

## Out of scope for #794 closure

- **Phase 1b.x body-skinning artifact.** Pre-existing, separately
  filed under M41.0 closure work. Its diagnostic plan ("dump
  skinned-mesh bones' GlobalTransforms + bind_inverses at runtime,
  compute palette by hand, compare against skinning_e2e's working
  palette") stands.
- **mtidle as the "default" idle.** mtidle is locomotion-idle (a
  4-second standing pose with subtle breath). For *demonstrably
  expressive* idle motion (talk gestures, weapon fidgets), the
  per-NPC IDLE form record + AI package layer (M42) drives talk_*,
  cigar_*, dlcanch_* and friends. mtidle just keeps the actor
  alive-looking; it isn't expressive.
- **Skinning palette uses post-propagation transforms.** Verified
  by inspection — `animation_system` runs in `Stage::Update`,
  `make_transform_propagation_system` runs in `Stage::PostUpdate`,
  and `build_render_data`'s `compute_palette_into` runs after both,
  reading `GlobalTransform`. The pipeline ordering is correct.

## Completeness Checks

- [x] **UNSAFE**: N/A — no `unsafe` blocks added.
- [x] **SIBLING**: All three suspects in the issue body each have
  a dedicated regression test pinning the negative result. Future
  parser drift on B-spline rotation, system drift on
  `local_time` advancement, or scope drift on subtree resolution
  lights up immediately.
- [x] **DROP**: N/A.
- [x] **LOCK_ORDER**: `animation_system` already takes
  `query_mut::<AnimationPlayer>()` followed by
  `query_mut::<Transform>()` — TypeId-sorted via the `Access`
  framework. No new lock pairings introduced.
- [x] **FFI**: N/A.
- [x] **TESTS**: 1 (parser, mtidle_motion_diagnostic) + 4
  (synthetic e2e, animation_system_e2e_tests) + 1 (real-data
  closure, `#[ignore]`-gated). All 4 default-on tests pass; the 2
  real-data tests pass when FNV BSA is reachable.
