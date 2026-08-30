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
use byroredux_core::character::CharacterLevel;
use byroredux_core::ecs::components::{
    Children, EquipmentSlots, EquippedWeapon, EscortState, FollowState, Inventory, Material,
    Parent, Seated,
};
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
    /// A component the save registry deliberately excludes as
    /// "re-derived from static ESM data, write-once" now holds state that
    /// isn't actually re-derivable — see [`validate_progression_state`].
    UnsavedProgression,
    /// A `Material` carries a non-finite (NaN/Inf) scalar — see
    /// [`validate_material_finiteness`] and [`Material::sanitize_finite`].
    NonFiniteMaterial,
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
    validate_saved_entity_references(world, next_entity, &mut errors);
    validate_animation(world, next_entity, &mut errors);
    validate_inventory_instances(world, &mut errors);
    validate_progression_state(world, &mut errors);
    validate_material_finiteness(world, &mut errors);

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
                    detail: format!(
                        "Parent({parent}) was never spawned (next_entity={next_entity})"
                    ),
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
                        detail: format!(
                            "Children lists {child}, never spawned (next_entity={next_entity})"
                        ),
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
                    detail: format!(
                        "Parent({parent}) was never spawned (next_entity={next_entity})"
                    ),
                });
            }
        }
    }
}

/// Every saved inventory index must resolve to the same entity's Inventory.
fn validate_equipment(world: &World, errors: &mut Vec<ValidationError>) {
    // #3580 — SNAPSHOT the inventories, then drop the guard, rather than
    // holding it across the `EquipmentSlots` / `EquippedWeapon` scans below.
    //
    // Those scans used to run underneath a live `Inventory` guard, recording
    // `Inventory -> EquipmentSlots` in `lock_tracker`'s process-wide graph.
    // The scripting runtime reaches the same pair through
    // `query_2_mut_mut::<Inventory, EquipmentSlots>`, which acquires in
    // **TypeId order** — an order no source site can predict, and which is
    // therefore the one that has to win. Any hand-ordered pair here is a
    // coin flip that closes a cycle whenever it lands the other way, which
    // is exactly what turned the `lock-order-check` CI job red from two
    // different crates. Not overlapping the guards at all is what makes this
    // site order-independent.
    //
    // The snapshot is the base FormID list per entity, which is everything
    // both scans need (`len()` for the range check, `[index]` for the
    // weapon's identity check). Validation runs once per save, over one
    // world.
    let inventories: std::collections::HashMap<EntityId, Vec<u32>> =
        match world.query::<Inventory>() {
            Some(q) => q
                .iter()
                .map(|(entity, inventory)| {
                    (
                        entity,
                        inventory
                            .items
                            .iter()
                            .map(|stack| stack.base_form_id)
                            .collect(),
                    )
                })
                .collect(),
            None => std::collections::HashMap::new(),
        };

    if let Some(q_equip) = world.query::<EquipmentSlots>() {
        for (entity, slots) in q_equip.iter() {
            let item_count = inventories.get(&entity).map(|items| items.len());
            // #3112 — spans the weapon slot too, which is a separate field
            // rather than a biped occupant and would otherwise restore
            // unvalidated.
            for occupant in slots.equipped_indices() {
                validate_inventory_index(entity, "EquipmentSlots", occupant.0, item_count, errors);
            }
        }
    }

    if let Some(q_weapons) = world.query::<EquippedWeapon>() {
        for (entity, weapon) in q_weapons.iter() {
            let inventory = inventories.get(&entity);
            validate_inventory_index(
                entity,
                "EquippedWeapon",
                weapon.inventory_index.0,
                inventory.map(|items| items.len()),
                errors,
            );
            if let Some(base_form_id) =
                inventory.and_then(|items| items.get(weapon.inventory_index.0 as usize))
            {
                if *base_form_id != weapon.base_form_id {
                    errors.push(ValidationError {
                        entity,
                        kind: ValidationKind::Equipment,
                        detail: format!(
                            "EquippedWeapon base FormID {:08X} does not match inventory[{}] ({:08X})",
                            weapon.base_form_id, weapon.inventory_index.0, base_form_id
                        ),
                    });
                }
            }
        }
    }
}

