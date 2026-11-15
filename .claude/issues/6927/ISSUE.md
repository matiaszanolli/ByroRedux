# #6927 — INC-2026-11-14-15: A live load leaves `PendingRelocations` / `PendingActorMoves` / `TravelReports` from the outgoing session

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-15). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/save_io/live_load.rs` (`discard_outgoing_session_requests`); frame order in `byroredux/src/app_events.rs` (`step_save_loads` → `step_cell_transition` → `step_actor_relocations`)
- **Changed in**: new resources in commit `4b71155ca`

## Description
The scheduler runs earlier in the frame (`travel_system`, `door_fade_system`, scripts) and may push relocations or scripted moves. `step_save_loads` then loads, and `step_cell_transition` → `step_actor_relocations` drains those queues against the loaded session, evicting and ledgering actors by FormID there. `discard_outgoing_session_requests` clears many other outgoing queues but none of these three; `TravelReports` also carries stale reasons into `actor.where` and the census.

## Evidence
No non-test reference to `PendingRelocations`, `PendingActorMoves` or `TravelReports` under `byroredux/src/save_io/`.

## Impact
A relocation from the outgoing timeline is applied in the loaded session (an actor moved or evicted that the save keeps in place).

## Suggested Fix
Clear all three in `discard_outgoing_session_requests`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other XCT pending resources added in the same commit)
- [ ] **TESTS**: A regression test pins this specific fix
