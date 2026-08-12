# #2730: docs/engine/ui.md undercounts the executable's winit-translation tests

- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/ui.md` ("Tests" section)
- **Status**: NEW
- **Description**: "The UI crate has 16 default tests plus three ignored
  installed-corpus smokes; the executable adds **three** winit-translation
  tests." The crate half is exactly right (verified: 16 passed, 3 ignored). The
  executable half is not — `byroredux/src/ui_input.rs` has **four** `#[test]`s.
- **Impact**: Trivial in isolation. Filed because this doc's test-count
  paragraph is otherwise precise enough to be used as a baseline, and a
  known-wrong number in it devalues the rest.
- **Suggested Fix**: "three" → "four".
- **Effort**: trivial

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD3-2026-08-12-03`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

