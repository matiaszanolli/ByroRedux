# SCR-D6-01: OnTriggerEnterEvent emitted but NOT drained — quest re-advances every frame after entry

Filed as: matiaszanolli/ByroRedux#1727
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: HIGH
- **Dimension**: Scripting Runtime Systems
- **Location**: `crates/scripting/src/cleanup.rs:27-38` (drain list omits it) vs `crates/scripting/src/trigger.rs:132` (emit) + `crates/scripting/src/papyrus_demo/quest_advance.rs:254-258` (consume)
- **Labels**: high, legacy-compat, bug

## Description
`trigger_detection_system` inserts `OnTriggerEnterEvent` (live M47.2 emit site). `event_cleanup_system` drains 9 marker types but NOT this one (root cause: stale `lib.rs:62` "deferred to Rapier" comment). The marker persists, so `quest_advance_system` re-consumes it every frame. For an unconditional/player-only advance, `SetStage` + `QuestStageAdvanced` re-fire every frame forever, burning MAX_CASCADE.

## Impact
The canonical `default*Trigger` quest-volume family re-fires every frame after the player crosses a volume. All-Skyrim+/FO4 trigger blast radius.

## Suggested Fix
Add `drain_component::<OnTriggerEnterEvent>(world)` to `event_cleanup_system`; extend `cleanup_removes_all_event_types`.
