# 3279: SCR-D5-2026-08-24-02: Effect::Conditional's lower_statements recursion has no explicit depth cap

**Severity**: LOW · **Report**: `docs/audits/AUDIT_SCRIPTING_2026-08-24.md` (SCR-D5-2026-08-24-02)

## Description

The new `Effect::Conditional` lowering path recurses into `lower_statements` once per level of nested `If`, with no local depth counter — unlike every other recursive pass in this domain (`MAX_REBUILD_DEPTH = 1024` in `control_flow.rs`/`boolean.rs`; `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256` in the papyrus parser). Not independently unbounded: transitively bounded by two already-tested upstream caps in both reachable input paths (`.psc` via `MAX_STMT_DEPTH`, `.pex` via `MAX_REBUILD_DEPTH`).

## Location

`crates/scripting/src/translate/effects.rs:301-387` (`lower_statements`, `Stmt::If` arm's recursive calls at `:358`/`:361`)

## Impact

Defense-in-depth gap, not independently exploitable. If either upstream cap is loosened without re-deriving this function's own stack budget, this becomes the first place that finds out via a crash rather than a bounds-checked `Err`.

## Suggested Fix

Thread an explicit `depth: u32` parameter through `lower_statements` capped at or below the smaller of the two upstream caps, returning `None` past it, plus a regression test analogous to `stmt_depth_cap_rejects_pathological_nested_if`.

## Completeness Checks
- [ ] **TESTS**: A regression test analogous to `stmt_depth_cap_rejects_pathological_nested_if`
