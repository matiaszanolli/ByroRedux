# TD3-2026-10-19-09: `npc-spawn-ai-packages.md` describes eight PACK `active_*_location/_target` selectors that #3042 deleted

Labels: low,gameplay,ai,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: NEW (doc is the authoritative reference for `/audit-gameplay`)
- Locations: `docs/engine/npc-spawn-ai-packages.md:184-193` (lists `active_sandbox_location ... active_patrol_location`, `active_follow_target`, `active_escort_target` as "independent mirror groups"), `:366-370` ("dead code tracked as #3042")
- Evidence: #3042 CLOSED 2026-08-27; `rg active_follow_target crates byroredux` -> 0 hits; what remains is the `active_package_is!` macro generating the seven `active_package_is_*` test helpers (`crates/plugin/src/esm/records/misc/pack.rs:1040-1067`). No `*_location` / `*_target` fn exists.
- Proposed fix: rewrite both passages to "the seven `active_package_is_*` helpers (test-only macro)"; drop the location/target names and the "tracked as #3042" clause.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
