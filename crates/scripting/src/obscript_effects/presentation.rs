//! Presentation handlers: dialogue, messages, sounds, idles, image-space modifiers, magic shaders, casting.

use super::*;

// `PlayIdle <idle>`: the calling actor plays that IDLE record's animation — the request the
// Papyrus `PlayIdle` writes (`ActorCinematicState::request_idle`), which the binary's idle
// runtime resolves to a clip and plays.
pub(super) fn play_idle(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _, mut bind, me)) = open_actor_effects(world, context) else { return Action::Declined };
    let effects: Vec<Effect> = {
        let Some(idle) = form_arg(args, 0) else { return Action::Declined };
        vec![Effect::PlayIdle { actor: me, idle: bind.object(idle) }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `AddNote <note>` / `RemoveNote <note>`: the player's note list (see `obscript_state`).
pub(super) fn add_note(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(note) = form_arg(args, 0) else { return Action::Declined };
    let Some(mut notes) = world.try_resource_mut::<crate::obscript_state::PipBoyNotes>() else { return Action::Declined };
    if cmd == CMD_ADD_NOTE {
        notes.add(note);
    } else {
        notes.remove(note);
    }
    Action::Done { value: None, effect: format!("{} {note:08X}", if cmd == CMD_ADD_NOTE { "AddNote" } else { "RemoveNote" }) }
}

// `Cast <spell> [target]` (Oblivion; "Cast WellRestedSpell Player") / `CastImmediateOnSelf <spell>`
// (Fallout): the calling reference casts the spell — on the target, or on itself. Every modelled
// effect of the spell lands at once (see `magic_effects`); a spell with none is declined.
pub(super) fn cast(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let caster = caller_entity;
    let Some(spell) = form_arg(args, 0) else { return Action::Declined };
    let target = if cmd == CMD_CAST { form_arg(args, 1).and_then(|form| live(world, form)).unwrap_or(caster) } else { caster };
    if !crate::magic_effects::cast_spell(world, Some(caster), target, spell) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("Cast {spell:08X} by {caller:08X}") }
}

// `PlayMagicShaderVisuals <shader> [seconds]` / `PlayMagicEffectVisuals <effect> [seconds]` /
// `StopMagicShaderVisuals <shader>`: an effect shader on the calling reference (see
// `magic_shaders`: the fill layer's glow is what is drawn). A zero or absent duration plays until
// stopped.
pub(super) fn play_magic_shader(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let Some(named) = form_arg(args, 0) else { return Action::Declined };
    let shader = if cmd == CMD_PLAY_MAGIC_EFFECT {
        match world.try_resource::<crate::magic_shaders::ShaderCatalog>().and_then(|catalog| catalog.of_effect(named)) {
            Some(shader) => shader,
            None => return Action::Declined,
        }
    } else {
        named
    };
    let seconds = num(args, 1);
    let duration = (seconds > 0.0).then_some(seconds);
    if !crate::magic_shaders::play_shader(world, entity, shader, duration) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("PlayMagicShader {shader:08X} on {caller:08X}") }
}

// `StopMagicEffectVisuals <effect>` (Oblivion's `SME`): the shader the effect plays, stopped.
pub(super) fn stop_magic_shader(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let entity = caller_entity;
    let Some(named) = form_arg(args, 0) else { return Action::Declined };
    let shader = if cmd == CMD_STOP_MAGIC_EFFECT {
        match world.try_resource::<crate::magic_shaders::ShaderCatalog>().and_then(|catalog| catalog.of_effect(named)) {
            Some(shader) => shader,
            None => return Action::Declined,
        }
    } else {
        named
    };
    crate::magic_shaders::stop_shader(world, entity, shader);
    Action::Done { value: None, effect: format!("StopMagicShader {shader:08X} on {caller:08X}") }
}

// `Look <target>` / `StopLook`: forced head-tracking (see `obscript_anim::HeadLook`).
pub(super) fn look(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let actor = caller_entity;
    let Some(target) = form_arg(args, 0).and_then(|form| live(world, form)) else { return Action::Declined };
    if !crate::obscript_anim::start_look(world, actor, target) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("Look {caller:08X}") }
}

pub(super) fn stop_look(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let actor = caller_entity;
    crate::obscript_anim::stop_look(world, actor);
    Action::Done { value: None, effect: format!("StopLook {caller:08X}") }
}

// `TriggerHitShader <strength>`: the screen blur of a hit (see `cinematic::trigger_hit_shader`).
pub(super) fn trigger_hit_shader(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    if !crate::cinematic::trigger_hit_shader(world, num(args, 0)) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: format!("TriggerHitShader {}", num(args, 0)) }
}

// `ShowWarning "text"`: the VM has already shown and logged the text (it is the command's
// string argument); nothing is left to do.
pub(super) fn show_warning(_world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], _context: &Context) -> Action {
    Action::Done { value: None, effect: "ShowWarning".into() }
}

// `Dispel <spell>`: the spell's effects on the calling reference end.
pub(super) fn dispel(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let target = caller_entity;
    let Some(spell) = form_arg(args, 0) else { return Action::Declined };
    crate::magic_effects::dispel(world, target, Some(spell));
    Action::Done { value: None, effect: format!("Dispel {spell:08X} on {caller:08X}") }
}

