# TD3-2026-10-19-02: ROADMAP Known Issues lists two fixed items as open (one is an open "HIGH")

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: NEW
- Locations: `ROADMAP.md:646-649` (ReSTIR claim reads a `writeonly` binding, #5765, HIGH), `ROADMAP.md:650-653` (FO76 `GeneratedMeshes` truncation tail); inconsistency also at `ROADMAP.md:720` ("re-measured 2026-09-29") vs `:219`/`:719` ("2026-10-03")
- Evidence: #5765 CLOSED 2026-10-15; `a03cf9174` added the `ReservoirClaimBuffer` (binding 22); `triangle.frag:4233` only stores `reservoirsCurr`; `bindings.glsl:613-618` and `shader-pipeline.md:~742` describe the fix. FO76: `crates/nif/tests/parse_real_nifs.rs:555-558` ("#4628 ... floors were re-tightened from 0.945/0.0"), #3461 and #4628 CLOSED, `ROADMAP.md:219` already says 102 980 clean. The item's own "then raise the 0.0 floor" is done.
- Proposed fix: delete both checkboxes (move #5765 to a Completed one-liner); change `:720` date to 2026-10-03.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
