# #7253: INC-2026-11-22-12: The widened #7230 drain guard still misses the "re-arms" / "restarts on load" wordings, so it does not "read every wording"

**Labels**: low,gameplay,save-load,test-gap,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-C-03**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: save Dim 1 (guard coverage) / test-gap
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs` `LOAD_RESET_PHRASES` (about `:1305-1316`); unmatched reasons: `AttractionState` ("a load re-arms"), `RandomConversationState` ("a load re-arms them"), `DeadBodyState` ("a load re-arms the scan"), `JailEscapeWatch` ("a load re-observes"), `ObScriptQuestTimers` ("cadence restarts on load"), `RestSession` ("reset on live load")
- **Changed in**: 48c6ea693 (#7230)
- **Status**: NEW
- **Description**: The phrase list is nine exact lowercase `a load <verb>` strings. At least six other NOT_SAVED rows promise a load-time reset in other spellings and are checked by nothing. `RestSession` happens to be reset by the drain; `AttractionState.pairs` (EntityId triples), `RandomConversationState` (EntityId maps) and `DeadBodyState.discovered` (a `HashSet<EntityId>` pruned only against live `Dead` bodies) are the same keyed-by-outgoing-entities class that `PackageDayBlocks` was, and the drain does not name them. I did not find a concrete wrong outcome: `AttractionState` rebuilds `pairs` each pass, and `DeadBodyState` prunes on its first pass; a recycled EntityId would have to collide.
- **Evidence**: `rg` over `NOT_SAVED_BY_DESIGN` reasons for `re-arm|on load|restarts on` returns the rows above; none appears in `discard_outgoing_session_requests`.
- **Impact**: The guard's stated purpose (the next unbacked promise fails a test) is only partly met; the commit message's "reads every wording" is an overstatement.
- **Related**: #7230, #6574, #6508
- **Suggested Fix**: Match on a stem (`"load"` within a few words of a reset verb including `re-arm`, `re-observe`, `restart`, `reset`) or add the six rows to `RESET_OUTSIDE_THE_DRAIN` with their real mechanism, so a rewording cannot dodge the check.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
