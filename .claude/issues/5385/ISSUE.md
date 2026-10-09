# #5385: ESM-2026-10-08-D4-01: Story Manager `SNAM` is the *previous* sibling, but it is decoded and dispatched as the next sibling, so every stacked SM sibling group is walked bottom-up

**Labels**: medium,esm-plugin,scripting,quests,bug,game:skyrim,game:fo4,game:fo76,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5385

**Source**: `docs/audits/AUDIT_ESM_2026-10-08.md` — `ESM-2026-10-08-D4-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM. Behaviour is wrong on every Skyrim / FO4 / FO76 / Starfield Story Manager event, and there is no workaround. It is not a crash.
- **Dimension**: Record Schema Dispatch & Coverage
- **Record / Sub-record**: `SMBN` / `SMEN` / `SMQN` · `SNAM`
- **Location**:
  - The decode: `crates/plugin/src/esm/records/misc/story_manager.rs:133-134` (`next_sibling` doc) and `:195-198`.
  - The doc claim: `docs/engine/story-manager.md:100`.
  - The consumer: `crates/scripting/src/story_manager.rs:288` (pointer resolve), `:322-337` (the head is "the member no in-group sibling points at"), and `:633-640` (the walk follows the pointer).
- **Status**: NEW
- **Description**: xEdit defines `SNAM` as `wbFormIDCkNoReach(SNAM, 'Previous Node', [SMQN, SMBN, SMEN, NULL], False, cpBenign)` on all three record types in TES5 (`wbDefinitionsTES5.pas:7059/7074/7101`), FO4, FO76 and SF1. The design doc marks "next-sibling" **[verified]**, but the test it cites (the target shares the source's `PNAM`) holds in both directions, so it cannot tell next from previous. The runtime takes the node nothing points at as the chain head and follows `SNAM` forward. Under xEdit's reading that head is the **bottom** of the CK list, so the evaluation order is reversed.
- **Evidence** (three independent signals, all agreeing with xEdit; scripts `sm_snam.py`, `sm_dlc.py`):
  1. **DLC append shape.** The CK adds a new node "below all other entries" (CK wiki, *Bendu and S M Nodes*).
     - DLC nodes that link via `SNAM` to a vanilla node nothing points at, without overriding any vanilla record: Dawnguard 7, HearthFires 2, Dragonborn 6, FO4 DLCCoast + NukaWorld 5. Under "previous" these are bottom-appends, which is the CK default. Under the decoded "next" they would be inserted at the **top**.
     - The opposite pattern, a vanilla `SNAM 0 → DLC` override, appears only 3×: WEQuestNode twice and QuestNode once.
  2. **FormID age.** The same-parent `SNAM` target has the lower (older) FormID in 302/441 Skyrim, 135/218 FO4 and 191/366 Starfield edges. That is "points at the node created before me".
  3. **Bethesda's own tutorial** (CK wiki, *Bethesda Tutorial Story Manager*) says of the Kill Actor event: "Click the plus-sign at the top of the tree to expand the node labeled 'DA08KillFriendNode'". In the decoded order `DA08KillFriendNode` (`0x10FAEF`) is the **last** of KILL's 10 children; in xEdit's order it is the first.
- **Impact**: Stacked order is semantic. The CK rule is that the first processed non-sharing quest node consumes the event, and the CK wiki warns that sharing nodes must sit higher than non-sharing ones. With the order reversed:
  - The **CLOC** event walks the `WI*SHARES` nodes, ClearSkies and the BQ/Favors/Dungeon branches *before* `CWChangeLocationScenes`. In the authored order, QuestNode / CompanionsNode / IntroScenesNode / `CWChangeLocationScenes` (non-sharing) come first, and once `CWChangeLocationScenes` fires it consumes the event before every `WI*` node.
  - Stacked pool order and every DLC node's position flip as well.
  - The `sm1-story-manager.sh` expected boot set (`WIGreetingNodeSHARES` + `CWChangeLocationScenes`, `CRHoldExpansion` stopped) was derived from the reversed walk and would need re-deriving.
- **Related**: #5366 (closed), D4-02. `/audit-scripting` owns the walk.
- **Suggested Fix**: Rename the field to `previous_sibling` with the xEdit citation. In `build_story_manager_tree`, take the member with `SNAM == 0` (or a dangling pointer) as the head and walk the inverse edges. Pin the order with a real-data test (Skyrim KILL head = `DA08KillFriendNode`), and re-derive the sm1 expected set.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the SNAM walk in `build_story_manager_tree` / `walk_siblings`, `docs/engine/story-manager.md`, `sm1-story-manager.sh` expected set)
- [ ] **TESTS**: A regression test pins this specific fix
