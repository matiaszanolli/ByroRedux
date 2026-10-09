# #5404: SCR-D5-2026-10-08-02: A processed non-sharing quest node consumes the event even when it starts nothing, which contradicts the CK tutorial's rule. Once its pool is running, `CWChangeLocationScenes` swallows every CLOC

**Labels**: medium,scripting,quests,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5404

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D5-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `state.consumed = true` at `story_manager.rs` ~672 fires on any passing non-sharing quest node with no check that a candidate was queued.

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue (Story Manager)
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/story_manager.rs:664-675`: consumption keys on `evaluate(..)` passing plus `kind == Quest &&
    !shares_event`, and does not check whether a candidate was queued.
  - `:712-730`: `process_quest_node` returns with no candidate when nothing is eligible.
  - `:605-621` (the doc) and `docs/engine/story-manager.md` §7 Phase 3 ("keyed on processing … not on a quest actually
    starting").
- **Status**: NEW
- **Description**: the design reads "the event will be consumed … as soon as it finishes with that node" from the *SM
  Event Node* page as keyed on processing. Bethesda's own *Bethesda Tutorial Story Manager*
  (`/mnt/data/src/reference/ck-uesp-wiki/(main)/Bethesda Tutorial Story Manager.wiki:49`) states the trigger
  explicitly: "without this box, **if a node actually manages to fire a quest**, the event will not go through the rest
  of the tree". The same page's *Num quests to run* text also has the node "attempt" starts that can fail.
- **Evidence**: `CWChangeLocationScenes` is a stacked, non-sharing node. Its pool is `CW00SolitudeMapTableScene`,
  `CW01SolitudeMapTableScene`, `CW00WindhelmMapTableScene` and `CW01WindhelmMapTableScene` (census). Its gate passes
  almost always (SCR-D5-01). Each CLOC starts the next pool entry. Once all four are running, `eligible` is empty, so no
  candidate is queued, yet the walk still sets `consumed = true`. From then on, every node the walk visits after it gets
  no CLOC: `IntroScenesNode`, `CompanionsNode` (with `C00LocationMonitoringNode*` and `CR08`/`CR09LocationMonitoring`)
  and `QuestNode`. The reverse `SNAM` reading (ESM-D4-01) only changes which nodes are starved: the WI/Dungeon/BQ/Favors
  branches instead.
- **Impact**: location-driven quest progress below any always-passing non-sharing node stops permanently after its pool
  fills. Vanilla relies on such nodes failing to start (alias fill, running quest) without blocking the tree.
- **Related**: SCR-D5-2026-10-08-01, SCR-D5-2026-10-08-04. In vanilla, alias-fill failure is the usual reason a start
  "fails". The runtime has no alias-gated start (see Future-Phase Readiness).
- **Suggested Fix**: set `consumed` only when the node queued (and phase B actually started) at least one quest. Because
  phase B decides the start, move the consume decision into a per-event replay, or re-check `start_quest`'s result per
  candidate. Correct the doc's reading, and add a test where a non-sharing node whose quest is already running lets a
  sibling below it fire.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the random-parent choose-one early return, which already keys on `state.candidates.len() > queued_before`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
