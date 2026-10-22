# #6219: SCR-2026-10-21-05: #6102's fix never retires the worldspace persistent CELL's pending `OnReset`, which brings back the per-frame rescan and late spurious resets
STATE: OPEN
LABELS: bug,medium,performance,scripting,game:fnv,game:fo3,game:oblivion

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Legacy ObScript (loader-fed state) · **Location**: `byroredux/src/cell_loader/exterior.rs:1166-1170` (the persistent root calls `note_player_entered_cell`); `PersistentCellApplyJob::advance`, `exterior.rs:329-431` (returns `Complete` with no `cell_load_finished`); the only retirers are `load.rs:733` and `exterior.rs:2521`; `crates/scripting/src/obscript_state.rs:243-280` (`mark_cell_reset`)

**Audit status**: NEW (residual of closed #6102, `2efed9843`)

**Untrusted-Input**: No

## Description

The persistent CELL is "entered" each time the worldspace is built. When the player returns after 72 game hours or more (`iHoursToRespawnCell`), `note_cell_entered` queues a reset for it. `mark_cell_reset` now keeps an entry until `cell_load_finished` marks it, or until its root despawns. The persistent job never calls `cell_load_finished`, so the entry lives as long as the player stays in the worldspace. Every frame, `mark_cell_reset` rebuilds the scripted-instance list and does two lookups per instance; that is the cost #6102 set out to remove. The persistent root also keeps acquiring references after its job completes: #6076 tile-streamed persistent actors are stamped under it (`exterior.rs:1329`), and so are re-spawns after eviction. Each of those receives `OnReset` when it appears, possibly hours after the entry.

## Evidence

grep finds `cell_load_finished` only at `load.rs:733` and `exterior.rs:2521`. #6102's three tests cover interior and tile loads, not the persistent job.

## Impact

On FNV, FO3 and Oblivion, trap and animated-object `OnReset` blocks fire on references the player meets mid-session, and per-frame work persists for the whole exterior stay.

## Related

#6102, #6076, SCR-SD-02.

## Suggested Fix

Call `obscript_state::cell_load_finished(world, self.cell_root)` before `PersistentCellApplyProgress::Complete`. Add a test that loads a persistent root and then stamps a later reference under it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every cell-root producer that calls note_player_entered_cell (interior, tile, persistent))
- [ ] **TESTS**: A regression test pins this specific fix

