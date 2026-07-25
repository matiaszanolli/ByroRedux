# TD4-NEW-01: Path-validation gate is RED again — 9 stale backticked-path refs across 7 skill files

**Labels**: low, tech-debt, documentation
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD4-NEW-01)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2196

## Severity
LOW (batched trivial fixes; flagged distinctly because the gate itself is a regression from GREEN)

## Dimension
4 (Audit-Finding Rot)

## Location
`.claude/commands/audit-ecs/SKILL.md:213`, `.claude/commands/audit-fnv/SKILL.md:149,152`, `.claude/commands/audit-incremental/SKILL.md:70`, `.claude/commands/audit-oblivion/SKILL.md:173`, `.claude/commands/audit-skyrim/SKILL.md:122`, `.claude/commands/audit-starfield/SKILL.md:228`, `.claude/commands/audit-tech-debt/SKILL.md:114`, `.claude/commands/_audit-common.md:76`

## Description
`.claude/commands/_audit-validate.sh` fails with 9 stale refs (re-confirmed at publish time), from 3 file-split commits since 07-16: `misc/ai.rs` split (#2054) — 5 refs; `records/actor.rs` → `actor/{mod,tests}.rs` (#2055) — 2 refs (incl. `audit-tech-debt/SKILL.md`'s own Dim-1 text, which suggested this exact split in the 07-16 report as TD1-006); `blocks/shader_tests.rs` → `shader_tests/{...}.rs` (#2056) — 2 refs.

Note: this finding was NOT treated as a blocker for processing this report's other 8 findings during `/audit-publish`, per the report's own framing.

## Evidence
`.claude/commands/_audit-validate.sh` output: `FAIL: 9 stale path reference(s)` across the 8 files above.

## Impact
Every one of the 6 per-game/domain skills above now has at least one dead entry-point reference an agent following it literally would fail to `Read`.

## Related
Same failure class as the already-fixed 07-16 TD4 batch (TD4-001/002/004/005).

## Suggested Fix
Update each ref to its current path; re-run `.claude/commands/_audit-validate.sh` to confirm GREEN.

## Completeness Checks
- [ ] **SIBLING**: Re-run the validate script after the fix to confirm all 9 refs resolved
- [ ] **TESTS**: N/A — the gate script is itself the regression test
