# TD3-001: feature-matrix.md M45 + M47.2 rows read "unstarted" for shipped milestones

Issue: #1699 · Labels: medium, tech-debt, documentation
Source: docs/audits/AUDIT_TECH_DEBT_2026-06-23.md

**Severity**: MEDIUM
**Dimension**: 3 (Stale Documentation)
**Location**: `docs/feature-matrix.md` (M47.2 transpiler row + M45 save/load gaps row)

## Description
The feature matrix carries two status rows that read "unstarted" for milestones that have **shipped**. The "Scripting (M47)" table row "Full Papyrus transpiler (M47.2)" reads `✗ Foundation done; transpiler unstarted`, and the "What Doesn't Work Yet" gaps table row "Save / load (M45)" reads `M45 (unstarted)`. Both M45/M45.1 (save/load) and the M47.2 compiled-`.pex` recognizer slice have landed.

## Evidence
- `crates/save/src/{snapshot,registry,disk,validate,driver,lib}.rs` exist; commits `bd2d0de2 feat(save): M45 — full-ECS-snapshot save/load` and `48e18c4f feat(save): M45.1 — live load-apply` landed.
- `crates/pex/src/` (opcode/reader/model/decompile) exists; commits `fcd46e90 feat(scripting): wire VMAD .pex scripts through the recognizer at cell load (M47.2)`, `92560525 test(m47.2)`, `f1a00e89 feat(cell): M47.2 script-attach summary` landed.
- `_audit-common.md` already flags this matrix's "Scripting (M47)" + "Save / load (M45)" rows as lagging the code.

Verified against current `docs/feature-matrix.md`: the M47.2 row still reads `✗ Foundation done; transpiler unstarted`; the gaps table still lists `Save / load (M45) | Game sessions persist | M45 (unstarted)`.

## Impact
The matrix is the canonical at-a-glance per-game/scripting status surface. A reader — or the next `/audit-scripting` / `/audit-save` run — would conclude the work is unstarted and re-scope finished milestones. Promoted to MEDIUM under the severity table ("stale doc baseline that would misdirect the next audit").

## Suggested Fix
- M47.2 row → `✓ .pex recognizer slice (CFG→lift→control-flow→lower→short-circuit); full transpiler deferred`.
- M45 row → `✓ full-ECS-snapshot save + M45.1 live load-apply`.
- Reconcile the "What Doesn't Work Yet" table (see TD3-002).

## Completeness Checks
- [ ] **SIBLING**: Other status surfaces (ROADMAP.md compat matrix) checked for the same M45/M47.2 lag
- [ ] **TESTS**: N/A (doc-only change)
