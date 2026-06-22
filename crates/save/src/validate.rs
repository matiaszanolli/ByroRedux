//! Pre-save referential-integrity pass.
//!
//! The thesis behind the full-snapshot format is that Bethesda's slow
//! save corruption comes from persisting *inconsistent* state, not from
//! careless serialisation. So before a save is written we walk the World
//! and refuse to persist a structurally broken one — better to fail the
//! save loudly than to seed a corruption tail.
//!
//! These checks need only `byroredux-core` types (hierarchy, inventory,
//! equipment, animation). Cross-plugin checks that need the `DataStore`
//! (e.g. "every `FormIdComponent` resolves to a loaded record") live in
//! the binary, which owns that resource — call [`validate_world`] first,
//! then layer game-specific checks on top.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::components::{Children, EquipmentSlots, Inventory, Parent};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;

/// A single referential-integrity violation found before save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The entity the broken reference lives on.
    pub entity: EntityId,
    /// Which check failed (for grouping / log filtering).
    pub kind: ValidationKind,
    /// Human-readable detail.
    pub detail: String,
}

/// The category of a [`ValidationError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationKind {
    /// A `Parent`/`Children` edge is one-directional or dangling.
    Hierarchy,
    /// An `EquipmentSlots` occupant indexes outside its `Inventory`.
    Equipment,
    /// An `AnimationPlayer.clip_handle` isn't in the clip registry.
    AnimationClip,
    /// An entity reference points past `next_entity` (never spawned).
    DanglingEntity,
}

/// Walk the world and collect every referential-integrity violation.
///
/// An empty result means the world is safe to snapshot. The save driver
/// (binary side) refuses the write when this is non-empty and dumps the
/// list to the log.
pub fn validate_world(world: &World) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let next_entity = world.next_entity_id();

    validate_hierarchy(world, next_entity, &mut errors);
    validate_equipment(world, &mut errors);
    validate_animation(world, next_entity, &mut errors);

    errors
}

/// `Parent` ⇄ `Children` must agree, and neither may point past
/// `next_entity` (an id that was never spawned).
fn validate_hierarchy(world: &World, next_entity: EntityId, errors: &mut Vec<ValidationError>) {
    // child -> parent, from the Parent column.
    let parent_of: std::collections::HashMap<EntityId, EntityId> = match world.query::<Parent>() {
        Some(q) => q.iter().map(|(c, p)| (c, p.0)).collect(),
        None => std::collections::HashMap::new(),
    };

    // Every Parent edge: target must be a spawned id, and the parent's
    // Children list (if it has one) must contain this child.
    if let Some(q_children) = world.query::<Children>() {
        let children_of: std::collections::HashMap<EntityId, Vec<EntityId>> =
            q_children.iter().map(|(p, c)| (p, c.0.clone())).collect();

        for (&child, &parent) in &parent_of {
            if parent >= next_entity {
                errors.push(ValidationError {
                    entity: child,
                    kind: ValidationKind::DanglingEntity,
                    detail: format!("Parent({parent}) was never spawned (next_entity={next_entity})"),
                });
                continue;
            }
            match children_of.get(&parent) {
                Some(list) if list.contains(&child) => {}
                Some(_) => errors.push(ValidationError {
                    entity: child,
                    kind: ValidationKind::Hierarchy,
                    detail: format!("Parent({parent}) but not in that parent's Children list"),
                }),
                None => errors.push(ValidationError {
                    entity: child,
                    kind: ValidationKind::Hierarchy,
                    detail: format!("Parent({parent}) but parent has no Children component"),
                }),
            }
        }

        // Every Children entry: the listed child must back-reference us.
        for (&parent, list) in &children_of {
            for &child in list {
                if child >= next_entity {
                    errors.push(ValidationError {
                        entity: parent,
                        kind: ValidationKind::DanglingEntity,
                        detail: format!("Children lists {child}, never spawned (next_entity={next_entity})"),
                    });
                } else if parent_of.get(&child) != Some(&parent) {
                    errors.push(ValidationError {
                        entity: parent,
                        kind: ValidationKind::Hierarchy,
                        detail: format!("Children lists {child}, but its Parent != {parent}"),
                    });
                }
            }
        }
    } else {
        // No Children column at all — only the dangling-parent check applies.
        for (&child, &parent) in &parent_of {
            if parent >= next_entity {
                errors.push(ValidationError {
                    entity: child,
                    kind: ValidationKind::DanglingEntity,
                    detail: format!("Parent({parent}) was never spawned (next_entity={next_entity})"),
                });
            }
        }
    }
}

/// Every `EquipmentSlots` occupant must index a live row in the same
/// entity's `Inventory`.
fn validate_equipment(world: &World, errors: &mut Vec<ValidationError>) {
    let Some(q_equip) = world.query::<EquipmentSlots>() else {
        return;
    };
    let inv = world.query::<Inventory>();

    for (entity, slots) in q_equip.iter() {
        let item_count = inv
            .as_ref()
            .and_then(|q| q.iter().find(|(e, _)| *e == entity).map(|(_, i)| i.items.len()));
        for occupant in slots.occupants.iter().flatten() {
            match item_count {
                None => errors.push(ValidationError {
                    entity,
                    kind: ValidationKind::Equipment,
                    detail: format!("equips inventory[{}] but entity has no Inventory", occupant.0),
                }),
                Some(n) if (occupant.0 as usize) >= n => errors.push(ValidationError {
                    entity,
                    kind: ValidationKind::Equipment,
                    detail: format!("equips inventory[{}] but Inventory holds {n} items", occupant.0),
                }),
                Some(_) => {}
            }
        }
    }
}

/// Every `AnimationPlayer.clip_handle` must resolve in the clip registry,
/// and its `root_entity` (if set) must be a spawned id.
fn validate_animation(world: &World, next_entity: EntityId, errors: &mut Vec<ValidationError>) {
    let Some(q) = world.query::<AnimationPlayer>() else {
        return;
    };
    let registry = world.try_resource::<AnimationClipRegistry>();

    for (entity, player) in q.iter() {
        if let Some(reg) = registry.as_ref() {
            if reg.get(player.clip_handle).is_none() {
                errors.push(ValidationError {
                    entity,
                    kind: ValidationKind::AnimationClip,
                    detail: format!("clip_handle {} not in AnimationClipRegistry", player.clip_handle),
                });
            }
        }
        if let Some(root) = player.root_entity {
            if root >= next_entity {
                errors.push(ValidationError {
                    entity,
                    kind: ValidationKind::DanglingEntity,
                    detail: format!("AnimationPlayer.root_entity {root} was never spawned"),
                });
            }
        }
    }
}
