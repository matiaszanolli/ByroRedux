//! Vehicle / cinematic primitives (#5801): idles, vehicles, horse tether,
//! motion type and cart exit. Applied by `fragment/effects/vehicle_cinematic.rs`.

use super::*;

pub(super) fn prim_play_idle(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "PlayIdle")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::PlayIdle {
        actor: receiver_actor(object, scope)?,
        idle: receiver_object(&args[0].value.node, scope)?,
    })
}

pub(super) fn prim_set_vehicle(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetVehicle")?;
    if args.len() != 1 {
        return None;
    }
    let vehicle = match &args[0].value.node {
        Expr::NoneLit => None,
        value => Some(receiver_object(value, scope)?),
    };
    Some(Effect::SetVehicle {
        actor: receiver_actor(object, scope)?,
        vehicle,
    })
}

pub(super) fn prim_tether_to_horse(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "TetherToHorse")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::TetherToHorse {
        cart: receiver_object(object, scope)?,
        horse: receiver_object(&args[0].value.node, scope)?,
    })
}

pub(super) fn prim_set_motion_type(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetMotionType")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let target = receiver_object(object, scope)?;
    let motion_type = motion_type_arg(&args[0].value.node, &target, scope)?;
    let allow_activate = bool_arg(args, 1)?.unwrap_or(true);
    Some(Effect::SetMotionType {
        target,
        motion_type,
        allow_activate,
    })
}

fn motion_type_arg(value: &Expr, target: &ObjectRef, scope: &Scope) -> Option<MotionType> {
    if let Expr::Cast { expr, .. } = value {
        return motion_type_arg(&expr.node, target, scope);
    }
    if let Expr::IntLit(raw) = value {
        // `ObjectReference.SetMotionType`'s argument is NOT the NIF
        // `hkMotionType` enum (`crates/nif`'s collision importer: Keyframed
        // 6, Static 7, Character 9) that #2286 copied here. The Papyrus
        // constants differ per game — CK wiki, `SetMotionType -
        // ObjectReference`: Skyrim Dynamic 1, SphereInertia 2, BoxInertia 3,
        // Keyframed 4, Fixed 5, ThinBoxInertia 6, Character 7; Fallout 4
        // Fixed 0, Dynamic 1, Keyframed 2 — and this lowerer does not know
        // the game. Only `1` means the same in both (Dynamic); every other
        // literal declines instead of guessing (#5633). Vanilla content is
        // unaffected: MQ101's fragment keeps the named `Motion_Keyframed`
        // property read (the arm below), it is not folded to a literal.
        return match *raw {
            1 => Some(MotionType::Dynamic),
            _ => None,
        };
    }
    let Expr::MemberAccess { object, member } = value else {
        return None;
    };
    if receiver_object(&object.node, scope).as_ref() != Some(target) {
        return None;
    }
    match member.node.0.to_ascii_lowercase().as_str() {
        "motion_dynamic" => Some(MotionType::Dynamic),
        "motion_keyframed" => Some(MotionType::Keyframed),
        "motion_fixed" => Some(MotionType::Static),
        "motion_character" => Some(MotionType::CharacterKinematic),
        _ => None,
    }
}

pub(super) fn prim_exit_cart(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ExitCart")?;
    if args.len() != 2 {
        return None;
    }
    // `ExitCart` is an MQ101 quest helper, so require a proven quest receiver
    // rather than accepting an arbitrary object method with the same name.
    receiver_quest(object, scope)?;
    let seat = u8::try_from(int_arg(args, 1)?).ok()?;
    if !(1..=5).contains(&seat) {
        return None;
    }
    Some(Effect::ExitCart {
        actor: receiver_object(&args[0].value.node, scope)?,
        seat,
    })
}
