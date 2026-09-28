# #4985 — ECS-2026-09-28-D1-04: Dim 1's `First step:` runs the detector on core only, where the graph is green

Filed 2026-09-28 via `/audit-publish docs/audits/AUDIT_ECS_2026-09-28.md`. Snapshot as filed; GitHub is authoritative for live state.

**Source**: `docs/audits/AUDIT_ECS_2026-09-28.md` (HEAD `21319618c`)

- **Severity**: LOW · **Dimension**: skill drift (audit infrastructure)
- **Location**: `.claude/commands/audit-ecs/SKILL.md` — Dim 1 `First step:`
- **Status**: NEW

`/audit-ecs` Dim 1 names `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-core`, which passes with
778 tests. Every cycle in this report lives in the binary's graph (`-p byroredux`). A delta-scoped
run that skips Dim 1 because its paths had no commits, as this one would have, never sees the lane
go red. The step should add `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux`, or check the CI
`ABBA lock-order detector` conclusion on the last main run
(`gh run view <id> --json jobs`). The "Paths unchanged → skim" rule is wrong for this one gate: the
graph changes whenever *any* caller changes.

## Completeness Checks
- [ ] **SIBLING**: `/audit-concurrency` Dim 3 first step checked for the same core-only scoping
- [ ] **VALIDATE**: `.claude/commands/_audit-validate.sh` passes after the skill edit
