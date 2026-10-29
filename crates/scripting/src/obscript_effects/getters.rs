//! The ObScript getters (#6139): commands that answer with a value (a number or a reference)
//! instead of an effect.

use super::*;

pub(super) const CMD_GET_ITEM_COUNT: u16 = 0x102f;
pub(super) const CMD_IS_IN_COMBAT: u16 = 0x1121;
pub(super) const CMD_GET_IS_REFERENCE: u16 = 0x1088;
pub(super) const CMD_GET_PLAYER_TEAMMATE: u16 = 0x11c6;
pub(super) const CMD_GET_LOCKED: u16 = 0x1005;
pub(super) const CMD_GET_LOCK_LEVEL: u16 = 0x1041;
pub(super) const CMD_GET_AV: u16 = 0x100e;
pub(super) const CMD_GET_BASE_AV: u16 = 0x1115;
pub(super) const CMD_GET_UNCONSCIOUS: u16 = 0x10f2;
pub(super) const CMD_GET_COMBAT_TARGET: u16 = 0x10e8;
pub(super) const CMD_GET_LINKED_REF: u16 = 0x116b;
pub(super) const CMD_GET_PARENT_REF: u16 = 0x1113;
pub(super) const CMD_GET_DESTRUCTION_STAGE: u16 = 0x11d7;
pub(super) const CMD_GET_DESTROYED: u16 = 0x10cb;
pub(super) const CMD_GET_RESTRAINED: u16 = 0x10f4;
pub(super) const CMD_GET_IN_SAME_CELL: u16 = 0x1020;
pub(super) const CMD_GET_IS_CURRENT_PACKAGE: u16 = 0x10a1;
pub(super) const CMD_GET_HEALTH_PERCENTAGE: u16 = 0x11af;
pub(super) const CMD_GET_CURRENT_TIME: u16 = 0x1012;
pub(super) const CMD_GET_POS: u16 = 0x1006;
pub(super) const CMD_IS_IMAGE_SPACE_ACTIVE: u16 = 0x1232;
pub(super) const CMD_GET_ANGLE: u16 = 0x1008;
pub(super) const CMD_GET_STARTING_ANGLE: u16 = 0x100b;
pub(super) const CMD_GET_STARTING_POS: u16 = 0x100a;
pub(super) const CMD_GET_SCALE: u16 = 0x1018;
pub(super) const CMD_GET_HAS_NOTE: u16 = 0x117e;
pub(super) const CMD_GET_DEAD_COUNT: u16 = 0x1054;
pub(super) const CMD_GET_GS: u16 = 0x1100;
pub(super) const CMD_GET_DETECTION_LEVEL: u16 = 0x10b4;
pub(super) const CMD_IS_SNEAKING: u16 = 0x111e;
pub(super) const CMD_IS_SWIMMING: u16 = 0x10b9;
pub(super) const CMD_IS_KILLER: u16 = 0x1199;
pub(super) const CMD_IS_WEAPON_OUT: u16 = 0x1065;
pub(super) const CMD_GET_IGNORE_CRIME: u16 = 0x10c0;
pub(super) const CMD_GET_DETECTED: u16 = 0x102d;
pub(super) const CMD_GET_IS_CURRENT_WEATHER: u16 = 0x1095;
pub(super) const CMD_HAS_MAGIC_EFFECT: u16 = 0x10d6;
pub(super) const CMD_SCRIPT_EFFECT_ELAPSED_SECONDS: u16 = 0x10ba;
pub(super) const CMD_GET_SITTING: u16 = 0x109f;
pub(super) const CMD_IS_IN_INTERIOR: u16 = 0x112c;
pub(super) const CMD_GET_OPEN_STATE: u16 = 0x109d;
pub(super) const CMD_GET_IN_WORLDSPACE: u16 = 0x1136;
pub(super) const CMD_IS_ANIM_PLAYING: u16 = 0x1128;

// CS *GetCurrentTime*: "the current game time, in decimal format, e.g. 4:30 am = 4.5,
// 7:45 pm = 19.75" — the hour of the game clock the update system reports each frame.
// No reference is involved.
pub(super) fn get_current_time(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _context: &Context) -> Action {
    world
        .try_resource::<crate::fragment::ScriptUpdates>()
        .and_then(|updates| updates.game_hours())
        .map_or(Action::Declined, |hours| Action::number(hours.rem_euclid(24.0) as f32))
}

