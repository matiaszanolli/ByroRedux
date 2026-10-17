//! Inventory primitives (#5801): items, caps and equipping. Applied by
//! `fragment/effects/inventory.rs`.

use super::*;

/// Positional count argument `idx` (default `default` when absent). See
/// [`ItemCount`].
pub(super) fn item_count_arg(
    args: &[byroredux_papyrus::ast::CallArg],
    idx: usize,
    default: u32,
    scope: &Scope,
) -> Option<ItemCount> {
    let Some(arg) = args.get(idx) else {
        return Some(ItemCount::Literal(default));
    };
    if let Some(value) = int_arg(args, idx) {
        // A negative literal declines.
        return u32::try_from(value).ok().map(ItemCount::Literal);
    }
    if as_num(&arg.value.node).is_some() {
        // A fractional or negative literal is not a count.
        return None;
    }
    global_value_read(&arg.value.node, scope)
        .map(ItemCount::Global)
        .or_else(|| classify_value(&arg.value.node, scope).map(ItemCount::Value))
}

/// The global a value expression reads: `<global>.GetValue()`,
/// `<global>.GetValueInt()` or `<global>.Value`, under any casts.
fn global_value_read(expr: &Expr, scope: &Scope) -> Option<ObjectRef> {
    let mut node = expr;
    while let Expr::Cast { expr, .. } = node {
        node = &expr.node;
    }
    for name in ["GetValueInt", "GetValue"] {
        if let Some((object, args)) = method_call(node, name) {
            return args
                .is_empty()
                .then(|| global_property(object, scope))
                .flatten();
        }
    }
    match node {
        Expr::MemberAccess { object, member } if member.node.0.eq_ignore_ascii_case("value") => {
            global_property(&object.node, scope)
        }
        _ => None,
    }
}

/// M47.2 — `Game.GetCaps()` (FO4's `Game.psc`: `Return GetForm(15) as
/// MiscObject`), cast or not.
fn is_get_caps(e: &Expr) -> bool {
    let mut e = e;
    while let Expr::Cast { expr, .. } = e {
        e = &expr.node;
    }
    static_call(e, "Game", "GetCaps").is_some_and(|args| args.is_empty())
}

/// M47.2 — the player gaining or losing a literal count of `Game.GetCaps()`
/// is [`Effect::PlayerCaps`], the effect `GivePlayerCaps` /
/// `RemovePlayerCaps` lower to.
fn player_caps_transfer(
    container: &ActorRef,
    item: &Expr,
    count: &ItemCount,
    sign: i32,
) -> Option<Effect> {
    if !matches!(container, ActorRef::Player) || !is_get_caps(item) {
        return None;
    }
    let ItemCount::Literal(count) = count else {
        return None;
    };
    Some(Effect::PlayerCaps {
        delta: sign * i32::try_from(*count).ok()?,
    })
}

pub(super) fn prim_add_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "AddItem")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    let container = receiver_actor(object, scope)?;
    if let Some(caps) = item_count_arg(args, 1, 1, scope).and_then(|count| {
        player_caps_transfer(&container, &args[0].value.node, &count, 1)
    }) {
        bool_arg(args, 2)?;
        return Some(caps);
    }
    let item = receiver_object(&args.first()?.value.node, scope)?;
    let count = item_count_arg(args, 1, 1, scope)?;
    // Optional 3rd arg (`abSilent`) — accepted only as a literal (parsed,
    // not applied; see the `Effect::AddItem` doc). A present-but-
    // non-literal 3rd arg declines via `bool_arg`'s `None`; a 4th arg
    // entirely is outside this primitive's understood shape.
    bool_arg(args, 2)?;
    if args.len() > 3 {
        return None;
    }
    Some(Effect::AddItem {
        container,
        item,
        count,
    })
}

/// `RemoveItem(akItemToRemove, aiCount = 1, abSilent = false,
/// akOtherContainer = None)`. The count must be a non-negative literal; the
/// other container a literal `None` or a resolvable reference.
pub(super) fn prim_remove_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "RemoveItem")?;
    if args.is_empty() || args.len() > 4 {
        return None;
    }
    let container = receiver_actor(object, scope)?;
    let no_destination = matches!(args.get(3).map(|arg| &arg.value.node), None | Some(Expr::NoneLit));
    if let Some(caps) = item_count_arg(args, 1, 1, scope)
        .filter(|_| no_destination)
        .and_then(|count| player_caps_transfer(&container, &args[0].value.node, &count, -1))
    {
        bool_arg(args, 2)?;
        return Some(caps);
    }
    let item = receiver_object(&args[0].value.node, scope)?;
    let count = item_count_arg(args, 1, 1, scope)?;
    bool_arg(args, 2)?;
    let destination = match args.get(3).map(|arg| &arg.value.node) {
        None | Some(Expr::NoneLit) => None,
        Some(other) => Some(receiver_actor(other, scope)?),
    };
    Some(Effect::RemoveItem {
        container,
        item,
        count,
        destination,
    })
}

/// `Game.GivePlayerCaps(int nCaps)` / `Game.RemovePlayerCaps(int nCaps)` with
/// a literal count; FO4's `Game.psc` bodies act only when `nCaps > 0`.
pub(super) fn prim_player_caps(e: &Expr, _scope: &Scope) -> Option<Effect> {
    let (args, sign) = match static_call(e, "Game", "GivePlayerCaps") {
        Some(args) => (args, 1),
        None => (static_call(e, "Game", "RemovePlayerCaps")?, -1),
    };
    if args.len() != 1 {
        return None;
    }
    let count = i32::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::PlayerCaps {
        delta: sign * count.max(0),
    })
}

pub(super) fn prim_equip_item(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "EquipItem")?;
    if args.is_empty() || args.len() > 3 {
        return None;
    }
    // Prevent-unequip needs persistent locked-equipment state. Decline that
    // shape instead of claiming it and silently losing the lock contract.
    if bool_arg(args, 1)?.unwrap_or(false) {
        return None;
    }
    Some(Effect::EquipItem {
        actor: receiver_actor(object, scope)?,
        item: receiver_object(&args[0].value.node, scope)?,
        silent: bool_arg(args, 2)?.unwrap_or(false),
    })
}
