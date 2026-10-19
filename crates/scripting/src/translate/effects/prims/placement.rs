//! Placement primitives (#5801): `MoveTo`, `MoveToMyEditorLocation`,
//! `Disable` / `Enable`. Applied by `fragment/effects/placement.rs`.

use super::*;

/// `ObjectReference.MoveTo`'s full parameter count: `akTarget` plus three
/// float offsets plus `abMatchRotation` is five on Skyrim/FO4, and
/// Starfield's `abRotateOffset` makes six. Anything longer is not a shape
/// any shipped game's compiler can emit (#3487).
const MOVE_TO_MAX_ARGS: usize = 6;

/// `ObjectReference.MoveTo(akTarget, afXOffset = 0.0, afYOffset = 0.0,
/// afZOffset = 0.0, abMatchRotation = true)` — Skyrim and Fallout 4;
/// Starfield appends `abRotateOffset = false` for six parameters total
/// (verified against the shipped Base scripts: `skse64`/`f4se`
/// `scripts/vanilla/ObjectReference.psc` and Starfield's
/// `ObjectReference.psc`).
///
/// #3487 — this used to require `args.len() == 1`, the shape an *author*
/// writes. Quest fragments only ever reach this table through the `.pex`
/// frontend, and the Papyrus compiler materializes **every** parameter of
/// a call, including the ones the author omitted; the `.psc` route is
/// test-only. So the authored shape does not occur in compiled content at
/// all, and this primitive declined 3,334 of 3,334 real `MoveTo` calls —
/// a shipped, dispatch-wired, alias-aware effect that could never fire.
///
/// The original reasoning (refusing to silently drop an offset and
/// misplace the object) is kept, applied to the right input shape: a
/// trailing argument is accepted only when it is a literal equal to its
/// declared Papyrus default, which is what the compiler emits for an
/// omitted parameter. 3,253 of the corpus's 3,334 calls are exactly that;
/// the ~81 carrying a real offset still decline, and so does a
/// non-literal (a variable whose runtime value is unknown), via
/// [`as_num`]/[`bool_arg`] returning `None`.
pub(super) fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    // `ReferenceAlias.TryToMoveTo(ref)` is `MoveTo(ref)` on whatever the alias
    // holds, a no-op when it holds nothing (CK *TryToMoveTo - ReferenceAlias*).
    let (object, args) = method_call_any(e, &["MoveTo", "TryToMoveTo"])?;
    if args.is_empty() || args.len() > MOVE_TO_MAX_ARGS {
        return None;
    }
    // args[1..=3] — the three float offsets, each defaulting to `0.0`; they
    // must be literals, since a variable's run-time value is unknown here.
    let mut offset = [0.0f32; 3];
    for (slot, arg) in offset.iter_mut().zip(args.iter().skip(1)) {
        *slot = as_num(&arg.value.node)?;
    }
    // args[4] `abMatchRotation` (default `true`) and Starfield's args[5]
    // `abRotateOffset` (default `false`), which is not modelled.
    let match_rotation = bool_arg(args, 4)?.unwrap_or(true);
    if bool_arg(args, 5)?.unwrap_or(false) {
        return None;
    }
    let destination = receiver_object(&args[0].value.node, scope)?;
    if player_expr_ref(object, scope) {
        // The player's teleport (a cell transition) carries neither.
        if offset != [0.0; 3] || !match_rotation {
            return None;
        }
        return Some(Effect::MovePlayerTo { destination });
    }
    let moved = receiver_object(object, scope)?;
    Some(Effect::MoveTo { moved, destination, offset, match_rotation })
}

/// `<reference>.MoveToMyEditorLocation()` (CK: "Moves this reference to its
/// original editor location and orientation").
pub(super) fn prim_move_to_editor_location(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "MoveToMyEditorLocation")?;
    if !args.is_empty() {
        return None;
    }
    Some(Effect::MoveToEditorLocation {
        moved: receiver_object(object, scope)?,
    })
}

/// `Disable([abFadeOut])`, FO4/Starfield's `DisableNoWait([abFadeOut])` (the
/// same disable, returning before the fade), and — M47.2 —
/// `ReferenceAlias.TryToDisable()`, the CK's "disable a reference in an alias
/// without calling GetReference()" (an empty alias is a no-op, which an
/// unresolved receiver already is at runtime).
pub(super) fn prim_disable(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call_any(e, &["Disable", "DisableNoWait", "TryToDisable"])?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::Disable {
        object: receiver_object(object, scope)?,
        fade_out: bool_arg(args, 0)?.unwrap_or(false),
    })
}

/// #3489 — mirrors [`prim_disable`]: same receiver treatment (including
/// alias-bound `ObjectReference` properties via `receiver_object`), same
/// optional literal `bool` argument shape (`abFadeIn` here vs `abFadeOut`),
/// and the same `EnableNoWait` / `TryToEnable` spellings.
pub(super) fn prim_enable(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call_any(e, &["Enable", "EnableNoWait", "TryToEnable"])?;
    if args.len() > 1 {
        return None;
    }
    Some(Effect::Enable {
        object: receiver_object(object, scope)?,
        fade_in: bool_arg(args, 0)?.unwrap_or(false),
    })
}
