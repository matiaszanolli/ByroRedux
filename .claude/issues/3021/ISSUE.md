# SAVE-D6-01: CurrentCellContext never cleared when leaving an interior

**Issue**: #3021
**Severity**: HIGH
**Dimension**: 6 — engine integration
**Labels**: `high,ecs,bug`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 6 — engine integration).

**Location**: `byroredux/src/cell_loader/load.rs`:542-546 (the only insert site) · `byroredux/src/cell_loader/transition.rs`:322-332 (`unload_current_interior`) · `byroredux/src/app_step.rs`:758 (Interior→Exterior transition arm) · `byroredux/src/save_io.rs`:846-850, :905-908, :952-960

## Description

`CurrentCellContext` is inserted when an interior cell loads and **never cleared when the session leaves that interior**. `unload_current_interior` resets `CurrentCellRoot(None)` but does not touch `CurrentCellContext`.

A subsequent exterior save therefore still carries the departed interior's context, so it **masquerades as an interior save** and `load` reloads the cell the player left.

## Evidence

```
$ grep -rn "CurrentCellContext" byroredux/src --include="*.rs" | grep -v _tests
byroredux/src/cell_loader/load.rs:542:    world.insert_resource(super::CurrentCellContext {
byroredux/src/save_io.rs:318:        .register_resource::<CurrentCellContext>("CurrentCellContext")
…
```

**One insert site, no clear site.** Contrast `unload_current_interior` (`transition.rs`:322-332), which explicitly does `world.insert_resource(CurrentCellRoot(None));` — the sibling resource *is* cleared, this one is not.

`save_io.rs`:366 even documents the assumption the bug violates: *"exterior modes never set `CurrentCellContext`"*. True on a fresh exterior launch; false after an interior→exterior transition.

Re-verified 2026-08-17.

## Impact

Save an exterior position after having visited any interior, then load: the drain's reload target (`save_io.rs`:952-960) takes the interior branch and reloads the departed cell. The player is restored into the wrong worldspace entirely — not a subtle drift, a different location.

The "loose/exterior save" guard at `:846-850` cannot help, because from its perspective the save genuinely looks interior.

## Suggested Fix

Clear `CurrentCellContext` alongside `CurrentCellRoot` in `unload_current_interior`, and in the Interior→Exterior arm at `app_step.rs`:758. Since the resource's whole contract is "set iff we are in an interior", the clear belongs everywhere the root is cleared.

## Related

- SAVE-D6-2026-08-16-03 (#3028) — `save-load-roundtrip.md` describes this path

## Completeness Checks
- [ ] **SIBLING**: Every site that clears `CurrentCellRoot` also clears `CurrentCellContext` — they are one invariant
- [ ] **DOC-TRUTH**: `save_io.rs`:366's "exterior modes never set `CurrentCellContext`" comment is true after the fix
- [ ] **ROUND-TRIP**: A save taken outdoors after an interior visit reloads outdoors
- [ ] **TESTS**: A regression test covers interior → exterior → save → load

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3021 --json state` when live state is needed.*
