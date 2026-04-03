# Investigation: Issue #10 — Name lookup O(N) per frame

## Root Cause
`animation_system` rebuilds `HashMap<String, EntityId>` every frame from all named entities.
Each iteration resolves FixedString → &str → String (heap allocation).

## Fix
Create a `NameIndex` resource:
- `map: HashMap<FixedString, EntityId>` — zero-allocation lookup via interned symbols
- `generation: u32` — tracks `world.entity_count()` at last rebuild
- Rebuilt only when entity count changes (no despawn exists, so count is monotonic)

Animation system: for each channel name (String), call `pool.get(name)` → Option<FixedString>
(no allocation, returns None if not interned), then look up in NameIndex.

## Files
1. `byroredux/src/main.rs` — add NameIndex resource, update animation_system
2. (optional) `crates/core/src/animation.rs` — no change needed, String keys are fine

**2 files — within threshold.**
