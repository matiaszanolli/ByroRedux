# SCR-D2-01: Decompiler boolean-collapse pass has no recursion-depth cap — stack overflow from untrusted .pex

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1815
**Source report**: docs/audits/AUDIT_SCRIPTING_2026-07-02.md
**Labels**: high, safety, bug

- **Severity**: HIGH
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/boolean.rs:110-145` (`BoolPass::rebuild`)
- **Status**: NEW

**Description**: `BoolPass::rebuild` recurses on `self.rebuild(block.on_true(), block.on_false)` / `self.rebuild(block.on_false, block.on_true())` for each conditional block whose fall-through edge is a short-circuit operand. Unlike every other decompiler tree walk that consumes untrusted structure, it carries **no depth guard**: `control_flow.rs::reconstruct` was capped with `MAX_REBUILD_DEPTH = 1024` (#1729 / SAFE-2026-06-23-02), and the `.psc` parser has `MAX_EXPR_DEPTH` / `MAX_STMT_DEPTH`. The boolean pre-pass — which runs on the same untrusted CFG (`lower.rs::decompile_body` line 216) *before* the capped control-flow pass — was missed.

**Evidence**: `grep -c depth crates/pex/src/decompile/boolean.rs` → `0`. The recursion at lines 127/131 has no `depth` parameter and no `MAX_*` check; `mod.rs::DecompileError` has a `RecursionLimit` variant used only by `control_flow.rs`.

**Impact**: A hostile/corrupt `.pex` in a modded `--scripts-bsa` archive with deeply-chained `&&`/`||` short-circuit conditionals recurses one frame per nesting level with no bound. A sufficiently deep chain overflows the stack — an **uncatchable abort** (`catch_unwind` does not catch a stack overflow), taking the whole engine down during cell load. Same bug class as the already-fixed #1729, one pass upstream.

**Related**: #1729 (the control-flow-pass sibling, fixed); SCR-D5-NEW-02 (#1816).

**Suggested Fix**: Thread a `depth: usize` through `BoolPass::rebuild` (and `collapse`, which calls back into `rebuild`), return `DecompileError::RecursionLimit` past the same `MAX_REBUILD_DEPTH` cap the control-flow pass uses. Add a pathological-nesting regression test mirroring `control_flow`'s `rebuild_rejects_excessive_recursion_depth`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other decompiler tree walks — `control_flow.rs`, `lift.rs`, `.psc` parser stmt/expr)
- [ ] **TESTS**: A regression test pins this specific fix
