# SCR-D2-2026-09-11-01: MAX_EXPR_DEPTH and MAX_REBUILD_DEPTH are independent, uncorrelated caps feeding the same recursive tree walks

URL: https://github.com/matiaszanolli/ByroRedux/issues/4113
Labels: bug, low, scripting

- **Severity**: LOW (measured non-exploitable today; hygiene/hardening gap)
- **Dimension**: Decompiler CFG Construction & Opcode→Node Lift (audit-scripting)
- **Location**: `crates/pex/src/decompile/control_flow.rs:44` (`MAX_REBUILD_DEPTH = 1024`); `crates/pex/src/decompile/lift.rs:380` (`MAX_EXPR_DEPTH = 256`); `crates/pex/src/decompile/node.rs:379-384` (`Node::is_final` — an `IfElse`/`While` node is always "final" and never subject to the expression fold-time depth check on its own construction)
- **Status**: NEW (not a regression of #3933 — this composition existed identically before that fix; #3933 changed *how* expression depth is tracked, not whether `If`/`While` nesting depth shares a budget with it)

**Description**

Nothing checks an `If`/`While` node's nesting depth against `MAX_EXPR_DEPTH` (or any cap) at construction time; the only cap on `If`/`While` nesting is `MAX_REBUILD_DEPTH`, a call-stack recursion cap on `Reconstructor::rebuild` completely independent of `MAX_EXPR_DEPTH`. The actual worst-case tree depth reaching `Node`'s unbounded-recursion `Clone`, `count_constant_id`/`replace_constant_id`, and `lower_expr` is bounded by `MAX_REBUILD_DEPTH + MAX_EXPR_DEPTH ≈ 1024 + 256 = 1280`, not by either cap alone — comfortably below the measured SIGABRT threshold (~10,000 on the smallest realistic 2MB worker stack, ~30,000 on an 8MB main thread) but with a ~7.8x margin instead of an asserted one.

**Impact**

None today; latent coupling — raising `MAX_REBUILD_DEPTH` alone in the future (e.g. for deeply-nested Starfield/FO4 bodies) without revisiting `MAX_EXPR_DEPTH`'s adjacency could erode the margin unnoticed.

**Related**

#3933 (CLOSED, orthogonal — does not regress it); SCR-D4-2026-09-06-02 (CLOSED via #3945 — same "unenforced alignment between independently-declared depth constants" class, one level up)

**Suggested Fix**

Either add a `const _: () = assert!(MAX_REBUILD_DEPTH + MAX_EXPR_DEPTH <= SOME_DOCUMENTED_SAFE_BOUND);` tying the two, or give `lower_expr`/the `Node` tree walks a defensive depth counter that errors past a fixed absolute ceiling regardless of which upstream cap let the tree grow.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the composed worst-case bound (or the new `const` assertion) so a future change to either constant alone is caught at compile time

Source: `docs/audits/AUDIT_SCRIPTING_2026-09-11.md`
