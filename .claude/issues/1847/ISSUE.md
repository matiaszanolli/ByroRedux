# SAVE-04: Live overlay is additive-only — a removed/disabled object reappears on live-load

**Labels**: low, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1847
**Source**: docs/audits/AUDIT_SAVE_2026-07-02.md

**Severity**: LOW
**Dimension**: M45.1 Live Load-Apply
**Data-Loss Class**: reference-break (latent)
**Location**: `crates/save/src/driver.rs:182-199` (`apply_deltas`); `crates/save/src/registry.rs:111-128` (`ApplyFn`)

## Description
`apply_deltas` / `ApplyFn` only *insert* remapped rows onto the freshly reloaded cell; there is no removal/despawn form. The reloaded cell respawns every authored REFR from the ESM. If, during the saved session, the player deleted / disabled / picked-up a world object, the reload respawns it and the overlay has no way to re-remove it → the object reappears after a live load. (`restore_world`'s clear-then-repopulate loose path doesn't have this issue, but it isn't the live path.)

## Evidence
No `Disabled`/`Deleted`/`Enabled` marker component exists in `crates/core/src/ecs/components/` (grep empty); `apply_deltas` has no `remove`/`despawn` (grep of `driver.rs`/`registry.rs` finds only doc-comment mentions).

## Impact
Latent — the engine currently has no enable/disable/delete persistence mechanism to save, so nothing regresses today. Becomes a real reference-break the moment object enable-state or a "deleted refs" set is persisted.

## Related
SAVE-03 (both are consequences of the additive form-id overlay model).

## Suggested Fix
When object enable-state lands, persist a per-cell disabled/deleted form-id set and have the drain apply it (despawn / hide the matching reloaded entities) after `apply_deltas`.

## Completeness Checks
- [ ] **TESTS**: A regression test covering the future enable/disable persistence includes a live-load case that a deleted/disabled object stays gone after reload
