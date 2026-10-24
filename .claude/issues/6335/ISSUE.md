# PERF-D1-2026-10-23-05: story_change_location_system per-frame location re-derive

- **GitHub**: #6335
- **Severity**: LOW
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-23.md

Found by `/audit-performance` 2026-10-23 (PERF-D1-05). Derived cost; unmeasured.

## Location
`byroredux/src/systems/story_events.rs`: `publish_player_cell_kind` (~29-46), the `PlayerLocation` block (~59-71), `resolve_current_lctn` (~117-139; interior `cell_editor_id.to_ascii_lowercase()`), second resolve + `location_key` when the `SmTree` is non-empty (~94).

## Description
#5447's contract is "no per-frame work where the load order authors nothing", but it covers only the CLOC key. On Skyrim/FO4 (non-empty `LocationTable` and `SmTree`) `story_change_location_system` calls `resolve_current_lctn` twice per frame (the `PlayerLocation` publish, then the CLOC key). Each call clones an `Arc` and, in an interior, lowercases `cell_editor_id` into a fresh `String` before a `HashMap<String, _>` lookup; exteriors do a string-hash worldspace lookup plus a grid lookup. `publish_player_cell_kind` adds a worldspace-keyed lookup and `location_key` hashes `esm_path` + `cell_editor_id` again. The result changes only on a cell transition, already a discrete event (`CurrentCellContext`/`CurrentExteriorContext` are replaced wholesale).

## Impact
Derived: 2 allocations + ~4 string-hash lookups per interior frame, a few hundred ns. Negligible — a per-frame allocation a change key removes. Related: #5447, #5372 (keep the "resolve before the marker guard" lock order if refactored).

## Suggested Fix
Cache `(cell identity → location, kind)` in the `PlayerLocation`/`PlayerCellKind` resources keyed on the identity hash `location_key` already computes; recompute only when the cell context moves and share one resolve between both uses.

## Completeness Checks
- [ ] **LOCK_ORDER**: `resolve_current_lctn` still runs before the `StoryEvent` marker guard (#5372)
- [ ] **SIBLING**: Same pattern checked in related files/systems
- [ ] **TESTS**: A regression test pins this specific fix
