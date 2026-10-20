# TD8-2026-10-19-02: `ObScriptLedger` stores state the engine never reads back (five fields, one with no production writer)

Labels: low,scripting,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW — **Dimension**: 8 — **Status**: NEW
- **Location**: `crates/scripting/src/obscript_ledger.rs`: `item_values` (`item_value` `:155`, zero callers anywhere; writer `obscript_more/state.rs:199` SetItemValue logs "done"), `tag_skills` getter `tag_skill` (`:188`, zero callers; `is_tag_skill` is the live reader), `known_topics` (`knows_topic` `:98`, test-only `obscript_more/tests.rs:149`; AddTopic writes it), `quest_objects` (`is_quest_object` `:114`, test-only `:151`; `state.rs:125` writes), `disposition_mods` (`disposition_mod` `:126`, test-only `tests.rs:141`; `state.rs:113` writes), `talked_to_player` (`note_talked_to_player` `:110` has NO production caller — only `tests.rs:160`; the reader `obscript_more.rs:324` GetTalkedToPC therefore can never be true outside a test)
- **Age**: 6e9d55aa9 (2026-10-18)
- **Effort**: small
- **Description**: the module doc says the ledger keeps values "nothing else in the engine owns", but for these five nothing reads them either: the commands report `done(...)` and the value is serialized into saves (FORMAT_MAJOR 112) with no effect. `talked_to_player` is the sharpest case: a getter wired to a condition with a setter nobody calls.
- **Evidence**: `git grep -nw` of each accessor excluding `obscript_ledger.rs` and test paths (table in session log); production-vs-test counts `knows_topic prod=0 test=1`, `is_quest_object 0/1`, `disposition_mod 0/1`, `item_value 0/0`, `tag_skill 0/0`, `note_talked_to_player 0/1`.
- **Proposed fix**: per field either (a) wire the consumer (dialogue topic gate, barter value lookup, disposition in greet/persuasion, call `note_talked_to_player` from the dialogue-open path) or (b) make the command decline (`Action::Declined`) per the decline-on-unmodeled rule and drop the field + getter. Public API of a workspace-internal crate, so note rather than delete the getters if a consumer is planned.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
