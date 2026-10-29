//! Inventory handlers: item hand-out / removal, equip state, placement.

use super::*;

pub(super) fn add_item(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(item) = form_arg(args, 0) else { return Action::Declined };
        let count = num(args, 1).max(0.0) as u32;
        if count == 0 {
            return Action::Done { value: None, effect: String::new() };
        }
        // A form list stands for each of its members.
        let items = crate::form_lists::forms_of(world, item, &[]).unwrap_or_else(|| vec![item]);
        items
            .into_iter()
            .map(|item| {
                let item = bind.object(item);
                if cmd == CMD_ADD_ITEM {
                    Effect::AddItem {
                        container: me.clone(),
                        item,
                        count: ItemCount::Literal(count),
                        silent: num(args, 2) != 0.0,
                    }
                } else {
                    Effect::RemoveItem { container: me.clone(), item, count: ItemCount::Literal(count), destination: None }
                }
            })
            .collect()
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

pub(super) fn equip_item(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(item) = form_arg(args, 0) else { return Action::Declined };
        if num(args, 1) != 0.0 {
            return Action::Declined;
        }
        vec![Effect::EquipItem { actor: me, item: bind.object(item), silent: num(args, 2) != 0.0 }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// GECK: "Removes all items from the calling reference's inventory. If a
// target container is specified, the items are moved ... otherwise the items
// are destroyed". The retain-ownership flag has no ledger to carry it.
pub(super) fn remove_all_items(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let source = caller_entity;
    let destination = match form_arg(args, 0) {
        Some(form) => match live(world, form) {
            Some(entity) => Some(entity),
            None => return Action::Declined,
        },
        None => None,
    };
    let stacks: Vec<(u32, u32)> = world
        .get::<byroredux_core::ecs::components::Inventory>(source)
        .map(|inventory| inventory.items.iter().filter(|stack| stack.count > 0).map(|stack| (stack.base_form_id, stack.count)).collect())
        .unwrap_or_default();
    for (item, count) in stacks {
        crate::fragment::remove_items(world, source, item, count, destination);
    }
    Action::Done { value: None, effect: format!("RemoveAllItems {caller:08X}") }
}

// `RemoveAllTypedItems [target] [retain ownership] [suppress messages] <type code> [exempt list]`:
// every item of that type (the GECK's FOSE type codes: 24 Armor, 40 Weapon, 47 AlchemyItem ...) leaves
// the calling container, for the target if one is named (else it is destroyed), except the base
// forms of the exemption form list. "Failing to specify a type code ... may" remove everything or
// nothing, so a call with none is declined. Ownership is not tracked per stack here, so the retain
// flag and the message flag have nothing to act on.
pub(super) fn remove_all_typed_items(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let source = caller_entity;
    let code = num(args, 3);
    if !(1.0..=255.0).contains(&code) {
        return Action::Declined;
    }
    let destination = match form_arg(args, 0) {
        Some(form) => match live(world, form) {
            Some(entity) => Some(entity),
            None => return Action::Declined,
        },
        None => None,
    };
    let exempt = form_arg(args, 4).and_then(|list| crate::form_lists::forms_of(world, list, &[])).unwrap_or_default();
    let Some(codes) = world.try_resource::<crate::obscript_state::ItemTypeCodes>().map(|codes| codes.clone()) else {
        return Action::Declined;
    };
    let stacks: Vec<(u32, u32)> = world
        .get::<byroredux_core::ecs::components::Inventory>(source)
        .map(|inventory| {
            inventory
                .items
                .iter()
                .filter(|stack| stack.count > 0 && codes.code_of(stack.base_form_id) == Some(code as u8) && !exempt.contains(&stack.base_form_id))
                .map(|stack| (stack.base_form_id, stack.count))
                .collect()
        })
        .unwrap_or_default();
    for (item, count) in stacks {
        crate::fragment::remove_items(world, source, item, count, destination);
    }
    Action::Done { value: None, effect: format!("RemoveAllTypedItems {caller:08X} type {code}") }
}

// `PlaceAtMe <base> [count] [distance] [direction]` at the calling
// reference: the created references are the ones `CreatedReferences` spawns
// when their cell loads. The offset is not modelled (the Papyrus path has none
// either), so a distance is declined rather than dropped.
pub(super) fn place_at_me(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _)) = live_caller(world, context) else { return Action::Declined };
    let Some(base) = form_arg(args, 0) else { return Action::Declined };
    let count = if args.len() > 1 { num(args, 1).clamp(0.0, 32.0) as u32 } else { 1 };
    if num(args, 2) != 0.0 {
        return Action::Declined;
    }
    let Some(ids) = world.try_resource::<crate::CreatedReferences>().map(|created| created.scripted_ids()) else {
        return Action::Declined;
    };
    let mut last = 0;
    for _ in 0..count {
        let Some(form) = ids.next() else { break };
        if let Some(mut created) = world.try_resource_mut::<crate::CreatedReferences>() {
            created.create_scripted(form, base, caller, false);
        }
        last = form;
    }
    Action::Done {
        value: Some(ObScriptValue::Ref(last)),
        effect: format!("PlaceAtMe {base:08X} x{count} at {caller:08X}"),
    }
}

// `UnequipItem <item> [no-equip flag] [hide message]`: the actor takes the item off. The
// no-equip flag ("unable to equip the item until EquipItem is called") has no state to
// carry it, so a set flag declines, as `EquipItem`'s no-unequip flag does.
pub(super) fn unequip_item(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    use byroredux_core::ecs::components::{EquipmentSlots, Inventory, InventoryIndex};
    let (Some(item), actor) = (form_arg(args, 0), caller_entity) else {
        return Action::Declined;
    };
    if num(args, 1) != 0.0 {
        return Action::Declined;
    }
    let released = {
        let Some((inventories, mut equipment)) = world.query_2_mut::<Inventory, EquipmentSlots>() else {
            return Action::Declined;
        };
        let Some(index) = inventories
            .get(actor)
            .and_then(|inventory| inventory.items.iter().position(|stack| stack.base_form_id == item && stack.count > 0))
            .and_then(|index| u32::try_from(index).ok())
            .map(InventoryIndex)
        else {
            return Action::Declined;
        };
        equipment.get_mut(actor).is_some_and(|slots| slots.release(index))
    };
    if released {
        crate::emit_equipment_changes(world, actor, [crate::EquipmentChange { item_form_id: item, equipped: false }]);
    }
    Action::Done { value: None, effect: format!("UnequipItem {caller:08X} {item:08X}") }
}
