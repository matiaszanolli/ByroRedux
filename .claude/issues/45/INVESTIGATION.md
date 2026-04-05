# Investigation: Issue #45

## Root Cause
build_subtree_name_map called at lines 253 and 374 for every animated
entity every frame. Allocates HashMap + BFS Vec each time. Subtree
hierarchy is static during playback.

## Fix
Create a SubtreeNameCache resource: HashMap<EntityId, HashMap<FixedString, EntityId>>.
- Keyed by root entity — each animation root has its own cache
- Built once on first access, reused every frame
- Cleared on cell load (when hierarchy changes)

Replace build_subtree_name_map calls with cache lookups.

## Scope
1 file: main.rs (add resource, cache lookup, clear on cell load).
