# Issue #3149: UI-D3-03: after #2963 a missing destroy trait is invisible — AdapterInjected no longer means the lifecycle is wired

- **Finding ID**: `UI-D3-03`
- **Severity**: MEDIUM
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_UI_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3149

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3149 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: 3 — AVM2 Adapter Injection
- **Profile**: `Fallout4Avm2`
- **Location**: `crates/ui/src/avm2_host.rs`:78, 94-100, 154 · `crates/ui/src/avm2_host.rs`:36-43 (`ScaleformHostObjectState`) · `crates/ui/src/player.rs`:484-486, 643-650 · `crates/ui/src/host.rs`:351-356, 379-386
- **Status**: NEW — successor to #2963 (CLOSED, and correctly so; this is the gap its fix opened)

## Description

`has_destroy_trait` is a local `bool` in `inject_host_object_adapter`
(`avm2_host.rs`:79). It selects whether `build_adapter_abc` emits the destroy
helper and its `addCallback` (`:154`), and is then discarded.

The public `ScaleformHostObjectState` still has exactly three variants:

```rust
pub enum ScaleformHostObjectState {
    NotRequired,
    NotPresent,
    AdapterInjected,
}
```

So a movie whose lifecycle class lacks `onCodeObjDestruction` reports
`AdapterInjected` — byte-identical to a movie with the full contract. There is
no `log::warn!` / `log::info!` on the `false` branch, and no accessor.

#2963 removed `KNOWN_MISSING_ON_DESTROY_TRAIT` and reduced the lifecycle-class
predicate to `[object.property, object.on_create]`, which is a real fix — those
four menus now load. But it **traded a hard failure for a silent one**:
`code_object_destruction_count()` is documented as "the only evidence the
lifecycle completed", and for these menus it is now permanently zero with no way
to tell that apart from a destroy hook that fired and failed.

## Evidence

```
$ grep -n "has_destroy_trait" crates/ui/src/player.rs
(no matches — the value never crosses the module boundary)
```

`SwfPlayer::drop` (`player.rs`:643-650) gates on `has_callback(DESTROY_CALLBACK)`,
which is `false` for these movies, so `record_call`'s `DESTROYED_EVENT` arm
(`host.rs`:379-386) never runs and `code_object_destructions` stays 0.

The lifecycle test (`host/tests.rs`:625-630) asserts

```rust
code_object_destruction_count() == u64::from(state == AdapterInjected)
```

— an invariant that is now **false** for `DialogueMenu`, `MultiActivateMenu`,
`SPECIALMenu` and `Terminal`. It only stays green because it covers HUD and
Pip-Boy, which do carry the trait.

## Impact

Four shipped vanilla Fallout 4 menus load (the #2963 win) but close without ever
notifying the engine. When menu-stack / focus policy lands — the Pending-row work
this dimension exists to protect — "the menu released its host object" will be
unreadable for exactly those menus, and the zero counter will look like a destroy
hook that fired and failed rather than one that was never installed.

Catalogued as instance **#9** in the "verification layer is green by
construction" table of `docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md` —
*"`KNOWN_MISSING_ON_DESTROY_TRAIT` removed / traded a hard failure for a silent
one"*.

## Related

- #2963 (CLOSED) — the fix that introduced this
- #3151 (UI-D3-04) — the sibling Dim-3 finding from this pass
- `docs/engine/ui.md`:400, 415 document the *old* three-trait predicate and an
  unconditional destroy — tracked separately as the UI-D4-03 doc-rot finding

## Suggested Fix

Add a fourth state (e.g. `AdapterInjectedWithoutDestroyHook`) or return
`has_destroy_trait` alongside the state, and `log::info!` it at injection. Then
relax `installed_fallout4_representative_menus_obey_host_object_lifecycle`'s
destruction assertion to key off that flag instead of off `AdapterInjected` —
which restores the test's ability to fail.

---
**Source**: `docs/audits/AUDIT_UI_2026-08-20.md` (finding `UI-D3-03`)

## Completeness Checks
- [ ] **SIBLING**: The same "local bool discarded at the module boundary" shape checked for the other conditional emission paths in `build_adapter_abc`
- [ ] **TESTS**: A regression test pins this specific fix — one that would go RED if the destroy half stopped being emitted for a menu that *does* declare the trait
