# #3434: UI-D2-2026-08-27-08: `MAX_DISTINCT_HOST_METHOD_NAMES` can lock out the adapter's own `__byroBGSCodeObj*` callbacks, and the constant's rationale does not say so

- **Severity**: LOW
- **Dimension**: Host Bridge Transport
- **Profile**: `Fallout4Avm2` (mechanism is profile-agnostic)
- **Location**: `crates/ui/src/host.rs:37-52`, `:194-217`, `:598-607` · `crates/ui/src/player.rs:606-613`, `:658-666`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D2-2026-08-27-08)

## Description

`on_callback_available` funnels every `ExternalInterface.addCallback` name through `insert_bounded` into the 1024-entry `callbacks` set. Untrusted movie content chooses those names (the constant's own doc says so). Once the cap latches, a *later* registration is dropped — including the injected adapter's `__byroBGSCodeObjReady` and `__byroBGSCodeObjDestroy`, which are registered from the installer running out of the movie's own constructor, i.e. after arbitrary movie code has had a chance to run. `has_callback` is the sole gate on both `SwfPlayer::invoke_callback` (`player.rs:611-613`) and the destroy hook in `Drop` (`player.rs:660`), so a capped set silently disables the readiness probe and the `code_object_destruction_count()` acknowledgement.

## Evidence

`insert_bounded` (`host.rs:201-217`) drops any *new* name once `set.len() >= MAX_DISTINCT_HOST_METHOD_NAMES`:

```rust
if set.len() >= MAX_DISTINCT_HOST_METHOD_NAMES {
    if !*capped { *capped = true; log::error!(...); }
    return;
}
```

There is no reserved band or prefix exemption for the engine's own `__byro`-namespaced names, even though `HELPER_PREFIX` / `READY_CALLBACK` / `DESTROY_CALLBACK` (`avm2_host.rs:24-31`) already give them a distinguishable namespace. The constant's doc comment (`host.rs:37-52`) lists only the heap-growth rationale.

## Impact

Requires a hostile or pathological movie registering 1024+ distinct callbacks before its own lifecycle installer runs — not a shape any vanilla menu produces. The finding is that the cap converts a *memory* hardening measure into a *functional* one without saying so.

## Related

#2964 (added the cap); #3156 (the one movie-keyed set the same work left unbounded) is a distinct defect.

## Suggested Fix

Exempt names beginning with `__byro` from the cap (they are engine-authored and finite: 3), and add one sentence to the constant's doc comment recording what the cap costs when it engages.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other three `insert_bounded` call sites: `known_methods`, `unknown_methods`, `unanswered_methods`)
- [ ] **TESTS**: A regression test pins this specific fix
