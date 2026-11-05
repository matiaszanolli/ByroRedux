# RF5-2026-11-05-07: 22 of the 52 `flush_*` methods in `deferred.rs` share one shape and could share a helper

Labels: low,scripting,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 5
**Status**: NEW. Part of #6680

## Location
- `crates/scripting/src/fragment/effects/deferred.rs` (52 `flush_*` methods; `apply_at_depth` at `:403`)

## Why
22 methods are the same code: skip if the queue is empty, acquire one resource or query, drain the queue into it. They are `flush_event_requests`, `flush_alias_collections`, `flush_location_keyword_changes`, `flush_reference_flag_changes`, `flush_owner_changes`, `flush_crime_gold_changes`, `flush_hit_requests`, `flush_animation_event_requests`, `flush_distance_requests`, `flush_follower_changes`, `flush_linked_ref_changes`, `flush_crime_faction_changes`, `flush_speechcraft_changes`, `flush_scanned_changes`, `flush_anim_archetype_changes`, `flush_form_list_changes`, `flush_loc_ref_type_changes`, `flush_achievements`, `flush_misc_stats`, `flush_star_map_changes`, `flush_reference_enable_changes`, `flush_cinematic_presentation`.

## Change
Add one generic helper and route the 22 through it. Keep the flush order in `apply_at_depth` unchanged. Do this after #6685, or extend `acquired_in` in the same commit, because hiding an acquisition in a helper is what blinds that scan.

## Constraints
Part of #5802. A queue that clears itself when its resource is absent must keep doing so.

## Acceptance Criteria
- The 22 methods use the helper
- The dialogue-row guard still reports every acquired type, shown by removing one declaration as an injected fault
- `cargo test -p byroredux-scripting fragment` passes

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6707 --json state`)._
