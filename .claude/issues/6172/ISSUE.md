# TD3-2026-10-19-12: Gamebryo 2.3 source path points at a mount that no longer exists

Labels: low,legacy-compat,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial (CLAUDE.md is user-owned) · Status: NEW
- Locations: `CLAUDE.md:418`, `docs/legacy/key-files.md:4`, `docs/legacy/gamebryo-2.3-architecture.md:3`
- Evidence: `/media/matias/Respaldo 2TB/...` does not exist (`/media/matias/` holds `Respaldo 4TB`, `ROMS`); the tree is at `/run/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/` (the user's memory notes the same move, 2026-04-19). Reference copies of v2.6/v3.2 are under `/mnt/data/src/reference/`.
- Proposed fix: state the path as `/run/media/matias/Respaldo 2TB/...` (or "mount point varies; look under /run/media/$USER") in all three.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
