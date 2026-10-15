//! The inventory effect applier plus `add_items` / `remove_items`. (#5468, split from `effects.rs`)

use super::*;

/// `RemoveItem`'s runtime (M47.2): take up to `count` of `item_form_id` from
/// `source`'s stacks, first-authored first, and move them to `destination` or
/// drop them.
///
/// Rows are zeroed in place, never removed, so the inventory indices that
/// `EquipmentSlots` holds stay valid (the `consume_item` / `transfer_loot`
/// discipline); an emptied row's equipment is released. A fully drained
/// stack that carries a per-instance identity travels whole to the
/// destination, or returns its instance to the [`ItemInstancePool`] when the
/// items vanish. Script `OnItemRemoved` / `OnItemAdded` see the move. Every
/// guard is scoped: the inventory write, then equipment, then the pool, then
/// the event batches — none is held across another.
pub fn remove_items(
    world: &World,
    source: EntityId,
    item_form_id: u32,
    count: u32,
    destination: Option<EntityId>,
) {
    use byroredux_core::ecs::components::InventoryIndex;
    if count == 0 || destination == Some(source) {
        return;
    }
    let mut removed = 0u32;
    let mut emptied: Vec<InventoryIndex> = Vec::new();
    let mut freed_instances = Vec::new();
    {
        let Some(mut inventories) = world.query_mut::<Inventory>() else {
            return;
        };
        if let Some(destination) = destination {
            // Validate the destination before taking anything: a missing
            // destination loses no items.
            if inventories.get_mut(destination).is_none() {
                inventories.insert(destination, Inventory::new());
            }
        }
        let Some(inventory) = inventories.get_mut(source) else {
            log::debug!("fragment RemoveItem skipped: container has no Inventory");
            return;
        };
        let mut moved: Vec<ItemStack> = Vec::new();
        for (index, stack) in inventory.items.iter_mut().enumerate() {
            if removed == count {
                break;
            }
            if stack.base_form_id != item_form_id || stack.count == 0 {
                continue;
            }
            let take = stack.count.min(count - removed);
            removed += take;
            if take == stack.count {
                // The whole row goes: its instance identity moves with it
                // (or is freed), and its equipment is released below.
                let whole = std::mem::replace(stack, ItemStack::new(item_form_id, 0));
                if let Ok(index) = u32::try_from(index) {
                    emptied.push(InventoryIndex(index));
                }
                if destination.is_some() {
                    moved.push(whole);
                } else if let Some(instance) = whole.instance {
                    freed_instances.push(instance);
                }
            } else {
                stack.count -= take;
                if destination.is_some() {
                    moved.push(ItemStack::new(item_form_id, take));
                }
            }
        }
        if let Some(destination) = destination {
            inventories
                .get_mut(destination)
                .expect("validated under the same guard")
                .items
                .extend(moved);
        }
    }
    if removed == 0 {
        return;
    }
    if !emptied.is_empty() {
        if let Some(mut equipment) = world.query_mut::<EquipmentSlots>() {
            if let Some(slots) = equipment.get_mut(source) {
                for index in &emptied {
                    slots.release(*index);
                }
            }
        }
    }
    if !freed_instances.is_empty() {
        if let Some(mut pool) =
            world.try_resource_mut::<byroredux_core::ecs::resources::ItemInstancePool>()
        {
            for instance in freed_instances {
                pool.release(instance);
            }
        }
    }
    let transfer = |added| crate::ItemTransfer {
        item_form_id,
        count: removed,
        added,
        stolen: false,
    };
    crate::emit_item_transfers(world, source, [transfer(false)]);
    if let Some(destination) = destination {
        crate::emit_item_transfers(world, destination, [transfer(true)]);
    }
}

