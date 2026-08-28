# #3487: SCR-D5-2026-08-27-01: three effect primitives guard on hand-authored .psc arity, but the .pex frontend materializes every default argument — MoveTo declines 100% of 3,334 real calls

**Labels**: medium, scripting, quests, ai, game:fo4, game:starfield, bug
**Filed**: 2026-08-27 (`/audit-publish` of `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`)

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:790-801` (`prim_move_to`); `crates/scripting/src/translate/effects.rs:1072-1080` (`prim_evaluate_package`); `crates/scripting/src/translate/effects.rs:889-893` (`prim_player_controls`)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

The Papyrus compiler emits **every** parameter of a call into the compiled `.pex`, including ones the author omitted and left at their declared default. Quest fragments only ever reach the effect table through the `.pex` frontend (`populate_quest_fragments_from_pex` → `decompile_script` → `lower_fragment_with_quest_properties`) — the `.psc` route is a test-only path. Several primitives, however, were written against the *authored* call shape, and reject the default-materialized one:

| Primitive | Accepted arity | Arities actually observed in the corpus | Declined |
|---|---|---|---|
| `prim_move_to` | exactly 1 | 5 (Skyrim, ×1742) and 6 (FO4/Starfield, ×1592) | **3,334 / 3,334 = 100%** |
| `prim_evaluate_package` | exactly 0 | 0 (×853) and 1 (FO4/Starfield `abResetAI`, ×2628) | **2,628 / 3,481 = 75%** |
| `prim_player_controls` | `<= 9` | 9 (×118) and 11 (FO4/Starfield, ×61) | 61 / 179 = 34% |

`MoveTo` is the severe case: it has an `Effect::MoveTo` variant, a dispatch arm in `fragment.rs` that resolves both receiver and destination through the alias-aware `resolve_object`, and its own regression tests — and it can **never** fire on production input. `prim_move_to`'s comment justifies the narrowness as refusing to "silently drop [an offset] and misplace the object", which is sound reasoning applied to the wrong input shape: the offsets it is refusing are, overwhelmingly, the compiler's own zeros.

## Evidence

`prim_move_to`, verbatim:

```rust
// effects.rs:790-801
fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "MoveTo")?;
    // The conservative 2-arg shape only (receiver + destination) — a 3rd+
    // argument (offsets / match-rotation) declines rather than silently
    // dropping it and misplacing the object.
    if args.len() != 1 {
        return None;
    }
    ...
}
```

Corpus probe over every `Fragment_*` body in `Skyrim - Misc.bsa` + `Fallout4 - Misc.ba2` + `Starfield - Misc.ba2` (26,641 `.pex`, 43,818 behavioral fragments), tallying the literal shape of `MoveTo`'s trailing arguments:

```
moveto  args=5  count=1742
moveto  args=6  count=1592
moveto-tail[f0,f0,f0,btrue]         args=5  count=1668
moveto-tail[f0,f0,f0,btrue,bfalse]  args=6  count=1585
...(43 further distinct tails, 81 calls total, carrying real offsets)
evalpkg-arg[bfalse]  args=1  count=2621
evalpkg-arg[btrue]   args=1  count=7
```

**3,253 of 3,334 `MoveTo` calls (97.6%) carry exactly `(0.0, 0.0, 0.0, matchRotation)` — precisely the "receiver + destination" semantics `Effect::MoveTo { moved, destination }` already models.** Only ~81 calls (2.4%) carry a real offset where the current decline is genuinely protective. Likewise 2,621 of 2,628 one-arg `EvaluatePackage` calls pass the literal `false` default.

## Impact

A shipped, tested, dispatch-wired, alias-aware effect (`MoveTo`) contributes nothing on any real game's content and cannot be observed to be broken by any existing gate — `fragment_coverage` reports a zero for it that reads identically to "authors don't use this". Every fragment containing a `MoveTo` call is guaranteed to decline in full, so this is also a hard ceiling on the whole-fragment claim rate (42.6% Skyrim / 34.9% FO4+SF), not just on one effect. `EvaluatePackage` is game-asymmetric: it works on Skyrim and silently declines on FO4/Starfield, which is exactly the kind of per-game divergence the domain's abstraction rules exist to prevent. This is a *decline*, so nothing is mis-lowered — hence MEDIUM, not HIGH.

## Related

`docs/engine/m47-3-quest-alias-design.md` Phase 2's unchecked "re-measure `AddItem`/`MoveTo` yield" item — this finding is that measurement's result. Same family as #3159 (`Lock`/`Unlock` absent): the effect table is growing faster than anything checks it against real input. See also SCR-D5-2026-08-27-04 (the missing decline-reason tally that concealed this).

## Suggested Fix

Accept the default-materialized tail where its literal value is the documented Papyrus default, and keep declining otherwise — i.e. for `MoveTo`, accept 5/6 args when args 1–3 are numeric-literal `0` and the rotation flags are literals, decline on a non-zero or non-literal offset; for `EvaluatePackage`, accept a literal `abResetAI`; for `prim_player_controls`, widen the bound to the FO4/Starfield parameter count. Then add a corpus-arity assertion to `fragment_coverage` (or a sibling instrument) so the next primitive written against a `.psc` arity fails a gate instead of silently measuring zero.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all 31 `prim_*` bodies re-checked against real corpus arity, not just these three)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
