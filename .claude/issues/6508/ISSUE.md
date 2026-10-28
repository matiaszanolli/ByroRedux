# SAVE-2026-10-27-01: Two transients added after #6239 escape the load's outgoing-session sweep, and their allowlist reasons are false (`PendingEvidenceDeposits`, `VatsState`)

Labels: low, save-load, bug
Source: docs/audits/AUDIT_SAVE_2026-10-27.md

**Source**: `docs/audits/AUDIT_SAVE_2026-10-27.md` finding `SAVE-2026-10-27-01` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/save_io/live_load.rs` (`discard_outgoing_session_requests`, ~:459-481, and the discard block ~:628-644); `crates/scripting/src/jail.rs` (`PendingEvidenceDeposits`, `confiscate_stolen_goods`); `byroredux/src/crime.rs` (`evidence_deposit_system`); `byroredux/src/boot/schedule/update.rs:322` (consumer) vs `:577` (`obscript_stage_result_system`); `byroredux/src/components/vats.rs` (`VatsState::exit_all`); `byroredux/src/boot/world.rs:37` (only production `VatsState` reset); `byroredux/src/app_events.rs` (~:1078-1099 F5/F9 intercept, not gated on VATS); allowlist rows in `byroredux/src/save_io/registry_completeness_tests.rs` (`VatsState` ~:454, `PendingEvidenceDeposits` ~:734)

## Description
Same class as closed #6239 / #6238 / #6083 / #5435 ("every load behaves like a fresh boot"). #6239 cleared the value-keyed request queues that existed on 10-21; neither the sweep nor a guard catches two transients that landed after it.

1. **`PendingEvidenceDeposits`** (`98d6eedfe`) holds `(chest FormID, item FormID, count)`.
   - Producers: `PayFine` and `GoToJail` push the deposit, from INFO result scripts inside the Late dialogue-selection system, or from quest-stage result scripts in an Update exclusive registered *after* `evidence_deposit_system`. Either way the consumer runs on the *next* frame's Update, so the queue always straddles the frame boundary where the save and load drains run. The allowlist reason "drained within the frame that queued them" is false.
   - Save at that boundary: the queued deposit is dropped. The ledger's `evidence` is saved, but nothing re-queues it (`ObScriptLedger::evidence()` has no production reader), so the confiscated goods never reach the `StolenGoods` chest and the player cannot recover them.
   - Load at that boundary: the outgoing deposit is FormID-keyed so it resolves in the loaded world; `deposit_into_reference` adds the goods to the loaded world's chest and `release_evidence` decrements the *loaded* ledger. The next save persists both (duplication).
2. **`VatsState`** (`818eba7f4`, #6350) holds the mode, target list and queue, all keyed by `EntityId`.
   - `execute_pending_save_loads` never resets it and F9 is not gated on VATS mode. An in-process quickload taken inside VATS brings the loaded session up in `TargetSelect`; `world_time_scale()` is 0, so the world is frozen. Target/queue lists still name the outgoing session's despawned entities.
   - Pressing the VATS key exits. The console `vats.confirm` instead charges the loaded player's AP and inserts `HitEvent`s on stale ids.
   - A cold-start load cannot do either, so in-process and cold loads diverge (breaks the #5435 contract). The allowlist reason "dies with the session" is false for an in-process load.

## Evidence
- `grep -n 'Vats\|Evidence' byroredux/src/save_io/live_load.rs` finds nothing.
- `exit_all` has one production caller, `vats_mode_system` (`systems/vats.rs`), which runs only on the VATS key press.
- `update.rs:320-331` registers the consumer as an Update exclusive at `:322`; exclusives sequence in registration order and the stage-result runner is registered at `:577`.
- `discard_outgoing_session_requests` (verified at HEAD) clears `PlayerMoveRequest`, `PendingPlayerXp`, `PendingSkillAdvances`, `PendingSleep`, `PendingMusic`, `PendingScriptMessages`, `MessageBoxQueue` only.

## Impact
- Evidence leg needs a save/load to drain on the exact frame `PayFine`/`GoToJail` ran. On the dialogue path F5/F9 are blocked while the page is open, so reachable triggers are console `save`/`load` and a stage-result `PayFine`. Narrow, hence LOW.
- VATS leg needs one keypress (F9 while in VATS): frozen post-load world until the player presses V; loses progress only through the console confirm path.
- The generic guard suggested in 10-21 -03 was never added; these two are the cost.

## Related
#6239, #6238, #6083, #5435, #6350, GAME-D4-2026-10-27-02 (a different VATS gate gap).

## Suggested Fix
- Clear `PendingEvidenceDeposits` in `discard_outgoing_session_requests` (or re-queue the ledger's undelivered evidence on load so the goods are not lost on the save side).
- Reset `VatsState` there too (`exit_all()` or `VatsState::default()`, keeping `roll_state` if determinism wants it). Consider refusing F5/F9 while `VatsState.mode != None`.
- Correct both allowlist reasons.
- Extend `live_load_drain_discards_the_outgoing_sessions_requests` with the two types.
- Add the generic source guard: every `NOT_SAVED_BY_DESIGN` row whose reason says "drained"/"consumed" must be discarded in the drain or justified as entity-keyed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