// CS/GECK *IsImageSpaceActive*: whether the modifier is playing. Fallout only; no reference.
pub(super) fn is_image_space_active(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let (true, Some(imad)) = (crate::ScriptGame::is_fallout3_nv(world), form_arg(args, 0)) else {
        return Action::Declined;
    };
    world
        .try_resource::<crate::cinematic::CinematicPresentationState>()
        .map_or(Action::Declined, |state| Action::number(f32::from(u8::from(state.is_image_space_modifier_active(imad)))))
}

// GECK *GetGS*: the game setting's value; the VM has already looked the name up, an unknown one is
// the null argument. No reference is involved.
pub(super) fn get_gs(_world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    match args.first() {
        Some(ObScriptValue::Num(value)) => Action::number(*value),
        _ => Action::Declined,
    }
}

// GECK *ScriptEffectElapsedSeconds*: the seconds the running effect script has been active.
pub(super) fn script_effect_elapsed_seconds(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _context: &Context) -> Action {
    world
        .try_resource::<crate::magic_effects::ScriptEffectRun>()
        .and_then(|run| run.elapsed)
        .map_or(Action::Declined, Action::number)
}

// GECK *GetIsCurrentWeather*: 1 when the sky runs that weather. No reference is involved.
pub(super) fn get_is_current_weather(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(weather) = form_arg(args, 0) else { return Action::Declined };
    world
        .try_resource::<crate::weather_state::CurrentWeather>()
        .and_then(|current| current.0)
        .map_or(Action::Declined, |current| Action::number(f32::from(u8::from(current == weather))))
}

// No reference is involved in these two: the player's notes, and a count by actor base.
pub(super) fn get_has_note(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(note) = form_arg(args, 0) else { return Action::Declined };
    world
        .try_resource::<crate::obscript_state::PipBoyNotes>()
        .map_or(Action::Declined, |notes| Action::number(f32::from(u8::from(notes.has(note)))))
}

pub(super) fn get_dead_count(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(base) = form_arg(args, 0) else { return Action::Declined };
    world
        .try_resource::<crate::obscript_state::KillRecords>()
        .map_or(Action::Declined, |kills| Action::number(kills.dead_count(base) as f32))
}

// The count the CTDA `GetItemCount` reads (#6251); a form list sums its members.
pub(super) fn get_item_count(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    match form_arg(args, 0) {
        Some(item) => Action::number(crate::condition::item_count(world, subject, item)),
        None => Action::Declined,
    }
}

pub(super) fn is_in_combat(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    let owner = QuestFormId(context.owner);
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    let guard = |guard: crate::translate::effects::FragmentGuard, bind: Binder| {
        crate::fragment::evaluate_guard_guard_free(world, &guard, owner, Some(&bind.vmad()))
            .map_or(Action::Declined, |holds| Action::number(f32::from(u8::from(holds))))
    };
    guard(crate::translate::effects::FragmentGuard::ActorInCombat { actor: me, expected: true }, bind)
}

pub(super) fn get_is_reference(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    let owner = QuestFormId(context.owner);
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    let guard = |guard: crate::translate::effects::FragmentGuard, bind: Binder| {
        crate::fragment::evaluate_guard_guard_free(world, &guard, owner, Some(&bind.vmad()))
            .map_or(Action::Declined, |holds| Action::number(f32::from(u8::from(holds))))
    };
    let Some(other) = form_arg(args, 0) else { return Action::Declined };
    let right = Some(bind.actor(other));
    guard(crate::translate::effects::FragmentGuard::SameReference { left: me, right, expected: true }, bind)
}

pub(super) fn get_player_teammate(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    let owner = QuestFormId(context.owner);
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    let guard = |guard: crate::translate::effects::FragmentGuard, bind: Binder| {
        crate::fragment::evaluate_guard_guard_free(world, &guard, owner, Some(&bind.vmad()))
            .map_or(Action::Declined, |holds| Action::number(f32::from(u8::from(holds))))
    };
    guard(
        crate::translate::effects::FragmentGuard::ActorFollower { actor: me, flag: crate::FollowerFlag::Teammate, expected: true },
        bind,
    )
}

