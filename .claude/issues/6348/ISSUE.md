# REN-D5-02: initialize_layouts caller scan misses pass-internal callers and keep-alive arms

- **Issue**: #6348 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, renderer, vulkan, test-gap, bug
- **Source**: docs/audits/AUDIT_RENDERER_2026-10-24.md

- **Severity**: LOW (test-coverage gap; the code is correct today except as noted in REN-D5-01)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/texture.rs`, module `initial_layout_submit_tests`
- **Status**: NEW (residue of #6146)
- **Description**: the scan's file list is `context/init.rs` and `context/resize.rs`. Four callers sit outside it: `ShadowFilterPipeline::new` (`shadow_filter.rs`, guarded by hand) and the `recreate_on_resize` bodies of `taa.rs`, `svgf.rs` and `caustic.rs`. A bare destroy added in any of them would pass. The scan also only looks for `.destroy(`, so it cannot see an arm that *keeps* a pass whose layout walk never ran, and its `failure_arm` returns `None` for chained results, so the shadow-filter resize arm (`filter.initialize_layouts(upload).map_err(...)` in `recreate_swapchain`) is skipped, not checked; that arm is correct today (a hand-rolled `may_be_in_flight` → `std::mem::forget`, else `destroy`) but is the one site not routed through `release_after_failed_submit`. `checked > 0` is the only count check, so a silently skipped arm is not noticed.
- **Impact**: the next new caller, or an edit to a pass-internal one, can reintroduce #6063's destroy-while-pending without a red test.
- **Related**: #6146, #6063, REN-D5-01.
- **Suggested Fix**: add the four pass files to the scan list and pin the number of `.initialize_layouts(` call sites per file, so an unrecognised arm shape fails instead of being skipped.


## Completeness Checks
- [ ] **SIBLING**: the four pass-internal callers (`shadow_filter.rs`, `taa.rs`, `svgf.rs`, `caustic.rs`) are in the scan list
- [ ] **TESTS**: the number of `.initialize_layouts(` call sites per file is pinned, so an unrecognised arm shape fails instead of being skipped; `source_scan::production_text` is used, not a whole-file `include_str!`
- [ ] **RELATED**: lands with or after REN-D5-01 (the keep-alive arms)

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-24.md` (static review; no engine or Vulkan device was launched). Failures invisible to `cargo test` are flagged NEEDS-RENDERDOC in the report._
