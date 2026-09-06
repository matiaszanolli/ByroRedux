# #4035 — REN-2026-09-06-D4-05: `draw_frame_does_not_re_upload_bloom_params_every_frame` scans `draw.rs`, but the per-frame UBO section it guards moved to `build_and_upload_instances.rs`

**Labels**: low, renderer, sync, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/bloom.rs`
  (`draw_frame_does_not_re_upload_bloom_params_every_frame`)
- **Status**: NEW — sibling of the open #3442
- **Description**: The test's own doc says *"`draw_frame`'s per-frame UBO section
  (composite/SVGF/TAA) must NOT call `bloom.upload_params`"*, and enforces it
  with `assert!(!include_str!("context/draw.rs").contains("bloom.upload_params"))`.
  That per-frame UBO section — the `composite.upload_params` /
  `svgf.upload_params` / `taa.upload_params` / `water.upload_params` block whose
  host writes the bulk barrier folds — now lives in
  `build_and_upload_instances.rs`. A re-added `bloom.upload_params` would
  naturally land there, where the scan cannot see it, and would additionally be
  a per-frame host write folded onto that same bulk barrier — i.e. exactly the
  redundant rewrite #2037 removed, silently reinstated.
- **Evidence**:
  - `grep -n "upload_params" crates/renderer/src/vulkan/context/build_and_upload_instances.rs`
    → `composite.upload_params`, `svgf.upload_params`, `taa.upload_params`,
    `water.upload_params`, plus the "#2037 / GPU-D5-01 — no per-frame upload
    needed here" comment that marks bloom's absence. All four are in that file;
    none is in `draw.rs`.
  - The bloom test still reads `include_str!("context/draw.rs")`.
- **Impact**: A negative pin pointed at the wrong file. Guard-coverage only; no
  live defect (bloom's UBOs are still written once in `BloomPipeline::new_inner`).
- **Related**: #2037 / GPU-D5-01, #3282, #3442 (open, same class), D4-04 above.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Point the scan at
  `include_str!("context/build_and_upload_instances.rs")` (or scan both files)
  and update the doc comment's "draw_frame's per-frame UBO section" wording. No
  production change.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
