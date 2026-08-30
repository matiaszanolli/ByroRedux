# #3604 — REN-2026-08-30-D11-03: `water.rs`'s module doc contradicts its own pipeline builder about which attachments water masks off

**Labels**: `low,renderer,water,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3604 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/water.rs:20-23` (module doc) vs. `:826-874` (the blend table in the pipeline builder)
- **Status**: OPEN
- **Description**: The module doc says "attachments 1..6 (normal, motion, mesh_id,
  raw_indirect, albedo, **reservoir**) are masked off". The builder 800 lines below masks
  off 1..=5 and deliberately **writes** 6 and 7 with `fsr_mask_max` (`MAX` over `ONE`/`ONE`,
  `color_write_mask = R`), and its own comment says so — "Attachments 1..=5 are write-masked
  off … Attachments 6 and 7 (the FSR masks) are written". The doc also names the removed
  reservoir slot.
- **Evidence**:
  - `water.rs:21-22` `//!   attachments 1..6 (normal, motion, mesh_id, raw_indirect, albedo,` / `//!   reservoir) are masked off (`color_write_mask = 0`) so water never pollutes`
  - `water.rs:828-831` `// Attachments 1..=5 are write-masked off … Attachments 6 and 7 (the FSR masks) are written — see below.`
  - `water.rs:864` `// the reservoir attachment was removed under #1583.` (in the same function)
  - `water.rs:865-873` the 8-entry `attachments` array: `[hdr_blend, masked_off × 5, fsr_mask_max, fsr_mask_max]`
- **Impact**: A reader taking the module doc at face value would conclude water writes no FSR
  mask, which is the opposite of the transparency-ghosting contract the code implements.
- **Needs RenderDoc**: no
- **Suggested Fix**: Update the module doc to "attachments 1..=5 … masked off; 6 and 7 (FSR reactive + transparency) MAX-blended at full strength", matching the in-function comment.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D11-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
