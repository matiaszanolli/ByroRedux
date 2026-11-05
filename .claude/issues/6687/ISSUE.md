# RF4-2026-11-05-04: Renderer: triage 445 standalone source scans and move host-logic checks to real tests

Labels: low,renderer,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 4
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (38 include sites)
- `crates/renderer/src/vulkan/sync.rs` (24)
- `crates/renderer/src/vulkan/svgf.rs` (24)
- `crates/renderer/src/vulkan/texture.rs` (23)
- `crates/renderer/src/vulkan/context/post_passes.rs` (22)

## Why
445 of the crate's 451 `include_str!` reads of Rust files are standalone scans, across 67 files. Shader-contract and barrier-order scans have no device-free alternative and stay. Host logic is different: `acceleration/predicates.rs` already shows that decisions can be pure, testable functions. Sampled candidates, not yet classified: `ensure_tlas_state_consumes_the_shrink_request_past_the_commit_point`, `gpu_instance_does_not_re_expand_with_per_material_fields` and `the_doc_names_the_file_that_holds_the_per_frame_eviction_call`.

## Change
Classify the scans. Where a decision can move into a pure function, move it and test the function, without changing call order. Do not mix this with any barrier or lifetime change.

## Acceptance Criteria
- The issue records each scan's class: needs a device or game data, replaceable by a runtime check, pins prose, or pins syntax
- Every scan classed replaceable has a behavioural test, and the scan is deleted
- One injected fault per changed file fails its new test
- No guard is removed without a replacement or a stated reason

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit
- [ ] **VALIDATION**: A `BYRO_VALIDATION=1` run matches the baseline's warning profile; no barrier or timer bracket moved

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6687 --json state`)._
