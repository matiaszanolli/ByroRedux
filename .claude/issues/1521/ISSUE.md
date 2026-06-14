# ECS-DOC-01: audit-ecs dim-5b claims AccessConflict::Parallel — code returns Unknown (no Parallel variant)

**Severity**: LOW (doc rot)
**Dimension**: ECS audit dim 5b — M27 scheduler access declarations
**Source**: docs/audits/AUDIT_ECS_2026-06-14.md
**Status**: NEW

## Description
`.claude/commands/audit-ecs/SKILL.md` (dimension 5b) states that under #1394 undeclared-access systems are "classified as `AccessConflict::Parallel` — NOT `Unknown`" and that `unknown_pair_count()` "will return 0 when all undeclared pairs are now tagged Parallel." This does not match the code.

The `AccessConflict` enum (`crates/core/src/ecs/access.rs:139-152`) has only `None`, `Unknown`, `Conflict` — there is **no** `Parallel` variant. `analyze_pair` returns `Unknown { left_undeclared, right_undeclared }` for any undeclared side, and `scheduler.rs` test `undeclared_closure_pairs_show_as_unknown` (`crates/core/src/ecs/scheduler.rs:1100`) asserts two undeclared closures yield `unknown_pair_count() == 1`.

`git show a7e1502b` (the #1394 commit) added the `undeclared_parallel_count()` accessor — that is the actual "undeclared-parallel guard," not a reclassification of pairs to `Parallel`.

## Impact
None to runtime. A future audit trusting the 5b wording would look for a nonexistent variant. The underlying behavior (undeclared pairs remain visible as `Unknown`) is correct.

## Suggested Fix
Edit `.claude/commands/audit-ecs/SKILL.md` dim-5b (and the matching scheduler note in the auto-memory) to say undeclared pairs classify as `AccessConflict::Unknown`, and that #1394 added `undeclared_parallel_count()` as the migration KPI driven toward 0.

## Completeness Checks
- [ ] **SIBLING**: same `AccessConflict::Parallel` claim mirrored in the project auto-memory (`command_system` / scheduler note) — fix both
- [ ] **TESTS**: no code change; existing `undeclared_closure_pairs_show_as_unknown` already pins the real behavior
