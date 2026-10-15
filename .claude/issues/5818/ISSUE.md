# TD2-2026-10-15-17: The new tree-LOD parser re-grew the unchecked LE reader that #5596 centralised three days earlier

Labels: low,terrain-exterior,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-17`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `byroredux/src/cell_loader/tree_lod.rs:111-123` (`u32_at`, `f32_at`); the sibling `placement_lod.rs:109-116` already wraps `byroredux_core::bytes::le_u32_at`
- **Status**: NEW (regression of the #5596 class; #5596 CLOSED `cce3d59b5` 2026-10-11)
- **Age**: `a9384f9c0` (#4913, 2026-10-14)
- **Effort**: trivial
- **Description**: `bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0],b[1],b[2],b[3]]))` - the exact shape `core::bytes` replaced, including the unchecked `at + 4` its module doc cites ("offset near `usize::MAX` is a miss rather than an overflow"). `from_le_bytes` appears nowhere else in `byroredux/src` after #5596.
- **Evidence**: `grep -rln from_le_bytes byroredux/src` = `tree_lod.rs` only.
- **Impact**: `at` comes from file-declared counts, so a crafted `.btt` can panic in debug; same class #5596 closed.
- **Related**: #5596.
- **Suggested Fix**: replace both bodies with `byroredux_core::bytes::le_u32_at(bytes, at).ok_or(TreeLodError::Truncated { needed: at.saturating_add(4), have: bytes.len() })`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
