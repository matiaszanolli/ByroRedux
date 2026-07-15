//! Sandbox seat procedure (M42) — seats sandboxing actors in nearby free
//! furniture. **Registered only when `BYRO_SANDBOX_SIT` is set** (see
//! `boot.rs`); gated off by default pending live confirmation of Phase A.
//!
//! For each [`SandboxBehavior`] actor not yet [`Seated`], find the nearest
//! unreserved [`Furniture`] with a sit marker within a radius, reserve it,
//! snap the actor's placement-root [`Transform`] onto the seat (the marker
//! `offset` is the actor's floor-root position per nif.xml), and park its
//! [`AnimationPlayer`] on the sit-enter clip's final (fully-seated) frame.
//!
//! ## The body-lowering mechanism (M42.1)
//!
//! M42.0 first swapped to the generic `dynamicidle_chairsit` / `dynamicidle_sit`
//! **loop** — but those carry **no `Bip01` / `Pelvis` / `NonAccum` channel**:
//! they fold the limbs but never lower the body, so the actor floated ~90 units
//! above the seat (feet measured world-y ≈ 3540 over a 3456 floor). Placement
//! and clip binding were verified correct at the time (a seated actor's
//! `Bip01 L Thigh` local rotation matched the clip's authored folded pose).
//!
//! The loops are meant to run *after* a sit-**enter** transition that lowers the
//! body. FNV ships those enter clips (e.g. `chairskirt_leftenter`): they drive
//! the accum root `Bip01` (y→0) and `Bip01 NonAccum` (y 66.67→36.87) down onto
//! the seat, and their **final frame is a complete grounded seated pose**. So
//! Phase A parks the player on that final frame: `local_time = duration`,
//! `playing = false` (the enter clip's `Reverse` cycle would otherwise ping-pong
//! back to standing). The engine applies the accum root's vertical translation
//! as pose (`split_root_motion` keeps Y; horizontal X/Z is discarded — no
//! `RootMotionDelta` consumer), so no approach-walk handling is needed.
//!
//! Each sit marker on a furniture is an independently reservable seat
//! (M42.2 seat-polish), keyed `(furniture, marker index)` — a multi-seat
//! piece (counter / bench / multi-chair table authored as one FURN with
//! several markers) now seats one actor per marker instead of one actor for
//! the whole piece.
//!
//! v0 scope (documented approximations): nearest free seat, seat once, no
//! scoring / scheduling / meals / wander / ownership, no pathing (snap-to-seat),
//! one verified chair enter for all sit markers (per-furniture-type enter/loop
//! mapping is Phase C), static held pose (fidget-loop blend is Phase B),
//! furniture-rotation facing. Legacy (FO3/FNV/Oblivion) markers carry no
//! AnimationType, so sleep/lean markers are still over-matched as sit — that
//! disambiguation needs the marker's furniture-type resolved and is visually
//! validated on-device (Phase C), not decodable from the marker alone.

use std::collections::HashSet;

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{
    Furniture, FurnitureMarker, GlobalTransform, SandboxBehavior, Seated, Transform,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};

use crate::components::{SandboxSitClip, SeatReservations};

/// Fallback max distance (world units) an actor claims a seat from its
/// current position, used when `SandboxBehavior.search_radius` is `None`
/// (no PLDT / radius 0 / a location type the parser doesn't resolve a
/// center for). Most sandboxing actors carry a real authored radius from
/// PLDT (npc_spawn.rs); 512 was the estimate before that landed — still a
/// reasonable FNV-interior-scale default for the no-PLDT case.
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
/// `Orientation` ushort that indexes `furnituremarkerNN.nif` (undecoded), so
/// legacy facing is derived geometrically: the actor faces from the marker
/// toward the furniture centre — the seating direction for tables/counters
/// (each seat around a table faces its middle). In furniture-local space that
/// direction is `normalize(-offset.xz)`; the model's forward is `+Z`, so the
/// local yaw is `atan2(-offset.x, -offset.z)`, and `compose` maps it to world
/// via `furn.rotation`. Inheriting the furniture's own rotation (the M42.0
/// approximation) sat actors 90° sideways on the chair. Degenerate near-zero
/// XZ (marker at the furniture pivot) → inherit furniture facing. The exact
/// per-marker `Orientation` decode is deferred to Phase C.
fn seat_world_transform(furn: &GlobalTransform, m: &FurnitureMarker) -> GlobalTransform {
    let seat_local = Vec3::from_array(m.local_offset);
    let facing = match m.heading_z_radians {
        Some(h) => Quat::from_rotation_y(h),
        None if seat_local.x.abs() > 1e-3 || seat_local.z.abs() > 1e-3 => {
            Quat::from_rotation_y((-seat_local.x).atan2(-seat_local.z))
        }
        None => Quat::IDENTITY, // degenerate → inherit furniture facing
    };
    GlobalTransform::compose(furn, seat_local, facing, 1.0)
}

