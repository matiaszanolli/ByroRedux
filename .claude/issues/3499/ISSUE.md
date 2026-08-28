# Issue #3499 — SAVE-D6-2026-08-27-04: the `FullRadius` bootstrap's worker-disconnect `break` re-opens a narrow #3280-shaped window — `apply_deltas` runs regardless of `state.pending`

Source audit: `docs/audits/AUDIT_SAVE_2026-08-27.md`
Filed: 2026-08-27 (HEAD `969d81c8`)
Labels: low, save-load, terrain-exterior, bug

---

Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D6-2026-08-27-04)
Severity: **LOW** · Dimension 6 — M45.1 Live Load-Apply
Data-Loss Class: reference-break (narrow window)

## Location
- `byroredux/src/scene/world_setup.rs:837-847` — the wait loop's worker-disconnect `break`
- `byroredux/src/save_io.rs:1258-1264` — the `count_label` that *does* report the pending count
- `byroredux/src/save_io.rs:1367-1368` — `build_form_id_remap` + `apply_deltas`, with no `state.pending` guard between them

## Status
NEW — residual of the fix for last cycle's SAVE-D6-2026-08-24-01 (#3280).

## Description
`exterior_reload_bootstrap_mode()` returns `FullRadius` specifically so `state.pending` is drained before `build_form_id_remap` scans the world, and `bootstrap_waiting(FullRadius, …)` is `!pending.is_empty()` — correct. But the wait loop has one non-`pending`-driven exit:

```rust
let payload = match state.payload_rx.recv() {
    Ok(p) => p,
    Err(_) => {
        log::error!(
            "Streaming worker disconnected mid-bootstrap with {} pending cells",
            state.pending.len(),
        );
        break;
    }
};
```

On that break, `stream_initial_radius` returns with a non-empty `pending`, `reload_exterior_session` reports it honestly in `count_label` (`"{} cells streaming ({} pending)"`) — and `execute_pending_save_loads` then calls `build_form_id_remap` + `apply_deltas` unconditionally anyway, silently dropping every saved delta row belonging to a cell that never arrived. This is the identical mechanism #3280 fixed, on a narrower trigger.

## Evidence
`world_setup.rs:837-847` (quoted). `byroredux/src/save_io.rs:1367-1368` — `let remap = byroredux_save::build_form_id_remap(…); match byroredux_save::apply_deltas(…)` with no guard on `state.pending` between them; the only condition guarding the tail is `outcome`'s `Some`/`None`, and `reload_exterior_session` returns `Some` unconditionally after `assemble_exterior_streaming`. Mitigation already present: `build_form_id_remap` now warns per unresolved `FormIdPair` (`driver.rs:278-291`), so the loss is at least diagnosable — which is why this is LOW rather than a repeat HIGH.

## Impact
Requires the streaming worker thread to die mid-bootstrap, which is already an engine-broken state; but the consequence is silent, permanent save-state loss layered on top of it, and the next save re-records the reverted state as truth. The `log::error!` names the pending count but not the resulting delta loss, so an operator reading the log would not connect the two.

## Related
#3280 / SAVE-D6-2026-08-24-01 (the primary fix); #2019 / SAVE-D6-04 (the unresolved-pair warning that limits the blast radius).

## Suggested Fix
Have `reload_exterior_session` return `None` (with a `notify_player` message) when `state.pending` is non-empty after a `FullRadius` bootstrap, so the load aborts loudly instead of half-applying — the same posture `validate_snapshot_types` and `validate_cell_loadable` already take for their own failure modes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other non-`pending`-driven exit from `stream_initial_radius` / `bootstrap_waiting`
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (the exterior *drain* currently has no test — only queueing)
