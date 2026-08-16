# UI-D2-03: the engine's per-frame drain never reads dropped_calls(), contradicting drain_calls' documented contract

**Issue**: #2969
**Severity**: LOW
**Dimension**: Host Bridge Transport
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 2 — Host Bridge Transport). Profile: both.

**Location**: `crates/ui/src/host.rs`:249-265 · `byroredux/src/app_frame.rs`:231-253

## Description

`drain_calls`' doc comment states:

> A drain that returns `MAX_QUEUED_CALLS` entries should be read together with `Self::dropped_calls` — the batch may not be contiguous.

The one live consumer — the per-frame block added by #2714 — iterates the batch and never calls `dropped_calls()`. A workspace grep finds **zero** references to `dropped_calls` outside `crates/ui`.

## Evidence

```
$ grep -rn "dropped_calls" byroredux/ tools/ --include="*.rs"
(no matches)
```

`byroredux/src/app_frame.rs`:231 is `for call in ui.drain_host_calls() { … }` with no gap check before or after.

## Impact

Bounded today, because the consumer only logs — a hole in the sequence costs a missing `debug!` line.

It stops being bounded the moment the drain routes calls into quest/inventory/player state, at which point a silently non-contiguous batch is a **lost state transition with no signal**. The counter exists precisely to prevent that and is unread.

## Suggested Fix

Latch `dropped_calls()` alongside the drain and `log::warn!` once when it increases. It is two lines and makes the backstop observable in the live engine rather than only in tests.

## Related

- UI-D2-01 (host bridge transport, unbounded diagnostics)
- #2714 (introduced `MAX_QUEUED_CALLS` and the per-frame drain)

## Completeness Checks
- [ ] **SIBLING**: Other bounded channels checked for an equally unread drop counter (e.g. `resource_errors_capped`)
- [ ] **ONE-SHOT**: The warn fires on increase, not every frame
- [ ] **TESTS**: A regression test overflows the queue and asserts the engine-side consumer observes the gap

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2969 --json state` when live state is needed.*