pub(super) fn get_locked(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(f32::from(u8::from(world.get::<byroredux_core::ecs::components::Locked>(subject).is_some())))
}

// `GetLockLevel`: an unlocked reference has no lock, so no level.
pub(super) fn get_lock_level(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(world.get::<byroredux_core::ecs::components::Locked>(subject).map_or(0.0, |lock| f32::from(lock.lock_level)))
}

pub(super) fn get_av(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    let owner = QuestFormId(context.owner);
    let mut bind = Binder::default();
    let me = bind.actor(caller);
    let value = |value: ValueExpr, bind: Binder| {
        crate::fragment::evaluate_value_guard_free(world, &value, owner, Some(&bind.vmad())).map_or(Action::Declined, Action::number)
    };
    let Some(avif) = crate::obscript_actor_values::actor_value_form(world, num(args, 0)) else {
        return Action::Declined;
    };
    let actor_value = bind.object(avif);
    let expr = if cmd == CMD_GET_AV {
        ValueExpr::ActorValue(me, actor_value)
    } else {
        ValueExpr::ActorValueReading(crate::translate::effects::ActorValueReading::Base, me, ActorValueArg::Form(actor_value))
    };
    value(expr, bind)
}

pub(super) fn get_unconscious(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(f32::from(u8::from(world.get::<crate::ActorControlState>(subject).is_some_and(|state| state.unconscious))))
}

pub(super) fn get_combat_target(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    match world.get::<crate::AiCombatState>(subject).map(|state| state.target) {
        Some(target) => match crate::condition::entity_global_form_id(world, target) {
            Some(form) => Action::reference(form),
            None => Action::Declined,
        },
        None => Action::reference(0),
    }
}

// `GetLinkedRef`: the reference's linked-ref (`XLKR`) target. `0x116b` is
// `PurgeCellBuffers` in Oblivion, so it is only this on the Fallout pair.
pub(super) fn get_linked_ref(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    Action::reference(crate::linked_ref(world, caller, 0).unwrap_or(0))
}

// CS *GetParentRef*: "the calling object's parent reference (0 if no
// parent)" — the reference in its Enable Parent tab (`XESP`).
pub(super) fn get_parent_ref(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    Action::reference(
        world
            .try_resource::<crate::AuthoredDisabledReferences>()
            .and_then(|authored| authored.enable_parents.get(&caller).map(|parent| parent.form_id))
            .unwrap_or(0),
    )
}

// "Returns 1 if the actor is currently running the specified package": the package
// the AI last reported for the actor.
pub(super) fn get_is_current_package(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    let Some(package) = form_arg(args, 0) else { return Action::Declined };
    let current = world.try_resource::<crate::fragment::ActivePackages>().and_then(|active| active.0.get(&subject).copied());
    Action::number(f32::from(u8::from(current == Some(package))))
}

// GECK *GetHealthPercentage*: "how injured an actor is ... a value from 0 to 1.0" — the
// current Health over the undamaged ceiling. Fallout only: `0x11af` is another command in Oblivion.
// Shared with the CTDA of the same index (#6251).
pub(super) fn get_health_percentage(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    crate::condition::health_percentage(world, subject).map_or(Action::Declined, Action::number)
}

// GECK *GetPos*: "the position of the calling object relative to the world on the specified
// axis" (world axes are Z-up; the transform is Y-up). *GetStartingPos*: where the plugin
// placed it.
pub(super) fn get_pos(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    let axis = num(args, 0) as usize;
    let translation = if cmd == CMD_GET_POS {
        world.get::<byroredux_core::ecs::components::GlobalTransform>(subject).map(|t| t.translation)
    } else {
        world.get::<crate::EditorLocation>(subject).map(|placed| placed.translation)
    };
    match translation {
        Some(translation) if axis <= 2 => Action::number(byroredux_core::math::coord::yup_to_zup_pos(translation.to_array())[axis]),
        _ => Action::Declined,
    }
}