// `Reset3DState`: the reference's animation starts over (see `obscript_anim::reset_animation`).
pub(super) fn reset_3d_state(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let name = |what: &str| format!("{what} {caller:08X}");
    let entity = caller_entity;
    if !crate::obscript_anim::reset_animation(world, entity) {
        return Action::Declined;
    }
    Action::Done { value: None, effect: name("Reset3DState") }
}

// `PickIdle`: the calling actor picks a new idle — the request the app's idle runtime answers.
pub(super) fn pick_idle(world: &World, _fallout: bool, _cmd: u16, _args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let name = |what: &str| format!("{what} {caller:08X}");
    let entity = caller_entity;
    let Some(mut requests) = world.query_mut::<crate::PickIdleRequest>() else { return Action::Declined };
    requests.insert(entity, crate::PickIdleRequest);
    Action::Done { value: None, effect: name("PickIdle") }
}

// `Say <topic> [force subtitle] [speaker] [position flag]` and `SayTo <target> <topic>
// [force subtitle] [no target look]`: the calling actor speaks one line of the topic,
// and the call answers how many seconds it takes (CS: "useful for timing followups").
// A topic with nothing eligible to say answers 0 and speaks nothing.
pub(super) fn say(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let speaker = caller_entity;
    let (target, topic) = if cmd == CMD_SAY {
        (None, form_arg(args, 0))
    } else {
        match form_arg(args, 0).map(|form| live(world, form)) {
            Some(Some(target)) => (Some(target), form_arg(args, 1)),
            _ => return Action::Declined,
        }
    };
    let Some(topic) = topic else { return Action::Declined };
    let seconds = crate::obscript_speech::say(world, speaker, topic, target).unwrap_or(0.0);
    Action::Done {
        value: Some(ObScriptValue::Num(seconds)),
        effect: format!("{} {caller:08X} topic {topic:08X}", if cmd == CMD_SAY { "Say" } else { "SayTo" }),
    }
}

// `ShowMessage <message> [value ...]`: the message record's notification or box, its
// `%` spots filled from the values (the Papyrus `Message.Show` effect).
pub(super) fn show_message(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let caller = caller_of(context);
    let mut bind = Binder::default();
    let effects: Vec<Effect> = {
        let Some(message) = form_arg(args, 0) else { return Action::Declined };
        let values: Vec<f32> = args.iter().skip(1).map(ObScriptValue::as_num).collect();
        vec![Effect::ShowMessage { message: bind.object(message), args: values }]
    };
    finish_effects(world, cmd, context, caller, &bind, effects)
}

// `StartConversation <target> [topic]` ("force a conversation ... starting with a specified
// topic", and "set the player as the TargetActor" to force-greet): the actor walks up to the
// player and opens the topic, through the force-greet the dialogue runtime already has. A
// conversation between two NPCs has no runtime, so another target is declined.
pub(super) fn start_conversation(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, caller_entity)) = live_caller(world, context) else { return Action::Declined };
    let speaker = caller_entity;
    if form_arg(args, 0) != Some(crate::PLAYER_REFERENCE) {
        return Action::Declined;
    }
    let Some(mut lines) = world.try_resource_mut::<crate::obscript_speech::PendingScriptedLines>() else {
        return Action::Declined;
    };
    lines.conversations.push((speaker, form_arg(args, 1)));
    Action::Done { value: None, effect: format!("StartConversation {caller:08X}") }
}

// `PlaySound3D <sound>`: "plays the specified sound, at the location of the calling reference".
pub(super) fn play_sound_3d(world: &World, _fallout: bool, _cmd: u16, args: &[ObScriptValue], context: &Context) -> Action {
    let Some((caller, _)) = live_caller(world, context) else { return Action::Declined };
    let Some(sound) = form_arg(args, 0) else { return Action::Declined };
    let Some(mut pending) = world.try_resource_mut::<crate::PendingSoundPlays>() else {
        return Action::Declined;
    };
    pending.0.push(crate::SoundPlay { sound, source: caller });
    Action::Done { value: None, effect: format!("PlaySound3D {sound:08X} at {caller:08X}") }
}

// `ApplyImageSpaceModifier <IMAD>` / `RemoveImageSpaceModifier <IMAD>`: play or take off a
// screen effect; the modifier runtime the cinematic presentation owns samples it into the
// frame's grade. An `IMAD` the load order lacks does nothing and is declined.
pub(super) fn imod(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], _context: &Context) -> Action {
    let Some(imad) = form_arg(args, 0) else { return Action::Declined };
    let Some(mut state) = world.try_resource_mut::<crate::cinematic::CinematicPresentationState>() else {
        return Action::Declined;
    };
    if cmd == CMD_IMOD {
        if !state.play_image_space_modifier(imad) {
            return Action::Declined;
        }
    } else {
        state.remove_image_space_modifier(imad);
    }
    Action::Done { value: None, effect: format!("{} {imad:08X}", if cmd == CMD_IMOD { "ApplyImageSpaceModifier" } else { "RemoveImageSpaceModifier" }) }
}
