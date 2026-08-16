# UI-D2-01: the host bridge's four movie-keyed BTreeSets are unbounded, in a crate that explicitly bounded every other content-driven channel

**Issue**: #2964
**Severity**: MEDIUM
**Dimension**: Host Bridge Transport
**Labels**: `medium,memory,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 2 — Host Bridge Transport). Profile: both.

**Location**: `crates/ui/src/host.rs`:161-164, 200-203, 352, 357, 498-504 · `byroredux/src/main.rs`:119, 421

## Description

`BridgeState` holds `callbacks`, `known_methods`, `unknown_methods` and `unanswered_methods` as `BTreeSet<String>`. Three of the four are keyed by a string the **movie** chooses: `record_call` inserts `normalized.method` into `unknown_methods` / `unanswered_methods`, and `on_callback_available` inserts whatever name ActionScript passed to `ExternalInterface.addCallback`.

Nothing caps, evicts or clears any of them for the life of the player. The engine mirrors the same shape in `App::ui_reported_host_methods` (a `HashSet<String>` that is deliberately never cleared).

The prior UI safety report explicitly excluded these as "bounded by the count of distinct method names" — but that count is chosen by untrusted content, not by the engine. Every *other* content-driven channel in this crate has since been bounded (`calls` by `MAX_QUEUED_CALLS` under #2714; `resource_errors` by dedup + `MAX_RECORDED_RESOURCE_ERRORS` under #2720); these four were not.

## Evidence

```rust
// crates/ui/src/host.rs:356 — one String per distinct unknown method, forever
} else {
    state.unknown_methods.insert(normalized.method.clone());
    ScaleformHostDispatch::Unknown
};
```

```rust
// crates/ui/src/host.rs:498 — and one per distinct addCallback name
fn on_callback_available(&self, name: &str) {
    self.bridge.state.borrow_mut().callbacks.insert(name.to_string());
}
```

## Impact

A movie running `ExternalInterface.call("m" + i++)` (or `addCallback("cb" + i++, f)`) inside `onEnterFrame` grows engine-resident heap every frame with no ceiling — the same per-frame-growth shape #2714 was filed for, on the one channel #2714 did not cover.

Ruffle movies are untrusted content (a menu can come from any mod), so this is a **trust-boundary gap** rather than a theoretical one. Vanilla menus are unaffected: the measured corpus produces at most one host call across 600 frames.

## Suggested Fix

Give all four sets the treatment `resource_errors` already has — a fixed cap with a one-shot `log::error!` on reaching it. A diagnostic set that stops recording past N distinct names is still a useful diagnostic; one that OOMs the process is not.

Include `App::ui_reported_host_methods` (`byroredux/src/main.rs`:119, 421) in the same pass — it is fed from these sets and has the identical unbounded shape.

## Related

- UI-D5-01 (the same unbounded-diagnostics shape in the navigator)
- #2714 (bounded `calls`), #2720 (bounded `resource_errors`) — the pattern this should follow

## Completeness Checks
- [ ] **SIBLING**: All four `BTreeSet`s bounded, not just the two named in the evidence
- [ ] **ENGINE-SIDE**: `App::ui_reported_host_methods` bounded too — the mirror is part of the leak
- [ ] **DIAGNOSTIC-VALUE**: Hitting the cap logs once and keeps the set readable, rather than silently dropping
- [ ] **TESTS**: A regression test drives N distinct method names past the cap and asserts bounded growth

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2964 --json state` when live state is needed.*
