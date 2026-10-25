# TD4-2026-10-25-06: `audit-tech-debt` "Recent yield: Dims 4, 3, 1, 8 dominate (then 2, 9)" contradicts the reports it cites

Labels: low, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6386
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:~108-110` ("Recent yield (`grep -h '\*\*Dimension\*\*' …`)")
- **Evidence**: counting `**Dimension**` leading digit over all 40 `AUDIT_TECH_DEBT_*.md`: Dim 2 = 106, 1 = 104, 3 = 81, 8 = 60, **4 = 54**, 7 = 33, 9 = 30, 6 = 23, 5 = 8. Last 5 reports: 2 = 63, 3 = 28, 1 = 26, 7 = 18, 4 = 17, 8 = 17, 9 = 15. Dim 2 is the top yield (the skill ranks it fifth); Dim 4 is fifth, not first; Dim 7 outranks 9. ("5 and 6 rarely surface real debt" holds.)
- **Suggested Fix**: "Dims 2, 1, 3 dominate (then 8, 4; 7, 9)", or drop the sentence — it steers agent ordering/effort.


#### Dimension 5 — Stale Markers (0)

No findings. See Checked-clean.


#### Dimension 6 — Stubs & Placeholders (2)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD4-2026-10-25-06`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
