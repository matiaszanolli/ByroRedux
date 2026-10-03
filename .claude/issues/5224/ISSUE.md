# #5224 — FO3-D2-02: FO3/FNV DIAL `DATA` Flags byte (`Top-level` 0x02 / `Rumors` 0x01) is dropped — 4 286 of 5 130 FO3 Topic DIALs are choice-only but would list as top-level entries

https://github.com/matiaszanolli/ByroRedux/issues/5224

Source: `docs/audits/AUDIT_FO3_2026-10-03.md` (HEAD `be3cd9468`)

- **Severity**: LOW. It is latent: FO3/FNV quests have no aliases, so no dialogue owner binds on FO3 today.
- **Dimension**: ESM Data Slice. The decode belongs to `/audit-esm`; the consumer is `/audit-gameplay` (`npc_dialogue`). FO3 and FNV reach.
- **Location**:
  - `crates/plugin/src/esm/records/misc/dialogue.rs:84-104` (`DialogueCategory::from_data` reads byte 0 only);
  - `crates/plugin/src/esm/records/misc/dialogue.rs:382`;
  - `byroredux/src/systems/npc_dialogue.rs:26-27, 141-158` (`topic_entry` returns `TopLevel` for every branch-less topic).
- **Status**: NEW.
  - The code itself acknowledges the gap ("FO3/FNV's DIAL `Top-level` flag is not decoded"), but there is no issue.
  - #5037 (closed) fixed only the Skyrim+ DLBR half.
  - It is distinct from #4469 (INFO `DATA`).
- **Description**: FO3/FNV DIAL `DATA` is `Type u8, Flags u8`. xEdit `wbDefinitionsFO3` defines the flags as Rumors 0x01 and Top-level 0x02.
  - Only Top-level Topic DIALs open the menu; the rest are reached through INFO `TCLT` choices.
  - 7ab87c0fb decodes the category but drops the flags.
  - So every owned, branch-less Fallout-era topic would surface as an opening entry. That is the #5037 failure mode.
- **Evidence**: FO3 master census. Type-0 (Topic) DIALs number 5 130. Their flags are 0x00 ×4 284, 0x02 ×844 and 0x01 ×1, so **844 are top-level**. `DialRecord` has no field that carries byte 1.
- **Impact**: none today. Once FO3/FNV dialogue ownership is wired, about 84% of Topic DIALs would appear as opening options.
- **Related**: #5037, #5045, #4469.
- **Suggested Fix**:
  - Decode byte 1 into a `DialRecord` top-level flag at the parser boundary.
  - Have `topic_entry` return `LinkOnly` for a branch-less Fallout-era topic that is not top-level.
  - Pin 844 / 5 130 against the FO3 master.

## Completeness Checks
- [ ] **SIBLING**: The FNV DIAL `DATA` flags byte decodes through the same arm
- [ ] **TESTS**: A real-data pin asserts 844 / 5 130 top-level Topic DIALs on Fallout3.esm, plus a `topic_entry` unit test for a non-top-level Fallout-era topic
