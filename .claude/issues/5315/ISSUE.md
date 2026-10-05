# #5315: TD3-2026-10-05-02: `draw.rs` carries a truncated, orphaned doc fragment above `draw_frame_size_budget_tests`

Labels: low,tech-debt,documentation,doc-rot,renderer
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD3-2026-10-05-02) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1304-1305`
- **Status**: NEW
- **Age**: lines from `a60a01534` (2026-07-04). They were orphaned when `e17b4b653` (2026-09-23) moved
  `next_clean_skin_frames` / `should_skip_skin_gpu_refresh` (now `frame_params.rs:1697-1720`, which carries its
  own complete `D6-04 / #1811` docs).
- **Effort**: trivial
- **Evidence**:
  ```rust
  /// Regression for D6-04 / #1811. `next_clean_skin_frames` /
  /// `should_skip_skin_gpu_refresh` gate the bone_world upload + device
  #[cfg(test)]
  mod draw_frame_size_budget_tests {
  ```
  A tree-wide scan for a `///` line that ends mid-sentence immediately before `#[cfg(test)]` finds only this one.
- **Impact**: rustdoc attaches a half-sentence about skin refresh to the size-budget test module. The next
  reader looks for a #1811 regression test that is not there.
- **Suggested Fix**: delete the two lines.
- **Related**: GAME-D1-2026-10-05-03 and UI-D5-2026-09-29-01 / #5029 (the same "doc comment spliced onto the
  wrong item" class).

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
