# Investigation — Issue #464

## Domain
ECS — `crates/core/src/ecs/systems.rs`

## Root cause
Scratch buffer declared `let mut queue: Vec<EntityId> = Vec::new();` at line 41. Phase 2 drain loop at line 109 calls `queue.pop()`, which is `Vec::pop` → LIFO. That is DFS, not BFS. Doc comments at lines 26, 48, 92 call it "BFS".

## Enqueue sites
- Line 105: `queue.extend_from_slice(&children.0)` — initial root children
- Line 138: `queue.extend_from_slice(&children.0)` — grandchildren during walk

`children.0` is a `Vec<EntityId>`.

## Chosen fix (option a from issue)
Switch scratch type to `VecDeque<EntityId>` and drain via `pop_front`. Near-zero overhead on small working sets; makes future parallel per-level dispatch trivial. Docs stay honest without rewording.

## Migration notes
- `VecDeque::extend_from_slice` does not exist. Use `queue.extend(children.0.iter().copied())`.
- `clear()`, `is_empty()`, `pop_front()`, `extend()` all available.
- No allocator concern — `VecDeque` uses ring buffer; scratch reuse preserved.

## Related
- #46 tracks lock-acquisition perf in this same system. Separate concern — not touched by this fix.
