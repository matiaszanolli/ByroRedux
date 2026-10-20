# TD3-2026-10-19-04: `docs/engine/architecture.md` says the workspace is "27 members"; it is 34

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: NEW
- Location: `docs/engine/architecture.md:17-22` (count + tree)
- Evidence: `Cargo.toml` members = 34 (29 crates + `byroredux` + 4 tools; ROADMAP `:716` states the same). The doc's tree omits crates `menuxml`, `game-detect`, `boot-request`, `settings-io`, `sdk` and tools `byro-detect`, `byro-launcher`; it says "two tools".
- Proposed fix: update the sentence and tree (copy the roster from `CLAUDE.md:300-372`), or replace the count with "see ROADMAP Project Stats".

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
