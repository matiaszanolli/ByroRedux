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
//!
//! #7264 — the table's binary-side rows are covered here, its scripting-side
//! rows by `cleanup.rs`'s own drain test, and
//! [`every_pattern_b_table_row_has_a_drain_test`] holds the table to that: a
//! row added with no drain test in either crate fails it.

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
    // #7273: the label must be the type raised, so the table-coverage check
    // below reads a case as covering exactly the marker it names.
    assert_eq!(
        std::any::type_name::<T>().rsplit("::").next(),
        Some(name),
        "drains({name:?}, ..) raises a different marker type"
    );
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
        UseWeaponRequest { weapon: 0, target: 0, shots: 1, harmless: false, options: Default::default() },
        |w| super::script_requests::use_weapon_system(w, 0.0),
    );
}

#[test]
fn reset_inventory_system_takes_its_request() {
    drains("ResetInventoryRequest", ResetInventoryRequest, |w| {
        super::script_requests::reset_inventory_system(w, 0.0)
    });
}

#[test]
fn ambient_ai_package_system_takes_its_release_marker() {
    drains("ReleasePackageHold", byroredux_scripting::package::ReleasePackageHold, |w| {
        crate::npc_spawn::ai_package::ambient_ai_package_system(w, 0.0)
    });
}

/// #7264 — every marker `cleanup.rs`'s Pattern-B table lists is run through its
/// consumer by a drain test: the scripting-side ones in that file's
/// `every_documented_pattern_b_marker_drains_in_its_named_consumer`, the
/// binary-side ones above. `OnResetEvent`'s consumer is private to
/// `obscript_objects`, which tests its drain itself.
///
/// #7304 — and every row has the other half too: a `survives(..)` case in
/// `cleanup.rs`'s `pattern_b_markers_survive_event_cleanup`, so a marker added
/// to `event_cleanup_system` by mistake is caught before its consumer misses it.
#[test]
fn every_pattern_b_table_row_has_a_drain_test() {
    const CLEANUP: &str = include_str!("../../../crates/scripting/src/cleanup.rs");
    const HERE: &str = include_str!("request_drain_tests.rs");
    const OBSCRIPT_OBJECTS: &str = include_str!("../../../crates/scripting/src/obscript_objects.rs");
    let elsewhere = [(
        "OnResetEvent",
        OBSCRIPT_OBJECTS.contains("fn the_reset_marker_is_taken_even_when_nothing_has_a_script()"),
    )];

    let table = CLEANUP
        .lines()
        .skip_while(|line| !line.starts_with("//! | Marker | Owning consumer |"))
        .skip(2)
        .take_while(|line| line.starts_with("//! | "));
    let mut markers = Vec::new();
    let mut rows = 0;
    for row in table {
        rows += 1;
        let first_cell = row["//! | ".len()..].split(" | ").next().unwrap_or_default();
        let before = markers.len();
        markers.extend(first_cell.split('`').skip(1).step_by(2));
        assert!(markers.len() > before, "table row names no marker: {row}");
    }
    assert!(rows > 0, "cleanup.rs's Pattern-B table was not found");

    // rustfmt may break a call after `drains(`, so match with whitespace
    // removed; comment lines are dropped first, so a commented-out case does
    // not count (#7273). Each `drains` asserts its label is the type it raises.
    let squash = |text: &str| {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .flat_map(str::split_whitespace)
            .collect::<String>()
    };
    let (cleanup, here) = (squash(CLEANUP), squash(HERE));
    for marker in markers {
        let call = format!("drains(\"{marker}\"");
        let covered = cleanup.contains(&call)
            || here.contains(&call)
            || elsewhere.iter().any(|&(name, tested)| name == marker && tested);
        assert!(
            covered,
            "`{marker}` is in cleanup.rs's Pattern-B table but no drain test runs its consumer: add a \
             `drains(\"{marker}\", ..)` case to cleanup.rs (scripting-side consumer) or to \
             byroredux/src/systems/request_drain_tests.rs (binary-side consumer)"
        );
        assert!(
            cleanup.contains(&format!("survives(\"{marker}\"")),
            "`{marker}` is in cleanup.rs's Pattern-B table but `pattern_b_markers_survive_event_cleanup` \
             does not check that `event_cleanup_system` leaves it for its consumer: add a \
             `survives(\"{marker}\", ..)` case there"
        );
    }
}
