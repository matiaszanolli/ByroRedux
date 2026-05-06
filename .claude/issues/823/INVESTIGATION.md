# Investigation — #823

**Domain**: ecs

## Code path

`track_read` (L54-86) and `track_write` (L90-127) in `crates/core/src/ecs/lock_tracker.rs` both have the same shape:

```rust
if is_new {
    let held_others: Vec<(TypeId, &'static str)> = map.iter()
        .filter(|(id, _)| **id != type_id)
        .map(|(id, state)| (*id, state.type_name))
        .collect();   // ← unconditional alloc
    drop(map);
    #[cfg(debug_assertions)]
    global_order::record_and_check(type_id, type_name, &held_others);
    #[cfg(not(debug_assertions))]
    let _ = held_others;   // ← discards the Vec we just built
}
```

The `held_others` Vec is built unconditionally before the cfg switch, then in release builds discarded via the `let _` no-op arm. Pure waste.

## Fix

Move the entire body of `if is_new { ... }` inside `#[cfg(debug_assertions)]`. In release builds the block becomes empty; the compiler DCE's the `is_new` boolean check itself. `is_new` is still used in `if is_new {}` even in release so no unused-variable warning.

`drop(map)` is also debug-only — `record_and_check` might re-borrow `LOCKS`, so dropping the `RefMut` was needed before that call. In release the call doesn't happen, so the `RefMut` simply lives to the end of the `LOCKS.with` closure (single-threaded, no aliasing problem).

Same pattern applied at both sites.

## Test strategy

The audit suggested an allocation-counter microbenchmark — same dhat-infra requirement as #828 / #832, deferred. Behavior equivalence is preserved (lock-order checking is debug-only by design, release builds had no functional output from the deleted Vec construction). Existing `lock_tracker_is_clean_after_poisoned_panic` and the rest of the lock_tracker test suite cover correctness.

## Scope

1 file: `crates/core/src/ecs/lock_tracker.rs`
