//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

use super::*;
use byroredux_core::ecs::components::Transform;
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;

/// A clean validation pass is the precondition every save checks.
#[test]
fn fresh_world_validates_clean() {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Transform::default());
    assert!(validate_world(&world).is_empty());
}

/// SAVE-D4-01 (SIBLING): a `FormIdComponent` whose handle doesn't
/// resolve in the live `FormIdPool` is rejected by the binary-side gate
/// before the write — otherwise the snapshot serializer silently drops
/// it and the entity reloads without its form id.
#[test]
fn unresolvable_form_id_is_rejected() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

    let mut world = World::new();
    world.insert_resource(FormIdPool::new()); // empty — resolves nothing

    // Mint a handle in a throwaway pool; the world's empty pool can't
    // resolve it (an empty `to_pair` yields `None` for any index).
    let stray = {
        let mut tmp = FormIdPool::new();
        tmp.intern(FormIdPair {
            plugin: PluginId::from_filename("Test.esm"),
            local: LocalFormId(0x07),
        })
    };

    let e = world.spawn();
    world.insert(e, FormIdComponent(stray));

    let errors = validate_form_ids(&world);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, ValidationKind::FormId);
    assert_eq!(errors[0].entity, e);
}

/// A resolvable handle (interned in the world's own pool) passes clean.
#[test]
fn resolvable_form_id_passes() {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

    let mut world = World::new();
    world.insert_resource(FormIdPool::new());
    let fid = {
        let mut pool = world.resource_mut::<FormIdPool>();
        pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Test.esm"),
            local: LocalFormId(0x07),
        })
    };
    let e = world.spawn();
    world.insert(e, FormIdComponent(fid));
    assert!(validate_form_ids(&world).is_empty());
}

/// #2535 / SAVE-D4-02: a `HorseTetherState.horse` pointing at an id
/// past `next_entity` (the tethered horse despawned mid-session while
/// the tether component survived) is flagged, mirroring
/// `validate_form_ids`'s dangling-reference contract.
#[test]
fn dangling_horse_tether_reference_is_rejected() {
    use byroredux_core::math::Quat;
    use byroredux_scripting::cinematic::HorseTetherState;

    let mut world = World::new();
    let e = world.spawn();
    world.insert(
        e,
        HorseTetherState {
            horse: 999, // never spawned
            horse_local_translation: Vec3::ZERO,
            horse_local_rotation: Quat::IDENTITY,
        },
    );

    let errors = validate_cinematic_entity_refs(&world);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, ValidationKind::DanglingEntity);
    assert_eq!(errors[0].entity, e);
}

/// Companion: a horse tether pointing at a live, actually-spawned
/// entity passes clean.
#[test]
fn live_horse_tether_reference_passes() {
    use byroredux_core::math::Quat;
    use byroredux_scripting::cinematic::HorseTetherState;

    let mut world = World::new();
    let horse = world.spawn();
    let e = world.spawn();
    world.insert(
        e,
        HorseTetherState {
            horse,
            horse_local_translation: Vec3::ZERO,
            horse_local_rotation: Quat::IDENTITY,
        },
    );

    assert!(validate_cinematic_entity_refs(&world).is_empty());
}

/// #2535 / SAVE-D4-02: same check for `ActorCinematicState.vehicle`.
#[test]
fn dangling_cinematic_vehicle_reference_is_rejected() {
    use byroredux_scripting::cinematic::ActorCinematicState;

    let mut world = World::new();
    let e = world.spawn();
    world.insert(
        e,
        ActorCinematicState {
            vehicle: Some(999), // never spawned
            ..Default::default()
        },
    );

    let errors = validate_cinematic_entity_refs(&world);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].kind, ValidationKind::DanglingEntity);
    assert_eq!(errors[0].entity, e);
}

/// Companion: `vehicle: None` (detached/never mounted) passes clean,
/// same as a live-vehicle reference.
#[test]
fn cinematic_state_without_vehicle_passes() {
    use byroredux_scripting::cinematic::ActorCinematicState;

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, ActorCinematicState::default());

    assert!(validate_cinematic_entity_refs(&world).is_empty());
}
