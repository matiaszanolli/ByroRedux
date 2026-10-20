# TD3-2026-10-19-06: CLAUDE.md smoke-test list is incomplete ("Currently:" names 19 of 35 scripts)

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial (user-owned file) · Status: NEW
- Location: `CLAUDE.md:51-110` vs `docs/smoke-tests/`
- Evidence: on disk, absent from CLAUDE.md: `interior-godrays.sh m34-day-night.sh m41-ragdoll.sh m43-quest-runtime.sh m47-3-obscript.sh m47-triggers.sh m48-5-fnv-hud.sh m-exteriors.sh m-trees.sh p0-door-interaction.sh p1-character-traversal.sh p2-melee-core.sh p3-hud.sh p3-player-body.sh p5-save-restart.sh r6a_stale_15_bench.sh` (16). Every script on disk IS in `docs/smoke-tests/README.md` (checked), and everything CLAUDE.md names exists.
- Proposed fix: replace the enumeration with "see `docs/smoke-tests/README.md` (every script is indexed there)" and keep only the pattern description; avoids the list drifting again.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
