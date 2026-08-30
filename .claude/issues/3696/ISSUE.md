# #3696 — ECS-2026-08-30-D1-02: the recursive-read fast path skips record_and_check, so a re-entrant read can silently close an ABBA cycle

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: Lock Ordering & Deadlock
**Location**: `crates/core/src/ecs/lock_tracker.rs` (`track_read`, the `if recursive_read` branch, ~:94-110)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D1-02)

## Description

`track_read` returns from the recursive-read branch *before* the `#[cfg(debug_assertions)]` `record_and_check` block that every other acquisition runs. The module doc justifies this as "re-entrant read acquires on the same type are handled by the thread-local tracker's count and don't add edges" — true for the *outgoing* edge, but the branch also skips the **incoming** edges `held_other -> T` and, with them, the reachability probe.

A thread that holds `A`, acquires `B` (recording `A -> B`), and then re-reads `A` while both are held is establishing `B -> A`; that closes a cycle the detector would otherwise panic on, and it is neither recorded nor tested.

## Evidence

```rust
// crates/core/src/ecs/lock_tracker.rs — the early return
if recursive_read {
    let mut map = locks.borrow_mut();
    let entry = map.get_mut(&type_id).expect("recursive read row vanished");
    if entry.read_count == 1 { log::warn!( /* #2386 hazard warning */ ); }
    entry.read_count = entry.read_count.saturating_add(1);
    return;                       // <- returns before record_and_check below
}
#[cfg(debug_assertions)]
{
    let held_others = locks.borrow().iter() /* ... */;
    global_order::record_and_check(type_id, type_name, &held_others);
}
```

## Impact

A narrow blind spot in a debug-only, opt-in detector. Partly mitigated: the same branch already emits the #2386 recursive-read hazard warning, so the situation is not silent — but that warning names the type, not the cycle, and #3249 (OPEN) records that it carries no call-site information either.

## Related

#2675 (the reachability generalisation this branch bypasses), #3249, #2386.

## Suggested Fix

Move the `record_and_check` block above the `if recursive_read` return (excluding `type_id` itself from `held_others`, which it already is by construction on the non-recursive path), so a re-entrant read is checked against the graph even though it adds no outgoing edge.

## Completeness Checks
- [ ] **SIBLING**: `track_write`'s equivalent path checked for the same gap
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (a re-entrant read that closes a B->A edge must panic under `BYRO_LOCK_ORDER_CHECK`)
