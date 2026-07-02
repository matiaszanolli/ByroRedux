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
use byroredux_core::ecs::resources::ItemInstancePool;
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
    /// An `ItemStack.instance` id doesn't resolve to a live entry in the
    /// per-world `ItemInstancePool` (a dangling per-instance reference).
    ItemInstance,
    /// A `FormIdComponent` handle doesn't resolve in the `FormIdPool`.
    ///
    /// Emitted only by the **binary-side** supplementary check (which owns
    /// the `FormIdPool`), not by [`validate_world`]; the variant lives here
    /// so the binary can reuse [`ValidationError`] for a uniform abort
    /// message. See `byroredux::save_io::validate_form_ids`.
    FormId,
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
    validate_inventory_instances(world, &mut errors);

    errors
}

/// Log a post-load validation pass at WARN, truncated to the first 20
/// issues. Shared by every load path that runs [`validate_world`] as a
/// diagnostic rather than a save-time abort gate (#1844 / SAVE-01):
/// unlike the save path, a load can't cleanly fall back to the previous
/// world, so the minimum viable response to a corrupt-but-decodable save
/// (older engine, hand-edited file with a still-valid CRC) is a loud
/// diagnostic, not silence. No-op when `issues` is empty.
///
/// `context` is a short caller-supplied label (e.g. `"restore_world"` or
/// `"save load: cell 'X'"`) prefixed onto the summary line so the log
/// makes clear which load path and target produced the warning.
pub fn log_validation_warnings(context: &str, issues: &[ValidationError]) {
    if issues.is_empty() {
        return;
    }
    log::warn!(
        "{context}: loaded with {} referential-integrity issue(s) (save may predate a \
         validation rule, or was hand-edited):",
        issues.len()
    );
    for issue in issues.iter().take(20) {
        log::warn!(
            "  [{:?}] entity {}: {}",
            issue.kind,
            issue.entity,
            issue.detail
        );
    }
    if issues.len() > 20 {
        log::warn!("  … and {} more", issues.len() - 20);
    }
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

/// Every `ItemStack.instance` that is `Some(id)` must resolve to a live
/// entry in the per-world [`ItemInstancePool`] (saved as a resource).
///
/// A dangling `ItemInstanceId` — the pool entry released while the stack
/// referencing it survived, or an id past the pool's length — would pass
/// the other gates, be written, and on load index a non-existent or wrong
/// instance: the "persist an inconsistent reference" corruption tail the
/// format exists to prevent. `instance == None` (the stackable common
/// case) is always fine. A stack that references an instance while the
/// world carries no pool at all is itself unresolvable, so it is flagged
/// too. SAVE-D4-01.
fn validate_inventory_instances(world: &World, errors: &mut Vec<ValidationError>) {
    let Some(q_inv) = world.query::<Inventory>() else {
        return;
    };
    let pool = world.try_resource::<ItemInstancePool>();

    for (entity, inventory) in q_inv.iter() {
        for (idx, stack) in inventory.items.iter().enumerate() {
            let Some(instance) = stack.instance else {
                continue;
            };
            let resolves = pool.as_ref().is_some_and(|p| p.get(instance).is_some());
            if !resolves {
                let detail = match pool.as_ref() {
                    Some(_) => format!(
                        "items[{idx}].instance {} not live in ItemInstancePool",
                        instance.0
                    ),
                    None => format!(
                        "items[{idx}].instance {} but world has no ItemInstancePool",
                        instance.0
                    ),
                };
                errors.push(ValidationError {
                    entity,
                    kind: ValidationKind::ItemInstance,
                    detail,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{ItemInstanceId, ItemStack};
    use byroredux_core::ecs::resources::ItemInstance;
    use std::num::NonZeroU32;

    fn instance_id(slot: u32) -> ItemInstanceId {
        ItemInstanceId(NonZeroU32::new(slot).expect("test slot is non-zero"))
    }

    /// A stack with `instance == None` (the stackable common case) never
    /// touches the pool, so it is clean even with no pool present.
    #[test]
    fn stackable_item_without_instance_is_clean() {
        let mut world = World::new();
        let e = world.spawn();
        let mut inv = Inventory::new();
        inv.push(ItemStack::new(0xDEAD, 99));
        world.insert(e, inv);
        assert!(validate_world(&world).is_empty());
    }

    /// A live instance id (allocated in the pool) resolves — clean.
    #[test]
    fn live_item_instance_passes() {
        let mut world = World::new();
        let mut pool = ItemInstancePool::new();
        let id = pool.allocate(ItemInstance::default());
        world.insert_resource(pool);

        let e = world.spawn();
        let mut inv = Inventory::new();
        let mut stack = ItemStack::new(0xDEAD, 1);
        stack.instance = Some(id);
        inv.push(stack);
        world.insert(e, inv);

        assert!(validate_world(&world).is_empty(), "{:?}", validate_world(&world));
    }

    /// SAVE-D4-01 regression: a dangling `ItemInstanceId` (the pool entry
    /// released — or never allocated — while the referencing stack
    /// survived) is rejected by the gate rather than silently written and
    /// indexing a non-existent instance on load.
    #[test]
    fn dangling_item_instance_is_rejected() {
        let mut world = World::new();
        world.insert_resource(ItemInstancePool::new()); // empty: only the sentinel

        let e = world.spawn();
        let mut inv = Inventory::new();
        let mut stack = ItemStack::new(0xDEAD, 1);
        stack.instance = Some(instance_id(42)); // never allocated
        inv.push(stack);
        world.insert(e, inv);

        let errors = validate_world(&world);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].kind, ValidationKind::ItemInstance);
        assert_eq!(errors[0].entity, e);
    }

    /// An instance reference with no `ItemInstancePool` resource in the
    /// world at all is also unresolvable — flagged, not silently passed.
    #[test]
    fn item_instance_without_pool_is_rejected() {
        let mut world = World::new();
        let e = world.spawn();
        let mut inv = Inventory::new();
        let mut stack = ItemStack::new(0xDEAD, 1);
        stack.instance = Some(instance_id(1));
        inv.push(stack);
        world.insert(e, inv);

        let errors = validate_world(&world);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].kind, ValidationKind::ItemInstance);
    }
}
