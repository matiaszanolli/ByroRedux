# SCR-D7-NEW3-01: quest_advance_system's one-signal-per-entity assumption is unenforced, true today only by coincidence

**Issue**: #2130
**Labels**: low, info, bug
**Dimension**: Engine Attach Path & Trigger-Volume Wiring
**Untrusted-Input**: No
**Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:235-335`
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 7 — informational, not reachable today, no fix required now)

## Description

`quest_advance_system` collects `(entity, activator/triggerer)` pairs from both `ActivateEvent` and `OnTriggerEnterEvent`, implicitly assuming a given entity never receives both in the same frame. This holds today only because (1) a `TriggerVolume` is only ever attached to a mesh-less REFR, so the mesh-bearing/mesh-less component sets are disjoint by construction, and (2) `ActivateEvent` has no live automatic emitter yet (only a debug console command) — the real "player activates a REFR" system (`boot.rs`'s "Stage 4") is unbuilt.

The recognizer test `on_activate_wins_over_on_trigger_enter` proves a single script can legitimately define both handlers, so once Stage 4 lands, if it doesn't explicitly exclude `TriggerVolume`-bearing entities from activation eligibility, the disjointness assumption breaks and a single player action could double-fire `QuestStageAdvanced` (idempotent for the stage value, but a genuine double-application risk for a non-idempotent fragment effect like `AddItem`).

## Impact

None today — both preconditions independently hold. Purely forward-looking.

## Suggested Fix

No code change needed now. When Stage 4 (the real "player activates a REFR" system) lands, either exclude `TriggerVolume`-bearing entities from activation eligibility, or add a per-frame per-entity dedup in `quest_advance_system`. A cheap regression test for that future work: insert both event types on the same entity in one frame, assert exactly one `QuestStageAdvanced` marker results.
