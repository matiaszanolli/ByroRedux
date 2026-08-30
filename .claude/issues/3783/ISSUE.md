# #3783 — SCR-D2-2026-08-30-01: a well-formed .pex within the wire format's own u16 ceiling aborts the process by stack overflow

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: high, scripting, safety, bug

---

**Audit**: `/audit-scripting` — `docs/audits/AUDIT_SCRIPTING_2026-08-30.md` (Dimension 2 — Decompiler CFG & Lift), HEAD `64f64480`
**Finding ID**: `SCR-D2-2026-08-30-01`

- **Severity**: HIGH
- **Status**: NEW
- **Untrusted-Input**: **Yes**

## Location

- `crates/pex/src/decompile/boolean.rs:158` — the trigger
- `crates/pex/src/decompile/node.rs` — the unbounded derived `Clone` / drop glue
- `crates/pex/src/decompile/lift.rs` — `rebuild_expression`, which builds the deep tree

## Severity rationale

The domain table rates "stack overflow via unbounded recursion in a decompiler tree walk" HIGH. A stack overflow is strictly worse than a panic: it is a `SIGABRT` that **`catch_unwind` cannot intercept**, so `translate_pex`'s panic guard (#1816 / #3287) is **bypassed entirely** and the graceful-degradation story does not apply.

This flips the domain's untrusted-input verdict from "no panic, no OOB, no OOM" to **"process abort"**.

## Mechanism

`rebuild_expression`'s copy-propagation nests each folded producer inside its consumer, so a chain of N temp-producing instructions (`::temp0 = a + b; ::temp1 = ::temp0 + b; …`) collapses into a single `Node` expression tree of depth N.

Nothing caps N except the wire format: a function's instruction count is a `u16` (max 65535), and the string table (also `u16`) has room for the ~40 000 distinct `::tempN` identifiers such a chain needs. **Every count stays inside the format's own ceilings — the input is well-formed, not malformed.**

`Node` derives `Clone` (`node.rs:21`) and its children are `Box<Node>`, so `Node::clone` (and the drop glue) recurse once per tree level with no cap. The first site to hit it is `BoolPass::rebuild`:

```rust
// crates/pex/src/decompile/boolean.rs:158
let scope = self.scopes.get(&current).cloned().unwrap_or_default();

let mut reprocess = false;
if block.is_conditional() && !scope.is_empty() {
```

That deep-clones the **entire** node scope of **every** block on **every** visit — *before* the `block.is_conditional() && !scope.is_empty()` test that is the only reason the clone exists. A straight-line function with no conditional blocks at all still pays the full deep copy, and blows the stack doing it.

## Reproduction (empirical, not inferred)

A temporary example built a `Pex` in memory with one function of N chained `iadd`s into `::tempK` plus a final `assign`, then called `decompile::decompile_script`. `crates/pex` only, no game data. Main thread, 8 MB stack:

| N | debug | release |
|---|---|---|
| 20 000 | OK | OK |
| 27 000 | OK | — |
| 30 000 | **abort** | — |
| 40 000 | abort | **abort** |
| 65 000 | abort | abort |

Phase isolation at N = 40 000 / 65 000:
- `build_cfg` + `lift_function` (including `rebuild_expression`, `count_constant_id`, `replace_constant_id`, and the deep tree's drop): **survives at 65 000** — those hand-written walks have small frames.
- `scopes.get(&0).cloned()` **alone**: OK at 30 000, **aborts at 40 000**. This isolates the failure to the derived `Clone`, not to any hand-written walk.
- The full pipeline aborts inside `rebuild_boolean_operators`, before its first progress print — consistent with line 158.

Exit status 134 (`SIGABRT`) with `fatal runtime error: stack overflow`. **No unwinding.**

Re-verified at HEAD: `boolean.rs:158` still `self.scopes.get(&current).cloned().unwrap_or_default();` ahead of the `is_conditional()` test at `:161`; `node.rs:21` still `#[derive(Debug, Clone, PartialEq)]`.

## Blast radius

`.pex` bytes reach `decompile_script` from a user/mod-supplied archive via `--scripts-bsa` → `ScriptProvider::extract_pex` → `translate_pex` (`byroredux/src/asset_provider/script.rs:279`, `cell_loader/references/attach.rs`) and via `populate_quest_fragments_from_pex`.

**One hostile or corrupt `.pex` in a mod archive kills the engine at cell load with no diagnosable error.** No vanilla script approaches 40 000 instructions in one function, so this is a robustness/untrusted-input defect, not a compatibility one.

The 8 MB figure is the **main** thread. Rust's default for a `thread::Builder` without an explicit `stack_size` is 2 MB, and no call site in this repo sets one (`grep stack_size` finds only `streaming.rs` and test files, none of which set it), so any future move of cell-load work onto a worker thread lowers the threshold by roughly 4x — to ~10 000, still far inside the wire format.

## Suggested Fix

Two independent, cheap pieces:

1. **Cap the tree at its source.** Thread a nesting depth through `rebuild_expression`'s fold loop and return a new `DecompileError::ExpressionTooDeep` past a bound comfortably above real Papyrus. The `.psc` frontend's `MAX_EXPR_DEPTH` is 256, and this is the *same quantity* arriving through the other frontend, so matching it is defensible without guessing. This is the structural fix — it also protects `lower_expr`'s recursion and every downstream consumer that walks the emitted `ast::Expr`, including `translate/effects.rs`.
2. **Drop the gratuitous clone.** `boolean.rs:158` should test `block.is_conditional()` (and the scope's emptiness / `last_result` shape) *before* cloning, or borrow rather than clone. Independently of the stack, it is a full deep copy of every block's expression trees on every visit, and once more per `reprocess` re-visit.

**Regression guards**: a `#[test]` building an N-chain (N a few thousand, under the new cap) asserting a clean `Err`; and a `#[test]` asserting the boolean pass does not clone a non-conditional block's scope.

## Related

- #1816 / #3287 (the `catch_unwind` panic guard in `translate_pex` that this bypasses)
- #3279 (`Effect::Conditional`'s `lower_statements` recursion has no explicit depth cap — the sibling uncapped recursion, one layer up in `crates/scripting`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other derived `Clone` over a `Box`-recursive tree in `crates/pex/src/decompile/`, and `ast::Expr`'s own clone/drop glue in `crates/papyrus`
- [ ] **TESTS**: A regression test pins this specific fix — a deep-chain `.pex` must return `Err`, not abort
