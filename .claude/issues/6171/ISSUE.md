# TD3-2026-10-19-11: `exterior-readiness-plan.md` reports `CellRootRefIndex` / `persistent_ref_index` as landed; both were deleted in #3884

Labels: low,terrain-exterior,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: NEW
- Locations: `docs/engine/exterior-readiness-plan.md:700-716` ("§7 step 1 landed"), `:1154` (`owned_by_persistent_cell`), `:1216-1219`, `:1288-1305` (`regn_active_entries`, `ai_package_rows` pieces)
- Evidence: `git log -S CellRootRefIndex`: deleted by `9bd432f8f` (2026-09-06, "Fix #3884, fix #3885: delete the dead single-root ref index and load_interior_cell"); `rg -w CellRootRefIndex crates byroredux` -> 0; `byroredux/src/components/navmesh.rs:17` already records "deleted in #3884". Also the plan's EX-16 items (#2737, #3299) are CLOSED.
- Proposed fix: add a one-line "superseded: index family deleted in #3884" under §7 step 1 and at `:1216`; the plan is historical, so no further rewrite.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
