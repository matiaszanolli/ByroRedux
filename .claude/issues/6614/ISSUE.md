# RT-2026-10-31-07: Skyrim's P3 gates are unreachable (exit 2) although the fixture and every doc call them Skyrim's default

Labels: low,tech-debt,doc-rot,documentation,game:skyrim,game:fo4
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-07 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `fixtures/skyrim_se.env:24` and `fixtures/fo4.env:13` are unchanged; the Skyrim fixture still carries its nine `P3_*` lines.

- **Severity**: LOW
- **Dimension**: Harness integrity / doc-rot
- **Location**: `docs/smoke-tests/fixtures/skyrim_se.env` (`FIXTURE_GATES`), `fixtures/fo4.env`, `docs/smoke-tests/README.md:37,50`, the skill's gate table
- **Status**: NEW (regression from #5674, `19b6fa93d`, which introduced the explicit list)
- **Description**: `skyrim_se.env` still carries a full `P3_*` block (cell, quest 0x18B4B, vital colours) but `FIXTURE_GATES=(p0-door-interaction p1-character-traversal p2-melee-core p6-loading-model w1-water-traversal)` omits `p3-hud` and `p3-player-body`. `p3-player-body.sh` with no argument (documented as "Skyrim SE: player body attach + view toggle") exits 2 "no measured route for this gate". The skill says "the two P3 gates are Skyrim-only today" and that `p6-loading-model` covers "Skyrim + FO4"; the FO4 fixture declares only `p0 p2`, so `p6-loading-model.sh fo4` exits 2.
- **Evidence**: `/tmp/audit/runtime/gates/p3-player-body-skyrim_se.log`, `p6-loading-fo4.log`.
- **Suggested Fix**: Either add `p3-hud p3-player-body` (and, if measured, the `p5-*` gates) to the Skyrim list, or delete the dead `P3_*` block and correct the README and skill.

## Completeness Checks
- [ ] **SIBLING**: The same harness defect checked in the other gates of its family
