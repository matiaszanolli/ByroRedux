# #3279 — SCR-D5-2026-08-24-02: Effect::Conditional's lower_statements recursion has no explicit depth cap, unlike every sibling recursive pass in this domain

**Severity**: LOW · **Dimension**: Scripting
**Location**: `crates/scripting/src/translate/effects.rs::lower_statements`

## Fix

Applied the issue's own suggested fix: threaded an explicit `depth: u32`
parameter through `lower_statements`, mirroring `stmt_depth`'s pattern in
`crates/papyrus/src/parser/stmt.rs`. Added `MAX_CONDITIONAL_DEPTH: u32 =
256` — the smaller of the two upstream caps this recursion was
transitively (but not independently) bounded by (`MAX_STMT_DEPTH = 256`
for `.psc`-sourced input, `MAX_REBUILD_DEPTH = 1024` for `.pex`-sourced
input, this feature's actual target). `lower_statements` now declines
(`None`) at `depth >= MAX_CONDITIONAL_DEPTH` instead of recursing further,
and the `Stmt::If` arm's two recursive calls (`then_effects`/
`else_effects`) pass `depth + 1`.

Confirmed the issue's premise still held before implementing: `grep -n
"depth" effects.rs` still returned nothing in `lower_statements` at HEAD.

## TESTS (issue's own checklist item)

Added tests analogous to `stmt_depth_cap_rejects_pathological_nested_if`:
`conditional_depth_cap_declines_pathological_nested_if` and
`conditional_depth_cap_accepts_legitimate_nesting`. Both build a nested
`If Self.GetStageDone(0) ... EndIf` chain **directly from AST nodes**
(`Stmt::If { .. }`, not `.psc` source text) — the papyrus parser's own
`MAX_STMT_DEPTH` would reject a chain deep enough to exercise this cap
before `lower_statements` ever saw it, which would prove nothing about
this recursion's *own* bound. The whole point of a defense-in-depth guard
is that it holds even if the upstream cap it currently rides on is ever
loosened, so the test constructs past that upstream limit on purpose.

Each level's condition is a bare `Self.GetStageDone(N)` — the minimal
shape `classify_guard_atom`'s `prim_stage_done` primitive accepts (a bare
call used as a boolean, `== 1` implied) — so every level of the synthetic
chain classifies as a real `StageDoneGuard` and the recursion actually
reaches the deep levels instead of declining immediately at the outermost
guard-classification step.

**Reintroduce-and-revert verification**: two separate probes.
1. Temporarily removed the `if depth >= MAX_CONDITIONAL_DEPTH { return
   None; }` check entirely — confirmed
   `conditional_depth_cap_declines_pathological_nested_if` failed,
   producing `Some(...)` (a fully-lowered, deeply nested `Effect::
   Conditional` structure) instead of the expected `None`.
2. (Discarded first attempt, noted for completeness): bumping the
   `MAX_CONDITIONAL_DEPTH` constant itself doesn't independently verify
   anything, since the test's chain depth is derived from that same
   constant (`MAX_CONDITIONAL_DEPTH * 2`) — it will always exceed
   whatever the constant is set to. The check-removal probe above is the
   one that actually exercises the guard.

Restored the fix and reran — both new tests pass again.

## Verification

- `cargo check -p byroredux-scripting --tests`: clean, zero warnings.
- `cargo test -p byroredux-scripting --lib translate::effects::tests::conditional_depth`:
  2 tests passing, 0 failing (both new).
- `cargo test -q -p byroredux-scripting`: 411 tests passing (+2), 0
  failing.
- `cargo test -q --no-fail-fast` (full workspace): **7151 passing, 0
  failing**.
