# CONC-D1-2026-08-30-02: `with_one_time_commands`' doc header still describes the pre-#1713 lock scope, contradicting the #1713 regression test 220 lines below it

**Issue**: #3644
**Labels**: documentation, low, sync, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D1-2026-08-30-02 (LOW, D1 · Vulkan Queue & AS Sync). Doc rot.

**Location**: `crates/renderer/src/vulkan/texture.rs:644-647`.

## Description

Two doc paragraphs are stacked on one function, and the first is the **pre-#1713** one. `with_one_time_commands` is documented as holding the queue `Mutex` "for the submit+wait" — the exact behaviour CONC-D1-01 / #1713 **removed**, and which the in-file regression test at `:863-903` now *asserts against* (it requires a scope-closing `}` between the submit and the wait). The live `_inner` comment at `:801-814` states the correct rule.

Same defect class as #3527 / #3493 (a fix orphaning its predecessor's rationale), on the one invariant this dimension's checklist is built around.

## Evidence

```rust
// texture.rs:644-647
/// Execute a one-time-submit command buffer: allocate, record, submit, wait, free.
///
/// The queue `Mutex` is locked only for the submit+wait, not during recording.
/// Run a closure in a one-time-submit command buffer, then wait for completion.
```

versus the code it documents:

```rust
// texture.rs:812-816
let submit_result = {
    let q = queue.lock().expect("graphics queue lock poisoned");
    device.queue_submit(*q, &[submit_info], fence)
};
```
(guard scope closes before `wait_for_fences` at `:825`).

## Verification Path

`cargo test` — `vulkan::texture::one_time_lock_scope_tests::queue_guard_released_before_one_time_fence_wait` passes at HEAD (verified 7/7 green in `cargo test -p byroredux-renderer --lib -- one_time_lock_scope_tests vulkan::sync::tests`), which is precisely what makes the doc line false.

## Impact

A maintainer optimising the one-time path could "restore" the documented behaviour and re-serialise every future second graphics-queue thread across a GPU-execution wait. The regression test would catch it — but only after the change. Also note the **duplicated summary sentence** (`:644` and `:647`) reads as an unresolved merge.

## Related

#1713 (CONC-D1-01), audit 2026-05-16 CONC-D2-NEW-01.

## Suggested Fix

Delete line 646 (or rewrite it as "the queue `Mutex` is locked for the submit **only** — released before the fence wait, see #1713") and drop the duplicated summary line so the header has one summary.

## Completeness Checks
- [ ] **SIBLING**: Other doc headers on `texture.rs`'s queue-submitting helpers checked for the same pre-#1713 wording
- [ ] **TESTS**: The existing `one_time_lock_scope_tests` pin is the guard; confirm the corrected doc matches what it asserts
