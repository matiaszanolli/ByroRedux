//! #5804 — the `evaluate_function` arms that read the Run-On reference's
//! place in the world: its distance to another, its cell, and whether its 3D
//! is loaded.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::condition::Condition;

use super::resolve_entity_by_global_form_id;

pub(super) fn get_distance(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // GetDistance(target_form_id) → ‖subject − target‖ in world units.
    // `param_1` is the target FormID, already remapped to global
    // load-order space at parse time (#1666) — the same space entity
    // `FormIdComponent`s resolve to — so the resolve is exact.
    use byroredux_core::ecs::components::GlobalTransform;
    let Some(target) = resolve_entity_by_global_form_id(world, condition.param_1) else {
        // Target not currently spawned (e.g. in an unloaded cell).
        // Model an absent target as infinitely far so a proximity
        // gate (`GetDistance < N`) correctly fails rather than
        // reading a missing ref as "right here" (a 0.0 would).
        return f32::MAX;
    };
    let Some(subject_pos) = world.get::<GlobalTransform>(entity).map(|t| t.translation)
    else {
        return f32::MAX;
    };
    // First `get` borrow has been dropped (translation is Copy) before
    // the second — avoids holding two read guards on one storage lock.
    let Some(target_pos) = world.get::<GlobalTransform>(target).map(|t| t.translation)
    else {
        return f32::MAX;
    };
    (subject_pos - target_pos).length()
}

pub(super) fn get_in_cell(condition: &Condition, entity: EntityId, world: &World) -> f32 {
    // Cell ownership is stamped on every loaded entity. The global
    // authored CELL FormID lives once on its root, avoiding duplicate
    // identity data across every mesh, actor, and child entity.
    use byroredux_core::ecs::components::{CellFormId, CellRoot};
    let cell_root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
    world.get::<CellFormId>(cell_root).map_or(0.0, |cell| {
        if cell.0 == condition.param_1 {
            1.0
        } else {
            0.0
        }
    })
}

pub(super) fn has_loaded_3d(entity: EntityId, world: &World) -> f32 {
    // A loaded reference has joined the spatial ECS and therefore
    // carries a GlobalTransform. Cell unload removes that component,
    // making stale alias bindings correctly report false.
    use byroredux_core::ecs::components::GlobalTransform;
    if world.get::<GlobalTransform>(entity).is_some() {
        1.0
    } else {
        0.0
    }
}
