# #5397: OBL-2026-10-08-D2-01: #5367 Phase L's Random-INFO rule inverts the authored semantics. A passing non-Random INFO beats earlier passing Random INFOs, Random sets pool across boundaries, and Random End is never read. Phase G makes this liv...

**Labels**: medium,scripting,gameplay,dialogue,bug,game:oblivion,game:fo3,game:fnv,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5397

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-08.md` — `OBL-2026-10-08-D2-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. The wrong line is chosen, and it can invert quest priority. There is no crash or data loss.
- **Dimension**: BSA v103 & ESM Data Slice. The data half is Oblivion's. The mechanism is owned by `/audit-scripting` (`crates/scripting/src/dialogue.rs`) and `/audit-gameplay` Dim 2.
- **Location**:
  - `crates/scripting/src/dialogue.rs:348-364`: the `select_info` pick.
  - `crates/scripting/src/dialogue.rs:645-662`: the test `non_random_candidate_keeps_priority_over_the_random_pool`, which pins the deviating behaviour.
  - `crates/plugin/src/esm/records/misc/dialogue.rs:388-411`: `InfoDataHeader` exposes only Goodbye, Random and Say Once. It has no Random End (`0x20`) accessor.
  - `docs/engine/dialogue-trees.md:98-100` and `:137`: the doc calls the rule "vanilla semantics".
  - `byroredux/src/systems/npc_dialogue.rs:275-280` (`generic_greeting_record`, with no game gate) and `:310-311`.
- **Status**: NEW.
  - `gh` searches for "Random End" and "random INFO dialogue" find only #5367 (the open epic, whose Phase L shipped this rule).
  - No 2026-10-08 report covers the selection rule. FNV-D2 mentions "Say-Once/Random pools" only in the context of DLC INFO loss.
- **Description**:
  - `select_info` collects every passing candidate in quest-priority / file order.
  - If any passing candidate is not Random-flagged, the first such one wins. Only when every passing candidate is Random does it roll uniformly over all of them.
  - Both authoring references describe a different, positional rule.
  - The CS wiki, `Category/Editing Dialogue.wiki` and `Dialogue Tutorial.wiki` (Oblivion):
    - "the game starts at the top of the list of info lines and proceeds down until it finds one that satisfies current conditions".
    - "All random infos that appear sequentially … are put in a list. One is chosen at random".
    - "An Info marked Random End will terminate the random set even if the next info is also marked Random."
  - The GECK wiki, `Category/Dialogue.wiki` (FO3/FNV): "If an actor qualifies for a Random info … the info is put on a stack … The stack keeps building until an info that is not marked Random is found, or an info marked Random End is found. At that point one of the infos in the stack is selected randomly."
  - The engine diverges in three ways:
    1. A later passing non-Random INFO beats an earlier passing Random set. Vanilla speaks from the Random set. With priority ordering, a lower-priority quest's plain line therefore outranks a higher-priority quest's Random greetings.
    2. Random INFOs pool across non-adjacent runs.
    3. `Random End` (Flags `0x20`, decoded in xEdit TES4 `wbDefinitionsTES4.pas` INFO `DATA`) is ignored, so adjacent sets merge.
- **Evidence**:
  - The in-tree test is the exact counter-example: `[0x401 Random, 0x402 plain]`, both passing, asserts `0x402`. Vanilla's stack is `{0x401}`, terminated by the non-Random `0x402`, so vanilla speaks `0x401`.
  - Raw `Oblivion.esm` census (`/tmp/audit/oblivion/greeting_census.py`): the `GREETING` DIAL 0xC8 has 3,743 INFOs.
    - 461 are Random, in 102 runs (the longest is 80), and 57 carry Random End.
    - 3,281 non-Random INFOs follow the first Random one.
    - Master-wide: 6,201 Random and 322 Random End INFOs.
  - Random End is load-bearing in vanilla. For example, the SECrime/Crime guard arrest sets "Halt, lawbreaker…" and "Halt, scofflaw…" are adjacent Random sets that are separated only by Random End.
  - Why it is live on Oblivion:
    - `generic_greeting_record` matches EDID `GREETING`.
    - `SceneAliasCandidate` is stamped regardless of game.
    - Oblivion NPCs own no topics (there are no aliases), so `open_conversation` falls through to the greeting at `npc_dialogue.rs:310` on every activation.
- **Impact**:
  - Every Oblivion activation greeting takes the first plain passing line, or a merged pool, instead of the authored positional Random set.
  - Generic and specific greeting variety collapses toward fixed lines.
  - Quest priority can be inverted.
  - The same mechanism runs on FO3/FNV, where the GECK states the same stack rule. FNV `Flags1` Random appears on about 5,500 INFOs (dialogue-trees.md census).
  - The doc presents the deviation as vanilla behaviour, so the next implementer has no warning.
- **Related**: #5367 (Phase L), #5271 (QSTI per INFO plus the priority sort it orders by), #5350 (Oblivion topic-list model), FNV-2026-10-08 D2 (DLC INFO merge).
- **Suggested Fix**:
  - Walk the candidates in priority / file order to the first passing INFO. If it is Random, gather the immediately following passing Random INFOs until a non-Random INFO or a Random End INFO, and roll within that stack.
  - Add `random_end()` (`flags1 & 0x20`).
  - Rewrite the pinned test to the vanilla expectation, and add an Oblivion `GREETING` real-data floor.
  - Correct the dialogue-trees.md wording.

## Completeness Checks
- [ ] **SIBLING**: Same positional Random-stack rule verified for the FO3/FNV path (GECK states the same rule) and the Skyrim/FO4 INFO flag layouts; `random_end()` added alongside `goodbye()`/`random()`/`say_once()` on both `InfoDataHeader` variants
- [ ] **TESTS**: A regression test pins this specific fix — `non_random_candidate_keeps_priority_over_the_random_pool` rewritten to the vanilla expectation, plus an Oblivion `GREETING` real-data floor
