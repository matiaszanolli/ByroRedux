# SCR-D1-2026-10-17-08: Local re-assignment in `lower_statements` ignores the compound `AssignOp`

Issue: #5976
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-08: Local re-assignment in `lower_statements` ignores the compound `AssignOp`

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/lower.rs:426-449`
- **Status**: NEW
- **Untrusted-Input**: No (latent)
- **Description**: `Stmt::Assign { target, op, value }` on a declared local calls `bind_local(scope, name, &value.node)` and never looks at `op`. `Int n = 1; n += 2` (`AssignOp::PlusEq`) lowers as `SetLocal n = 2`, `n *= 3` as `n = 3`, etc. The sibling writers (`lower_quest_variable_write`, `lower_self_variable_write`, `lower_reference_variable_write`) all decline `op != Eq`.
- **Evidence**: `AssignOp` has `Eq/PlusEq/MinusEq/MulEq/DivEq/ModEq` (`papyrus/src/ast.rs:225`); the parser produces them (`parser/stmt.rs:296-301`); the `.pex` decompiler only ever emits `Eq` (`pex/src/decompile/lower.rs:221`) and the `.psc` route is test-only, so this is unreachable from compiled content today.
- **Impact**: None in production today; a wrong-AST-accepted hazard for any future `.psc`-source lowering.
- **Related**: #5638 (assignment-to-undeclared decline, same code).
- **Suggested Fix**: `if *op != AssignOp::Eq { return None; }` before `bind_local` (one line + a test).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
