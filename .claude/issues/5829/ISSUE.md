# TD3-2026-10-15-05: `feature-matrix.md` omits the Papyrus VM, marks Story Manager payload and per-material footsteps as not started

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-05`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:158, 185, 207, 343`
- **Status**: NEW
- **Age**: Papyrus VM `0c6fe4d34`..`195e3b4ef` (2026-10-12/13); Story Manager Phases 1-4 (2026-10-07/08, ROADMAP:286); footsteps `886edcded` (2026-10-10, ROADMAP:366 "implemented in full")
- **Effort**: small
- **Description**: (a) Row 185 "Full Papyrus transpiler (M47.2) ... full transpiler deferred" and row 343 "Arbitrary script execution on real content (`.pex` recognizer slice shipped Session 51)" never mention the interpreter that now runs what the lowering declines (`fragment/vm.rs`, per-frame budget #5763, reference/alias/update/event scripts); the word "VM" does not appear in the file. ROADMAP:75 and :195 say it exists. (b) Row 207 "Story Manager event payload / search ✗" is the file's only Story Manager line; Phases 0-4 shipped (SMBN/SMEN/SMQN decode, dispatcher, R1/R2/L1/L2 payload slots, policy bits, KILL/CLOC/AHEL producers, `sm1-story-manager.sh`). (c) Row 158 "Per-material footsteps (FOOT records) ✗" — the selection mechanism (`select_footstep_sound`, `PhysicsWorld::surface_material_under`) shipped; the table is empty pending IPDS decode, so "~ mechanism shipped, no authored data" is the accurate cell.
- **Evidence**: `grep -n "Story Manager\|VM\b" docs/feature-matrix.md`; `sed -n 281,330p byroredux/src/systems/audio.rs`.
- **Impact**: the doc the audits treat as a floor under-states shipped work in the highest-churn subsystem.
- **Related**: #5366, #5367, TD3-2026-10-11-04 (#5600, same class), #5763.
- **Suggested Fix**: add a "Papyrus VM (bounded, decline-fallback)" row and an SM row, change row 207 to the remaining gap (SCPT/LEVL/SKIL producers), re-mark 158 as `~`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
