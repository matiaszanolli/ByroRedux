# RF5-2026-11-05-06: `scripting::register`: move the direct resource setup into per-subsystem `install`

Labels: low,scripting,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `crates/scripting/src/registration.rs:14` (27 direct `init_resource` calls)

## Why
25 `install` functions exist, but `registration.rs` still sets up resources directly for the subsystems that have not moved. Each new subsystem edits this central list.

## Change
Give each remaining subsystem an `install`, and reduce `register` to a flat list of calls, one subsystem per commit.

## Constraints
Part of #5802.

## Acceptance Criteria
- No direct `init_resource` call remains in `registration.rs`, or each one left has a stated reason
- A test confirms the same resource set is present after `register`
- `cargo test -p byroredux-scripting` passes

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6706 --json state`)._
