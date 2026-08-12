# #2655: SCR-D3-NEW11-01: Boolean pass's missing debug-line guard silently erases a While loop whose one-statement body writes the loop-condition variable

**Severity**: MEDIUM
**Dimension**: Decompiler Control-Flow / Boolean / Lower (Dimension 3)
**Untrusted-Input**: Yes
**Location**: `crates/pex/src/decompile/boolean.rs:143-158` (the `&&`/`||` candidate test) and `:172-247` (`collapse`); documented departure at `:17-22`
**Status**: NEW -- corrects the "benign" adjudication in AUDIT_SCRIPTING_2026-07-02.md:103, _07-06.md:109, _08-03.md:148, _08-07.md:137. Distinct from #2028, which covered only the `operand_key == rejoin_key` degenerate shape.

## Description

`collapse` decides a block pair is a short-circuit `&&`/`||` from three structural signals only: the source block is conditional, its last statement computes the condition variable, and the fall-through edge's block is a single statement recomputing *the same* variable. It never checks that the operand block actually **falls through to the rejoin block**.

A loop body satisfies all three signals while its `next` edge points *backwards* to the loop head. The collapse then deletes the operand block -- destroying the back edge -- merges the rejoin block's statements and adopts its edges, so the `While` disappears from the output entirely and is replaced by a fabricated `&&` that was never in the source.

Four prior passes adjudicated this departure benign by reasoning about one ambiguous shape (an `If`-guarded reassignment of the condition variable, which genuinely *is* semantics-preserving). The loop shape is a second ambiguous shape and is **not**.

## Evidence

Empirically reproduced via a throwaway harness calling the public `byroredux_pex::decompile::decompile_script` (harness deleted; tree clean). Input -- a structurally valid instruction stream, `build_cfg` accepts it and every jump is in range:

```
0: cmp_eq ::temp0, a, b        ; loop condition
1: jmpf   ::temp0, +3  -> 4    ; loop exit test
2: cmp_eq ::temp0, c, d        ; loop body -- single stmt, writes ::temp0
3: jmp    -3           -> 0    ; back edge
4: return
```

CFG: block `0`=[0,1] cond `::temp0`, on_true=2, on_false=4; block `2`=[2,3] next=**0** (back edge); block `4`=[4,4]. `collapse(0, "::temp0", And)` takes operand_key=2, rejoin_key=4, accepts, removes block 2, adopts block 4's edges.

Output (verbatim, trimmed):

```
Function Case1
  body: [ ExprStmt( BinaryOp{ left: BinaryOp{a Eq b}, op: And,
                              right: BinaryOp{c Eq d} } ),
          Return(None) ]
```

No `Stmt::While` anywhere. Control case confirms the benign shape prior audits reasoned about still behaves correctly (`If bDone / bDone = Bar() / EndIf` collapses to `bDone = Foo() && Bar()`, which *is* equivalent) -- so the fix must not break it.

## Impact

A wrong-but-non-panicking AST out of the decompiler -- the exact failure class the domain escalation table calls out, and one **invisible to both instruments `boolean.rs:21-22` cites as validation**:

- the corpus smoke harness scores such a file as a clean success (it discards the resulting `Script` without any shape check, so it measures robustness, not fidelity);
- the R5 fidelity gate is a single `#[ignore]`d script (`da10_main_door_decompiles_to_the_r5_reference_shape`) that does not run without Skyrim SE game data on disk.

So the departure's documented justification is not actually supported by its cited evidence.

MEDIUM rather than HIGH because the shape could not be constructed from official-Papyrus-compiler output (a discarded call result is written to `::NoneVar`, never the condition temp), vanilla decompiles 26640/26641 with no reported shape regressions, and the downstream recognizer chain declines the resulting bare `ExprStmt` rather than acting on it. The exposure is a hand-crafted or third-party-compiled `.pex` shipped by a mod -- real untrusted input, no vanilla-content path. **Escalate to HIGH if a vanilla instance is ever found.**

## Related

#2028 (sibling degenerate-shape guard in the same function), #1732 (the control-flow pass's fail-closed sibling), #2542 (the pass-order doc-rot)

## Suggested Fix

Add one edge check to `collapse` before accepting -- the operand block must fall through to the rejoin:

```rust
self.cfg.block(operand_key)
    .is_some_and(|b| b.next == rejoin_key && !b.is_conditional())
```

Verified against every shape in this pass: it rejects the loop case (operand next=0 != rejoin 4) and preserves both real short-circuit shapes (`and_collapses_...` / `or_collapses_...`, operand next=3=rejoin) and the benign `If`-guard case.

Add the loop instruction stream above as a regression test, and correct `boolean.rs:20-22` -- the corpus decompile rate cannot validate a wrong-AST-without-error departure, and the R5 gate is one `#[ignore]`d script.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
