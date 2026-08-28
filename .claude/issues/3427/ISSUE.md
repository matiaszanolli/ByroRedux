# #3427: UI-D3-2026-08-27-02: `ScaleformHostObjectState` has no engine consumer — a menu that loaded with no host object at all logs identically to one that injected cleanly

- **Severity**: MEDIUM
- **Dimension**: AVM2 Adapter Injection
- **Profile**: `Fallout4Avm2`
- **Location**: `crates/ui/src/avm2_host.rs:104-106` · `crates/ui/src/player.rs:500-502` · `byroredux/src/scene.rs:1560-1568`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D3-2026-08-27-02)

## Description

`/audit-ui` Dim 3's checklist requires: "A movie that never creates `BGSCodeObj` must land in `NotPresent` **and be visible to the engine** — not silently look identical to success." It lands in `NotPresent` correctly, and it is not visible to the engine. The `NotPresent` return is completely silent:

```rust
// crates/ui/src/avm2_host.rs:104
if !declares_contract {
    return Ok((swf_data.to_vec(), ScaleformHostObjectState::NotPresent));
}
```

no `log::`, no counter. And `SwfPlayer::host_object_state()` (`player.rs:500`) has **no caller outside `crates/ui`** — `grep -rn "host_object_state" byroredux/ crates/renderer/ tools/` returns nothing. `UiManager` does not re-export it either, so the engine could not read it without a new accessor.

## Evidence

The `--menu` route's only success observable is `byroredux/src/scene.rs:1560-1566`:

```rust
log::info!(
    "ui.menu: loaded path={} archive={} profile={:?} texture={:?}",
    menu_path, archive_path, profile, handle
);
```

— profile and texture handle, no host-object state. An AVM2 menu that reaches `NotPresent` prints exactly this line, loads, renders pixels, and answers no host call ever, with zero log evidence of why. The sibling states *are* logged (`avm2_host.rs:222-225` for `AdapterInjectedWithoutDestroyHook`, `:188-194` for uncataloged forwarders), which makes the silent one the odd case out.

## Impact

The single hardest UI failure to diagnose — "the menu draws but nothing responds" — is exactly the one with no signal. It is also the shape a future DLC/mod menu will most likely take.

## Related

#3149 (the same class of invisibility for the destroy hook, fixed by making the state distinguishable — this is the half that was left unwired). Sibling finding UI-D5-2026-08-27-05 (the `--menu` smoke gate blind spot).

## Suggested Fix

Add `UiManager::host_object_state()` and fold it into the `ui.menu: loaded` line (`state={:?}`), or `log::warn!` on `NotPresent` for an AVM2 profile inside `inject_host_object_adapter`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `--swf` loose-file route, the other two `SwfPlayer` constructors)
- [ ] **TESTS**: A regression test pins this specific fix
