# #2729: ROADMAP.md's M48 row still lists input routing as remaining work 16 days after it shipped

- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `ROADMAP.md:628`, `docs/feature-matrix.md:160-170`
- **Status**: NEW
- **Description**: `ROADMAP.md:628` ends "Remaining work: method behavior and
  `_global.gfx` stubs, font fidelity/**input**/menu lifecycle, and Papyrus/ECS ↔
  UI callbacks." Input routing landed in `3ea5e275` (2026-07-27, *feat(ui):
  implement input routing for Scaleform menus with winit integration*), shipping
  `crates/ui/src/input.rs`, `byroredux/src/ui_input.rs`, focus transfer, modal
  capture ahead of world controls, and window→movie coordinate scaling.
  `docs/engine/ui.md` documents all of it as shipped. `ROADMAP.md` was edited as
  recently as 2026-08-11 without the row being reconciled. Separately,
  `docs/feature-matrix.md`'s UI table has six rows and none of them mentions
  input/focus routing, so the matrix under-reports the subsystem.
- **Impact**: Exactly the failure the skill's Dim 3 recipe targets — "flag any
  row whose status contradicts the crate that implements it." A reader planning
  M48 work would re-scope a slice that is already done.
- **Related**: Verified the *counts* in the same rows are fine — `ROADMAP.md:627-628`
  and `feature-matrix.md:166-168` both say 74/138, matching `catalog.rs`. Only
  the remaining-work list is stale. Same class as #2416 (`feature-matrix.md`
  stale `hkx` rows, still OPEN).
- **Suggested Fix**: Drop "input" from the `ROADMAP.md:628` remaining-work list
  (keep font fidelity, menu lifecycle, `_global.gfx`, Papyrus↔UI — those are
  genuinely open) and add a `Scaleform menu input routing + modal focus | ✓ M48`
  row to `docs/feature-matrix.md`'s UI table.
- **Effort**: trivial

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD3-2026-08-12-02`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

