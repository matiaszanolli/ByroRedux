# #1907 — SCR-D5-NEW-04: lower_fragment silently drops a non-quest binding's side-effect instead of declining

_Filed from `docs/audits/AUDIT_SCRIPTING_2026-07-06.md`. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 1907 --json state`)._

---

**Severity**: MEDIUM (latent; becomes HIGH when the fragment lowerer is wired) · **Dimension**: Recognizer-Chain Soundness · **Untrusted-Input**: Yes (would consume decompiled fragment `.pex` once wired)
**Location**: `crates/scripting/src/translate/effects.rs:113-157` (`lower_fragment` / `bind_local`)
**Source**: audit `docs/audits/AUDIT_SCRIPTING_2026-07-06.md` (SCR-D5-NEW-04)

## Description
A `Stmt::VarDecl`/`Stmt::Assign` whose initializer is a side-effecting **non-quest** expression (e.g. `ObjectReference k = akActor.PlaceAtMe(...)`) is routed through `bind_local`, which records the name in `decl_locals` and continues — producing **no effect and no decline**. The initializer's side-effect (the spawn) is silently dropped, yet a following `Self.SetStage(20)` is still lowered. This contradicts the function's own module doc (`effects.rs` — a non-quest binding "is itself an unmodeled statement → decline") and the flat-sequence decline contract. `decl_locals` only guards the later *use* site (an assignment to a field/index at `:244`), never the binding's own side-effect.

## Evidence
`lower_fragment:122-137` handles `VarDecl`/`Assign` via `bind_local`; `bind_local:151-156` inserts into `decl_locals` and returns without emitting or declining. Only `Stmt::ExprStmt` (`:140`) reaches `classify_effect`, so a side-effecting RHS on an assignment is never evaluated as an effect. No guard test covers a non-quest side-effecting initializer.

## Impact
**Not reachable today** — `lower_fragment` is the unwired Phase-3 fragment lowerer (the designed #1739 gap; the `RECOGNIZERS` table has no fragment entry), so there is no live corruption. But it is a genuine leak of the decline invariant *inside the one function whose entire contract is that invariant*. When the QUST `VMAD` fragment decoder wires `lower_fragment` into the boundary, a quest fragment that spawns/does side-effect work before a `SetStage` would advance the quest while silently discarding the spawn — HIGH impact at that point.

## Related
Distinct from #1739 ("the lowerer isn't wired") — this is a soundness defect within the lowerer. Future-Phase-Readiness item for b2. Must be closed before wiring `lower_fragment` into `RECOGNIZERS`.

## Suggested Fix
In `bind_local` (or its callers), decline (`return None`) when the initializer is neither a quest expression nor a side-effect-free value — i.e. treat a non-quest *side-effecting* initializer as an unmodeled statement, matching the documented contract. Add a guard test.

## Completeness Checks
- [ ] **DECLINE-INVARIANT**: `bind_local` declines on a non-quest side-effecting initializer rather than recording-and-continuing
- [ ] **SIBLING**: The side-effect-free-value vs side-effecting distinction is consistent with how `classify_effect` treats the same expression kinds
- [ ] **TESTS**: A guard test pins that a fragment binding a non-quest side-effecting initializer (`k = X.PlaceAtMe(...)`) declines; the existing `declines_on_unmodeled_effect` / `empty_fragment_is_understood_as_noop` guards still pass
