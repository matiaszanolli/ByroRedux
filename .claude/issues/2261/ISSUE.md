# TD4-001: _audit-common.md's '22-crate roster' is stale — crates/hkx is missing, live count is 23

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2261

**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/_audit-common.md:120-126` (Crate count paragraph), `.claude/commands/audit-tech-debt/SKILL.md:21` ("the 22-crate roster")
**Status**: NEW

**Description**: `_audit-common.md` states "Crate count: 22 under `crates/`" with an explicit enumerated list. `crates/hkx` (`byroredux-hkx`, minimal Havok packfile reader added for the Session 62 M47.2 MQ101 cinematic slice) is absent from both the count and the list. `audit-tech-debt/SKILL.md` repeats the stale number in prose. `ROADMAP.md` is already correct: "Workspace members | 26 (23 crates + `byroredux` binary + 2 tools)".

**Evidence**: `find crates -maxdepth 1 -type d | wc -l` → 24 (23 subdirectories + the `crates` dir itself); live subdirectory list includes `crates/hkx` alongside the 22 already enumerated.

**Impact**: `_audit-common.md` explicitly says to use this list "as a coverage sanity check: an audit that never touches a relevant crate here is incomplete" — a future audit checking crate coverage against this list would not know `crates/hkx` exists or needs auditing (it also has no dedicated owner audit skill, same gap class as `fsr3-sys` noted in the very next paragraph of the same file).

**Related**: Same root-cause pattern as the already-fixed TD3-NEW-04 (07-25 report) — a new crate landing without a matching `_audit-common.md` update, recurring one cycle later for a different crate.

**Suggested Fix**: Add `hkx` to the enumerated crate list and its own Project Layout row (mirroring the `fsr3-sys` treatment); bump "Crate count: 22" to 23; update `audit-tech-debt/SKILL.md`'s "22-crate roster" phrase.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