// GECK *GetAngle* / *GetStartingAngle*, in degrees. The player's pitch and heading are the
// camera's, which is not on its body: only a non-player reference answers.
pub(super) fn get_angle(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    let axis = num(args, 0) as usize;
    let rotation = if cmd == CMD_GET_ANGLE {
        world.get::<byroredux_core::ecs::components::GlobalTransform>(subject).map(|t| t.rotation)
    } else {
        world.get::<crate::EditorLocation>(subject).map(|placed| placed.rotation)
    };
    match rotation {
        Some(rotation) if axis <= 2 && crate::papyrus_demo::PapyrusPlayerEntity::get(world) != Some(subject) => {
            let (rx, ry, rz) = byroredux_core::math::coord::quat_yup_to_euler_zup(rotation);
            Action::number(script_angle_degrees(axis, [rx, ry, rz][axis]))
        }
        _ => Action::Declined,
    }
}

// GECK *IsInInterior*: 1 in an interior cell. *GetInWorldspace <worldspace>*: 1 in one of that
// worldspace's exterior cells, "always 0 if the actor is in an interior cell".
pub(super) fn is_in_interior(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    match crate::cell_kind::cell_kind_of(world, subject) {
        Some(kind) => Action::number(f32::from(u8::from(kind == crate::cell_kind::CellKind::Interior))),
        None => Action::Declined,
    }
}

pub(super) fn get_in_worldspace(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    match form_arg(args, 0).and_then(|asked| crate::cell_kind::in_worldspace(world, subject, asked)) {
        Some(inside) => Action::number(f32::from(u8::from(inside))),
        None => Action::Declined,
    }
}

// GECK *HasMagicEffect*: 1 when an effect of that magic effect is running on the calling actor.
pub(super) fn has_magic_effect(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    match form_arg(args, 0) {
        Some(mgef) => Action::number(f32::from(u8::from(crate::magic_effects::has_effect(world, subject, mgef)))),
        None => Action::Declined,
    }
}

// GECK *GetDetectionLevel*: 0 Lost, 1 Unseen, 2 Noticed, 3 Seen. The engine's sight test is yes or
// no — an actor either sees the target or does not — so the level is 3 or 0 (the GECK notes
// the original too answers only 0 or 3 for every target but the player).
pub(super) fn get_detection_level(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    let Some(target) = form_arg(args, 0).and_then(|form| live(world, form)) else { return Action::Declined };
    match world.try_resource::<crate::SightQuery>() {
        Some(sight) => Action::number(if (sight.0)(world, subject, target) { 3.0 } else { 0.0 }),
        None => Action::Declined,
    }
}

// GECK *IsSneaking*: "1 if the actor is sneaking (crouched)". The engine has no sneak stance —
// no actor, the player included, can crouch — so nobody is.
pub(super) fn is_sneaking(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    Action::number(0.0)
}

// GECK *IsSwimming*: the walk/swim verdict of the character system, through the installed query.
pub(super) fn is_swimming(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    match world.try_resource::<crate::SwimQuery>() {
        Some(swim) => Action::number(f32::from(u8::from((swim.0)(world, subject)))),
        None => Action::Declined,
    }
}

// GECK *GetDetected*: 1 when the calling actor currently detects the target — the sight test the
// engine's own hostility uses (range from the detection GMSTs, line of sight through physics),
// asked through the query the binary installs.
pub(super) fn get_detected(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    let Some(target) = form_arg(args, 0).and_then(|form| live(world, form)) else { return Action::Declined };
    match world.try_resource::<crate::SightQuery>() {
        Some(sight) => Action::number(f32::from(u8::from((sight.0)(world, subject, target)))),
        None => Action::Declined,
    }
}

// GECK *IsKiller*: 1 when the calling (dead) actor was killed by the named actor.
pub(super) fn is_killer(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    match form_arg(args, 0) {
        Some(killer) => Action::number(f32::from(u8::from(
            world.try_resource::<crate::obscript_state::KillRecords>().is_some_and(|kills| kills.killer_of(caller) == Some(killer)),
        ))),
        None => Action::Declined,
    }
}

pub(super) fn is_weapon_out(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(f32::from(u8::from(crate::obscript_state::is_weapon_out(world, subject))))
}

pub(super) fn get_ignore_crime(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(f32::from(u8::from(world.has::<crate::obscript_state::IgnoreCrime>(subject))))
}

