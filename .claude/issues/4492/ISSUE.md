# EXT-D7-2026-09-19-05: none of the four exterior harnesses runs in any automated lane

- **ID**: EXT-D7-2026-09-19-05
- **Labels**: medium,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4492

**Severity**: MEDIUM · **Dimension**: Acceptance harness · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-05)

**Location**: `.github/workflows/playable-smoke.yml:17-24` (gate choices: p0/p1/p2 only, games skyrim_se/fnv only); no workflow references any of the four scripts

**Description**
`m-exteriors.sh`, `m34-day-night.sh`, `w1-water-traversal.sh`, and `renderer-eval-groundcover.sh` are invoked by no workflow. The manually dispatched `Playable Smoke Gates` workflow exposes only the three playable gates and two games (despite fo3/fo4 fixtures existing). The four gates that see GPU output — the only gates that can, per skyal.md §4 "cargo test cannot see any of it" — run exactly when someone remembers.

**Impact**
An unrun harness is a silently passing one; the exterior matrix (WATR provenance, LOD coverage, terrain seams, env.health) can regress for weeks unnoticed even on a machine that has all the data.

**Related**: EXT-D7-2026-09-19-02 (the all-skip exit-0 bug makes an automated run's signal worse)

**Suggested Fix**
Add `w1-water-traversal` and an `m-exteriors static` job as gate choices to `playable-smoke.yml` (then the other modes; add fo3/fo4 game choices); schedule or dispatch the ground-cover eval on the game-data runner so references are re-minted on purpose.

## Completeness Checks
- [ ] **TESTS**: A workflow_dispatch dry run of each new gate choice on the game-data runner
