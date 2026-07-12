//! Sandbox seat procedure (M42) — seats sandboxing actors in nearby free
//! furniture. **Registered only when `BYRO_SANDBOX_SIT` is set** (see
//! `boot.rs`); gated off by default pending the sit-enter transition.
//!
//! For each [`SandboxBehavior`] actor not yet [`Seated`], find the nearest
//! unreserved [`Furniture`] with a sit marker within a radius, reserve it,
//! snap the actor's placement-root [`Transform`] onto the seat (the marker
//! `offset` is the actor's floor-root position per nif.xml), and switch its
//! [`AnimationPlayer`] to the generic sit loop.
//!
//! ## Why it's gated (M42.0b diagnosis, live bone inspection)
//!
//! Placement + clip binding are **verified correct**: seated actors land on
//! the right furniture marker at floor height, and the sit clip *is* applied
//! (a seated actor's `Bip01 L Thigh` local rotation matches the clip's
//! authored folded-leg pose exactly, distinct from a standing actor's).
//!
//! But the generic `dynamicidle_chairsit` / `dynamicidle_sit` clips carry
//! **no `Bip01` / `Pelvis` / `NonAccum` channel** — they fold the limbs but
//! never lower the body. They are *loops* authored to run *after* a sitdown
//! transition has already translated `Bip01` down onto the seat. Played from
//! the standing bind pose with no transition, the actor holds a correct
//! sitting leg-fold while floating ~85 units above the seat (feet measured at
//! world-y ≈ 3540 over a 3456 seat floor). FNV ships no standalone sitdown
//! KF — the enter is driven by the anim-group/special-idle system we don't
//! have. The fix (a furniture sit-enter transition that lowers `Bip01`) is
//! its own milestone; everything here is the groundwork for it.
//!
//! v0 scope (documented approximations): nearest free chair, seat once, no
//! scoring / scheduling / meals / wander / ownership, no pathing
//! (snap-to-seat), generic sit clip, furniture-rotation facing.

use std::collections::HashSet;

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{
    Furniture, FurnitureMarker, GlobalTransform, SandboxBehavior, Seated, Transform,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};

use crate::components::{SandboxSitClip, SeatReservations};

/// Max distance (world units) an actor claims a seat from its current
/// position. FNV interiors are small; 512 ≈ the `DefaultSandbox…512…`
/// package radius. v0 uses the actor's own position as the sandbox center
/// (no PLDT location parse yet).
const SEAT_SEARCH_RADIUS: f32 = 512.0;

/// True when a furniture marker is a *sit* entry. Skyrim+ tags this
/// (`animation_type == 1`); legacy FNV/FO3 markers carry no AnimationType
/// (`heading_z_radians == None`) and are treated as sit in v0 — the
/// dominant furniture kind in the target cells (bars, offices). Legacy
/// sleep/lean markers are a known v0 over-match.
fn is_sit_marker(m: &FurnitureMarker) -> bool {
    m.animation_type == 1 || m.heading_z_radians.is_none()
}

/// World-space seat transform: the furniture's world transform ∘ the
/// marker's entity-local offset.
///
/// **Placement (M42.0b).** Per nif.xml `FurniturePosition`, the marker
/// `offset` **is** the actor's root (floor, between-feet) position in
/// furniture-local space — not an approach point outside the footprint.
/// So the root goes exactly there; the `dynamicidle_*` sit loop poses the
/// pelvis onto the seat relative to that root. (My earlier "centre the root
/// at X/Z = 0" attempt discarded the marker's real XZ and planted actors at
/// the furniture pivot — wrong for anything but a chair whose pivot is the
/// seat.)
///
/// **Facing.** Skyrim+/FO4 markers carry a radian `heading` about +Z
/// (`heading_z_radians`). Legacy Oblivion/FO3/FNV markers carry only an
/// `Orientation` ushort that indexes `furnituremarkerNN.nif` (undecoded),
/// so legacy inherits the furniture's own world rotation (local identity).
fn seat_world_transform(furn: &GlobalTransform, m: &FurnitureMarker) -> GlobalTransform {
    let seat_local = Vec3::from_array(m.local_offset);
    let facing = match m.heading_z_radians {
        Some(h) => Quat::from_rotation_y(h),
        None => Quat::IDENTITY, // inherit furniture facing via `compose`
    };
    GlobalTransform::compose(furn, seat_local, facing, 1.0)
}

