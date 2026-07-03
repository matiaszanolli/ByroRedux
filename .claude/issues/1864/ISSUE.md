# #1864: SCR-D7-NEW-01: QuestStageAdvanced markers collide on a shared single-entity sink

- **Severity**: MEDIUM
- **Labels**: `medium`, `bug`
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-03.md` (SCR-D7-NEW-01)
- **Dimension**: Scripting Runtime Systems / Engine Attach & Trigger Wiring

## Location
- `crates/scripting/src/papyrus_demo/quest_advance.rs:304-330` (`quest_advance_system`)
- `crates/scripting/src/fragment.rs:197-240` (`quest_fragment_dispatch_system`'s chained re-emission)

## Description
`QuestStageAdvanced` is a `SparseSetStorage` component (overwrite-on-repeat-insert). Both producers write multiple events onto the same sink entity (`PlayerEntity`) within one system call; only the last survives.

## Impact
Authoritative quest state is unaffected (written via `HashMap`). The notification layer silently drops all but the last same-frame advance — dormant for the fragment cascade (#1739 gated) but live-reachable via the wired `quest_advance_system`.

## Suggested Fix
Fan the marker out across per-source entities, or switch the sink to an accumulating `Vec<QuestStageAdvanced>` resource drained wholesale. Add a regression test for two same-frame advances.