/// `AddItem` / `EquipItem` — the `Inventory` + `EquipmentSlots` arms.
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_inventory_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &QuestStageState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::AddItem {
            container,
            item,
            count,
        } => {
            let container_entity = resolve_actor(
                vmad,
                world,
                context,
                container,
                &deferred.scene_actor_bindings,
                deferred.speaker,
            )?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let count = &resolve_item_count(count, context, vmad, world, stages, deferred)?;
            add_items(world, container_entity, item_form_id, *count);
            None
        }
        Effect::RemoveItem {
            container,
            item,
            count,
            destination,
        } => {
            let bindings = &deferred.scene_actor_bindings;
            let source = resolve_actor(vmad, world, context, container, bindings, deferred.speaker)?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let count = &resolve_item_count(count, context, vmad, world, stages, deferred)?;
            let destination = match destination {
                Some(via) => Some(resolve_actor(vmad, world, context, via, bindings, deferred.speaker)?),
                None => None,
            };
            remove_items(world, source, item_form_id, *count, destination);
            None
        }
        Effect::EquipItem {
            actor,
            item,
            silent: _,
        } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings, deferred.speaker)?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let Some(slot_mask) = world
                .try_resource::<crate::EquipItemCatalog>()
                .and_then(|catalog| catalog.slot_mask(item_form_id))
            else {
                log::debug!(
                    "fragment EquipItem skipped: item {item_form_id:08X} has no armor biped-slot metadata"
                );
                return None;
            };
            let Some((mut inventories, mut equipment)) =
                world.query_2_mut_mut::<Inventory, EquipmentSlots>()
            else {
                log::debug!(
                    "fragment EquipItem skipped: inventory/equipment storage is unavailable"
                );
                return None;
            };
            let Some(inventory) = inventories.get_mut(actor) else {
                log::debug!("fragment EquipItem skipped: actor has no Inventory");
                return None;
            };
            let Some(index) = inventory
                .items
                .iter()
                .position(|stack| stack.base_form_id == item_form_id && stack.count > 0)
                .and_then(|index| u32::try_from(index).ok())
                .map(InventoryIndex)
            else {
                log::debug!(
                    "fragment EquipItem skipped: actor does not carry item {item_form_id:08X}"
                );
                return None;
            };
            if equipment.get_mut(actor).is_none() {
                equipment.insert(actor, EquipmentSlots::new());
            }
            let slots = equipment
                .get_mut(actor)
                .expect("just present or inserted above");
            if slots.is_equipped(index) {
                return None;
            }
            let displaced = slots.equip(slot_mask, index);
            let mut changes = displaced
                .into_iter()
                .filter(|displaced| !slots.is_equipped(*displaced))
                .filter_map(|displaced| {
                    inventory
                        .get(displaced)
                        .map(|stack| crate::EquipmentChange {
                            item_form_id: stack.base_form_id,
                            equipped: false,
                        })
                })
                .collect::<Vec<_>>();
            changes.push(crate::EquipmentChange {
                item_form_id,
                equipped: true,
            });
            drop(inventories);
            drop(equipment);
            crate::emit_equipment_changes(world, actor, changes);
            None
        }
        _ => unreachable!("apply_inventory_effect received a non-inventory effect"),
    }
}

/// Add `count` of `item_form_id` to `container`'s inventory (creating one: in
/// Bethesda's runtime every object can receive items), emitting `OnItemAdded`.
pub fn add_items(world: &World, container: EntityId, item_form_id: u32, count: u32) {
    let Some(mut inventories) = world.query_mut::<Inventory>() else {
        log::debug!("fragment AddItem skipped: Inventory component never registered");
        return;
    };
    if inventories.get_mut(container).is_none() {
        // An interior-mutable insert onto an already-registered storage, not
        // a structural `&mut World` mutation.
        inventories.insert(container, Inventory::new());
    }
    inventories
        .get_mut(container)
        .expect("just present or inserted above")
        .push(ItemStack::new(item_form_id, count));
    drop(inventories);
    // `OnItemAdded` sees a scripted add like any other (M47.2).
    crate::emit_item_transfers(
        world,
        container,
        [crate::ItemTransfer {
            item_form_id,
            count,
            added: true,
            stolen: false,
        }],
    );
}