/// Pick the nearest unreserved seat to `actor_pos` within `radius`. Pure —
/// the selection core, unit-tested. Generic over the seat key `K` so the
/// same logic serves both a bare furniture-entity key (tests) and the
/// production `(furniture, marker index)` key, which lets several markers on
/// one multi-seat furniture reserve independently.
fn pick_nearest_seat<K: Copy + Eq + std::hash::Hash>(
    actor_pos: Vec3,
    seats: &[(K, GlobalTransform)],
    reserved: &HashSet<K>,
    radius: f32,
) -> Option<(K, GlobalTransform)> {
    let r2 = radius * radius;
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
    // No sit-enter clip → nothing to seat (Skyrim+/Havok games, or the clip
    // wasn't archived). Resolved once per cell into this resource as
    // `(handle, hold_time)` — `hold_time` is the clip duration; parking the
    // player there with `playing = false` holds the final seated frame.
    let Some((sit_handle, hold_time)) =
        world.try_resource::<SandboxSitClip>().and_then(|r| r.0)
    else {
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

        // World-space seat transform for *every* sit marker on each
        // furniture, keyed `(furniture entity, marker index)` so a multi-seat
        // piece (counter / bench / multi-chair table) offers one seat per
        // marker instead of just its first (M42.2 seat-polish). `seat_meta`
        // mirrors `seats` for the one-shot diagnostic log emitted per
        // assignment below (M42.0b co-debug: readable numbers instead of dark
        // screenshots).
        let mut seats: Vec<((EntityId, u32), GlobalTransform)> = Vec::new();
        let mut seat_meta: std::collections::HashMap<(EntityId, u32), ([f32; 3], Vec3)> =
            std::collections::HashMap::new();
        for (furn_e, furn) in furn_q.iter() {
            let Some(furn_g) = gq.get(furn_e) else {
                continue;
            };
            for (idx, marker) in furn.markers.iter().enumerate() {
                if !is_sit_marker(marker) {
                    continue;
                }
                let seat_id = (furn_e, idx as u32);
                seats.push((seat_id, seat_world_transform(furn_g, marker)));
                seat_meta.insert(seat_id, (marker.local_offset, furn_g.translation));
            }
        }
        if seats.is_empty() {
            return;
        }

        let mut reservations = world.resource_mut::<SeatReservations>();
        for (npc, behavior) in sandbox_q.iter() {
            if seated_q.as_ref().is_some_and(|s| s.contains(npc)) {
                continue; // already seated (one-shot guard)
            }
            let Some(npc_g) = gq.get(npc) else {
                continue;
            };
            let radius = behavior.search_radius.unwrap_or(SEAT_SEARCH_RADIUS);
            if let Some((seat_id, seat)) =
                pick_nearest_seat(npc_g.translation, &seats, &reservations.0, radius)
            {
                reservations.0.insert(seat_id); // claim this marker so no two share it
                let (furn_e, marker_idx) = seat_id;
                // One-shot per NPC (Seated is tagged in pass 2, skipping it
                // next frame) — safe to log at info without spamming.
                let (offset, furn_world) =
                    seat_meta.get(&seat_id).copied().unwrap_or(([0.0; 3], Vec3::ZERO));
                log::info!(
                    "[sandbox] seat npc={} npc_pos=({:.1},{:.1},{:.1}) -> furn={} marker={} \
                     furn_world=({:.1},{:.1},{:.1}) marker_offset=({:.1},{:.1},{:.1}) \
                     seat_world=({:.1},{:.1},{:.1}) dist={:.1}",
                    npc,
                    npc_g.translation.x, npc_g.translation.y, npc_g.translation.z,
                    furn_e, marker_idx,
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
    // Park on the sit-enter clip's FINAL frame: `local_time = hold_time`
    // (clip duration) so the apply phase samples each channel's last key (the
    // fully-seated end pose), and `playing = false` so `advance_time` freezes
    // it — the enter clip's cycle is `Reverse`, which would otherwise ping-pong
    // back to standing. This is what lowers the body onto the seat (the enter
    // clip's `Bip01`/`NonAccum` channels, absent from the sit loops). See the
    // M42.1 diagnosis in this module's docs.
    if let Some(mut pq) = world.query_mut::<AnimationPlayer>() {
        for (npc, _, _) in &assignments {
            if let Some(p) = pq.get_mut(*npc) {
                p.clip_handle = sit_handle;
                p.local_time = hold_time;
                p.prev_time = hold_time;
                p.playing = false;
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
    fn seat_world_legacy_faces_furniture_centre() {
        // Legacy marker (no heading) offset toward +X of an identity furniture:
        // the actor should face back toward the centre (−X), not sideways.
        let furn = GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0);
        let seat = seat_world_transform(&furn, &marker([30.0, -30.0, 0.0], None, 0));
        let fwd = seat.rotation * Vec3::Z;
        assert!(
            (fwd - (-Vec3::X)).length() < 1e-5,
            "should face the furniture centre (−X), got {fwd:?}"
        );
    }

    #[test]
    fn seat_world_legacy_degenerate_offset_inherits_furniture_rotation() {
        // Marker at the furniture pivot (near-zero XZ) → no centre direction;
        // fall back to the furniture's own facing.
        let furn =
            GlobalTransform::new(Vec3::ZERO, Quat::from_rotation_y(core::f32::consts::FRAC_PI_2), 1.0);
        let seat = seat_world_transform(&furn, &marker([0.0, -30.0, 0.0], None, 0));
        let furn_fwd = furn.rotation * Vec3::Z;
        let seat_fwd = seat.rotation * Vec3::Z;
        assert!(
            (seat_fwd - furn_fwd).length() < 1e-5,
            "degenerate offset should inherit furniture facing, got {seat_fwd:?}"
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
            pick_nearest_seat(Vec3::ZERO, &seats, &reserved, SEAT_SEARCH_RADIUS).map(|(e, _)| e),
            Some(2)
        );
        // Reserve 2 → next nearest is 1 (3 is out of radius).
        reserved.insert(2);
        assert_eq!(
            pick_nearest_seat(Vec3::ZERO, &seats, &reserved, SEAT_SEARCH_RADIUS).map(|(e, _)| e),
            Some(1)
        );
        // Reserve 1 too → only the out-of-range seat 3 remains → None.
        reserved.insert(1);
        assert!(pick_nearest_seat(Vec3::ZERO, &seats, &reserved, SEAT_SEARCH_RADIUS).is_none());
    }

    #[test]
    fn pick_nearest_seat_respects_per_actor_radius() {
        let seats = vec![(1, GlobalTransform::new(Vec3::new(300.0, 0.0, 0.0), Quat::IDENTITY, 1.0))];
        let reserved = HashSet::new();
        // A small authored radius excludes a seat the default 512 would include.
        assert!(pick_nearest_seat(Vec3::ZERO, &seats, &reserved, 128.0).is_none());
        assert_eq!(
            pick_nearest_seat(Vec3::ZERO, &seats, &reserved, 512.0).map(|(e, _)| e),
            Some(1)
        );
    }

    #[test]
    fn pick_nearest_seat_empty_is_none() {
        assert!(pick_nearest_seat(
            Vec3::ZERO,
            &[] as &[((EntityId, u32), GlobalTransform)],
            &HashSet::new(),
            SEAT_SEARCH_RADIUS
        )
        .is_none());
    }

    #[test]
    fn distinct_markers_of_one_furniture_reserve_independently() {
        // The M42.2 seat-polish invariant: two markers on the SAME furniture
        // entity (a counter/bench) are two seats. Reserving one must not lock
        // the other — a second actor takes marker 1 of the same furniture.
        let furn = 42; // one furniture entity, two sit markers
        let seats = vec![
            ((furn, 0u32), GlobalTransform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, 1.0)),
            ((furn, 1u32), GlobalTransform::new(Vec3::new(20.0, 0.0, 0.0), Quat::IDENTITY, 1.0)),
        ];
        let mut reserved: HashSet<(EntityId, u32)> = HashSet::new();
        // Actor A near marker 0 claims it.
        let a = pick_nearest_seat(Vec3::ZERO, &seats, &reserved, SEAT_SEARCH_RADIUS);
        assert_eq!(a.map(|(id, _)| id), Some((furn, 0)));
        reserved.insert((furn, 0));
        // Actor B still finds marker 1 of the *same* furniture — pre-fix this
        // returned None because the whole furniture was reserved.
        let b = pick_nearest_seat(Vec3::ZERO, &seats, &reserved, SEAT_SEARCH_RADIUS);
        assert_eq!(b.map(|(id, _)| id), Some((furn, 1)));
        reserved.insert((furn, 1));
        // Both markers taken → the furniture is full.
        assert!(pick_nearest_seat(Vec3::ZERO, &seats, &reserved, SEAT_SEARCH_RADIUS).is_none());
    }
}
