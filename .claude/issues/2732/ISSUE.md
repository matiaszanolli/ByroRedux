# #2732: Four new #[allow(dead_code)] in interaction.rs; the enum-level one is broader than needed

- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `byroredux/src/interaction.rs:34`, `:85`, `:139`, `:148`
- **Status**: NEW (tracking)
- **Description**: The repo-wide `allow(dead_code)` count moved 48 → 52 since
  08-07; all four additions are here. Three are the **correct narrow form**
  (`#[cfg_attr(not(test), allow(dead_code))]` on `bind_key`, `is_held`,
  `was_released` — used by tests, not yet by production). The fourth is a
  blanket `#[allow(dead_code)]` on the whole `InputAction` enum, justified
  inline as "Mouse/gamepad sources for these declared actions land next"; four
  of its eleven variants (`Attack`, `Block`, `Inventory`, `Pause`) have no
  producer yet.
- **Impact**: None today. Flagged so the next sweep can tell whether the
  enum-level allow outlived its stated driver: once mouse/gamepad sources land,
  it should be deleted rather than inherited.
- **Suggested Fix**: No action now. On the next audit, check whether the
  gamepad/mouse source work has landed; if it has and the attribute survives,
  it becomes a real finding.
- **Effort**: n/a (tracking only)

---

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD8-2026-08-12-04`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

