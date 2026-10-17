//! Lock primitives (#5801): `Lock`, `SetLockLevel`. Applied by
//! `fragment/effects/lock.rs`.

use super::*;

/// `<object>.Lock()` / `.Lock(true)` / `.Lock(false)` /
/// `.Lock(abLock, abAsOwner)` (#3159, #5653).
///
/// `Lock` declares TWO parameters — `Lock(bool abLock = true, bool
/// abAsOwner = false)` — and the compiler emits the declared arity, so
/// the pre-#5653 bound of one argument never matched a compiled call:
/// real content's `Lock(false, true)` was declined as over-arity and the
/// primitive only ever fired on hand-written shapes. `abAsOwner` (flag
/// the adjoining cell public/private as the owner opened or closed the
/// door) has no model here, so it follows the `AddItem`-`abSilent`
/// discipline: accepted only as a literal, parsed and not applied;
/// anything non-literal declines the whole call. Papyrus's own default
/// for `abLock` is `true`.
///
/// Starfield's dialect adds a THIRD parameter — `Lock(bool abLock, bool
/// abAsOwner, bool abTraverseLinkedLockables = true)`, read straight off
/// the shipped `ObjectReference` engine-class stub (FO4 and FO76 stubs
/// stop at two). It qualifies whether the lock propagates to *linked*
/// lockables, which [`Effect::SetLocked`] does not model for the receiver
/// either way, so it follows the same literal-only-not-applied discipline
/// as `abAsOwner`. Before this bound, `Lock(bool, false, true)` — the
/// near-universal compiled spelling, e.g. the whole SF `QF_` lock family —
/// declined as over-arity.
pub(super) fn prim_lock(e: &Expr, scope: &Scope) -> Option<Effect> {
    // FO4/Starfield `Unlock(abAsOwner = false)` — "Unlocks this object" — is
    // `Lock(false, abAsOwner)`.
    if let Some((object, args)) = method_call(e, "Unlock") {
        if args.len() > 1 {
            return None;
        }
        bool_arg(args, 0)?;
        return Some(Effect::SetLocked {
            target: receiver_object(object, scope)?,
            locked: false,
        });
    }
    let (object, args) = method_call(e, "Lock")?;
    if args.len() > 3 {
        return None;
    }
    let locked = bool_arg(args, 0)?.unwrap_or(true);
    // 2nd arg `abAsOwner`, 3rd arg (Starfield) `abTraverseLinkedLockables` —
    // literal-only; see the primitive doc.
    bool_arg(args, 1)?;
    bool_arg(args, 2)?;
    Some(Effect::SetLocked {
        target: receiver_object(object, scope)?,
        locked,
    })
}

/// `<object>.SetLockLevel(<literal>)` (#3159).
///
/// Requires exactly one argument — unlike `Lock`, Papyrus gives
/// `SetLockLevel` no default, so a bare call is malformed rather than
/// meaningful and declines. The level is an authored lock difficulty that
/// the wire format carries as a single byte, so a literal outside `0..=255`
/// is not a level this engine can represent and declines too, rather than
/// truncating into a different difficulty.
pub(super) fn prim_set_lock_level(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetLockLevel")?;
    if args.len() != 1 {
        return None;
    }
    let level = u8::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetLockLevel {
        target: receiver_object(object, scope)?,
        level,
    })
}
