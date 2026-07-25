# TD3-NEW-04: _audit-common.md's crate count (21) and Project Layout table omit crates/fsr3-sys

**Labels**: medium, tech-debt, documentation
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD3-NEW-04)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2194

## Severity
MEDIUM

## Dimension
3 (Stale Documentation & Comments)

## Location
`.claude/commands/_audit-common.md:1-90` (Project Layout table, "Crate count: 21" line)

## Description
`_audit-common.md` states "Crate count: 21 under `crates/`" with an explicit enumerated list. `crates/fsr3-sys` (added by commit `c4b070a7`, 2026-07-22) is not in that list, nor are the three new renderer files that consume it (`frame_upscaler.rs`, `exposure.rs`, `upscaling.rs`) or the vendored `third_party/fidelityfx-sdk-v1.1.4/` tree. ROADMAP.md already correctly says 22.

## Evidence
`find crates -maxdepth 1 -type d | wc -l` → 22 subdirectories (confirmed live, incl. `fsr3-sys`); `_audit-common.md:103`'s crate list predates the crate's addition.

## Impact
`_audit-common.md` explicitly instructs "an audit that never touches a relevant crate here is incomplete" — a future audit would never know `fsr3-sys` needs auditing at all.

## Related
TD3-NEW-02 / Existing #2162 (same root cause — FSR work outpacing doc sync, though #2162 covers a different specific location in the same file — the `VulkanContext` file-listing row, not this crate-count line).

## Suggested Fix
Add an `FSR3:` row to the Project Layout table, bump "Crate count: 21" to 22, update `audit-tech-debt/SKILL.md`'s "21-crate roster" phrase.

## Completeness Checks
- [ ] **SIBLING**: Check other skill files repeating the "21 crates" figure in prose
- [ ] **TESTS**: N/A — documentation-only fix
