# #3435: UI-D1-2026-08-27-09: minor hygiene — a substring-overlapping state probe, a saturating request-ID bound, and two smoke-test nits

- **Severity**: LOW
- **Dimension**: Profile & VM Selection / AVM2 Adapter Injection / Resource Navigator
- **Profile**: both
- **Location**: `crates/ui/src/avm2_host.rs:89-95` · `crates/ui/src/host.rs:558-564` · `docs/smoke-tests/m48-menu-load.sh:45`, `:48`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D1-2026-08-27-09)

## Description

Three unrelated one-liners, grouped so they are not three issues.

**1. Substring-overlapping state probe.** `crates/ui/src/avm2_host.rs:89-95` — the re-injection probe separates `AdapterInjected` from `AdapterInjectedWithoutDestroyHook` with `contains_bytes(abc, DESTROY_CALLBACK.as_bytes())`. `DESTROY_CALLBACK` (`"__byroBGSCodeObjDestroy"`) is a strict **prefix** of `DESTROYED_EVENT` (`"__byroBGSCodeObjDestroyed"`), so the probe cannot distinguish them. It is correct today only because `build_adapter_abc` emits all four destroy strings together or none (`:747-754`) — i.e. correct by an invariant held two functions away, with no test pinning it.

**2. Saturating request-ID bound.** `crates/ui/src/host.rs:558-564` — `numeric_request_id`'s upper bound is `value <= u64::MAX as f64`; `u64::MAX as f64` rounds *up* to 2^64, so the value 2^64 passes the guard and then saturates to `u64::MAX` in `value as u64`. Cosmetic (a `GameDelegate` ID is a small counter), but the guard does not do what it reads as. `value < 2f64.powi(64)` is exact.

**3. Smoke-test nits.** `docs/smoke-tests/m48-menu-load.sh:48` defines `PORT="${BYRO_DEBUG_PORT:-9876}"` and never uses it except inside a failure message (`:134`) — it is neither exported nor passed to `byro-dbg`. And `:45` reads `BYROREDUX_SKYRIM_DATA` while the crate's own Skyrim corpus test (`crates/ui/src/host/tests.rs:574`) and `README.md:378` read `BYROREDUX_SKYRIMSE_DATA` for the same directory, so an operator who sets one silently skips the other. (The `_SKYRIM_` spelling is the smoke-test-wide convention — 7 scripts plus `docs/smoke-tests/lib/fixture.sh` — so this is a repo-wide split, noted here because it bites the UI gate specifically.) `m48-menu-load.sh` was also not migrated to the `lib/fixture.sh` harness that `3aebf414` introduced for `p0`/`p1`/`p2`.

## Evidence

```rust
// crates/ui/src/avm2_host.rs:89-95
let state = if movie.tags.iter().any(|tag| {
    abc_payload(tag).is_some_and(|abc| contains_bytes(abc, DESTROY_CALLBACK.as_bytes()))
}) { ScaleformHostObjectState::AdapterInjected }
  else { ScaleformHostObjectState::AdapterInjectedWithoutDestroyHook };
```

```rust
// crates/ui/src/host.rs:559
if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= u64::MAX as f64 {
```

`grep -n PORT docs/smoke-tests/m48-menu-load.sh` -> `:48` (definition) and `:134` (failure message only).

## Impact

None observable today; all three are hygiene.

## Suggested Fix

(1) match on `DESTROYED_EVENT` instead, or add a test pinning the invariant. (2) exact bound `value < 2f64.powi(64)`. (3) drop `PORT` or export it; pick one Skyrim env-var spelling repo-wide, and migrate `m48-menu-load.sh` to `lib/fixture.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `contains_bytes` prefix probes; other smoke-test scripts' env-var spellings)
- [ ] **TESTS**: A regression test pins this specific fix
