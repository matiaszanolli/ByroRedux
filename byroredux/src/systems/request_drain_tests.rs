//! #6688 — the request-driven systems drain their one-shot request marker.
//!
//! `crates/scripting/src/cleanup.rs` documents these markers as Pattern B: the
//! one system that owns a marker takes it, so it never outlives the frame it was
//! raised in and the end-of-frame cleanup must not touch it. The scripting
//! crate cannot call these systems (they live in the binary), so its test used
//! to look for each `take_requests::<T>(world)` call in the system's source
//! text. Here each system is run on a world holding its marker, and the marker
//! is gone afterwards — including when the request has nothing to act on, which
//! is the case a marker left behind by an early return would show.

use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::World;
use byroredux_scripting::obscript_more::{
    FleeRequest, PushActorRequest, ResetInventoryRequest, UseWeaponRequest,
};
use byroredux_scripting::obscript_state::ReviveRequest;
use byroredux_scripting::{
    HavokImpulseRequest, MotionTypeChangeRequest, PickIdleRequest,
};

fn present<T: Component>(world: &World) -> bool {
    world.query::<T>().is_some_and(|query| !query.is_empty())
}

/// Raise `marker` on a bare entity, run `system`, and expect the marker gone.
fn drains<T: Component>(name: &str, marker: T, system: impl FnOnce(&World)) {
    let mut world = World::new();
    byroredux_scripting::register(&mut world);
    let entity = world.spawn();
    world.insert(entity, marker);
    assert!(present::<T>(&world), "{name}: the fixture failed to raise the marker");
    system(&world);
    assert!(
        !present::<T>(&world),
        "`{name}` is Pattern B (drained by its own consumer), but its consumer left it in place — \
         every request system opens with `take_requests` (#6150); restore it, or move the drain \
         to `event_cleanup_system` and update the module doc in cleanup.rs (#2672)"
    );
}

#[test]
fn pick_idle_system_takes_its_request() {
    drains("PickIdleRequest", PickIdleRequest, |w| super::pick_idle_system(w, 0.0));
}

#[test]
fn scripted_motion_type_system_takes_its_request() {
    drains(
        "MotionTypeChangeRequest",
        MotionTypeChangeRequest {
            motion_type: byroredux_core::ecs::components::MotionType::Static,
            allow_activate: false,
        },
        |w| super::scripted_motion_type_system(w, 0.0),
    );
}

#[test]
fn scripted_havok_impulse_system_takes_its_request() {
    drains("HavokImpulseRequest", HavokImpulseRequest { vector: [0.0; 3] }, |w| {
        super::scripted_havok_impulse_system(w, 0.0)
    });
}

#[test]
fn flee_system_takes_its_request() {
    drains("FleeRequest", FleeRequest, |w| super::flee::flee_system(w, 0.0));
}

#[test]
fn push_actor_system_takes_its_request() {
    drains("PushActorRequest", PushActorRequest { from: 0, force: 0.0 }, |w| {
        super::knockdown::push_actor_system(w, 0.0)
    });
}

#[test]
fn revive_system_takes_its_request() {
    drains("ReviveRequest", ReviveRequest, |w| super::revive::revive_system(w, 0.0));
}

#[test]
fn use_weapon_system_takes_its_request() {
    drains(
        "UseWeaponRequest",
        UseWeaponRequest { weapon: 0, target: 0, shots: 1, harmless: false },
        |w| super::script_requests::use_weapon_system(w, 0.0),
    );
}

#[test]
fn reset_inventory_system_takes_its_request() {
    drains("ResetInventoryRequest", ResetInventoryRequest, |w| {
        super::script_requests::reset_inventory_system(w, 0.0)
    });
}
