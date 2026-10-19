# GAME-SD-02: An ObScript `AddScriptPackage` slot (`ScriptPackage`) is not in the eviction row, so a streamed-out actor drops back to its schedule

Labels: medium,gameplay,save-load,scripting,ai,bug

**Severity**: MEDIUM · **Dimension**: Persistence across the stream boundary / AI packages (Dim 5, Dim 7)
**Location**: `byroredux/src/cell_loader/reference_state.rs:28-62`, `:240-263`; component `crates/scripting/src/script_packages.rs:45-60`; registered only as a resident column at `byroredux/src/save_io/registry.rs:384`
**Introduced by**: `af5db229d` (FORMAT_MAJOR 110). The #6051 follow-up (`a5343c806`) touched selection, not persistence.

## Description
The GECK contract encoded in the module doc is that a constrained script package "stays until removed, replaced, evaluated away, or completed". `ScriptPackage` lives on the actor entity only. `reference_state::capture` has no field for it and `stream_snapshot` carries only `AmbientPackageRuntime.active_package_form_id` (a FormID), which `restore_actor_snapshot` writes back verbatim while the `ScriptPackage` slot that justified it is gone. On the first tick `script_package_of` is `None`, `select_active_package` returns the schedule winner, the id differs from the restored one, and the package handover runs: the actor silently reverts to its own stack and the package's completion (`Traveled` -> `finish_script_package`) never happens.

Trigger: FNV/FO3/Oblivion quest script runs `actor.AddScriptPackage <pack>` (a Must-reach / Must-complete Travel is the typical shape); the player moves far enough that the actor's tile (exterior) or cell (interior) unloads; the player returns.

## Evidence
`grep -rn ScriptPackage byroredux/src` hits only `ai_package.rs`, `save_io/registry.rs` and its tests; `reference_state.rs` and `stream_snapshot.rs` do not mention it. A save taken while the actor is *evicted* also loses it for the same reason (the actor is not a resident row).

## Impact
A quest NPC sent somewhere by a script resumes his routine as soon as the player is out of radius; a "must complete" errand a quest waits on cannot finish. Self-heals to a plausible state (the schedule), so it is not loud.

## Related
GAME-SD-01 (same fix shape), GAME-SD-04 (the snapshot's package id also masks the handover in the alias case), SAVE-SD-03, SAVE-SD-04 (load-side face), #5859 (the precedent: `Bleedout` added to the row).

## Suggested Fix
Add `script_package: Option<ScriptPackage>` to `ReferenceState` (bump `FORMAT_MAJOR` with GAME-SD-01), restore it before the actor's first ambient tick, and test evict -> respawn -> `ambient_ai_package_system` keeps the scripted package.

Source: docs/audits/AUDIT_GAMEPLAY_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix
