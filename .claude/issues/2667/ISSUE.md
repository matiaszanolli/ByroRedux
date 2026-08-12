# #2667: SCR-D3-NEW11-02: collapse's edge adoption can delete an on-stack ancestor block; the .expect survives only because the depth cap fires first

**Severity**: LOW
**Dimension**: Decompiler Control-Flow / Boolean / Lower (Dimension 3)
**Untrusted-Input**: Yes
**Location**: `crates/pex/src/decompile/boolean.rs:172-177` (`.expect("source block exists")`), `:203-206` (`.expect("conditional source has a last statement")`), `:221-232` (edge adoption + `blocks.remove(&rejoin_key)`); doc claim at `:23-26`
**Status**: NEW

## Description

When the rejoin block of an *inner* collapse is an *enclosing* conditional block (a backward edge to a block currently on the recursion stack), `collapse` removes that ancestor from the CFG (`:230`) and its scope (`:216`) while the ancestor's own `collapse` call has not yet run. The ancestor's `collapse` then does `self.cfg.block(current).cloned().expect("source block exists")` on a block that no longer exists.

Separately, the module doc's departure (2) states the re-process loop "always terminates" because a merge strictly shrinks the graph. That is true of the `while` loop in `rebuild` (each reprocess removes two blocks) but says nothing about `rebuild`'s **recursion**, which the same edge adoption can drive into the same block range.

## Evidence

Attempted to reach the panic with a crafted stream (harness since deleted):

```
0: cmp_eq ::temp0, a, b
1: jmpf   ::temp0, +4  -> 5
2: cmp_eq ::temp1, c, d
3: jmpf   ::temp1, -3  -> 0    ; inner false edge back to the outer block
4: cmp_eq ::temp1, e, f
5: return
```

Result: `ERR control-flow reconstruction in 'Case3' exceeded the recursion limit (1024)` -- **fails closed, no panic**.

Why: after the inner collapse adopts the ancestor's edges, the adopted `next` necessarily points back into the block range currently being walked (for `&&` the ancestor's `next` *is* the range start; for `||` its `on_false` is), so `rebuild` re-enters the same block and recurses until `MAX_REBUILD_DEPTH` fires. No variant could be constructed that removes an on-stack block *without* also creating that self-recursion -- so the `.expect` appears unreachable, but only as a consequence of the depth cap, not of any local invariant, and nothing records that dependency.

## Impact

No demonstrated panic today; the depth cap converts the pathological shape into a clean `RecursionLimit` `Err`, which `translate_pex` degrades to a decline.

Residual risks are maintenance-shaped:
1. if the cap is ever raised, removed, or bypassed, the `.expect` becomes a live panic on untrusted `.pex`;
2. the error a user sees for a *boolean*-pass overflow reads "control-flow reconstruction ... exceeded the recursion limit" because both passes share `DecompileError::RecursionLimit`, whose `#[error]` string (`crates/pex/src/decompile/mod.rs:70`) hardcodes the control-flow wording -- misleading during triage.

## Related

SCR-D3-NEW11-01 (same function, same missing edge validation -- that fix also rejects most of these shapes); #1815; #1729

## Suggested Fix

Replace both `.expect`s in `collapse` with a decline (`return Ok(false)`) so the invariant is local rather than inherited from the depth cap; narrow the doc at `:23-26` to say the guard bounds the *iterative* loop and that recursion is bounded by `MAX_REBUILD_DEPTH`; and either genericize `RecursionLimit`'s message or add a pass discriminant.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
