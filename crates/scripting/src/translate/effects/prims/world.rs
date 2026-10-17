//! World / message primitives (#5801): map markers, messages and
//! notifications, keywords, delete-when-able, barter, XP / achievements /
//! stats, linked references, location data and activation blocking — the
//! effects `fragment/effects/deferred.rs` commits.

use super::*;

/// `<actor>.SetValue(akAV, afValue)` — the two-argument form is the actor
/// value setter (`GlobalVariable.SetValue` takes one argument and is
/// [`prim_set_global_value`]'s). The value must be a numeric literal.
/// `ObjectReference.AddToMap(bool abFastTravel = False)`.
pub(super) fn prim_add_to_map(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddToMap")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::AddToMap {
        marker: receiver_object(object, scope)?,
        fast_travel: bool_arg(args, 0)?.unwrap_or(false),
    })
}

/// `Message.Show(float afArg1 = 0.0, … afArg9 = 0.0)` (nine literal
/// numbers at most) and `Message.ShowAsHelpMessage(string asEvent, float
/// afDuration, float afInterval, int aiMaxTimes[, string asContext = "", int
/// aiPriority = 0, Message GamepadMsg = None])` with literal arguments. The context restricts a message
/// to a menu ("Pass 'NoMenu' for this message only to appear when the game is
/// not in menu mode"); only the unrestricted `""` and `"NoMenu"` are shown
/// here, any menu name declines. The priority only orders competing messages.
pub(super) fn prim_show_message(e: &Expr, scope: &Scope) -> Option<Effect> {
    if let Some((object, args)) = method_call(e, "ShowAsHelpMessage") {
        if !(4..=7).contains(&args.len()) || !matches!(args[0].value.node, Expr::StringLit(_)) {
            return None;
        }
        for arg in &args[1..4] {
            as_num(&arg.value.node)?;
        }
        if let Some(context) = args.get(4) {
            match &context.value.node {
                Expr::StringLit(text) if text.is_empty() || text.eq_ignore_ascii_case("NoMenu") => {}
                _ => return None,
            }
        }
        if let Some(priority) = args.get(5) {
            as_num(&priority.value.node)?;
        }
        // Starfield's `GamepadMsg`: the text shown instead on a gamepad. The
        // keyboard message is the one shown here.
        if let Some(gamepad) = args.get(6) {
            if !matches!(gamepad.value.node, Expr::NoneLit) {
                receiver_object(&gamepad.value.node, scope)?;
            }
        }
        return Some(Effect::ShowHelpMessage {
            message: receiver_object(object, scope)?,
        });
    }
    let (object, args) = method_call(e, "Show")?;
    if args.len() > 9 {
        return None;
    }
    let args = args
        .iter()
        .map(|arg| as_num(&arg.value.node))
        .collect::<Option<Vec<f32>>>()?;
    Some(Effect::ShowMessage {
        message: receiver_object(object, scope)?,
        args,
    })
}

/// `Debug.Notification(string asNotificationText)` with a literal text.
pub(super) fn prim_notification(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Debug", "Notification")?;
    if args.len() != 1 {
        return None;
    }
    let Expr::StringLit(text) = &args[0].value.node else {
        return None;
    };
    Some(Effect::Notification { text: text.clone() })
}

/// `ObjectReference.AddKeyword(apKeyword)` / `RemoveKeyword(apKeyword)` /
/// FO4's `ResetKeyword(apKeyword)`.
pub(super) fn prim_change_keyword(e: &Expr, scope: &Scope) -> Option<Effect> {
    use crate::keywords::KeywordChange;
    let (object, args, change) = [
        ("AddKeyword", KeywordChange::Add),
        ("RemoveKeyword", KeywordChange::Remove),
        ("ResetKeyword", KeywordChange::Reset),
    ]
    .into_iter()
    .find_map(|(method, change)| method_call(e, method).map(|(o, a)| (o, a, change)))?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::ChangeKeyword {
        object: receiver_actor(object, scope)?,
        keyword: receiver_object(&args[0].value.node, scope)?,
        change,
    })
}

/// `ObjectReference.DeleteWhenAble()` (Skyrim: a script loop over
/// `GetParentCell().IsAttached()` then `Delete()`; FO4 / Starfield native)
/// and `Delete()`, which the CK says "flags this object reference as wanting
/// to be deleted" once nothing holds it — a created reference's deletion
/// either way, an authored one cannot be deleted.
pub(super) fn prim_delete_when_able(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call_any(e, &["DeleteWhenAble", "Delete"])?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::DeleteWhenAble {
        object: receiver_object(object, scope)?,
    })
}

