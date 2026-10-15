# OBL-D2-2026-10-14-01: #5734 quest-level dialogue gates fail closed on 339 of 390 Oblivion quests, declining 87 % of all INFOs and 87 % of GREETING

**Labels**: medium,game:oblivion,legacy-compat,scripting,dialogue,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D2-2026-10-14-01`. Re-verified against current code before filing.

- **Severity**: MEDIUM (HIGH once any Oblivion dialogue gate / live path exists; today nothing observes Oblivion dialogue)
- **Dimension**: 2 (DIAL/INFO selection; owner `/audit-scripting` for the condition catalog)
- **Location**: `select_info` / `quest_gate_passes` (`crates/scripting/src/dialogue.rs`), `is_modeled` + `ConditionFunction::from_index` (`crates/scripting/src/condition.rs`), install in `populate_scene_runtime` (`byroredux/src/asset_provider/script.rs`, unconditional, no game gate), `parse_qust_contextual` (`crates/plugin/src/esm/records/misc/quest.rs`, pre-`INDX` `CTDA` → `QustRecord.dialogue_conditions`)
- **Status**: NEW (commit `6709eb65e`, after the 2026-10-12 baseline). Sibling of open #5750 (Skyrim leg) and #5752 (INFO-level uncatalogued functions); #5734 itself is closed and its commit message names "Oblivion 254/365" as accepted fallout. Dedup checked: no open issue covers the quest-level Oblivion leg.
- **Description**: #5734 evaluates each quest's Quest-Conditions list before any of its INFOs and applies the #5380 doctrine to it: one uncatalogued function in the list and the whole quest's INFOs decline. Oblivion authors a quest-level list on nearly every dialogue quest, and the lists carry fn 254 and fn 365, neither of which is in the 48-function catalog (`from_index` has no arm for either).
- **Evidence**: raw-byte census of `Oblivion.esm`. 339 of 390 QUST have `CTDA` records before the first `INDX` (e.g. `0x42b9c`: EDID SCRI FULL ICON DATA **CTDA(fn 254)** INDX …; `0x3be84`: … DATA CTDA(254) CTDA(71)). Function counts: fn 254 ×308, fn 365 ×297, fn 71 ×75, 69 ×12, 67 ×11, 72 ×10, 70 ×2, 310 / 313 ×1. All 339 lists contain an uncatalogued function. INFOs owned (by `QSTI`) by those quests: **16 799 of 19 278**; GREETING: **3 247 of 3 743**. The remaining 496 GREETING INFOs belong to quests with no list. The pre-`INDX` placement is quest-level (stage and target `CTDA` come after `INDX` / `QSTA`; the 3 065 stage-level `CTDA` are parsed into the stage, not the gate).
- **Impact**: once an Oblivion dialogue route is exercised, 87 % of lines can never be selected. Every activation opens GREETING (#5367 Phase G), so NPCs fall back to the ~13 % of lines in ungated quests. The fail-closed choice is correct for the priority-hijack case #5734 targeted, but on Oblivion these are ordinary "playable race / not in Shivering Isles" gates. Static analysis plus code reading; not executed in a live conversation (no Oblivion dialogue gate exists).
- **Related**: #5750, #5752, #5734, #5380. 254 and 365 are very probably `GetIsPlayableRace` / `GetPlayerInSEWorld`, but the opcode-to-name binding is unsourced (the CS wiki list carries no opcode numbers, the xEdit TES4 table is not on disk). Do not guess.
- **Suggested Fix**: source the TES4 opcodes (OBSE command table / xEdit `wbDefinitionsTES4`), catalogue 254 and 365, and re-run the census. Until then consider scoping the fail-closed rule to quest lists that actually outrank another quest (the hijack case) rather than every unreadable list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
