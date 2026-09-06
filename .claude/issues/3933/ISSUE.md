# #3933 — SCR-D3-2026-09-06-01: #3783's `MAX_EXPR_DEPTH` cap is per-`rebuild_expression`-call — the control-flow and boolean passes re-fold already-folded trees with a fresh `vec![1; len]` ledger, so a well-formed `.pex` still drives `lower_expr` to a stack...

- **Finding ID**: SCR-D3-2026-09-06-01
- **Labels**: high,scripting,safety,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3933

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: HIGH (domain table: stack overflow via unbounded recursion in a decompiler tree walk; a `SIGABRT` bypasses `catch_unwind`)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: **Yes**
- **Location**: `crates/pex/src/decompile/lift.rs:401` (per-call ledger), `crates/pex/src/decompile/control_flow.rs:225` (whole-body re-fold), `crates/pex/src/decompile/boolean.rs:281-292` (`combine` + merged-scope re-fold), `crates/pex/src/decompile/lower.rs:88-120` (the recursion that aborts)
- **Status**: NEW (#3783 CLOSED — its fix is present and works for the single-block shape; this is the same failure class through a different, equally well-formed block shape)
- **Description**: `rebuild_expression` bounds nesting with a ledger initialised to `vec![1; len]` on the premise (`lift.rs:396-400`) that every freshly-lifted node has depth 1. True for the one call in `lift_function`, false for the two later ones: `Reconstructor::rebuild` splices every unconditional block's already-folded scope into `result` and re-folds (`control_flow.rs:225`) — a 256-deep tree from block *k* folds into block *k+1*'s 256-deep tree and the ledger records depth 2; `BoolPass::collapse` nests `left`/`right` under a new `BinaryOp` in `combine` with no depth check, then re-folds the merged scope with a fresh ledger, and the `reprocess` loop repeats once per `&&` link *iteratively*, so `MAX_REBUILD_DEPTH` never sees it. `lower_expr` recurses once per level.
- **Evidence** (Dim 3's scratchpad crate, re-run by the orchestrator on the 8 MB main thread; every count inside the wire format's `u16` ceilings):

  | Shape | Instructions | opt-level 0 | release |
  |---|---|---|---|
  | single block, N=1000 (#3783's own shape) | 1 002 | `Err: ExpressionTooDeep` | same — **the cap works within one block** |
  | `jmp +1` every 250 producers, N=1 000 | 1 005 | `Ok` | `Ok`, max `Expr` depth **1 001** |
  | same, N=20 000 | 20 081 | **SIGABRT** (exit 134) | `Ok`, depth **20 001** |
  | same, N=40 000 | 40 161 | **SIGABRT** | `Ok`, depth **40 001** (orchestrator-confirmed) |
  | same, N=63 000 | 63 251 | **SIGABRT** | **SIGABRT** (orchestrator-confirmed) |
  | left-assoc `&&` chain, 10 000 links | 20 002 | **SIGABRT** | `Ok`, depth 10 000 (orchestrator-confirmed) |

  `gdb`: every frame is `lower_expr` at `lower.rs:120`. Note the scratch crate's debug profile is `opt-level = 0`; the workspace `[profile.dev]` is `opt-level = 1` (`Cargo.toml:251`), so the workspace debug threshold lies between 20k and 63k instructions — the release abort at the ceiling and the 40 001-deep `Ok` tree are the load-bearing numbers.
- **Impact**: identical blast radius to #3783 — `.pex` from a `--scripts-bsa` archive reaches `decompile_script` via `translate_pex` and `populate_quest_fragments_from_pex`; one hostile/corrupt script kills the engine at cell load with no diagnosable error. Where release survives, a 20 000–40 000-deep `ast::Expr` reaches the recognizer chain, and `compose::split_and` (`compose.rs:143-155`, verified recursive on `And` by the orchestrator) recurses on exactly the `&&` shape — the abort moves downstream rather than disappearing. The #3783 commit's claim that the cap "also protects `lower_expr` and every downstream consumer" is not true as landed.
- **Disproof attempted**: boolean/reconstruct only move the tree (`take_scope`, `combine`), no cap; `MAX_REBUILD_DEPTH` bounds recursion into an operand range (depth 1 per link, returns), not the iterative `reprocess` chain; #3783's own shape confirmed still capped; `jmp +1`/`jmpf` sequences pass `checked_target` with no `DecompileError`; aborting frame confirmed via `gdb`. Dim 2 had reasoned the caps compose to "low thousands" — the empirical run refutes that reasoning.
- **Related**: #3783 (CLOSED; incomplete), #1816/#3287 (the net this bypasses), #2667, Dim 5 `split_and`
- **Suggested Fix**: make depth a property of the tree, not of one call — carry a memoised `depth` on `Node` (constructors set `1 + max(children)`; `replace_constant_id` and `combine` update it), seed `rebuild_expression`'s ledger from `scope[i].depth`, and check the cap in `combine` too. Give `lower_expr` its own defensive depth counter returning `ExpressionTooDeep`. Regression guards: `jmp +1`-split shape and `&&`-chain shape at a few thousand, both asserting a clean `Err`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
