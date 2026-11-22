# #7115 — ESM-D7-03: `npc_dialogue_follow_up` rescans every DIAL's INFOs each frame while all follow-ups fail their gates

Labels: low,esm-plugin,performance,dialogue,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D7-03)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D7-03**

- **Severity**: LOW (per-frame linear scan of an `EsmIndex` map, but only in one rare conversation state)
- **Dimension**: 7 (EsmIndex → ECS handoff, per-frame scans)
- **Record / Sub-record**: `INFO` `TCFU` follow-ups (`InfoRecord::follow_ups`)
- **Location**: `byroredux/src/systems/npc_dialogue.rs:743-788` (the scan is `:774`), called every frame from `npc_dialogue_selection_system_inner` (`:875`)
- **Status**: NEW (`0d1c81b7b`, 2026-11-11)
- **Description**: once the open line's `line_ends_at` has passed, the function looks up the current INFO's `follow_ups`; for each it does `index.dialogues.values().find_map(|record| record.infos.iter().find(|info| info.form_id == follow_up) ...)`, a scan over every topic and every INFO in the load order, then clones the info and the topic header into a one-INFO `DialRecord` and evaluates its conditions. When no follow-up passes ("None passing leaves the conversation as it is", per its doc), nothing clears `line_ends_at` or records that the check ran, so the same scan, clone and condition pass repeat every frame for as long as the dialogue surface stays on that line.
- **Evidence**: the only writers of `line_ends_at` are `npc_dialogue.rs:469` (presenting a line) and `scaleform_dialogue.rs:161` (clamping it); the failure path of `npc_dialogue_follow_up` (`continue` at `:775`, `:779`, `:782`, end of loop) writes nothing. `rg 'info_to_topic|topic_of_info|infos_by'` over `crates` and `byroredux/src` is empty: there is no INFO → topic reverse index.
- **Impact**: bounded by how often a line with `TCFU` follow-ups sits open with every follow-up gated off (New Vegas intro lines are the authored case). The scan is on the order of the INFO count (tens of thousands on FNV/Skyrim) per follow-up per frame plus two heap clones: tenths of a millisecond to about a millisecond per frame while the menu idles, not a correctness bug.
- **Suggested fix**: remember the checked `(topic, info)` pair on `DialogueSurfaceState` (or clear `line_ends_at` once the follow-up decision is made) so the decision runs once per line; and resolve a follow-up INFO's topic through a once-built `info_form_id → topic` map on `EsmIndex` (or on the dialogue runtime) instead of a scan.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
