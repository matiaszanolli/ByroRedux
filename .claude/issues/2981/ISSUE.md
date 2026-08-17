# TD8-2026-08-16-02: ActionState::is_held's test-only allow(dead_code) is redundant — it has 14 production callers

**Issue**: #2981
**Severity**: LOW
**Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 8 — Dead Code & Backwards-Compat Cruft). Effort: trivial.

**Location**: `byroredux/src/interaction.rs`:582-585
**Age**: attribute `fe3431e4`, 2026-08-09; invalidated by `4a404f5c` / `eb5d76fe`, 2026-08-15/16
**Status note**: NEW — a regression of the "justified" verdict in `AUDIT_TECH_DEBT_2026-08-12` § TD8-2026-08-12-04

## Description

`#[cfg_attr(not(test), allow(dead_code))]` on `ActionState::is_held` dates from when the action layer was test-only. It now has non-test callers in three modules, so **the attribute suppresses nothing**.

The 2026-08-12 sweep examined this cluster and recorded all four attributes as justified; one week of gameplay-slice work invalidated one of them — which is the point of re-running the recipe.

## Evidence

```
$ grep -rn '\.is_held(' byroredux/src --include="*.rs" | grep -v '_tests.rs' | grep -v assert
byroredux/src/combat.rs:75
byroredux/src/systems/character.rs:168,171,174,177,180,181
byroredux/src/systems/camera.rs:53,56,59,62,65,73
```

14 production call sites across three modules (re-measured 2026-08-16).

```rust
// byroredux/src/interaction.rs:582-585
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn is_held(&self, action: InputAction) -> bool {
    self.held & action.bit() != 0
}
```

Its sibling `was_released` (:591-594) still has **zero** non-test callers — its attribute remains correct, so this is a one-site fix, not a cluster removal.

## Impact

None functional. It is a false "this is only used by tests" signal on the single most-called accessor in the input layer.

## Suggested Fix

Delete the attribute on `is_held`. Leave `was_released` and `ActionBindings::bind_key` as they are.

## Related

- `AUDIT_TECH_DEBT_2026-08-12` § TD8-2026-08-12-04 (the verdict this regresses)
- #1761 (TD8-004, OPEN — the same "attribute outlived its need" shape in `Dx10Chunk::start_mip`)

## Completeness Checks
- [ ] **SIBLING**: `was_released` and `ActionBindings::bind_key` deliberately left alone — verify they still have zero production callers rather than assuming
- [ ] **NO-WARN**: `cargo check` clean after removal (the attribute really was inert)
- [ ] **SCOPE**: One-site fix, not a cluster sweep

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2981 --json state` when live state is needed.*
