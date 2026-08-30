# CONC-D4-2026-08-30-01: `make_billboard_system` (PostUpdate) reads the camera pose `camera_follow_system` (Late) authors — one frame of lag, invisible to both KPIs

**Issue**: #3652
**Labels**: bug, ecs, medium, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D4-2026-08-30-01 (MEDIUM, D4 · Scheduler Access Declarations, cross-stage sequencing).

**Same shape as #3180, which fixed the inversion for `submersion_system` only.**

**Location**: `byroredux/src/boot.rs:1243-1253` (billboard registration) + `byroredux/src/boot.rs:1328-1348` (camera_follow registration); read site `byroredux/src/systems/billboard.rs:63-84`.

## Description

`make_billboard_system` is a `Stage::PostUpdate` **exclusive**; its first act is to read the active camera's `GlobalTransform` and derive `cam_pos` / `cam_forward`, which is the **entire input** to every billboard rotation it writes.

In `PlayerMode::Character` the **sole author** of that camera pose is `camera_follow_system`, registered `add_to_with_access(Stage::Late, ...)` declaring `.writes::<GlobalTransform>()` + `.writes::<Transform>()` (`fly_camera_system` early-returns in Character mode, `systems/camera.rs:20-26`).

`Stage::PostUpdate` (discriminant 2) executes **strictly before** `Stage::Late` (discriminant 4), so within frame N the billboard system reads the pose authored in Late of frame **N-1** — and the `transform_propagation` pass that runs immediately before it in PostUpdate recomposes the camera `GlobalTransform` from that same frame-N-1 `Transform`, so **there is no second path to a fresh value**. The renderer then draws frame N from the frame-N camera pose (`build_render_data` runs after the whole schedule), against billboards oriented to frame N-1.

This is exactly the defect #3180 found and fixed for `submersion_system` — commit `5ce2b1c5` moved that one system from `PostUpdate` to `Late` and **left the sibling PostUpdate consumer of the same camera pose in place**. The #1375 invariant comment directly above the billboard registration (`boot.rs:1220-1236`) reasons only about Late-stage *writes* of `GlobalTransform` versus `WorldBound` propagation; it never considers a PostUpdate *read* of a Late-authored pose.

## Evidence

```rust
// byroredux/src/boot.rs:1243-1253
scheduler.add_exclusive_with_access(
    Stage::PostUpdate,
    make_billboard_system(),
    Access::new()
        .reads_resource::<ActiveCamera>()
        ...
        .writes::<byroredux_core::ecs::GlobalTransform>(),
);

// byroredux/src/systems/billboard.rs:77-85
let Some(mut gq) = world.query_mut::<GlobalTransform>() else { return; };
let Some(cam_global) = gq.get(cam_entity).copied() else { return; };
let cam_pos = cam_global.translation;
let cam_forward = cam_global.rotation * -Vec3::Z;

// byroredux/src/boot.rs:1328-1332  (the sole Character-mode author of that pose)
scheduler.add_to_with_access(
    Stage::Late,
    crate::systems::camera_follow_system,
```

Stage order that makes it structural — `crates/core/src/ecs/scheduler.rs:27-38` (`Early=0 ... PostUpdate=2 ... Late=4`, `BTreeMap` ascending) and `:497-515` (per stage: whole parallel phase, then exclusives).

## Trigger Conditions

`PlayerMode::Character` (the gameplay camera) + any frame in which the camera pose changes. **Not** reachable in `PlayerMode::FlyCam`, where `fly_camera_system` writes the camera `Transform` in `Stage::Early` and `transform_propagation` composes its `GlobalTransform` in the same PostUpdate parallel phase that precedes the billboard exclusive.

## Verification Path

`cargo test` cannot see it — the analyzer only reasons **within** a stage, so `known_conflict_count()` / `unknown_pair_count()` both stay 0 (`analyze_pair` never compares systems in different stages). Confirm by hand from the stage table, or visually: fast mouse-yaw in an exterior in player mode — billboard/impostor quads shear or show a sliver edge that snaps back when the camera stops.

## Impact

**One full frame of camera lag on every billboard rotation in gameplay (player) mode.** At 60 fps and a 400 deg/s flick that is ~6.7 deg of facing error, which for a camera-facing quad is visible as shear/sliver on grass, tree impostors and SpeedTree billboards during fast turns, resolving as soon as the camera stops (the `camera_changed` gate at `billboard.rs:93-96` means the steady state is correct).

No race, no unsoundness — a pure ordering defect, and **invisible to the scheduler KPIs**.

## Related

#3180 (`5ce2b1c5`, the identical inversion for `submersion_system`); #1374 / #1375 (billboard camera-motion gate + the PostUpdate ordering contract); #217 (bounds propagation must run after billboard rotations); CONC-D4-2026-08-30-02 (same class, smaller blast radius).

## Suggested Fix

Move `camera_follow_system` so the pose is authored **before** its PostUpdate consumer — it only needs the player body's *propagated* `GlobalTransform`, so a `Stage::PostUpdate` **exclusive** registered between `transform_propagation` and `make_billboard_system` satisfies every existing contract (billboards see the current pose; bounds propagation, still last, sees the final camera GT; the Late water/audio consumers still sequence after it).

**Note this contradicts** `submersion_runs_after_camera_follow_and_before_water_audio`, which asserts `!late.systems[camera_follow].is_exclusive` — that pin has to be rewritten in the same commit, and the #3180 orderings (camera_follow before submersion before water_audio before audio_system) re-expressed **across** stages rather than within Late.

## Completeness Checks
- [ ] **SIBLING**: Every PostUpdate consumer of a Late-authored value enumerated, not just the billboard one — #3180 fixed one instance of a class
- [ ] **TESTS**: `submersion_runs_after_camera_follow_and_before_water_audio` rewritten in the same commit; the #3180 orderings re-expressed across stages
- [ ] **TESTS**: A regression test pins the cross-stage ordering, since `analyze_pair` is intra-stage and cannot
