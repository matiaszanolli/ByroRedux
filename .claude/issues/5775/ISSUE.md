# REN-D5-2026-10-14-01: `memory-budget.md` says the Sky and Ground Cover rows total "About 19.8 MB"; they sum to ~154.6 MB

**Labels**: low,memory,documentation,doc-rot
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D5-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `docs/engine/memory-budget.md` (Sky and Ground Cover table, sentence "About 19.8 MB together, flat across resolutions.")

## Description
The four rows sum to about 154.6 MB and the rough-budget row on the same page already says ~155 MB. The ledger test `memory_budget_ledgers_the_sky_and_ground_cover_owners` pins each row, not this sentence.

## Evidence
`memory-budget.md:1107`.

## Impact
Doc rot; misleads VRAM budgeting.

## Suggested Fix
Fix the sentence, or compute it from the pinned rows.
