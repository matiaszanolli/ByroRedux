//! Scene / activation primitives (#5801): scene start/stop, `Activate`,
//! `SetOpen`. Applied by `fragment/effects/scene.rs`.

use super::*;

pub(super) fn prim_start_scene(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Start")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StartScene {
        scene: receiver_object(object, scope)?,
    })
}

pub(super) fn prim_stop_scene(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Stop")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::StopScene {
        scene: receiver_object(object, scope)?,
    })
}

pub(super) fn prim_activate(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "Activate")?;
    if args.len() > 2 {
        return None;
    }
    // ObjectReference.Activate's optional first arg is the activator. The
    // MQ101 fragment corpus uses either the default or Game.GetPlayer();
    // decline other runtime expressions rather than invent their identity.
    let activator = match args.first().map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        // (The VM binds the player it holds as a player local.)
        Some(expression) if player_expr_ref(expression, scope) => None,
        Some(expression) => Some(receiver_object(expression, scope)?),
    };
    // `abDefaultProcessingOnly=true`: "no OnActivate event will be sent to any
    // scripts and the object will ignore the blocked flag" (CK).
    let native_only = bool_arg(args, 1)? == Some(true);
    Some(Effect::Activate {
        target: receiver_object(object, scope)?,
        activator,
        native_only,
    })
}

pub(super) fn prim_set_open(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetOpen")?;
    if args.len() > 1 {
        return None;
    }
    let open = bool_arg(args, 0)?.unwrap_or(true);
    Some(Effect::SetOpen {
        target: receiver_object(object, scope)?,
        open,
    })
}
