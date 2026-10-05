# #5324: TD4-2026-10-05-02: Two skill cross-references rotted: audit-parsers routes HKX playback to a nonexistent `/audit-scripting` Dim 8, and audit-speedtree tracks an open question on closed #3740

Labels: low,tech-debt,documentation,doc-rot
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD4-2026-10-05-02) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-parsers/SKILL.md:10`; `.claude/commands/audit-speedtree/SKILL.md:137`
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - **audit-parsers.**
    - `/audit-scripting` has Dimensions 1–7.
    - Cinematic, root-motion and completion-event playback is Dim 5, "Scene / Package / Dialogue / Cinematic
      Playback".
    - A scan of every `/audit-x Dim N` cross-reference in `.claude/commands` finds only this one mismatch.
  - **audit-speedtree.**
    - "whether Oblivion should size from BNAM or MODB is an open format question (#3740)". #3740 was CLOSED on
      2026-08-31 for its comment half.
    - No open issue tracks the behaviour question, and `AUDIT_SPEEDTREE_2026-10-05` again declines to answer it.
- **Suggested Fix**:
  - Change "Dim 8" to "Dim 5".
  - Either open a research issue for BNAM-vs-MODB and cite it, or reword the line as a dated known-open fact
    with no issue number.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
