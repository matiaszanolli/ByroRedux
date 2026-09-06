# #3858: TD1-2026-09-05-09: the #2731 / #3282 file splits produced six single-function files — the extracted functions were relocated, never decomposed

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-09) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3858 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-09), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/scene/nif_loader.rs::load_nif_bytes_with_skeleton` (1105 LOC of a 1614-line file); `crates/renderer/src/vulkan/context/build_and_upload_instances.rs::build_and_upload_instances` (919 of 1058); `byroredux/src/render/static_meshes.rs::collect_static_mesh_draws` (916 of 1339); `crates/renderer/src/vulkan/context/skinned_blas_refit.rs::record_skinned_blas_refit` (837 of 1207); `byroredux/src/app_events.rs::about_to_wait` (771 of 1310); `byroredux/src/app_frame.rs::render_one_frame` (671 of 905)
- **Status**: NEW
- **Description**: The workspace has **133 production functions over 200 LOC**. Most are the shapes
  #2412 already accepted and this finding does not re-flag them: field-proportional parsers
  (`parse_block_inner` 1102, `dispatch_blocks` 503, `parse_esm_with_load_order` 429,
  `parse_cell_group_inner` 525, `parse_refr_group_inner` 457, `parse_wrld_children_inner` 363,
  `parse_qust_alias` 358, `open` 412), Vulkan constructors (`composite::new_inner` 786,
  `volumetrics::new_inner` 774), and build/example targets (`renderer/build.rs::main` 829,
  `crates/scripting/examples/mq101_conformance.rs::run` 1445).

  The six above are a **different, newer class**: each is >59 % of a file that exists *only* to hold
  it, created by a file-level split that moved the function without decomposing it. They are the
  mirror image of the #3739/#3738 pattern the SKILL warns about (function split that did not move
  the file) — here the file split did not move the function.
- **Evidence**: `context/{build_and_upload_instances,skinned_blas_refit}.rs` came out of #1857/#3282;
  `app_events.rs`/`app_frame.rs` came out of #2731. None of these functions appears in any open or
  closed issue title. `crates/renderer/src/vulkan/context/init.rs::build_pipelines_and_finish`
  (1118 LOC) looks like a seventh but is **deliberately excluded** — `init.rs`'s module doc records
  that a finer phase-3 split was evaluated and rejected under #1749 because every value it builds
  feeds the final struct literal in the same phase; that is a downstream symptom of #3736
  (`VulkanContext`'s field count), not an independent finding.
- **Impact**: LOW individually. Reported as one census entry so the next sweep can diff the count
  (133) and so nobody reads "the file was split" as "the complexity was reduced".
- **Related**: #3739, #3738 (the inverse pattern); #2412 (the accepted-shape taxonomy this reuses);
  #3736 (owns `build_pipelines_and_finish` transitively); `feedback_speculative_vulkan_fixes.md` —
  the two `context/` entries are render-recording paths, so any decomposition must be verified in
  RenderDoc, not by `cargo test`.
- **Suggested Fix**: no bulk action. When one of these files is next touched, decompose the function
  in place along its existing comment sections rather than adding to it. Highest value first:
  `load_nif_bytes_with_skeleton` (not a Vulkan path, so testable, and it is the NIF→ECS entry point).
- **Effort**: medium (per function; do not batch)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
