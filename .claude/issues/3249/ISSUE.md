# 3249: ECS-2026-08-24-02: #2386 recursive-read hazard warning is unbounded and carries no call-site information

**Severity**: LOW · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-02)

## Description

`track_read`'s comment says the warning fires "once (on 1→2)" — but that means once per *acquisition*, not once per process or per type. A recursive read on a per-frame path (exactly the shape of ECS-2026-08-24-01/#3250) emits one warning every frame, forever, and the message names the component type but not the acquisition site, so an operator has no way to locate it among the dozens of `query::<Transform>()` call sites in the workspace without a static sweep.

## Location

`crates/core/src/ecs/lock_tracker.rs:92-98`

## Evidence

```rust
if entry.read_count == 1 {
    log::warn!(
        "ECS recursive-read hazard: a second `{type_name}` read guard is live on this thread; reuse/drop the first guard when both reads target one World (#2386)"
    );
    ...
}
```

## Impact

Log-spam / diagnostic-noise risk on any hot per-frame recursive-read path (pays a `format!` per frame at `RUST_LOG=warn`), and the warning is currently un-actionable without a bisect.

## Related

New context introduced this session (commit `5428e872`).

## Suggested Fix

De-duplicate per `TypeId` with a thread-local "already warned" set, or attach `#[track_caller]`/`std::panic::Location` to `track_read` and include it in the message, or gate the warn behind `BYRO_LOCK_ORDER_CHECK=1`.

## Completeness Checks
- [ ] **TESTS**: A regression test for the de-dup/rate-limit behavior