// GECK *GetOpenState*: 0 none (no open/closed states), 1 open, 2 opening, 3 closed, 4 closing —
// read from the two-state activator, the only open/closed state the runtime keeps; a reference
// without one has none.
pub(super) fn get_open_state(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(match world.get::<crate::vm_state::TwoStateActivator>(subject) {
        None => 0.0,
        Some(state) => match (state.is_open, state.is_animating) {
            (true, false) => 1.0,
            (true, true) => 2.0,
            (false, false) => 3.0,
            (false, true) => 4.0,
        },
    })
}

// GECK *GetSitting*: 0 not sitting, 3 is sitting (the "getting ready to sit / stand" states 2
// and 4 have no runtime here: an actor is either seated on its furniture or not).
pub(super) fn get_sitting(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(if world.has::<byroredux_core::ecs::components::Seated>(subject) { 3.0 } else { 0.0 })
}

pub(super) fn is_anim_playing(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    let group = match args.first() {
        None => None,
        Some(_) => match crate::obscript_anim::group_name(world, num(args, 0)) {
            Some(name) => Some(name),
            None => return Action::Declined,
        },
    };
    crate::obscript_anim::is_anim_playing(world, subject, group)
        .map_or(Action::Declined, |playing| Action::number(f32::from(u8::from(playing))))
}

pub(super) fn get_scale(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    world
        .get::<byroredux_core::ecs::components::Transform>(subject)
        .map_or(Action::Declined, |transform| Action::number(transform.scale))
}

// GECK *GetDestructionStage*: the object's current Damage Stage. An object with no destruction
// data has none to answer.
pub(super) fn get_destruction_stage(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    if crate::destruction::is_destructible(world, subject) {
        // An undamaged destructible is at stage 0 (the engine's -1 is "no damage yet").
        Action::number(crate::destruction::destruction_stage(world, subject, caller).max(0) as f32)
    } else {
        Action::Declined
    }
}

pub(super) fn get_destroyed(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    if live(world, caller).is_none() {
        return Action::Declined;
    }
    Action::number(f32::from(u8::from(crate::destruction::is_destroyed(world, caller))))
}

pub(super) fn get_restrained(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    Action::number(f32::from(u8::from(world.get::<crate::ActorControlState>(subject).is_some_and(|state| state.restrained))))
}

// "Returns true if the calling reference is in the same cell as the target
// actor": two loaded references with the same, known cell.
pub(super) fn get_in_same_cell(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let Some(subject) = live(world, caller) else { return Action::Declined };
    use byroredux_core::ecs::components::{CellFormId, CellRoot};
    let Some(other) = form_arg(args, 0).and_then(|form| live(world, form)) else {
        return Action::Declined;
    };
    let cell_of = |entity: EntityId| {
        let root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
        world.get::<CellFormId>(root).map(|cell| cell.0)
    };
    match (cell_of(subject), cell_of(other)) {
        (Some(a), Some(b)) => Action::number(f32::from(u8::from(a == b))),
        _ => Action::Declined,
    }
}

