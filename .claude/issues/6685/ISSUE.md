# RF4-2026-11-05-02: `acquired_in` finds lock acquisitions by string search, so helpers and cross-file accessors go unseen

Labels: low,ecs,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 4
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/boot/schedule/mod.rs:919` `acquired_in`
- Consumers: `every_parallel_system_declares_everything_it_acquires` (`byroredux/src/boot/schedule/mod.rs:1178`), `cross_file_hops_and_get_forms_reach_their_acquisitions` (`:1136`) and the per-system `*_declares_everything_it_acquires` tests (`:1209-1326`)

## Why
The scan matches turbofish spellings as text, and it assembles needles at run time (`format!("{}{}", "PlayerEntity::", "get(")`) so that its own file does not match. A helper, a table or a cross-file accessor hides an acquisition, and the declaration test then passes without checking it. #6362 had to add a by-name special case, and #6361 had to keep a table inside the function body so the scan could see it.

## Change
Parse function bodies with `syn` and visit method calls and paths, instead of searching text. `syn` is only a transitive dependency today (`Cargo.lock`), so add it as a dev-dependency of the `byroredux` binary crate, which is where `acquired_in` lives. If it goes in `crates/core/src/source_scan.rs` instead, keep it behind a feature, because that module is public (`crates/core/src/lib.rs:29`).

## Acceptance Criteria
- `acquired_in` reads a parsed syntax tree, and no run-time-composed needles remain
- A before/after diff of the type sets across the real schedule is empty, or each difference is a real declaration gap that gets fixed or filed
- Tests show an acquisition is found through a helper call and through the function-pointer spelling `remove_component::<T>,`
- `syn` does not enter the release dependency tree of `byroredux`

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6685 --json state`)._
