# SCR-D1-2026-10-17-06: `ValueExpr` arithmetic is untyped f32 - Papyrus `Int` division / `SetValueInt` truncation is lost

Issue: #5974
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-06: `ValueExpr` arithmetic is untyped f32 - Papyrus `Int` division / `SetValueInt` truncation is lost

- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/guards.rs:34-47` (`BinaryOp::Div` -> `ArithOp::Div`), `model.rs:278-288` (`ArithOp::apply` f32), `prims/quest_scoped.rs:26-33` (`SetValueInt` lowered identically to `SetValue`), apply `fragment/effects.rs:~145`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The decompiler's `IDIV`/`FDIV` both become `BinaryOp::Div`, `classify_value` strips `as Int` casts (`guards.rs:12-14`), and `ValueExpr::Arith` evaluates in f32 with float division. So `G.SetValueInt(H.GetValueInt() / 2)` stores 7.5 for H=15 where Papyrus stores 7, `(a / 2) * 2`, and `If a / 10 == 3` comparisons differ from Papyrus. `SetValueInt` is not distinguished from `SetValue` at all. This is an unmodeled-term acceptance (integer typing) rather than a decline. (The VM keeps `Int` arithmetic exact - `docs/engine/scripting.md:739-740` - so the two fragment paths disagree.)
- **Evidence**: code above; no `Div` test exists in `effects/tests.rs`; `ItemCount` only truncates at the final count (`resolve.rs:82`). Reachability in the vanilla corpus was not measured (corpus instruments need the full fragment walk).
- **Impact**: Wrong stored global / wrong branch for integer-division expressions; also f32 cannot represent ints above 2^24.
- **Related**: none filed.
- **Suggested Fix**: Decline `Div` (and `SetValueInt` with a non-literal) at lowering time unless both operands are provably Float, or carry an Int/Float tag in `ValueExpr::Arith` and truncate toward zero for Int.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
