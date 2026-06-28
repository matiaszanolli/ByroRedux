# #1768: SCR-D6-NEW-02: recurring_update_tick_system + quest_fragment_dispatch_system never added to the scheduler

Filed from `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` on 2026-06-27. Snapshot as-filed (GitHub is authoritative for live state).

**Severity**: MEDIUM · **Dimension**: Scripting Runtime — lifecycle / stage wiring · **Untrusted-Input**: No
**Location**: `byroredux/src/main.rs` (no registration); systems at `crates/scripting/src/recurring_update.rs:152` and `crates/scripting/src/fragment.rs:177`
**Status**: NEW
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-06-27.md` (SCR-D6-NEW-02)

## Description
`main.rs` schedules `timer_tick_system` (`:674`), `trigger_detection_system` (`:715`), `quest_advance_system` (`:718`), and `event_cleanup_system` (`:967`) — but **not** `recurring_update_tick_system` (the only `RecurringUpdate` token in `main.rs` is a comment) nor `quest_fragment_dispatch_system`. `lib.rs::register` calls `recurring_update::register(world)`, which registers the *component/resource*, not the per-frame *system*. Confirmed by exhaustive grep: both systems appear only in their own modules, `lib.rs` re-exports, and unit tests.

## Impact
`RecurringUpdate` subscriptions never count down in-engine, so `OnUpdateEvent` never fires at runtime (the inverse of an undrained marker — it *is* drained at `cleanup.rs:36`, but has no live emitter); `QuestStageAdvanced` never dispatches fragments. Today's blast radius is limited (no `RegisterForUpdate` caller ships, and the fragment resource is empty pending the QUST-VMAD decoder per #1739), so nothing real depends on either system yet — but the moment any script uses `RegisterForUpdate`, OnUpdate silently never fires. The systems' internal lock discipline and logic are correct; the defect is purely scheduling.

## Suggested Fix
Register `recurring_update_tick_system` next to `timer_tick_system` and `quest_fragment_dispatch_system` in `Stage::Update` after `quest_advance` and before cleanup, mirroring the existing closure-wrapper pattern. If the omission is deliberate (demos-only until the fragment population path lands), add an explicit `main.rs` comment recording it so the dead-handler state is documented, not latent.

## Related
#1739 (fragment lowerer staged-not-wired — this is its scheduling half).

## Completeness Checks
- [ ] **LOCK_ORDER**: when adding the two systems to the schedule, confirm their two-phase lock-drop (already verified internally correct) composes with neighbours without holding a component lock across the fragment system's 3 resource locks
- [ ] **SIBLING**: audit `lib.rs::register` vs the `main.rs` schedule for any other registered-but-unscheduled scripting system
- [ ] **TESTS**: an integration test that registers a `RecurringUpdate` and asserts `OnUpdateEvent` fires after the interval once the system is scheduled
