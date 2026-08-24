# 3278: SCR-D5-2026-08-24-01: Effect::Disable has no production consumer, and its receiver resolution is narrower than sibling object-targeting effects

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_SCRIPTING_2026-08-24.md` (SCR-D5-2026-08-24-01)

## Description

Two independent defects in the same 2026-08-24 addition (`5f38402e`).

**(a) No runtime consumer.** `ReferenceEnableState::is_enabled` is called from nowhere in `byroredux/` (only from a test). A `Disable()` effect records intent but has zero observable runtime effect.

**(b) Receiver-resolution asymmetry.** `prim_disable` lowers its receiver through `receiver_object` (the same alias-aware function `AddItem`/`MoveTo` use), but dispatch resolves it through the strict `resolve_property_form_id` instead of the alias-aware `resolve_object` its siblings use. An alias-bound `Disable()` call silently declines in exactly the cases where the equivalent `AddItem`/`MoveTo`/`SetOpen` would resolve and apply.

## Location

`crates/scripting/src/translate/effects.rs:803-810` (`prim_disable`); `crates/scripting/src/fragment.rs:741-748` (dispatch); `crates/scripting/src/fragment.rs:65-73` (`ReferenceEnableState::is_enabled`)

## Evidence

```rust
// fragment.rs:741-748 — dispatch uses the strict, non-alias-aware resolver
Effect::Disable { object, fade_out: _ } => {
    let form_id = resolve_property_form_id(vmad, object.property_name())?;
    deferred.reference_enable_changes.push((form_id, false));
    None
}
```

## Impact

Even once (a) is fixed with a real consumer, (b) means every alias-bound `Disable()` call — plausibly the majority of authored uses — continues to silently decline at dispatch.

## Related

Same root commit as the confirmed-open `ReferenceEnableState` future-phase item.

## Suggested Fix

(a) Give `Disable`/`Enable` a real consumer at the cell-loader/streaming visibility decision point. (b) Route `Effect::Disable`'s receiver through `resolve_object` (alias-aware), matching its siblings.

## Completeness Checks
- [ ] **SIBLING**: Receiver resolution matches `AddItem`/`MoveTo`/`EquipItem`
- [ ] **TESTS**: A test with an alias-bound `Disable()` call asserting it resolves and dispatches
- [ ] **TESTS**: A runtime consumer test asserting `ReferenceEnableState` actually gates visibility/collidability
