# SAVE-2026-10-31-01: `Faction.SendPlayerToJail` removes the whole inventory and hands it to a one-frame deposit queue that has no durable record; an unreachable chest deletes it

Labels: medium,save-load,bug,game:skyrim

Source: `docs/audits/AUDIT_SAVE_2026-10-31.md` (SAVE-2026-10-31-01). Severity MEDIUM. Dimension: Snapshot Completeness (allowlist reason) / Live Load-Apply (frame boundary). Data-loss class: silent-drop (irrecoverable). Builds on open #6508 (SAVE-2026-10-27-01) and widens it.

**Location**:
- `crates/scripting/src/jail.rs:339-376` (`send_player_to_jail`) and `:395-405` (`deposit_inventory`)
- `crates/scripting/src/jail.rs:77` (`PendingEvidenceDeposits`)
- `byroredux/src/crime.rs:713-729` (`evidence_deposit_system`)
- `byroredux/src/cell_loader/reference_state.rs:117-137` (`deposit_into_reference`)
- `byroredux/src/save_io/registry_completeness_tests.rs:756` (the allowlist row)

## Description
1. **The Oblivion path has a durable record.** `confiscate_stolen_goods` calls `ledger.confiscate(item, taken)` on `ObScriptLedger` (saved, v118), then queues the deposit. The allowlist row relies on this ("the ObScriptLedger's saved evidence is the durable record until delivered").
2. **The Skyrim path (`36d7bfbd6`) has none.** `deposit_inventory` calls `remove_items(world, player, item, count, None)` for every stack the player holds, then pushes an `EvidenceDeposit`, never touching the ledger. Until `evidence_deposit_system` runs on the *next* Update, the only copy of the player's whole inventory is an unsaved `Vec` in `PendingEvidenceDeposits`.
3. **A deposit that cannot land is dropped.** `evidence_deposit_system` does `mem::take` on the queue. If `deposit_into_reference` returns `false` (no resolvable PLCN chest, or no `GlobalFormIdResolver` / `PersistentReferenceStates`) it logs "stay in the evidence"; on the Skyrim path there is no evidence, so the goods are gone.
4. **Side effects.** `deposit_into_reference` writes `StoredStack { instance: None }`, so an improved item (v122 `quality`) loses its instance. A parked row replaces the chest's authored inventory at respawn; `send_player_to_jail` checks only `player_inventory_container != 0`, not that the chest is authored empty, which the `deposit_into_reference` doc requires of its caller.

## Trigger paths
- **A save or load drain between the removal and the next Update.** `SendPlayerToJail` runs from INFO result scripts in the Late dialogue-selection system, quest-stage fragments, or the `story_actions` console command. The save drain runs post-scheduler of the same tick; the consumer runs next Update. A save taken there is written with an empty inventory, an empty chest, `ledger.jail` set and `PlayerMoveRequest` pending; loading it loses the inventory permanently. A load at that boundary instead deposits the outgoing timeline's goods into the loaded world's chest (the #6508 mechanism). F5/F9 are blocked while a game-menu page is open, so practical triggers are console `save`/`load` and fragment-driven calls.
- **An unreachable chest.** Needs no save. Theoretical on vanilla Skyrim (`GlobalFormIdResolver` resolves any load-order FormID); a mod faction whose PLCN names a form not in the load order would delete the player's kit.

## Evidence
`jail.rs:395-405` contains no `ledger` access; `jail.rs:210-215` (Oblivion path) calls `ledger.confiscate` before the push; `live_load.rs` has no `Evidence` or `Jail` reference and `discard_outgoing_session_requests` does not clear the queue.

## Impact
Medium rather than low because the magnitude changed: the 10-27 finding concerned stolen goods only; this path moves the player's entire carried inventory with no ledger fallback. The reachable window stays narrow.

## Suggested Fix
- **Preferred**: deposit synchronously in `send_player_to_jail` when `deposit_into_reference` succeeds, taking goods from the inventory only after it returns `true`. If the chest is unreachable take nothing. `release_evidence` bookkeeping becomes Oblivion-only.
- Or: record the Skyrim goods in `ObScriptLedger` evidence as `confiscate_stolen_goods` does, making the allowlist row true.
- Either way, clear `PendingEvidenceDeposits` in `discard_outgoing_session_requests` (the #6508 fix), correct the row, and refuse a chest that is not authored empty.

Related: #6508.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
