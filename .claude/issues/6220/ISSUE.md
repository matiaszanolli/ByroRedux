# #6220: SCR-2026-10-21-06: ObScript `GameMode` still runs every frame on identity-only stubs whose home cell is not loaded (#5997 fixed only `OnLoad`)

**Labels**: medium,scripting,bug,game:fnv,game:fo3,game:oblivion

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Attach & Reference Identity / Legacy ObScript · **Location**: `crates/scripting/src/obscript_objects.rs:347` (`BLOCK_GAME_MODE => runs.push`, no residency test); `byroredux/src/cell_loader/references/attach.rs:368` (`attach_object_script` runs whatever the `has_3d` value); stub producers `exterior.rs:393` (remote persistent actors) and `asset_provider/script.rs:1586` (`RemoteSceneActorStub`)

**Audit status**: NEW (residual of closed #5997, `a44d4a20d`; this is the GameMode half SCR-SD-03 named on 10-19, now untracked)

**Untrusted-Input**: No

## Description

The local CS wiki answers the open question SCR-SD-03 raised. `(main)/GameMode.wiki`, Notes: "Not all scripts run every frame, e.g, Quest scripts … and scripts on references not loaded". `Disable.wiki` (both CS and GECK) says scripts *do* run on disabled references, so disabled placements in loaded cells are correct. Only stubs standing for references in unloaded cells are wrong.

## Evidence

Since #5997, `collect_due`'s `OnLoad` arm is gated on `OnCellLoadEvent`, which a stub no longer gets. No gate exists for `GameMode`.

## Impact

`GameMode` bodies (timers, `MoveTo`, `PlaceAtMe`, `AddScriptPackage`) of persistent scripted actors run every frame from the far side of the worldspace. The per-frame cost grows with the persistent cell's scripted-actor count.

## Related

#5997, SCR-SD-03, #2664 (stub/real reconciliation, unbuilt).

## Suggested Fix

Skip `BLOCK_GAME_MODE`, and the other per-frame blocks, for entities carrying `RemoteSceneActorStub` or a logical-stub marker the persistent loop stamps. Or attach no `ObScriptInstance` until the reference spawns with 3D.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-frame ObScript blocks (ScriptEffectUpdate, OnTrigger) and the legacy load-order GameMode handler)
- [ ] **TESTS**: A regression test pins this specific fix