fn validate_inventory_index(
    entity: EntityId,
    component: &str,
    index: u32,
    item_count: Option<usize>,
    errors: &mut Vec<ValidationError>,
) {
    match item_count {
        None => errors.push(ValidationError {
            entity,
            kind: ValidationKind::Equipment,
            detail: format!(
                "{component} references inventory[{index}] but entity has no Inventory"
            ),
        }),
        Some(n) if index as usize >= n => errors.push(ValidationError {
            entity,
            kind: ValidationKind::Equipment,
            detail: format!(
                "{component} references inventory[{index}] but Inventory holds {n} items"
            ),
        }),
        Some(_) => {}
    }
}

/// Validate one session-local entity reference using the common save gate.
/// Binary-owned component validators use this too, so the range contract and
/// diagnostics cannot drift between crates.
pub fn validate_entity_reference(
    owner: EntityId,
    component_field: &str,
    target: EntityId,
    next_entity: EntityId,
    errors: &mut Vec<ValidationError>,
) {
    if target >= next_entity {
        errors.push(ValidationError {
            entity: owner,
            kind: ValidationKind::DanglingEntity,
            detail: format!(
                "{component_field} references {target}, never spawned (next_entity={next_entity})"
            ),
        });
    }
}

fn validate_saved_entity_references(
    world: &World,
    next_entity: EntityId,
    errors: &mut Vec<ValidationError>,
) {
    if let Some(query) = world.query::<FollowState>() {
        for (owner, state) in query.iter() {
            if let Some(target) = state.target_entity {
                validate_entity_reference(
                    owner,
                    "FollowState.target_entity",
                    target,
                    next_entity,
                    errors,
                );
            }
        }
    }
    if let Some(query) = world.query::<EscortState>() {
        for (owner, state) in query.iter() {
            if let Some(target) = state.target_entity {
                validate_entity_reference(
                    owner,
                    "EscortState.target_entity",
                    target,
                    next_entity,
                    errors,
                );
            }
        }
    }
    if let Some(query) = world.query::<Seated>() {
        for (owner, state) in query.iter() {
            validate_entity_reference(
                owner,
                "Seated.furniture",
                state.furniture,
                next_entity,
                errors,
            );
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
                    detail: format!(
                        "clip_handle {} not in AnimationClipRegistry",
                        player.clip_handle
                    ),
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

/// `CharacterLevel` is deliberately excluded from the save registry — see
/// `REDERIVED_NOT_SAVED`, `byroredux/src/save_io/round_trip_tests.rs` —
/// classified as "re-derived from static ESM `NPC_` data, write-once". That
/// premise holds only because no leveling runtime exists yet: NPC spawn
/// always stamps `xp: 0` (`byroredux/src/npc_spawn.rs`), and `CharacterLevel`
/// itself defines `xp` as runtime progress toward the next level — state a
/// static ESM record cannot supply by construction. The day XP starts
/// accumulating, the exemption is false and every save would silently
/// discard it (#2947). Abort loudly instead of letting that happen quietly:
/// a non-zero `xp` at snapshot time means either a leveling runtime landed
/// without registering `CharacterLevel` in `build_save_registry` (do that,
/// dropping it from `REDERIVED_NOT_SAVED` in the same commit, per #1835's
/// established pattern), or this check needs to move with it.
fn validate_progression_state(world: &World, errors: &mut Vec<ValidationError>) {
    let Some(q_level) = world.query::<CharacterLevel>() else {
        return;
    };
    for (entity, level) in q_level.iter() {
        if level.xp != 0 {
            errors.push(ValidationError {
                entity,
                kind: ValidationKind::UnsavedProgression,
                detail: format!(
                    "CharacterLevel.xp = {} is unsaved runtime progress, but CharacterLevel \
                     is excluded from the save registry as write-once/re-derivable-from-ESM \
                     (#2947) — register it in build_save_registry before this can be true",
                    level.xp
                ),
            });
        }
    }
}

/// Every `Material` must carry only finite (non-NaN, non-±inf) scalars —
/// see [`Material::sanitize_finite`] for why. Probes a clone rather than
/// mutating the live world: `validate_world` takes `&World` and is meant
/// to be a pure pre-save check, and reusing `sanitize_finite`'s own field
/// list here (instead of re-deriving one) means a future field added to
/// `Material` can't drift the two checks apart. #2687 (SAFE-D9-01).
fn validate_material_finiteness(world: &World, errors: &mut Vec<ValidationError>) {
    let Some(q) = world.query::<Material>() else {
        return;
    };
    for (entity, material) in q.iter() {
        let mut probe = material.clone();
        if probe.sanitize_finite() {
            errors.push(ValidationError {
                entity,
                kind: ValidationKind::NonFiniteMaterial,
                detail: "Material has a non-finite (NaN/Inf) scalar field — would poison \
                         GpuMaterial on the GPU"
                    .to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{InventoryIndex, ItemInstanceId, ItemStack};
    use byroredux_core::ecs::resources::ItemInstance;
    use byroredux_core::math::Vec3;
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

        assert!(
            validate_world(&world).is_empty(),
            "{:?}",
            validate_world(&world)
        );
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

    #[test]
    fn equipped_weapon_must_resolve_to_matching_inventory_row() {
        let mut world = World::new();
        let actor = world.spawn();
        let mut inventory = Inventory::new();
        inventory.push(ItemStack::new(0x1234, 1));
        world.insert(actor, inventory);
        world.insert(
            actor,
            EquippedWeapon {
                inventory_index: InventoryIndex(1),
                base_form_id: 0x1234,
                damage: 10.0,
                reach: 0.0,
                speed: 0.0,
            },
        );
        let errors = validate_world(&world);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].kind, ValidationKind::Equipment);
    }

    #[test]
    fn saved_ai_and_seat_entity_references_share_the_dangling_gate() {
        let mut world = World::new();
        let actor = world.spawn();
        let never_spawned = world.next_entity_id() + 10;
        world.insert(
            actor,
            FollowState {
                target_entity: Some(never_spawned),
            },
        );
        world.insert(
            actor,
            EscortState {
                target_entity: Some(never_spawned),
                destination: Some(Vec3::ZERO),
            },
        );
        world.insert(
            actor,
            Seated {
                furniture: never_spawned,
                animation_restore: Default::default(),
            },
        );

        let errors = validate_world(&world);
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors
            .iter()
            .all(|error| error.kind == ValidationKind::DanglingEntity));
    }

    /// #2947 — the exemption `REDERIVED_NOT_SAVED` documents
    /// (`byroredux/src/save_io/round_trip_tests.rs`) holds only because NPC
    /// spawn always stamps `xp: 0`; a freshly spawned actor must pass clean.
    #[test]
    fn character_level_with_no_progress_is_clean() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, CharacterLevel { level: 5, xp: 0 });
        assert!(validate_world(&world).is_empty());
    }

    /// #2687 (SAFE-D9-01) — a `Material` with a non-finite scalar (the
    /// shape a hand-edited/corrupted save would carry) trips the gate
    /// rather than being written silently. A clean `Material` passes.
    #[test]
    fn material_with_non_finite_scalar_trips_the_gate() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(
            e,
            Material {
                roughness: f32::NAN,
                ..Material::default()
            },
        );

        let errors = validate_world(&world);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].entity, e);
        assert_eq!(errors[0].kind, ValidationKind::NonFiniteMaterial);
    }

    #[test]
    fn material_with_only_finite_scalars_is_clean() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Material::default());
        assert!(validate_world(&world).is_empty());
    }

    /// #2947 — the moment `xp` accumulates, `CharacterLevel` is no longer
    /// re-derivable from static ESM data (it's runtime progress, by
    /// CHARAL's own definition), so the save-exemption premise is false.
    /// This must abort the save loudly rather than silently discard the
    /// progress, exactly like every other referential-integrity gate here.
    #[test]
    fn character_level_with_progress_trips_the_unsaved_progression_gate() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, CharacterLevel { level: 5, xp: 42 });

        let errors = validate_world(&world);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].entity, e);
        assert_eq!(errors[0].kind, ValidationKind::UnsavedProgression);
        assert!(errors[0].detail.contains("42"));
    }
}
