# RF4-2026-11-05-05: Scripting: triage 39 standalone source scans in a crate that needs no GPU

Labels: low,scripting,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 4
**Status**: NEW. Part of #6680

## Location
- `crates/scripting/src/story_manager.rs:1410` `phase_b_never_takes_the_node_state_under_the_stages_guard`
- `crates/scripting/src/condition/tests.rs:1306` `get_actor_value_does_not_hold_actor_values_across_ruleset`
- `crates/scripting/src/obscript_objects.rs:884` `collect_due_does_not_rebuild_a_stub_set_or_use_siphash`
- `crates/scripting/src/condition/tests.rs:734` `get_distance_variant_keeps_its_doc_comment`

## Why
The crate has 124 `include_str!` reads of Rust files, but 85 are `SOURCES` concat rows, so the real figure is 39. Nothing here needs a Vulkan device, which weakens the usual reason for a text scan. The sites above are samples: some pin lock shape, which the lock-order detector may be able to check at run time, and some pin prose.

## Change
Classify the 39 scans. Move lock-shape checks to the lock-order detector where it can see the path, and replace the rest with behavioural tests. Leave the `SOURCES` lists alone.

## Acceptance Criteria
- The issue records each scan's class: needs a device or game data, replaceable by a runtime check, pins prose, or pins syntax
- Every scan classed replaceable has a behavioural test, and the scan is deleted
- One injected fault per changed file fails its new test
- No guard is removed without a replacement or a stated reason

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6688 --json state`)._
