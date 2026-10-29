//! World-state handlers: locks and open state, destruction, ownership, weather, placement and orientation, saves.

use super::*;

pub(super) fn lock(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _)) = live_caller(world, context) else { return Action::Declined };
    let mut bind = Binder::default();
    let effects: Vec<Effect> = {
        let target = bind.object(caller);
        let mut effects = vec![Effect::SetLocked { target: target.clone(), locked: cmd == CMD_LOCK }];
        // GECK: "Lock 0 (default) will lock it but NOT change the lock level;
        // 1-255 will change the level of the lock AND lock it." The CS
        // Oblivion page has no such zero: the level written is the level.
        let level = num(args, 0);
        let sets_level = if crate::ScriptGame::is_fallout3_nv(world) { level >= 1.0 } else { !args.is_empty() };
        if cmd == CMD_LOCK && sets_level {
            effects.push(Effect::SetLockLevel { target, level: level.min(255.0) as u8 });
        }
        effects
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `AutoSave`: "saves the game ... as an autosave" — the same request `Game.RequestAutoSave` raises.
pub(super) fn auto_save(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let bind = Binder::default();
    let effects: Vec<Effect> = vec![Effect::RequestSave { auto: true }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `ForceSave` ("a new save in a new slot, as if the player manually did so") and `SystemSave` (the one
// overwritten "SYS" save): the manual-save request `Game.RequestSave` raises.
pub(super) fn force_save(world: &World, _fallout: bool, cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let bind = Binder::default();
    let effects: Vec<Effect> = vec![Effect::RequestSave { auto: false }];
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `SetOpenState <0|1>`: a door the engine tracks no open/closed state for yet gets one (the
// two-state activator, which is saved) so the call is not lost and `GetOpenState` reads it back.
pub(super) fn set_open_state(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let mut bind = Binder::default();
    let effects: Vec<Effect> = {
        if let (entity, Some(mut states)) = (caller_entity, world.query_mut::<crate::vm_state::TwoStateActivator>()) {
            if states.get(entity).is_none() {
                states.insert(entity, crate::vm_state::TwoStateActivator::default());
            }
        }
        vec![Effect::SetOpen { target: bind.object(caller), open: num(args, 0) != 0.0 }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `ForceWeather <weather> [flag]` / `SetWeather <weather> [flag]`: "forces the weather to
// immediately switch to the specified weather. If OverrideWeatherFlag is 1, the weather will not
// change again until ReleaseWeatherOverride is called" (GECK). The engine runs one weather per
// worldspace and never changes it on its own, so the flag is kept (and saved) for the day it
// does; the exterior's sky is re-installed by the app (`apply_forced_weather`).
pub(super) fn force_weather(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(weather) = form_arg(args, 0) else { return Action::Declined };
    if !world.try_resource::<crate::weather_state::WeatherClassifications>().is_some_and(|known| known.knows(weather)) {
        return Action::Declined;
    }
    let Some(mut forced) = world.try_resource_mut::<crate::obscript_state::ForcedWeather>() else { return Action::Declined };
    *forced = crate::obscript_state::ForcedWeather { weather: Some(weather), locked: num(args, 1) != 0.0 };
    Action::Done { value: None, effect: format!("{} {weather:08X}", if cmd == CMD_FORCE_WEATHER { "ForceWeather" } else { "SetWeather" }) }
}

pub(super) fn release_weather_override(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(mut forced) = world.try_resource_mut::<crate::obscript_state::ForcedWeather>() else { return Action::Declined };
    *forced = crate::obscript_state::ForcedWeather::default();
    Action::Done { value: None, effect: "ReleaseWeatherOverride".into() }
}

pub(super) fn set_destroyed(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _)) = live_caller(world, context) else { return Action::Declined };
    crate::destruction::set_destroyed(world, caller, num(args, 0) != 0.0);
    Action::Done { value: None, effect: format!("SetDestroyed {caller:08X} {}", num(args, 0) != 0.0) }
}

// GECK: "set the delete flag on a disabled reference ... It will not work on
// enabled references". A created reference is deleted when its cell goes; an
// authored one is not truly deleted, but "the script on such objects is
// removed and no longer runs".
pub(super) fn mark_for_delete(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    if crate::reference_enabled(world, caller) {
        return Action::Done { value: None, effect: String::new() };
    }
    let created = world
        .try_resource_mut::<crate::CreatedReferences>()
        .is_some_and(|mut created| created.delete_when_able(caller));
    if !created {
        if let Some(mut instances) = world.query_mut::<crate::obscript_objects::ObScriptInstance>() {
            instances.remove(caller_entity);
        }
    }
    Action::Done { value: None, effect: format!("MarkForDelete {caller:08X}") }
}

// `SetOwnership [owner]`: the calling reference is owned by the NPC or faction
// named, or — "if the parameter is omitted" — by the player, whose things anyone
// may take: no owner, which is how a cleared one is read. `ClearOwnership` is the
// second spelling of that.
pub(super) fn set_ownership(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _)) = live_caller(world, context) else { return Action::Declined };
    let owner = if cmd == CMD_SET_OWNERSHIP { form_arg(args, 0).unwrap_or(0) } else { 0 };
    let Some(mut overrides) = world.try_resource_mut::<crate::OwnerOverrides>() else {
        return Action::Declined;
    };
    overrides.set(caller, owner);
    Action::Done { value: None, effect: format!("SetOwnership {caller:08X} {owner:08X}") }
}

// `SetPos <axis> <value>`: "the position of the calling object relative to the world on the
// specified axis" (GECK) — world axes are Gamebryo's Z-up, the transform is Y-up. A child
// entity's transform is local to its parent, so only a root entity is moved this way.
pub(super) fn set_pos(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let axis = num(args, 0) as usize;
    if axis > 2 || world.has::<byroredux_core::ecs::components::Parent>(entity) {
        return Action::Declined;
    }
    let Some(mut transforms) = world.query_mut::<byroredux_core::ecs::components::Transform>() else {
        return Action::Declined;
    };
    let Some(transform) = transforms.get_mut(entity) else { return Action::Declined };
    let mut zup = byroredux_core::math::coord::yup_to_zup_pos(transform.translation.to_array());
    zup[axis] = num(args, 1);
    transform.translation = byroredux_core::math::Vec3::from(byroredux_core::math::coord::zup_to_yup_pos(zup));
    Action::Done { value: None, effect: format!("SetPos {caller:08X} axis {axis} {}", num(args, 1)) }
}

// `SetAngle <axis> <degrees>` / `Rotate <axis> <degrees per second>`: the object's rotation
// is the Euler triple a placement stores (`Rx(-rx) · Rz(ry) · Ry(-rz)`, Z-up); `SetAngle`
// sets one component and keeps the other two, `Rotate` turns one by the speed over this
// frame ("a reference can only be rotated around one axis per frame", GECK). Only a root
// entity, and not the player — whose pitch and heading belong to the camera.
pub(super) fn set_angle(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let axis = num(args, 0) as usize;
    if axis > 2
        || world.has::<byroredux_core::ecs::components::Parent>(entity)
        || crate::papyrus_demo::PapyrusPlayerEntity::get(world) == Some(entity)
    {
        return Action::Declined;
    }
    let Some(mut transforms) = world.query_mut::<byroredux_core::ecs::components::Transform>() else {
        return Action::Declined;
    };
    let Some(transform) = transforms.get_mut(entity) else { return Action::Declined };
    let (rx, ry, rz) = byroredux_core::math::coord::quat_yup_to_euler_zup(transform.rotation);
    let mut euler = [rx, ry, rz];
    let value = num(args, 1).to_radians();
    euler[axis] = if cmd == CMD_SET_ANGLE { value } else { euler[axis] + value * context.elapsed };
    transform.rotation = byroredux_core::math::coord::euler_zup_to_quat_yup(euler[0], euler[1], euler[2]);
    Action::Done { value: None, effect: format!("{} {caller:08X} axis {axis} {}", if cmd == CMD_SET_ANGLE { "SetAngle" } else { "Rotate" }, num(args, 1)) }
}

// `PlayGroup <group> <flag>`: play the object's animation group (see `obscript_anim`); an
// object with no sequence of that name, or an actor's combat group, declines. Flag `0` holds
// the request until the current cycle finishes (#4128); `1`/`2` start at once.
pub(super) fn play_group(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let Some(group) = crate::obscript_anim::group_name(world, num(args, 0)) else { return Action::Declined };
    let flag = num(args, 1) as i64;
    if !crate::obscript_anim::play_group(world, entity, group, flag) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("PlayGroup {caller:08X} {group} flag {flag}") }
}

// `SetScale <factor>`: "sets the scale of the calling object ... 1.0 being normal size".
pub(super) fn set_scale(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let Some(mut transforms) = world.query_mut::<byroredux_core::ecs::components::Transform>() else {
        return Action::Declined;
    };
    let Some(transform) = transforms.get_mut(entity) else { return Action::Declined };
    transform.scale = num(args, 0).max(0.0);
    Action::Done { value: None, effect: format!("SetScale {caller:08X} {}", num(args, 0)) }
}