/// The getters (#6139): they answer with a value (a number or a reference) instead of an effect. The
/// rows that read a calling reference are declined without a live one.
pub(crate) const GETTER_COMMANDS: &[Command] = &[
    Command { id: CMD_GET_CURRENT_TIME, games: Games::Any, needs_caller: false, handler: get_current_time },
    Command { id: CMD_IS_IMAGE_SPACE_ACTIVE, games: Games::Any, needs_caller: false, handler: is_image_space_active },
    Command { id: CMD_GET_GS, games: Games::Any, needs_caller: false, handler: get_gs },
    Command { id: CMD_SCRIPT_EFFECT_ELAPSED_SECONDS, games: Games::Any, needs_caller: false, handler: script_effect_elapsed_seconds },
    Command { id: CMD_GET_IS_CURRENT_WEATHER, games: Games::Any, needs_caller: false, handler: get_is_current_weather },
    Command { id: CMD_GET_HAS_NOTE, games: Games::Any, needs_caller: false, handler: get_has_note },
    Command { id: CMD_GET_DEAD_COUNT, games: Games::Any, needs_caller: false, handler: get_dead_count },
    Command { id: CMD_GET_ITEM_COUNT, games: Games::Any, needs_caller: true, handler: get_item_count },
    Command { id: CMD_IS_IN_COMBAT, games: Games::Any, needs_caller: true, handler: is_in_combat },
    Command { id: CMD_GET_IS_REFERENCE, games: Games::Any, needs_caller: true, handler: get_is_reference },
    Command { id: CMD_GET_PLAYER_TEAMMATE, games: Games::Any, needs_caller: true, handler: get_player_teammate },
    Command { id: CMD_GET_LOCKED, games: Games::Any, needs_caller: true, handler: get_locked },
    Command { id: CMD_GET_LOCK_LEVEL, games: Games::Any, needs_caller: true, handler: get_lock_level },
    Command { id: CMD_GET_AV, games: Games::Any, needs_caller: true, handler: get_av },
    Command { id: CMD_GET_BASE_AV, games: Games::Any, needs_caller: true, handler: get_av },
    Command { id: CMD_GET_UNCONSCIOUS, games: Games::Any, needs_caller: true, handler: get_unconscious },
    Command { id: CMD_GET_COMBAT_TARGET, games: Games::Any, needs_caller: true, handler: get_combat_target },
    Command { id: CMD_GET_LINKED_REF, games: Games::Fallout, needs_caller: true, handler: get_linked_ref },
    Command { id: CMD_GET_PARENT_REF, games: Games::Any, needs_caller: true, handler: get_parent_ref },
    Command { id: CMD_GET_IS_CURRENT_PACKAGE, games: Games::Any, needs_caller: true, handler: get_is_current_package },
    Command { id: CMD_GET_HEALTH_PERCENTAGE, games: Games::Fallout, needs_caller: true, handler: get_health_percentage },
    Command { id: CMD_GET_POS, games: Games::Any, needs_caller: true, handler: get_pos },
    Command { id: CMD_GET_STARTING_POS, games: Games::Any, needs_caller: true, handler: get_pos },
    Command { id: CMD_GET_ANGLE, games: Games::Any, needs_caller: true, handler: get_angle },
    Command { id: CMD_GET_STARTING_ANGLE, games: Games::Any, needs_caller: true, handler: get_angle },
    Command { id: CMD_IS_IN_INTERIOR, games: Games::Any, needs_caller: true, handler: is_in_interior },
    Command { id: CMD_GET_IN_WORLDSPACE, games: Games::Any, needs_caller: true, handler: get_in_worldspace },
    Command { id: CMD_HAS_MAGIC_EFFECT, games: Games::Any, needs_caller: true, handler: has_magic_effect },
    Command { id: CMD_GET_DETECTION_LEVEL, games: Games::Any, needs_caller: true, handler: get_detection_level },
    Command { id: CMD_IS_SNEAKING, games: Games::Any, needs_caller: true, handler: is_sneaking },
    Command { id: CMD_IS_SWIMMING, games: Games::Any, needs_caller: true, handler: is_swimming },
    Command { id: CMD_GET_DETECTED, games: Games::Any, needs_caller: true, handler: get_detected },
    Command { id: CMD_IS_KILLER, games: Games::Any, needs_caller: true, handler: is_killer },
    Command { id: CMD_IS_WEAPON_OUT, games: Games::Any, needs_caller: true, handler: is_weapon_out },
    Command { id: CMD_GET_IGNORE_CRIME, games: Games::Fallout, needs_caller: true, handler: get_ignore_crime },
    Command { id: CMD_GET_OPEN_STATE, games: Games::Any, needs_caller: true, handler: get_open_state },
    Command { id: CMD_GET_SITTING, games: Games::Any, needs_caller: true, handler: get_sitting },
    Command { id: CMD_IS_ANIM_PLAYING, games: Games::Any, needs_caller: true, handler: is_anim_playing },
    Command { id: CMD_GET_SCALE, games: Games::Any, needs_caller: true, handler: get_scale },
    Command { id: CMD_GET_DESTRUCTION_STAGE, games: Games::Any, needs_caller: true, handler: get_destruction_stage },
    Command { id: CMD_GET_DESTROYED, games: Games::Any, needs_caller: true, handler: get_destroyed },
    Command { id: CMD_GET_RESTRAINED, games: Games::Any, needs_caller: true, handler: get_restrained },
    Command { id: CMD_GET_IN_SAME_CELL, games: Games::Any, needs_caller: true, handler: get_in_same_cell },
];
