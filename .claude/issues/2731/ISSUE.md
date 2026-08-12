# #2731: byroredux/src/main.rs crossed 2000 LOC, taking the oversized-file set from 9 to 10

- **Severity**: LOW
- **Dimension**: 1 (File / Function / Module Complexity)
- **Location**: `byroredux/src/main.rs` (2054 LOC)
- **Status**: NEW
- **Description**: 1958 → 2054 LOC across the 86-commit window since the
  08-07 report, crossing the 2000-LOC Session-34 split threshold. The other
  nine members of the oversized set are unchanged in membership. Roughly 60 of
  those lines are `#[cfg(test)]` (`:1991`, `:2034`), so this is genuine
  production growth, unlike the test-bulk crossings tracked as TD1-004
  (`save_io.rs`) and TD1-009 (`vulkan/material.rs`).
- **Impact**: Standard oversized-file tax. `main.rs` is the file every new
  system-registration or event-loop change touches, so it is a merge-conflict
  hotspot as well as a review-cost one.
- **Related**: TD1-001..012 in `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`
  (the nine-file set); this is the tenth. Open siblings #2410 (TD1-007,
  `cell_loader/spawn.rs`).
- **Suggested Fix**: Split along the axis the skill already prescribes for this
  file — `App`/`ApplicationHandler` winit event loop vs. system registration vs.
  boot/config. The repo already has `byroredux/src/boot.rs`, so the third of
  those has a landing site.
- **Effort**: medium

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD1-2026-08-12-01`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

