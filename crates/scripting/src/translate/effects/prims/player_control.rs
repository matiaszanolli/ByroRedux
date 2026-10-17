//! Player-control primitives (#5801): controls and restraint, AI-driven,
//! HUD cart mode, chargen / race menu / save requests, sitting rotation and
//! player animation events. Applied by `fragment/effects/player_control.rs`.

use super::*;

/// `Game.DisablePlayerControls`/`EnablePlayerControls` take nine
/// parameters, and exist only on Skyrim — see [`prim_player_controls`]
/// for why this is not the FO4/Starfield count (#3487).
const SKYRIM_PLAYER_CONTROLS_ARGS: usize = 9;

pub(super) fn prim_set_player_restrained(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetRestrained")?;
    if !is_game_get_player(object) || args.len() > 1 {
        return None;
    }
    Some(Effect::SetPlayerRestrained {
        restrained: bool_arg(args, 0)?.unwrap_or(true),
    })
}

pub(super) fn prim_disable_player_controls(e: &Expr, _scope: &Scope) -> Option<Effect> {
    prim_player_controls(e, "DisablePlayerControls", false)
}

pub(super) fn prim_enable_player_controls(e: &Expr, _scope: &Scope) -> Option<Effect> {
    prim_player_controls(e, "EnablePlayerControls", true)
}

/// `Game.DisablePlayerControls(abMovement, abFighting, abCamSwitch,
/// abLooking, abSneaking, abMenu, abActivate, abJournalTabs,
/// aiDisablePOVType)` — nine parameters, and **Skyrim only**.
///
/// #3487 asked for this bound to be widened to the FO4/Starfield
/// parameter count, on a corpus tally of 61 eleven-argument calls. It is
/// deliberately not widened, because those calls are not this function:
/// Fallout 4 and Starfield have no `Game.DisablePlayerControls` at all
/// (their `Game` script keeps only the `Is*ControlsEnabled` queries), and
/// moved the pair to `InputEnableLayer`, whose eleven-parameter
/// signature ends `..., abJournalTabs, abVATS, abFavorites, abRunning` —
/// arg 8 is a `bool`, where Skyrim's is `int aiDisablePOVType`.
/// [`game_call`] already declines every one of those on the receiver, so
/// widening the arity would claim nothing; if it ever did match, arg 8
/// would be read as a POV type it is not. Signatures verified against the
/// shipped Base scripts (`skse64`/`f4se` `scripts/vanilla/Game.psc` and
/// `InputEnableLayer.psc`, and Starfield's `Game.psc`).
///
/// Claiming the `InputEnableLayer` shape needs its own primitive and a
/// canonical model of input layers (priority stacking, per-layer enable),
/// which [`PlayerControlSelection`] does not have.
fn prim_player_controls(e: &Expr, method: &str, enabled: bool) -> Option<Effect> {
    let args = game_call(e, method)?;
    if args.len() > SKYRIM_PLAYER_CONTROLS_ARGS {
        return None;
    }
    // #5651 — each function fills absent arguments from its own CK-wiki
    // default table; the two calls do not share one.
    let defaults = if enabled {
        PlayerControlSelection::ENABLE_PLAYER_CONTROLS_DEFAULT
    } else {
        PlayerControlSelection::DISABLE_PLAYER_CONTROLS_DEFAULT
    };
    let selection = PlayerControlSelection {
        movement: bool_arg(args, 0)?.unwrap_or(defaults.movement),
        fighting: bool_arg(args, 1)?.unwrap_or(defaults.fighting),
        camera_switching: bool_arg(args, 2)?.unwrap_or(defaults.camera_switching),
        looking: bool_arg(args, 3)?.unwrap_or(defaults.looking),
        sneaking: bool_arg(args, 4)?.unwrap_or(defaults.sneaking),
        menu: bool_arg(args, 5)?.unwrap_or(defaults.menu),
        activation: bool_arg(args, 6)?.unwrap_or(defaults.activation),
        journal_tabs: bool_arg(args, 7)?.unwrap_or(defaults.journal_tabs),
        pov_type: optional_int_arg(args, 8, defaults.pov_type)?,
    };
    Some(Effect::SetPlayerControls { enabled, selection })
}

pub(super) fn prim_set_player_ai_driven(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetPlayerAIDriven")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetPlayerAiDriven {
        ai_driven: bool_arg(args, 0)?.unwrap_or(true),
    })
}

pub(super) fn prim_set_hud_cart_mode(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetHudCartMode")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetHudCartMode {
        cart_mode: bool_arg(args, 0)?.unwrap_or(true),
    })
}

pub(super) fn prim_set_in_chargen(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetInChargen")?;
    if args.len() != 3 {
        return None;
    }
    Some(Effect::SetInChargen {
        disable_saving: bool_arg(args, 0)?.unwrap_or(false),
        disable_waiting: bool_arg(args, 1)?.unwrap_or(false),
        show_controls_disabled_message: bool_arg(args, 2)?.unwrap_or(false),
    })
}

pub(super) fn prim_show_race_menu(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "ShowRaceMenu")?;
    args.is_empty().then_some(Effect::ShowRaceMenu)
}

pub(super) fn prim_request_save(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "RequestSave")?;
    args.is_empty().then_some(Effect::RequestSave { auto: false })
}

pub(super) fn prim_request_auto_save(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "RequestAutoSave")?;
    args.is_empty().then_some(Effect::RequestSave { auto: true })
}

pub(super) fn prim_set_sitting_rotation(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = game_call(e, "SetSittingRotation")?;
    if args.len() != 1 {
        return None;
    }
    let degrees = as_num(&args[0].value.node)?;
    degrees
        .is_finite()
        .then_some(Effect::SetSittingRotation { degrees })
}

pub(super) fn prim_player_imod_animation(e: &Expr, scope: &Scope) -> Option<Effect> {
    prim_player_animation_event(
        e,
        scope,
        "PlayerImodAnimation",
        CinematicAnimationEvent::PlayImod,
    )
}

pub(super) fn prim_player_furniture_animation(e: &Expr, scope: &Scope) -> Option<Effect> {
    prim_player_animation_event(
        e,
        scope,
        "PlayerFurnitureAnimation",
        CinematicAnimationEvent::IdleFurnitureExit,
    )
}

fn prim_player_animation_event(
    e: &Expr,
    scope: &Scope,
    method: &str,
    event: CinematicAnimationEvent,
) -> Option<Effect> {
    let (object, args) = method_call(e, method)?;
    if !args.is_empty() {
        return None;
    }
    receiver_quest(object, scope)?;
    Some(Effect::RegisterPlayerAnimationEvent { event })
}
