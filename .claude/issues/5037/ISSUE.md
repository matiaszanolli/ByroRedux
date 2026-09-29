# #5037 — GAME-D2-2026-09-29-01: NPC topic ownership ignores speaker, DIAL category and branch structure — Eltrys "owns" all 117 MS01 topics, and the opening line is picked by lowest form id

**Labels**: medium,gameplay,dialogue,quests,game:skyrim,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: 2 — NPC dialogue (P4)

## Location
- `byroredux/src/systems/npc_dialogue.rs` — `owned_topic_records` (filters `index.dialogues` only by `quest_refs` ∩ owned quests, sorts by form id) and the list + first-passing selection
- `crates/debug-ui/src/panels.rs` — renders every entry
- `crates/plugin/src/esm/records/misc/dialogue.rs` — `parse_dial`: `dial_type = sub.data[0]`

## Description
An NPC owns every DIAL whose `quest_refs` contains any running quest that binds it. Nothing checks whether the topic's INFOs are spoken by this NPC, its category (Topic vs Scene/Combat/Misc), or whether it starts a branch or is a child reachable only through a parent INFO's link. The topic list is built from all of them, and the opening line is the INFO on the lowest-form-id DIAL with a passing INFO.

## Evidence
Raw census of Skyrim SE `Skyrim.esm`, DIALs with `QNAM == 0x00018B4B` (MS01):
- **117 DIALs**: 108 Topic (DATA byte 1 = 0), 5 Scene (2: `0xD6627`, `0xD6628`, `0xD6641`, `0xD6642`, `0xD6643`), 4 Misc (7: `HELO 0x228A5`, `GBYE 0x228A4`, `IDAT 0x82546`, `NOTI 0x9C88D`).
- Editor IDs name ~14 speakers (Eltrys, Hogni, Kerah, Guard, Margret, Innkeeper, Thonar, Rhiada, Weylin, Nepos, Uaile, Garvey, Mulush, Omluag).
- Lowest-form-id-first puts mid-branch child `0x18A30 MS01EltrysBlockingShrineBranch01Topic01` ahead of the branch entry `0x806B8 MS01EltrysBlockingShrineBranch01EntryTopic`.
- `dial_type` cannot be used as a filter today: `parse_dial` stores `DATA[0]`, which on Skyrim is the Topic *Flags* byte (0 on all 117); the category is byte 1.

## Impact
On the one supported P4 route (start MS01, activate Eltrys in `MarkarthWarrens`), the response surface lists every MS01 prompt of every speaker plus scene lines and barks. Clicking another NPC's topic fails `select_on_topic` ("no INFO passes") with only a `log::warn!`. Whenever a mid-branch child's INFO passes, the NPC opens by answering a prompt the player never chose. Applies to any NPC bound to any running quest.

## Related
ECS-2026-09-29-D7-01 (snapshot reads the first `NpcDialogueTopic`), ESM-2026-09-29-D2-03 (QNAM docs), LC-D3-01 (DIAL `DATA` byte 0 read as the category — `/audit-esm` owns the byte layout); GAME-D2-2026-09-29-02 (#5043).

## Suggested Fix
Keep only topics with at least one INFO whose speaker condition can match this NPC; exclude Scene/Combat/Misc categories once `DATA[1]` is decoded (Skyrim/FO4 layout fix via `/audit-esm`); start from branch-entry topics (DLBR `SNAM`, or the top-level flag) rather than lowest form id. Add a fixture test with two speakers and a branch child.

Validated at HEAD 9fcfdc3fc: `owned_topic_records` filters solely on `record.quest_refs` and sorts by `form_id`; `parse_dial` still assigns `dial_type = sub.data[0]`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
