# RF4-2026-11-05-06: NIF, plugin and physics crates: triage 67 standalone source scans

Labels: low,nif-parser,esm-plugin,physics,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 4
**Status**: NEW. Part of #6680

## Location
- `crates/nif/src/import/collision/mod.rs:798` `dispatched_shape_structs`
- `crates/nif/src/import/walk/lights.rs:251` `dispatched_light_structs`
- `crates/plugin/src/esm/records/tests.rs:3275` `remap_fid_has_exactly_one_definition`
- `crates/plugin/src/esm/records/tests.rs:3312` `dispatch_handled_fourccs_matches_the_live_dispatch_arms`
- `crates/physics/src/world/mod.rs:4041` `buoyancy_applies_forces_through_the_public_wrappers`
- `crates/physics/src/sync.rs:2651` `profile_accessors_read_their_own_variables`

## Why
These crates hold 24 (`nif`), 21 (`plugin`) and 22 (`physics`) standalone scans. None of them needs a device. The sites above are samples. Some pin dispatch-to-resolve parity, which has caught real gaps and may need to stay unless the table becomes data-driven. Others look testable directly.

## Change
Classify the 67 scans and replace the replaceable ones, one crate per commit. Function-size pins are excluded, because the line-budget issue in #6680 covers them.

## Acceptance Criteria
- The issue records each scan's class: needs a device or game data, replaceable by a runtime check, pins prose, or pins syntax
- Every scan classed replaceable has a behavioural test, and the scan is deleted
- One injected fault per changed file fails its new test
- No guard is removed without a replacement or a stated reason

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6689 --json state`)._