/// Pick the nearest unreserved seat to `actor_pos` within
/// [`SEAT_SEARCH_RADIUS`]. Pure — the selection core, unit-tested.
fn pick_nearest_seat(
    actor_pos: Vec3,
    seats: &[(EntityId, GlobalTransform)],
    reserved: &HashSet<EntityId>,
) -> Option<(EntityId, GlobalTransform)> {
    let r2 = SEAT_SEARCH_RADIUS * SEAT_SEARCH_RADIUS;
    seats
        .iter()
        .filter(|(e, _)| !reserved.contains(e))
        .map(|(e, seat)| (*e, *seat, (seat.translation - actor_pos).length_squared()))
        .filter(|(_, _, d2)| *d2 <= r2)
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(e, seat, _)| (e, seat))
}

/// Seat sandboxing actors in nearby furniture. Registered
/// `add_exclusive(Stage::PostUpdate, …)` so it reads this frame's
/// propagated `GlobalTransform`s; the snapped root propagates to the
/// skeleton next frame.
pub fn sandbox_seat_system(world: &World, _dt: f32) {
    // No sit clip → nothing to seat (Skyrim+/Havok games, or the clip
    // wasn't archived). Resolved once per cell into this resource.
    let Some(sit_handle) = world.try_resource::<SandboxSitClip>().and_then(|r| r.0) else {
        return;
    };
    let Some(sandbox_q) = world.query::<SandboxBehavior>() else {
        return;
    };

    // ── Pass 1: gather assignments (reads + within-frame reservation). ──
    // All held guards are distinct component/resource types, so the
    // lock-tracker sees no conflict; writes happen in Pass 2 after these
    // read guards drop.
    let mut assignments: Vec<(EntityId, EntityId, GlobalTransform)> = Vec::new();
    {
        let Some(gq) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(furn_q) = world.query::<Furniture>() else {
            return; // no furniture in this cell
        };
        let seated_q = world.query::<Seated>();

        // World-space seat transform for each furniture with a sit marker.
        // `seat_meta` mirrors `seats` keyed by furniture entity for the
        // one-shot diagnostic log emitted per assignment below (M42.0b
        // co-debug: readable numbers instead of dark screenshots).
        let mut seats: Vec<(EntityId, GlobalTransform)> = Vec::new();
        let mut seat_meta: std::collections::HashMap<EntityId, ([f32; 3], Vec3)> =
            std::collections::HashMap::new();
        for (furn_e, furn) in furn_q.iter() {
            let Some(marker) = furn.markers.iter().find(|m| is_sit_marker(m)) else {
                continue;
            };
            let Some(furn_g) = gq.get(furn_e) else {
                continue;
            };
            seats.push((furn_e, seat_world_transform(furn_g, marker)));
            seat_meta.insert(furn_e, (marker.local_offset, furn_g.translation));
        }
        if seats.is_empty() {
            return;
        }

        let mut reservations = world.resource_mut::<SeatReservations>();
        for (npc, _) in sandbox_q.iter() {
            if seated_q.as_ref().is_some_and(|s| s.contains(npc)) {
                continue; // already seated (one-shot guard)
            }
            let Some(npc_g) = gq.get(npc) else {
                continue;
            };
            if let Some((furn_e, seat)) =
                pick_nearest_seat(npc_g.translation, &seats, &reservations.0)
            {
                reservations.0.insert(furn_e); // claim now so no two share it
                // One-shot per NPC (Seated is tagged in pass 2, skipping it
                // next frame) — safe to log at info without spamming.
                let (offset, furn_world) =
                    seat_meta.get(&furn_e).copied().unwrap_or(([0.0; 3], Vec3::ZERO));
                log::info!(
                    "[sandbox] seat npc={} npc_pos=({:.1},{:.1},{:.1}) -> furn={} \
                     furn_world=({:.1},{:.1},{:.1}) marker_offset=({:.1},{:.1},{:.1}) \
                     seat_world=({:.1},{:.1},{:.1}) dist={:.1}",
                    npc,
                    npc_g.translation.x, npc_g.translation.y, npc_g.translation.z,
                    furn_e,
                    furn_world.x, furn_world.y, furn_world.z,
                    offset[0], offset[1], offset[2],
                    seat.translation.x, seat.translation.y, seat.translation.z,
                    (seat.translation - npc_g.translation).length(),
                );
                assignments.push((npc, furn_e, seat));
            }
        }
    }
    if assignments.is_empty() {
        return;
    }

    // ── Pass 2: apply writes (each a scoped single-type lock). ──
    // Snap the placement root — a propagation root, so local == world.
    if let Some(mut tq) = world.query_mut::<Transform>() {
        for (npc, _, seat) in &assignments {
            if let Some(t) = tq.get_mut(*npc) {
                t.translation = seat.translation;
                t.rotation = seat.rotation;
                // scale left as-authored
            }
        }
    }
    // Switch to the sit loop, restarting cleanly.
    if let Some(mut pq) = world.query_mut::<AnimationPlayer>() {
        for (npc, _, _) in &assignments {
            if let Some(p) = pq.get_mut(*npc) {
                p.clip_handle = sit_handle;
                p.local_time = 0.0;
                p.prev_time = 0.0;
                p.speed = 1.0;
            }
        }
    }
    // Tag Seated (storage pre-registered at boot so insert lands).
    if let Some(mut sq) = world.query_mut::<Seated>() {
        for (npc, furn, _) in &assignments {
            sq.insert(*npc, Seated { furniture: *furn });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(offset: [f32; 3], heading: Option<f32>, anim: u16) -> FurnitureMarker {
        FurnitureMarker {
            local_offset: offset,
            heading_z_radians: heading,
            animation_type: anim,
        }
    }

    #[test]
    fn is_sit_marker_modern_sit_and_legacy() {
        assert!(is_sit_marker(&marker([0.0; 3], Some(0.0), 1))); // Skyrim+ sit
        assert!(!is_sit_marker(&marker([0.0; 3], Some(0.0), 2))); // Skyrim+ sleep
        assert!(is_sit_marker(&marker([0.0; 3], None, 0))); // legacy → sit (v0)
    }

    #[test]
    fn seat_world_places_root_at_marker_offset() {
        // Furniture at (10,0,0) identity; marker 3 +X, 30 down. The root lands
        // AT the marker in world space (13,-30,0) — the marker offset is the
        // actor root position, not the furniture pivot.
        let furn = GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        let seat = seat_world_transform(&furn, &marker([3.0, -30.0, 0.0], None, 0));
        assert!(
            (seat.translation - Vec3::new(13.0, -30.0, 0.0)).length() < 1e-5,
            "root should land at the marker offset, got {:?}",
            seat.translation
        );
    }

    #[test]
    fn seat_world_legacy_facing_inherits_furniture_rotation() {
        // Furniture yawed 90° about Y; a legacy marker (no heading) inherits
        // the furniture's own facing (local rotation identity).
        let furn =
            GlobalTransform::new(Vec3::ZERO, Quat::from_rotation_y(core::f32::consts::FRAC_PI_2), 1.0);
        let seat = seat_world_transform(&furn, &marker([0.0, -30.0, 0.0], None, 0));
        let furn_fwd = furn.rotation * Vec3::Z;
        let seat_fwd = seat.rotation * Vec3::Z;
        assert!(
            (seat_fwd - furn_fwd).length() < 1e-5,
            "legacy facing should match furniture, got {seat_fwd:?} vs {furn_fwd:?}"
        );
    }

    #[test]
    fn seat_world_modern_heading_sets_facing() {
        // Skyrim+ marker with a radian heading → yaw about Y by that heading.
        let furn = GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0);
        let seat = seat_world_transform(&furn, &marker([0.0, -30.0, 0.0], Some(0.0), 1));
        let fwd = seat.rotation * Vec3::Z;
        assert!((fwd - Vec3::Z).length() < 1e-5, "heading 0 faces +Z, got {fwd:?}");
    }

    #[test]
    fn pick_nearest_seat_picks_closest_free_in_range() {
        let seats = vec![
            (1, GlobalTransform::new(Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY, 1.0)),
            (2, GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0)),
            (3, GlobalTransform::new(Vec3::new(5000.0, 0.0, 0.0), Quat::IDENTITY, 1.0)),
        ];
        let mut reserved = HashSet::new();
        // Nearest free is entity 2.
        assert_eq!(
            pick_nearest_seat(Vec3::ZERO, &seats, &reserved).map(|(e, _)| e),
            Some(2)
        );
        // Reserve 2 → next nearest is 1 (3 is out of radius).
        reserved.insert(2);
        assert_eq!(
            pick_nearest_seat(Vec3::ZERO, &seats, &reserved).map(|(e, _)| e),
            Some(1)
        );
        // Reserve 1 too → only the out-of-range seat 3 remains → None.
        reserved.insert(1);
        assert!(pick_nearest_seat(Vec3::ZERO, &seats, &reserved).is_none());
    }

    #[test]
    fn pick_nearest_seat_empty_is_none() {
        assert!(pick_nearest_seat(Vec3::ZERO, &[], &HashSet::new()).is_none());
    }
}
