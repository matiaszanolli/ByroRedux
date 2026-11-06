//! World-bound propagation.
//!
//! Split out of `post_update.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};
use crate::systems::make_world_bound_propagation_system;

/// World-bound propagation.
pub(super) fn register_world_bound_systems(scheduler: &mut Scheduler) {
    // PostUpdate ordering contract (#1375 invariant pin, revised by #3652):
    //   1. transform_propagation — BFS GlobalTransform composition
    //   2. make_world_bound_propagation_system — drains GT dirty set,
    //      folds WorldBounds from the per-frame transforms available so far
    //
    // #3652 — `make_billboard_system` used to run here as step 2 (between
    // the two above), but its entire input is the active camera's
    // GlobalTransform, which `camera_follow_system` authors in `Stage::Late`
    // in player/third-person mode. `Stage::Late` runs strictly after
    // `Stage::PostUpdate`, so billboard was reading last frame's camera pose
    // every frame — visible as shearing/sliver billboard quads on a fast
    // camera turn, resolving the instant the camera stopped (see
    // `register_late_systems` for why `camera_follow_system` itself cannot
    // move the other direction, to PostUpdate: it needs `Stage::Physics`'s
    // post-step body position). Moved to `Stage::Late`, immediately after
    // `camera_follow_system`, below.
    //
    // INVARIANT: no Stage::Late system may write GlobalTransform on
    // a LocalBound-bearing entity. If it does, that entity's
    // WorldBound will silently lag one frame because bound
    // propagation's GT drain fires before the Late write. `camera_follow_
    // system` + audio emitters write GT in Late but carry no LocalBound —
    // the lag is benign for them. `make_billboard_system` is the first
    // exception: billboard meshes DO carry a LocalBound. The resulting
    // one-frame-stale WorldBound is accepted (not proven zero-cost) — a
    // billboard quad's local bounding-sphere center sits at its own pivot
    // for essentially every vanilla asset, and rotation about a sphere's own
    // center doesn't move it, so the practical drift is at most the
    // radius-scale term (rotation-invariant) recomputed from a pose one
    // frame stale, versus the shearing defect being visibly wrong on every
    // fast turn. Any future Late system that writes GT on a bounded entity
    // must make the same call, or be promoted to PostUpdate (before bounds).
    //
    // #2391 — the two systems of this PostUpdate chain declare their access
    // (`add_exclusive_with_access`) even though the analyzer doesn't pair
    // exclusives: the ordering contract above is entirely about who touches
    // `GlobalTransform` when, and a blank `sys.accesses` row is exactly the
    // wrong place for that to be invisible.
    // Bound propagation runs last in PostUpdate so it sees final
    // world transforms for everything PostUpdate itself writes. See #217.
    // #3652 — billboard rotations moved to Stage::Late (below), so this no
    // longer sees the CURRENT frame's billboard pose; see this function's
    // INVARIANT comment above for why that's accepted.
    //
    // The `GlobalTransform` entry is a **write**: the system takes a
    // write guard on that storage to drain its change-tracking dirty set
    // (`bounds.rs`, `drain_dirty_into`) before reading transforms
    // through it. It mutates no transform values, but the lock it takes
    // is exclusive, and the declaration describes lock acquisition.
    scheduler.add_exclusive_with_access(
        Stage::PostUpdate,
        make_world_bound_propagation_system(),
        Access::new()
            .reads::<byroredux_core::ecs::LocalBound>()
            .reads::<byroredux_core::ecs::Parent>()
            .reads::<byroredux_core::ecs::Children>()
            .reads::<byroredux_core::ecs::SkinnedMesh>()
            .writes::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::WorldBound>(),
    );
}
