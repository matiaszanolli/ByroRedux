# #3496: SCR-D5-2026-08-27-03: prim_set_stage and the three objective primitives are the only four of 31 effect primitives with no upper argument-count guard — an over-arity call silently lowers

**Labels**: low, scripting, quests, bug
**Filed**: 2026-08-27 (`/audit-publish` of `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`)

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: Yes (a modded `.pex` reaches this code)
- **Location**: `crates/scripting/src/translate/effects.rs:588-595` (`prim_set_stage`); `:699-709` (`prim_set_objective_displayed`); `:711-720` (`prim_set_objective_completed`); `:722-731` (`prim_set_objective_failed`)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

Every other primitive in `EFFECT_PRIMITIVES` bounds its argument count — `prim_add_item` (`args.len() > 3 → None`), `prim_activate` (`> 2`), `prim_disable` (`> 1`), `prim_set_open`, `prim_start_scene` (`!args.is_empty()`), and so on — and #2289 added a decline-path test for each. These four read only positional args 0 and 1 and ignore any further argument silently.

`prim_set_stage` is the highest-traffic effect in the domain (20,322 real calls, all one-argument) and the one whose false-positive lowering has the largest blast radius: a fragment shaped `SomeQuest.SetStage(10, <unmodeled term>)` lowers to a plain `SetStage { stage: 10 }` rather than declining.

## Evidence

```rust
// effects.rs:588-595 — no args.len() bound anywhere in the body
fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetStage")?;
    let stage = u16::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetStage {
        quest: receiver_quest(object, scope)?,
        stage,
    })
}
```

The three objective primitives have the same shape (`int_arg(args, 0)` + `bool_arg(args, 1)?.unwrap_or(true)`, no bound). Mechanical sweep of all 31 `fn prim_*` bodies for an `args.len()` / `args.is_empty()` guard: 27 guarded (directly or via a guarded delegate), 4 unguarded — the four above.

## Impact

Not reachable from vanilla content (the compiler emits exactly the declared arity, and all four functions' Papyrus signatures are within the read range), so no shipped game is affected — hence LOW. It is a real hole in the decline discipline for modded/hand-authored input, and an inconsistency a reader of the other 27 primitives would not expect.

## Related

#2289 (CLOSED — added decline tests for 14 primitives, but arg-count declines for these four were not among them); #2540 (CLOSED — added negative-index and i32-overflow declines for the three objective primitives, but not an over-arity decline).

## Suggested Fix

Add `if args.len() > N { return None; }` to each (N = 1 for `SetStage`, 3 for `SetObjectiveDisplayed`, 2 for the other two, matching their real Papyrus signatures), plus one decline test each in the block #2289 already established.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (re-sweep all 31 `prim_*` bodies after the fix so the guarded count is 31/31)
- [ ] **TESTS**: A regression test pins this specific fix
