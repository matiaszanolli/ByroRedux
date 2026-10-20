# TD5-2026-10-19-01: two untracked `TODO`s in `loading_screen.rs` name unbuilt LSCR work with no issue (watch item, crosses 30 days on 2026-11-01)

Labels: low,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 5 Stale Markers
- **Location**: `byroredux/src/loading_screen.rs:10` ("Full legacy XML/NIF menu animation, TRNS \"Around Origin\" semantics, ONAM/ZNAM interactive zoom bounds and the MOD2 camera-path NIFs remain TODO"), `:46` ("authored camera path NIFs, MOD2, are a separate TODO")
- **Age**: `e609118644`, 2026-10-02 (17 days; under the 30-day triage floor, reported because it is the only real production marker and names no issue)
- **Effort**: trivial
- **Status**: NEW
- **Description**: These are the only two genuine TODOs in the tree. They are not tied to an issue or ROADMAP row (`rg "MOD2" ROADMAP.md` and an issue-title search return nothing), so when they age past six months nothing will say whether the work is wanted. Not reachable from a broken path: the LSCR cover renders with a fixed 0.3 rad elevation (`STAGE_CAMERA_ELEVATION_RAD`).
- **Evidence**: `git blame -L10,10 -L46,46` -> `e609118644` 2026-10-02.
- **Proposed fix**: File one tracking issue for the four remaining LSCR items and cite it as `(#N)` in both comments, or note them in the P6 ROADMAP row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