/// `Actor.ShowBarterMenu()` — the same zero-argument native on Skyrim,
/// Fallout 4 and Starfield.
pub(super) fn prim_show_barter_menu(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "ShowBarterMenu")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::ShowBarterMenu {
        vendor: receiver_actor(object, scope)?,
    })
}

/// `Game.RewardPlayerXP(int auiXPAmount, bool abDirect = false)` with a
/// literal amount.
pub(super) fn prim_reward_player_xp(e: &Expr, scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "RewardPlayerXP")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    // A literal, or the `XPMiscQuestSmall.GetValue() as Int` global FO4
    // quests reward.
    Some(Effect::RewardPlayerXp {
        amount: item_count_arg(args, 0, 0, scope)?,
        direct: bool_arg(args, 1)?.unwrap_or(false),
    })
}

/// `ObjectReference.SetLinkedRef(apRef, apKeyword = None)`; `None` for the
/// target or the keyword is written `None`. Starfield's signature adds
/// `abPromoteParentRefr`: it promotes a non-persistent parent reference so
/// the link survives, which [`crate::LinkedRefOverrides`] already does by
/// keying the saved link on FormIDs, so the flag changes nothing here.
pub(super) fn prim_set_linked_ref(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetLinkedRef")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    if let Some(promote) = args.get(2) {
        if !matches!(promote.value.node, Expr::BoolLit(_)) {
            return None;
        }
    }
    let optional = |index: usize| -> Option<Option<ObjectRef>> {
        match args.get(index).map(|arg| &arg.value.node) {
            None | Some(Expr::NoneLit) => Some(None),
            Some(other) => receiver_object(other, scope).map(Some),
        }
    };
    Some(Effect::SetLinkedRef {
        object: receiver_object(object, scope)?,
        target: optional(0)?,
        keyword: optional(1)?,
    })
}

/// `<reference>.SetLocRefType(akLoc, akRefType)` (CK *SetLocRefType -
/// ObjectReference*); the location is a reference to a `Location` or `None`.
pub(super) fn prim_set_loc_ref_type(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetLocRefType")?;
    if args.len() != 2 {
        return None;
    }
    if !matches!(args[0].value.node, Expr::NoneLit) {
        receiver_object(&args[0].value.node, scope)?;
    }
    let ref_type = match &args[1].value.node {
        Expr::NoneLit => None,
        other => Some(receiver_object(other, scope)?),
    };
    Some(Effect::SetLocRefType {
        object: receiver_object(object, scope)?,
        ref_type,
    })
}

/// `Game.AddAchievement(int aiAchievementID)` with a literal ID.
pub(super) fn prim_add_achievement(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "AddAchievement")?;
    if args.len() != 1 {
        return None;
    }
    Some(Effect::AddAchievement {
        id: i32::try_from(int_arg(args, 0)?).ok()?,
    })
}

/// `Game.IncrementStat(string asStatName, int aiModAmount = 1)` with literal
/// arguments.
pub(super) fn prim_increment_stat(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let args = static_call(e, "Game", "IncrementStat")?;
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let Expr::StringLit(stat) = &args[0].value.node else {
        return None;
    };
    let amount = match args.get(1) {
        None => 1,
        Some(_) => i32::try_from(int_arg(args, 1)?).ok()?,
    };
    Some(Effect::IncrementStat {
        stat: stat.clone(),
        amount,
    })
}

/// `Location.SetCleared(bool abCleared = true)`.
pub(super) fn prim_set_location_cleared(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetCleared")?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::SetLocationCleared {
        location: receiver_location(object, scope)?,
        cleared: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `BlockActivation(abBlocked = true)` (Skyrim); FO4 / Starfield add
/// `abHideActivateText = false`, a prompt flag accepted as a literal. The
/// receiver is the reference (an alias getter resolves through it).
pub(super) fn prim_block_activation(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "BlockActivation")?;
    if args.len() > 2 {
        return None;
    }
    bool_arg(args, 1)?;
    Some(Effect::BlockActivation {
        object: receiver_object(object, scope)?,
        blocked: bool_arg(args, 0)?.unwrap_or(true),
    })
}

/// `<location>.SetKeywordData(akKeyword, afData)` (CK *SetKeywordData - Location*).
pub(super) fn prim_set_location_keyword_data(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetKeywordData")?;
    if args.len() != 2 {
        return None;
    }
    Some(Effect::SetLocationKeywordData {
        location: receiver_object(object, scope)?,
        keyword: receiver_object(&args[0].value.node, scope)?,
        value: classify_value(&args[1].value.node, scope)?,
    })
}
