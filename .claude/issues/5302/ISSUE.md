# #5302 — SCR-D2-2026-10-05-02: #5152 doc rot — apply_spoken_info_fragment was spliced into quest_fragment_dispatch_system's doc block, and the DialogueInfoFragments doc still says OnEnd is never dispatched

- **Labels**: low,scripting,doc-rot,documentation
- **Filed from**: `docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5302

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/fragment/systems.rs:209-239`: the doc block and the two functions it now straddles.
  - `crates/scripting/src/fragment/state.rs:353-366`: the `DialogueInfoFragments` doc.
- **Status**: NEW. This is the same class as #5104 and #5227 (both closed).
- **Description**: the new `///` block was inserted between `quest_fragment_dispatch_system`'s doc ("Consume
  `QuestStageAdvanced` markers … bounded by `MAX_CASCADE` …") and its `pub fn`. The result:
  - `apply_spoken_info_fragment` now wears both docs.
  - The crate's central public dispatcher is undocumented.

  Separately, the `DialogueInfoFragments` doc says "The OnEnd binding is stored but not yet dispatched … Vanilla's 3 773
  OnEnd-only INFOs stay inert". The same commit dispatches OnEnd on selection change and on close
  (`npc_dialogue.rs:353-362, 400-418`).
- **Impact**: rustdoc and readers are misled about the dispatcher's contract and about OnEnd. No runtime effect.
- **Suggested Fix**: move the `apply_spoken_info_fragment` doc below `quest_fragment_dispatch_system`'s `pub fn`, or above
  its doc block. Reword `state.rs:360-366` so it says OnEnd fires on selection change and on close.

_Source: `AUDIT_SCRIPTING_2026-10-05.md` (SCR-D2-2026-10-05-02), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
